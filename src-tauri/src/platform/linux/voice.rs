//! Your recording, played with the song ("Play with the song", src/voice.rs): native/voice.swift's counterpart.
//! The recording is decoded whole (audiofile.rs) and played through a sound server stream (render.rs) whose clock says which
//! sample the output has played, so where the voice is heard is known exactly, and a start is put at an exact
//! moment by the silence written ahead of it. A peak limiter brings the quiet microphone up to a listening level
//! (its pre-gain) without clipping, as Apple's peak limiter does on the Mac.
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::render::{Fill, Stream};

/// Fades in and out over this long (s).
const FADE: f64 = 0.06;
/// A start is set this far ahead (s): time to fade out what plays, then it begins exactly then.
const LEAD: f64 = 0.1;
/// What the stream's clock calls played reaches what listening hears of the music app (its per-process loopback)
/// this much later (s). On the Mac it's measured (0.063 s, a process tap); here both come out of the same audio
/// engine mix, so none is assumed until it's measured on a real machine.
const HANDOFF: f64 = 0.0;
/// The limiter keeps peaks under this, and lets its gain back up over about RELEASE (s).
const CEILING: f32 = 0.95;
const RELEASE: f64 = 0.05;

enum Mode {
    Stopped,
    /// to begin at `start` (a frame of the recording) at the moment `at`
    Waiting { at: Instant, start: usize },
    /// began at frame `start`, as the stream's frame `began`; `next` is the next frame to give it
    Playing { start: usize, began: u64, next: usize, stopping: bool },
}

struct Voice {
    x: Vec<f32>,
    rate: f64,
    mode: Mode,
    /// the limiter's pre-gain (linear) and its gain now
    pre: f32,
    env: f32,
    /// the fade's volume now, and the step towards 0 or 1 each frame
    vol: f32,
    step: f32,
    release: f32,
}

impl Voice {
    fn sample(&mut self, i: usize) -> f32 {
        let y = self.x.get(i).copied().unwrap_or(0.0) * self.pre;
        // recover first, then hold to the ceiling: recovering after the check let a sample through above it
        self.env += (1.0 - self.env) * self.release;
        if y.abs() * self.env > CEILING {
            self.env = CEILING / y.abs();
        }
        self.vol = (self.vol + self.step).clamp(0.0, 1.0);
        y * self.env * self.vol
    }
}

impl Fill for Voice {
    fn fill(&mut self, out: &mut [f32], first: u64, ahead: f64) {
        out.fill(0.0);
        let mut k = 0;
        if let Mode::Waiting { at, start } = self.mode {
            // silence until the moment it begins
            let wait = at.saturating_duration_since(Instant::now()).as_secs_f64() - ahead;
            let gap = (wait * self.rate).max(0.0).round() as usize;
            if gap >= out.len() {
                return;
            }
            k = gap;
            self.vol = 0.0;
            self.step = 1.0 / (FADE * self.rate) as f32;
            self.mode = Mode::Playing { start, began: first + gap as u64, next: start, stopping: false };
        }
        let Mode::Playing { next, stopping, .. } = self.mode else { return };
        let mut i = next;
        while k < out.len() {
            out[k] = self.sample(i);
            i += 1;
            k += 1;
        }
        if let Mode::Playing { next, .. } = &mut self.mode {
            *next = i;
        }
        if stopping && self.vol == 0.0 {
            self.mode = Mode::Stopped;
        }
    }
}

static VOICE: Mutex<Option<Stream<Voice>>> = Mutex::new(None);

/// Open a take's recording: its length (s), or -1 if it can't be opened (or there's no output to play it on).
pub fn open(path: &Path) -> f64 {
    let mut v = VOICE.lock().unwrap();
    *v = None;
    let kind = path.extension().map(|x| x.to_string_lossy().to_uppercase()).unwrap_or_default();
    let t = Instant::now();
    let d = match super::audiofile::decode(path, true) {
        Ok(d) => d,
        Err(e) => {
            crate::engine::log(&format!("voice playback: couldn't read the {kind} recording (error {e:#010x})"));
            return -1.0;
        }
    };
    if d.frames == 0 {
        crate::engine::log(&format!("voice playback: the {kind} recording read as empty"));
        return -1.0;
    }
    crate::engine::log(&format!("voice playback: read a {:.1} s {kind} recording ({:.0} Hz) in {:.2} s", d.frames as f64 / d.rate, d.rate,
                                t.elapsed().as_secs_f64()));
    let length = d.frames as f64 / d.rate;
    let voice = Voice { x: d.samples, rate: d.rate, mode: Mode::Stopped, pre: 1.0, env: 1.0, vol: 0.0, step: 0.0,
                        release: (1.0 - (-1.0 / (RELEASE * d.rate)).exp()) as f32 };
    match Stream::open(d.rate as u32, 1, voice) {
        Some(s) => {
            *v = Some(s);
            length
        }
        None => {
            crate::engine::log("voice playback: no output to play it on");
            -1.0
        }
    }
}

/// A play just started: where from (s into the recording) and when, to check once where it's heard 2 s later.
static CHECK: Mutex<Option<(f64, Instant)>> = Mutex::new(None);

/// Two seconds into a play: where it's heard against where it should be by the clock (logged; a tester's report
/// shows playback that drifts, starts late or doesn't start).
fn check_position(pos: f64) {
    let mut c = CHECK.lock().unwrap_or_else(|e| e.into_inner());
    let Some((from, at)) = *c else { return };
    let t = at.elapsed().as_secs_f64();
    if t < 2.0 {
        return;
    }
    *c = None;
    if pos < 0.0 {
        crate::engine::log(&format!("voice playback: {t:.1} s after playing from {from:.1} s nothing is heard yet"));
    } else {
        crate::engine::log(&format!("voice playback: {t:.1} s after playing from {from:.1} s it's at {pos:.2} s ({:+.3} s from the clock)", pos - (from + t)));
    }
}

