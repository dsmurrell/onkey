//! The demo song's player (src/demo.rs), native/demo.swift's counterpart: the song decoded whole (mf.rs) and played
//! through a WASAPI stream (render.rs) in OnKey's own process, so the capture hears it as it hears a music app:
//! through per-process loopback of OnKey (audio.rs, given OnKey's own id: see procs.rs `own`).
use std::path::Path;
use std::sync::Mutex;

use super::render::{Fill, Stream};

struct Song {
    /// the song, interleaved, `ch` channels at `rate`
    x: Vec<i16>,
    ch: usize,
    rate: f64,
    frames: usize,
    playing: bool,
    /// the next frame to give the stream (where it carries on from when paused)
    next: usize,
    /// (the stream's frame, the song's frame) where the current stretch began; None until it's given
    anchor: Option<(u64, usize)>,
    /// played to its end by itself (until played or seeked again)
    ended: bool,
}

impl Fill for Song {
    fn fill(&mut self, out: &mut [f32], first: u64, _ahead: f64) {
        out.fill(0.0);
        if !self.playing {
            return;
        }
        if self.anchor.is_none() {
            self.anchor = Some((first, self.next));
        }
        let n = (out.len() / self.ch).min(self.frames.saturating_sub(self.next));
        let from = self.next * self.ch;
        for (o, s) in out[..n * self.ch].iter_mut().zip(&self.x[from..from + n * self.ch]) {
            *o = *s as f32 / 32768.0;
        }
        self.next += n;
    }
}

impl Song {
    /// The frame heard now, from the stream's clock (`played` frames).
    fn heard(&self, played: Option<f64>) -> usize {
        match (self.playing, self.anchor, played) {
            (true, Some((w, f)), Some(p)) => (f + (p - w as f64).max(0.0) as usize).min(self.next),
            _ => self.next,
        }
    }
}

static DEMO: Mutex<Option<Stream<Song>>> = Mutex::new(None);

/// Open the demo's audio file (anything Media Foundation reads), ready to play from the start: its length (s), or -1
/// if it can't be opened (or there's no output).
pub fn open(path: &Path) -> f64 {
    let mut g = DEMO.lock().unwrap();
    *g = None;
    let Ok(d) = super::mf::decode(path, false) else { return -1.0 };
    if d.frames == 0 {
        return -1.0;
    }
    let x: Vec<i16> = d.samples.iter().map(|v| (v * 32768.0).round().clamp(-32768.0, 32767.0) as i16).collect();
    let song = Song { x, ch: d.channels as usize, rate: d.rate, frames: d.frames, playing: false, next: 0, anchor: None, ended: false };
    match Stream::open(d.rate as u32, d.channels as u16, song) {
        Some(s) => {
            *g = Some(s);
            d.frames as f64 / d.rate
        }
        None => -1.0,
    }
}

fn with(f: impl FnOnce(&mut Song)) {
    if let Some(s) = DEMO.lock().unwrap().as_ref() {
        s.with(f);
    }
}

pub fn play() {
    with(|s| {
        if s.ended || s.next >= s.frames {
            s.next = 0;     // (as AVAudioPlayer: played again from the start once it's ended)
        }
        s.ended = false;
        s.playing = true;
        s.anchor = None;
    });
}

/// (What's queued, ~0.1 s, still plays: it's carried on from after it.)
pub fn pause() {
    with(|s| {
        s.playing = false;
        s.anchor = None;
    });
}

/// Go to `seconds` into the song (playing on if it was).
pub fn seek(seconds: f64) {
    with(|s| {
        s.next = ((seconds.max(0.0) * s.rate) as usize).min(s.frames);
        s.anchor = None;
    });
}

/// Stop and let the file go.
pub fn close() {
    *DEMO.lock().unwrap() = None;
}

/// Where the player is (s) and whether it plays: 1 playing, 0 paused, 2 came to its end by itself (then the position
/// is the song's length), -1 nothing open.
pub fn state(position: &mut f64) -> i32 {
    let g = DEMO.lock().unwrap();
    let Some(stream) = g.as_ref() else {
        *position = 0.0;
        return -1;
    };
    let played = stream.played();
    stream.with(|s| {
        let heard = s.heard(played);
        if s.playing && s.next >= s.frames && heard >= s.frames {
            s.playing = false;
            s.ended = true;
        }
        if s.ended {
            *position = s.frames as f64 / s.rate;
            return 2;
        }
        *position = heard as f64 / s.rate;
        s.playing as i32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_song_is_given_from_where_it_is_and_silence_when_paused() {
        let mut s = Song { x: (0..2000).map(|i| i as i16).collect(), ch: 2, rate: 1000.0, frames: 1000, playing: true, next: 990,
                           anchor: None, ended: false };
        let mut out = vec![1f32; 40];
        s.fill(&mut out, 500, 0.0);
        assert_eq!(s.anchor, Some((500, 990)));
        assert_eq!(out[0], 1980.0 / 32768.0);
        assert!(out[20..].iter().all(|v| *v == 0.0), "silence after the end");
        assert_eq!(s.next, 1000);
        assert_eq!(s.heard(Some(505.0)), 995);
        assert_eq!(s.heard(Some(400.0)), 990, "not reached yet");
        s.playing = false;
        out.fill(1.0);
        s.fill(&mut out, 520, 0.0);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    /// With an output (CI machines often have none), a short file plays to its end and says so.
    #[test]
    fn a_song_plays_to_its_end() {
        if !super::super::render::has_output() {
            eprintln!("no audio output: skipped");
            return;
        }
        let d = std::env::temp_dir().join(format!("onkey-demo-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("demo.m4a");
        super::super::mf::aac_write(&vec![0i16; 48_000], 48_000.0, 64_000, &p).unwrap();
        let len = open(&p);
        if len < 0.0 {
            eprintln!("the output wouldn't open: skipped");
            return;
        }
        let mut pos = 0.0;
        assert_eq!(state(&mut pos), 0);
        play();
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(state(&mut pos), 1);
        assert!(pos > 0.2 && pos < 0.7, "{pos}");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        assert_eq!(state(&mut pos), 2);
        assert!((pos - len).abs() < 1e-9);
        close();
        assert_eq!(state(&mut pos), -1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
