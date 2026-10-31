//! Audio in: the music app's output and your mic, sample-aligned, from the Swift capture library
//! (native/capture.swift). Frames arrive on Core Audio's real-time thread and go into a lock-free ring buffer,
//! so that thread never waits on us.
use std::ffi::{c_char, c_void, CString};
use std::sync::Mutex;

use rtrb::{Consumer, Producer, RingBuffer};

use crate::platform::native::*;

/// Interleaved frames: [music L, music R, mic].
pub const CHANNELS: usize = 3;

static PRODUCER: Mutex<Option<Producer<f32>>> = Mutex::new(None);

extern "C" fn on_audio(data: *const f32, frames: i32, _ctx: *mut c_void) {
    let n = frames.max(0) as usize * CHANNELS;
    let samples = unsafe { std::slice::from_raw_parts(data, n) };
    // try_lock: the real-time thread must never block. The lock is only ever taken here and in start/stop.
    if let Ok(mut guard) = PRODUCER.try_lock() {
        if let Some(p) = guard.as_mut() {
            if let Ok(chunk) = p.write_chunk_uninit(n) {
                chunk.fill_from_iter(samples.iter().copied());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CaptureError {
    /// The music app isn't running or hasn't played anything yet.
    AppSilent,
    /// macOS refused the audio tap (permission).
    TapRefused,
    /// Couldn't set up or start the audio device.
    DeviceFailed,
}

pub struct Capture {
    pub rate: f64,
    /// The microphone captured ("" for both when there's no input device: the mic channel is silent).
    pub mic_name: String,
    pub mic_uid: String,
    consumer: Consumer<f32>,
    /// development builds: the simulated player's feed (devtest.rs), told to stop when this is dropped; no tap
    sim: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

/// A microphone: Core Audio's uid for it, and its name ("MacBook Pro Microphone").
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Mic {
    pub uid: String,
    pub name: String,
    /// installed by another app (BlackHole, Krisp, Zoom): Settings lists these apart
    #[serde(rename = "virtual")]
    pub is_virtual: bool,
}

fn c_str(buf: &[c_char]) -> String {
    unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
}

/// The microphone OnKey listens to now, the take engine and setup's mic check alike (native/capture.swift
/// `chooseMic`): the one chosen in Settings while it's connected, else the Mac's default input (not Bluetooth
/// headphones' while the Mac has another mic). None: no input device at all.
/// (`is_virtual` isn't looked up here, as the engine reads this every round: `mics` has it.)
pub fn chosen_mic() -> Option<Mic> {
    let (mut uid, mut name) = ([0 as c_char; 512], [0 as c_char; 512]);
    let found = unsafe { onkey_mic_choice(uid.as_mut_ptr(), name.as_mut_ptr(), 512) } == 1;
    found.then(|| Mic { uid: c_str(&uid), name: c_str(&name), is_virtual: false })
}

/// Every microphone connected.
pub fn mics() -> Vec<Mic> {
    let mut buf = vec![0 as c_char; 64 * 1024];
    unsafe { onkey_mic_list(buf.as_mut_ptr(), buf.len() as i32) };
    let list: Vec<serde_json::Value> = serde_json::from_str(&c_str(&buf)).unwrap_or_default();
    list.iter().filter_map(|m| Some(Mic { uid: m["uid"].as_str()?.to_string(), name: m["name"].as_str()?.to_string(),
                                       is_virtual: m["virtual"].as_bool().unwrap_or(false) })).collect()
}

/// Prefer this microphone (a uid from `mics`) while it's connected; "" for automatic.
pub fn set_mic_preference(uid: &str) {
    if let Ok(u) = CString::new(uid) {
        unsafe { onkey_set_mic_preference(u.as_ptr()) };
    }
}

impl Capture {
    /// Start capturing the app whose bundle id starts with `bundle` (e.g. "com.spotify.client") and the
    /// microphone `chosen_mic` picks.
    pub fn start(bundle: &str) -> Result<Capture, CaptureError> {
        let (producer, consumer) = RingBuffer::new(48_000 * CHANNELS * 20);   // 20 s of slack
        if crate::devtest::sim_player_on() {
            // the simulated player's sound instead of a tap (its mic channel is silent: ONKEY_SIM sings into it)
            let (rate, stop) = crate::devtest::sim_capture(producer);
            let mic = chosen_mic().unwrap_or(Mic { uid: String::new(), name: String::new(), is_virtual: false });
            return Ok(Capture { rate, mic_name: mic.name, mic_uid: mic.uid, consumer, sim: Some(stop) });
        }
        *PRODUCER.lock().unwrap() = Some(producer);
        let b = CString::new(bundle).unwrap();
        let mut rate = 0.0;
        let code = unsafe { onkey_capture_start(b.as_ptr(), on_audio, std::ptr::null_mut(), &mut rate) };
        match code {
            0 => {
                let (mut uid, mut name) = ([0 as c_char; 512], [0 as c_char; 512]);
                unsafe { onkey_capture_mic(uid.as_mut_ptr(), name.as_mut_ptr(), 512) };
                let c = Capture { rate, mic_name: c_str(&name), mic_uid: c_str(&uid), consumer, sim: None };
                // (a rate Core Audio didn't give, 0: the resampler would never move on and grow until memory ran out)
                if !(rate.is_finite() && rate >= 8000.0) {
                    drop(c);     // (stops it)
                    return Err(CaptureError::DeviceFailed);
                }
                Ok(c)
            }
            1 => Err(CaptureError::AppSilent),
            2 => Err(CaptureError::TapRefused),
            _ => Err(CaptureError::DeviceFailed),
        }
    }

    /// Move everything captured so far onto the end of `out` (interleaved, CHANNELS per frame).
    pub fn drain_into(&mut self, out: &mut Vec<f32>) {
        let n = self.consumer.slots();
        if n == 0 {
            return;
        }
        if let Ok(chunk) = self.consumer.read_chunk(n - n % CHANNELS) {
            let (a, b) = chunk.as_slices();
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            chunk.commit_all();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(stop) = &self.sim {
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
            return;
        }
        unsafe { onkey_capture_stop() };
        *PRODUCER.lock().unwrap() = None;
    }
}

/// The default output device's uid ("" if unknown).
pub fn output_uid() -> String {
    let mut buf = [0 as std::ffi::c_char; 256];
    unsafe { onkey_output_uid(buf.as_mut_ptr(), buf.len() as i32) };
    unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
}

/// The default output device's name ("AirPods Pro", "MacBook Pro Speakers"; "" if unknown).
pub fn output_name() -> String {
    let mut buf = [0 as std::ffi::c_char; 256];
    unsafe { onkey_output_name(buf.as_mut_ptr(), buf.len() as i32) };
    unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
}

/// How long after an app plays audio you hear it (headphones ~215 ms over Bluetooth, speakers ~30 ms), as Core
/// Audio reports it. What OnKey allows for is `timing::heard_ms`: this, corrected by your tap-along check of the
/// device when you've done one (often wrong over Bluetooth).
pub fn output_latency_ms() -> i32 {
    unsafe { onkey_output_latency_ms() }
}

/// An interface with several inputs: which one OnKey is taking your voice from (native/capture.swift picks the one
/// your voice is on), logged when it changes so a report shows it. Called about once a second.
pub fn note_mic_channel() {
    static LAST: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);
    let mut count = 0i32;
    let ch = unsafe { onkey_mic_channel(&mut count) };
    if count < 2 {
        return;
    }
    let now = ((count as i64) << 32) | ch as i64;
    if LAST.swap(now, std::sync::atomic::Ordering::Relaxed) != now {
        crate::engine::log(&format!("mic: using input {} of {count}", ch + 1));
    }
}

/// Whether an app (bundle id prefix) is playing any sound right now.
pub fn app_sounding(bundle: &str) -> bool {
    let b = CString::new(bundle).unwrap();
    unsafe { onkey_app_sounding(b.as_ptr()) == 1 }
}

/// Whether WebKit's shared sound process (Safari's, Mail's and OnKey's own window's) is told apart by the app it
/// plays for, so that capturing "Safari" never hears OnKey (native/capture.swift `belongs`).
pub fn webkit_separable() -> bool {
    unsafe { onkey_webkit_separable() == 1 }
}

/// How many times the Mac has woken from sleep since this was first called (native/capture.swift).
pub fn wake_count() -> i32 {
    unsafe { onkey_wake_count() }
}

/// When the capture should be restarted because the tap has gone dead: after sleep and wake, or a Core Audio
/// reset, a process tap can stop delivering the app's sound for good. A restart loses the audio while it
/// happens (the take's clock is its audio), so it's kept for a tap that's really dead, never for a quiet intro:
/// - the Mac woke from sleep;
/// - the followed app says it's playing (past its first 5 s) and the tap has delivered digital silence (exact
///   zeros, or nothing) for 5 s; twice at most while it stays silent. Then, if Core Audio says the app isn't sending
///   any sound on this Mac (Spotify Connect playing on another device, the Mac asleep), not again until it is, or
///   the sound comes back (`elsewhere` says so meanwhile: live, 17 Oct, 28 restarts 15 s apart, none of them heard
///   anything); if it is, further apart each time, as for the next case;
/// - nothing says what plays (nothing recognised yet), Core Audio says the app is sending sound, and the tap has
///   delivered nothing at all (not a single non-zero sample, which even a whisper-quiet intro has) for 10 s.
///
/// Never within 10 s of the capture starting; restarts for the last case get further apart while they don't help.
pub struct Watchdog {
    quiet_since: Option<std::time::Instant>,
    dead_since: Option<std::time::Instant>,
    last_restart: std::time::Instant,
    /// the least time to the next restart for a dead tap: doubles while restarts don't bring the sound back
    gap_s: u64,
    sounding: bool,
    last_asked: std::time::Instant,
    wakes: i32,
    /// restarts for a silent tap while the app says it plays, since sound was last heard
    quiet_tries: u32,
    /// after those: the least time to the next one (doubles), and whether the app sends sound on this Mac at all
    quiet_gap_s: u64,
    here: bool,
    here_asked: std::time::Instant,
}

impl Default for Watchdog {
    fn default() -> Self {
        // (checked: on Windows an Instant can't go back before boot, and subtracting panicked within 10 minutes of it)
        let now = std::time::Instant::now();
        let long_ago = now.checked_sub(std::time::Duration::from_secs(600)).unwrap_or(now);
        Watchdog { quiet_since: None, dead_since: None, last_restart: long_ago, gap_s: Self::MIN_GAP_S, sounding: false,
                   last_asked: long_ago, wakes: wake_count(), quiet_tries: 0, quiet_gap_s: Self::QUIET_GAP_S, here: true, here_asked: long_ago }
    }
}

impl Watchdog {
    /// Seconds of silence from the tap while the app says it plays before it's restarted.
    pub const QUIET_S: u64 = 5;
    /// Seconds of nothing at all from the tap while Core Audio says the app sounds (nothing recognised yet).
    pub const DEAD_S: u64 = 10;
    const MIN_GAP_S: u64 = 10;
    const MAX_GAP_S: u64 = 300;
    /// restarts for a silent tap before it's judged by whether the app sends sound here at all
    const QUIET_TRIES: u32 = 2;
    const QUIET_GAP_S: u64 = 30;

    /// The capture (re)started: nothing is judged for MIN_GAP_S.
    pub fn started(&mut self) {
        self.started_at(std::time::Instant::now());
    }

    fn started_at(&mut self, now: std::time::Instant) {
        self.last_restart = now;
        self.quiet_since = None;
        self.dead_since = None;
    }

    /// Called with each block's `any_sound` (the block as the tap delivered it: in listening mode the engine then
    /// empties a block that isn't the song being sung, and judging that one, live on a fresh install, restarted a
    /// healthy tap every 20 s until Shazam named the song), and whether the followed app says it's playing (past
    /// its first 5 s). Why to restart the capture now, if it should (see `Watchdog`).
    pub fn check(&mut self, anything: bool, playing: bool, bundle: &str) -> Option<&'static str> {
        let r = self.check_at(anything, playing, std::time::Instant::now(), wake_count(), &mut || app_sounding(bundle));
        if !self.here {
            set_elsewhere(Some(bundle));
        }
        r
    }

    /// `check` at `now`, with the Mac's wake count and Core Audio's word on whether the app sends sound (`sounds`).
    fn check_at(&mut self, anything: bool, playing: bool, now: std::time::Instant, wakes: i32, sounds: &mut dyn FnMut() -> bool) -> Option<&'static str> {
        use std::time::Duration;
        let since = |t: std::time::Instant| now.saturating_duration_since(t);
        if wakes != self.wakes {
            self.wakes = wakes;
            self.started_at(now);
            self.heard();
            return Some("the Mac woke from sleep");
        }
        if anything {
            self.dead_since = None;
            self.gap_s = Self::MIN_GAP_S;
            self.heard();
        }
        // given up on a silent tap, the app playing elsewhere: until it sends sound here again
        if !self.here && since(self.here_asked) >= Duration::from_secs(1) {
            self.here_asked = now;
            if sounds() {
                self.heard();
            }
        }
        if anything || !playing {
            self.quiet_since = None;
        }
        if playing || anything {
            self.sounding = false;
            self.dead_since = None;
        } else if since(self.last_asked) >= Duration::from_secs(1) {
            self.last_asked = now;
            self.sounding = sounds();
            if !self.sounding {
                self.dead_since = None;       // a pause: the tap is meant to be silent
            }
        }
        if since(self.last_restart) < Duration::from_secs(Self::MIN_GAP_S) {
            return None;
        }
        if playing && !anything && since(*self.quiet_since.get_or_insert(now)) > Duration::from_secs(Self::QUIET_S) {
            if self.quiet_tries < Self::QUIET_TRIES {
                self.quiet_tries += 1;
                self.started_at(now);
                return Some("capture silent while the music plays");
            }
            // restarts haven't helped: does the app send any sound on this Mac?
            if self.here && since(self.here_asked) >= Duration::from_secs(1) {
                self.here_asked = now;
                if !sounds() {
                    self.here = false;
                    return None;
                }
            }
            if self.here && since(self.last_restart) >= Duration::from_secs(self.quiet_gap_s) {
                self.started_at(now);
                self.quiet_gap_s = (self.quiet_gap_s * 2).min(Self::MAX_GAP_S);
                return Some("capture still silent while the music plays");
            }
        }
        if self.sounding && since(*self.dead_since.get_or_insert(now)) > Duration::from_secs(Self::DEAD_S)
            && since(self.last_restart) >= Duration::from_secs(self.gap_s) {
            self.started_at(now);
            self.gap_s = (self.gap_s * 2).min(Self::MAX_GAP_S);
            return Some("capture delivering nothing while the app plays sound");
        }
        None
    }
}

impl Watchdog {
    /// Sound heard (or the Mac woke, or the app sends sound here again): a silent tap is restarted promptly again.
    fn heard(&mut self) {
        self.quiet_tries = 0;
        self.quiet_gap_s = Self::QUIET_GAP_S;
        if !self.here {
            self.here = true;
            set_elsewhere(None);
        }
    }

    /// The app says it plays, but sends no sound on this Mac (see `Watchdog`).
    pub fn elsewhere(&self) -> bool {
        !self.here
    }
}

/// The app (bundle id prefix) whose music plays, but not on this Mac (the watchdog gave up restarting a silent tap
/// for it): nothing is followed from it, and the status line says why.
static ELSEWHERE: Mutex<Option<String>> = Mutex::new(None);

fn set_elsewhere(bundle: Option<&str>) {
    *ELSEWHERE.lock().unwrap() = bundle.map(str::to_string);
}

/// Whether `bundle`'s music plays, but not on this Mac (see `Watchdog`).
pub fn elsewhere(bundle: &str) -> bool {
    ELSEWHERE.lock().unwrap().as_deref() == Some(bundle)
}

/// Whether a block of interleaved [L, R, mic] frames has any music in it at all, however faint: music with the
/// app's own volume turned right down is -60 to -100 dBFS, quiet but alive; a dead tap gives exact zeros (or nothing).
pub fn any_sound(block: &[f32]) -> bool {
    block.chunks_exact(CHANNELS).any(|f| f[0] != 0.0 || f[1] != 0.0)
}

/// The captured music's level. A process tap hears an app after its own volume: Spotify at 20% captures at -60 to
/// -100 dBFS against -20 to -35 at 100%. That faint, what the mic hears can't be told apart from the song (the
/// echo canceller has next to nothing to cancel with), so the take isn't scored and the window says to turn the
/// app up. Judged over a few seconds of sound only (a pause's digital silence leaves it as it was). Measured,
/// never changed: separation and GAME work at any level since 729e6b3.
pub struct MusicLevel {
    /// mean square of the music, smoothed over ~TAU_S of sound
    ms: f64,
    heard_s: f64,
}

impl Default for MusicLevel {
    fn default() -> Self {
        MusicLevel { ms: 0.0, heard_s: 0.0 }
    }
}

impl MusicLevel {
    /// Below this (smoothed) the music is too quiet to trust.
    pub const QUIET_DB: f32 = -55.0;
    const TAU_S: f64 = 4.0;

    /// Measure a block of interleaved [L, R, mic] frames at `rate`.
    pub fn measure(&mut self, block: &[f32], rate: f64) {
        let frames = block.len() / CHANNELS;
        if frames == 0 {
            return;
        }
        let ms: f64 = block.chunks_exact(CHANNELS).map(|f| 0.5 * ((f[0] * f[0] + f[1] * f[1]) as f64)).sum::<f64>() / frames as f64;
        if ms > 0.0 {
            let dt = frames as f64 / rate;
            let a = if self.heard_s == 0.0 { 1.0 } else { 1.0 - (-dt / Self::TAU_S).exp() };
            self.ms += a * (ms - self.ms);
            self.heard_s += dt;
        }
    }

    /// The music's level (dBFS, smoothed); -120 before any sound.
    pub fn level_db(&self) -> f32 {
        if self.heard_s == 0.0 { -120.0 } else { (10.0 * (self.ms + 1e-15).log10()) as f32 }
    }

    /// The music has been too quiet to trust (after a couple of seconds of it, so a song's first notes don't
    /// count).
    pub fn quiet(&self) -> bool {
        self.heard_s > 2.0 && self.level_db() < Self::QUIET_DB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed the watchdog `secs` of blocks; the times (s) it said to restart.
    fn run(w: &mut Watchdog, secs: f64, block: &[f32], playing: bool) -> Vec<f64> {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut restarts = Vec::new();
        while t0.elapsed() < Duration::from_secs_f64(secs) {
            if w.check(any_sound(block), playing, "org.example.no-such-app").is_some() {
                restarts.push(t0.elapsed().as_secs_f64());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        restarts
    }

    #[test]
    fn the_watchdog_restarts_only_a_tap_that_is_really_silent() {
        let zeros = vec![0.0f32; 3 * 480];
        let whisper: Vec<f32> = (0..3 * 480).map(|i| if i % 3 == 2 { 0.0 } else { 3e-6 }).collect();   // -110 dBFS: Spotify at 20%, a quiet intro
        let mut w = Watchdog::default();
        w.started();
        // just started: nothing judged, even silent while playing
        assert!(run(&mut w, 1.0, &zeros, true).is_empty());
        let mut w = Watchdog::default();
        // paused, the app making no sound: never
        assert!(run(&mut w, 5.6, &zeros, false).is_empty());
        // a whisper-quiet stretch: never, whether the app says it plays or not
        assert!(run(&mut w, 1.0, &whisper, false).is_empty());
        assert!(run(&mut w, 6.0, &whisper, true).is_empty());
        // the app says it plays but the tap is silent: restarted after QUIET_S, once
        let r = run(&mut w, 6.5, &zeros, true);
        assert_eq!(r.len(), 1, "{r:?}");
        assert!(r[0] > Watchdog::QUIET_S as f64);
        // then nothing for MIN_GAP_S after that restart
        assert!(run(&mut w, 1.0, &zeros, true).is_empty());
    }

    /// Drive the watchdog over `secs` of fake time from `t0`, a check every 50 ms; the times (s from `t0`) it said
    /// to restart. `here`: Core Audio's word on whether the app sends sound on this Mac.
    fn run_at(w: &mut Watchdog, t0: std::time::Instant, from: f64, secs: f64, anything: bool, playing: bool, here: bool) -> Vec<f64> {
        let mut out = Vec::new();
        let mut t = from;
        while t < from + secs {
            let now = t0 + std::time::Duration::from_secs_f64(t);
            if w.check_at(anything, playing, now, w.wakes, &mut || here).is_some() {
                out.push(t);
            }
            t += 0.05;
        }
        out
    }

    #[test]
    fn a_silent_tap_is_restarted_twice_then_left_while_the_app_plays_elsewhere() {
        // Spotify says it plays, but plays on another device (Spotify Connect): live, 28 restarts 15 s apart
        let t0 = std::time::Instant::now();
        let mut w = Watchdog::default();
        w.started_at(t0);
        let r = run_at(&mut w, t0, 0.0, 300.0, false, true, false);
        assert_eq!(r.len(), 2, "{r:?}");
        assert!(w.elsewhere());
        // it plays here again (Core Audio says so): restarted promptly again if the tap stays silent
        let r = run_at(&mut w, t0, 300.0, 20.0, false, true, true);
        assert!(!w.elsewhere());
        assert!(!r.is_empty() && r[0] - 300.0 < 1.0, "{r:?}");
        // sound heard: all is well, and nothing is restarted
        assert!(run_at(&mut w, t0, 320.0, 60.0, true, true, true).is_empty());
    }

    #[test]
    fn a_silent_tap_while_the_app_sounds_here_is_restarted_further_apart() {
        let t0 = std::time::Instant::now();
        let mut w = Watchdog::default();
        w.started_at(t0);
        let r = run_at(&mut w, t0, 0.0, 600.0, false, true, true);
        assert!(!w.elsewhere());
        // twice 15 s apart, then 30 s, 60 s, 120 s, 240 s (and more) apart
        let gaps: Vec<f64> = r.windows(2).map(|p| p[1] - p[0]).collect();
        assert!(r.len() >= 5 && r.len() <= 7, "{r:?}");
        assert!(gaps.windows(2).skip(1).all(|g| g[1] > g[0] * 1.5), "{gaps:?}");
    }

    #[test]
    fn music_too_quiet_to_score_is_told_from_normal_music() {
        let rate = 48_000.0;
        let tone = |amp: f32| -> Vec<f32> { (0..4800).flat_map(|i| { let v = amp * ((i as f32) * 0.05).sin(); [v, v, 0.01] }).collect() };
        let level_after = |amp: f32, blocks: usize| { let mut l = MusicLevel::default(); let b = tone(amp); for _ in 0..blocks { l.measure(&b, rate); } l };
        // Spotify at 100%: about -23 dBFS
        let l = level_after(0.1, 100);
        assert!((l.level_db() - -23.0).abs() < 1.0 && !l.quiet());
        // Spotify at 20%: about -63 dBFS, too quiet; but not in the first 2 s
        assert!(level_after(0.001, 100).quiet());
        assert!(!level_after(0.001, 15).quiet());
        // a pause (digital silence) changes nothing
        let mut l = level_after(0.001, 100);
        let lv = l.level_db();
        for _ in 0..50 { l.measure(&vec![0.0f32; 4800 * 3], rate); }
        assert_eq!(l.level_db(), lv);
    }
}