/// Louder or quieter (dB, -40 to 40): the limiter's pre-gain, so it never clips.
pub fn gain(db: f64) {
    if let Some(s) = VOICE.lock().unwrap().as_ref() {
        s.with(|v| v.pre = 10f64.powf(db.clamp(-40.0, 40.0) / 20.0) as f32);
    }
}

/// Fade out what plays and wait until it has (up to a moment), then it's stopped.
fn fade_out(s: &Stream<Voice>) {
    let fading = s.with(|v| match &mut v.mode {
        Mode::Playing { stopping, .. } => {
            *stopping = true;
            v.step = -1.0 / (FADE * v.rate) as f32;
            true
        }
        m => {
            *m = Mode::Stopped;
            false
        }
    });
    let until = Instant::now() + Duration::from_secs_f64(FADE * 4.0);
    while fading && Instant::now() < until && !s.with(|v| matches!(v.mode, Mode::Stopped)) {
        std::thread::sleep(Duration::from_millis(5));
    }
    s.with(|v| v.mode = Mode::Stopped);
}

/// Play from `seconds` into the recording, as heard now (a move if it plays already: a short fade out, then in).
/// It begins exactly LEAD from now, that much further on, and HANDOFF further still.
pub fn play(seconds: f64) {
    let v = VOICE.lock().unwrap();
    let Some(s) = v.as_ref() else { return };
    let at = Instant::now() + Duration::from_secs_f64(LEAD);
    fade_out(s);
    s.with(|v| {
        let start = ((seconds + LEAD + HANDOFF).max(0.0) * v.rate) as usize;
        if start < v.x.len() {
            v.mode = Mode::Waiting { at, start };
        }
    });
    *CHECK.lock().unwrap_or_else(|e| e.into_inner()) = Some((seconds, at - Duration::from_secs_f64(LEAD)));
}

/// Stop (a short fade out).
pub fn stop() {
    *CHECK.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if let Some(s) = VOICE.lock().unwrap().as_ref() {
        fade_out(s);
    }
}

/// Let the recording go.
pub fn close() {
    *CHECK.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *VOICE.lock().unwrap() = None;
}

/// Where in the recording the sound heard now is (s, less HANDOFF), or -1 when it isn't playing (stopped, not begun
/// yet, or past its end).
pub fn position() -> f64 {
    let pos = heard();
    check_position(pos);
    pos
}

fn heard() -> f64 {
    let v = VOICE.lock().unwrap();
    let Some(s) = v.as_ref() else { return -1.0 };
    let Some(played) = s.played() else { return -1.0 };
    s.with(|v| match v.mode {
        Mode::Playing { start, began, .. } if played >= began as f64 => {
            let pos = (start as f64 + played - began as f64) / v.rate;
            if pos < v.x.len() as f64 / v.rate { pos - HANDOFF } else { -1.0 }
        }
        _ => -1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limiter_brings_the_voice_up_without_clipping() {
        let rate = 48_000.0;
        let x: Vec<f32> = (0..48_000).map(|i| 0.3 * (i as f32 * 0.03).sin()).collect();
        let mut v = Voice { x, rate, mode: Mode::Waiting { at: Instant::now(), start: 0 }, pre: 10f32.powf(20.0 / 20.0), env: 1.0,
                            vol: 0.0, step: 0.0, release: (1.0 - (-1.0 / (RELEASE * rate)).exp()) as f32 };
        let mut out = vec![0f32; 48_000];
        v.fill(&mut out, 0, 0.0);
        assert!(out.iter().all(|y| y.abs() <= CEILING + 1e-6), "clipped");
        let late = out[24_000..].iter().fold(0f32, |m, y| m.max(y.abs()));
        assert!(late > 0.8, "brought up: {late}");
    }

    /// With an output to play to (CI machines often have none: then there's nothing to check), the position moves
    /// with the wall clock once it's begun, and stops when stopped.
    #[test]
    fn the_position_follows_the_wall_clock() {
        if !super::super::render::has_output() {
            eprintln!("no audio output: skipped");
            return;
        }
        let d = std::env::temp_dir().join(format!("onkey-voice-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("mic.wav");
        let pcm: Vec<i16> = (0..48_000 * 4).map(|i| ((i as f64 * 0.04).sin() * 3000.0) as i16).collect();
        crate::engine::write_wav(&p, &pcm, 48_000).unwrap();
        let len = open(&p);
        if len < 0.0 {
            eprintln!("the output wouldn't open: skipped");
            return;
        }
        assert!((len - 4.0).abs() < 0.05, "{len}");
        gain(-30.0);        // (quiet: a CI machine with speakers)
        play(1.0);
        std::thread::sleep(Duration::from_millis(400));
        let (a, t0) = (position(), Instant::now());
        std::thread::sleep(Duration::from_millis(1000));
        let (b, t1) = (position(), Instant::now());
        assert!(a > 1.0 && a < 1.8, "begun from 1 s: {a}");
        let moved = b - a;
        let wall = (t1 - t0).as_secs_f64();
        assert!((moved - wall).abs() < 0.05, "moved {moved:.3} s in {wall:.3} s");
        stop();
        assert_eq!(position(), -1.0);
        close();
        let _ = std::fs::remove_dir_all(&d);
    }
}
