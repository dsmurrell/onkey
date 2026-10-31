//! Development-only test hooks. They exist only in builds with the `devtest` feature (release builds don't have
//! them at all), and even then only act when these environment variables are set:
//!   ONKEY_EVENTS_LOG=<file>   every event sent to the window is also appended to this file (one JSON per
//!                             line), so a session can be replayed in a browser with the same UI (?replay=)
//!   ONKEY_SIM=<dir>           replace the microphone with a simulated singer: <dir> holds the song as
//!                             mix.f32 (48 kHz stereo) and the singer as mic_<kind>.f32 (48 kHz mono, already
//!                             delayed by the headphones' latency), lined up with the mix
//!   ONKEY_SIM_KIND=<kind>     which singer (default "perfect")
//!   ONKEY_MIRROR=1            the live mirror: the window, live, in a browser tab (mirror.rs)
//! (OnKey's developer also has a hook that keeps test audio for offline tests. It isn't published: its file,
//! dev/devkeep.rs, isn't in this repository, so a build from this source, development or release, can't save the
//! music or a separated vocal. See keep_audio below.)
//!   ONKEY_SIM_PLAYER=<dir>    a stand-in for the music app, for measuring the whole app (memory, CPU) with no
//!                             Spotify, no audio tap and no permission asked: <dir>'s song (mix.f32, 48 kHz
//!                             stereo; meta.json's track) "plays" from ONKEY_SIM_PLAYER_START seconds after
//!                             launch (default 20), its sound captured as if from the app, and pauses at its
//!                             end (the take ends a minute later, as after any pause). With ONKEY_SIM=<dir> too,
//!                             the simulated singer sings along. Set the settings' "listening" off: the player
//!                             stands in for the apps asked, not for sound recognised.
//!   ONKEY_SIM_LISTEN=1        with ONKEY_SIM_PLAYER in listening mode: the simulated player is the app Core Audio
//!                             says is sounding, and (with control of Spotify allowed in the settings) answers as
//!                             Spotify does, so listening's app-first path is measured too; ONKEY_SIM_OSA=1 also
//!                             runs a trivial osascript for each AppleScript call the real app would make (its
//!                             process cost), ONKEY_SIM_APP_LEAD=<s> has the player say a position that far ahead
//!                             of its sound (Spotify's lead)
//!   ONKEY_PERF=1              log the live pitch's latency and cadence every 10 s ("perf: ..."), for the sing
//!                             benchmark (a timed sing-along run in a development build)
//! The simulated singer finds where in the song Spotify is by matching 8 s of what it hears against the
//! stored mix, then sings from that point on, exactly as a person singing along would.
/// An experiment's knob (ONKEY_LOCK_S, TRACKER_TRACE, ...): the environment variable's value in development builds.
/// Release builds don't read it and don't hold its name at all (scripts/check-release.sh fails if one is there), so
/// a knob left set on someone's Mac can't change the app. To turn one in a tool, build it with `--features devtest`.
macro_rules! knob {
    ($name:literal) => {{
        #[cfg(feature = "devtest")]
        let v = std::env::var($name).ok();
        #[cfg(not(feature = "devtest"))]
        let v: Option<String> = None;
        v
    }};
}
pub(crate) use knob;

/// Keeping a play's audio for offline tests: only in OnKey's private development checkout, whose build adds
/// dev/devkeep.rs (build.rs sets `keepaudio` when that file is present in a development build). Everywhere else,
/// including every build of the published source, these do nothing: OnKey never saves the music.
#[cfg(keepaudio)]
#[path = "../dev/devkeep.rs"]
mod devkeep;
#[cfg(keepaudio)]
pub use devkeep::{keep_audio, keep_mic, keep_music};
// and the tools that use what it keeps (lib.rs dev_hook)
#[cfg(keepaudio)]
#[path = "../dev/devhooks.rs"]
mod devhooks;
#[cfg(keepaudio)]
pub use devhooks::dev_hook;

#[cfg(not(keepaudio))]
mod nokeep {
    pub fn keep_music() -> bool { false }
    pub fn keep_mic(_track: &crate::source::Track, _song_offset: f64, _heard: i64, _mic16: &[f32], _clean16: &[f32], _ref16: &[f32]) {}
    pub fn keep_audio(_track: &crate::source::Track, _song_offset: f64, _mix16: &[f32], _vocal16: &[f32],
                      _lyrics_given: &[(f64, String)], _lyrics_aligned: &[(f64, String)], _music48: Option<(&[i16], u32)>) {}
    pub fn dev_hook(_name: &str, _args: &serde_json::Value) -> serde_json::Value { serde_json::Value::Null }
}
#[cfg(not(keepaudio))]
pub use nokeep::*;

#[cfg(feature = "devtest")]
pub use real::*;
#[cfg(not(feature = "devtest"))]
pub use stub::*;

#[cfg(feature = "devtest")]
mod real {
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::dsp::{locate, onset_envelope, FP_BANDS};
use crate::resample::Resampler;

static EVENTS: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// The simulated player (ONKEY_SIM_PLAYER): its song, and when it starts.
struct SimPlayer {
    track: crate::source::Track,
    starts: Instant,
    /// the song, 48 kHz stereo interleaved f32, read as it plays (held whole, it would add the song's 117 MB to
    /// the app being measured)
    mix: std::path::PathBuf,
    frames: usize,
}

static PLAYER: Mutex<Option<SimPlayer>> = Mutex::new(None);
/// The song's frame (48 kHz) the simulated player's capture began at: where the simulated singer is in the song
static SIM_CAPTURE_FROM: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);

pub fn sim_player_on() -> bool {
    std::env::var("ONKEY_SIM_PLAYER").is_ok()
}

/// The simulated player, loaded on first use. The guard holds None when its song couldn't be read (said in the log).
fn player() -> Option<std::sync::MutexGuard<'static, Option<SimPlayer>>> {
    static TRIED: AtomicBool = AtomicBool::new(false);
    let dir = std::path::PathBuf::from(std::env::var("ONKEY_SIM_PLAYER").ok()?);
    let mut g = PLAYER.lock().unwrap();
    if g.is_none() && !TRIED.swap(true, Ordering::SeqCst) {
        *g = load_player(&dir);
        if g.is_none() {
            crate::engine::log(&format!("simulated player: {} has no readable mix.f32 and meta.json; nothing plays", dir.display()));
        }
    }
    Some(g)
}

fn load_player(dir: &std::path::Path) -> Option<SimPlayer> {
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("meta.json")).ok()?).ok()?;
    let track: crate::source::Track = serde_json::from_value(meta["track"].clone()).ok()?;
    let mix = dir.join("mix.f32");
    let frames = std::fs::metadata(&mix).ok()?.len() as usize / 8;
    let wait = std::env::var("ONKEY_SIM_PLAYER_START").ok().and_then(|v| v.parse().ok()).unwrap_or(20.0);
    crate::engine::log(&format!("simulated player: {} by {} ({:.0} s) plays in {wait:.0} s", track.name, track.artist, frames as f64 / 48_000.0));
    Some(SimPlayer { track, starts: Instant::now() + Duration::from_secs_f64(wait), mix, frames })
}

/// What the simulated player is doing, as `source::poll` reports a music app (Some(None): nothing to play yet);
/// None without ONKEY_SIM_PLAYER.
pub fn sim_player() -> Option<Option<crate::source::State>> {
    let g = player()?;
    let Some(p) = g.as_ref() else { return Some(None) };
    let now = Instant::now();
    if now < p.starts {
        return Some(None);
    }
    let t = (now - p.starts).as_secs_f64();
    let len = p.frames as f64 / 48_000.0;
    Some(Some(crate::source::State { playing: t < len, track: p.track.clone(), position: t.min(len), source: crate::source::Source::Spotify }))
}

/// Listening mode with the simulated player standing in for Spotify (ONKEY_SIM_LISTEN): Some(sounding now).
/// ONKEY_ON_BATTERY=1 (or 0): as if the Mac were on its battery (or not), for measuring the window's drawing.
pub fn force_battery() -> Option<bool> {
    std::env::var("ONKEY_ON_BATTERY").ok().map(|v| v != "0" && !v.is_empty())
}

pub fn sim_sounding() -> Option<bool> {
    if std::env::var("ONKEY_SIM_LISTEN").is_err() {
        return None;
    }
    Some(matches!(sim_player(), Some(Some(s)) if s.playing))
}

/// The simulated player answering as Spotify's AppleScript would (ONKEY_SIM_LISTEN): None when not standing in.
/// ONKEY_SIM_APP_LEAD=<s>: the position it says runs that far ahead of its sound.
pub fn sim_app_state() -> Option<Option<crate::source::State>> {
    if std::env::var("ONKEY_SIM_LISTEN").is_err() {
        return None;
    }
    let lead = std::env::var("ONKEY_SIM_APP_LEAD").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    Some(sim_player().flatten().map(|mut s| {
        s.position += lead;
        s
    }))
}

/// The simulated player stands in for the music app (ONKEY_SIM_LISTEN).
pub fn sim_app() -> bool {
    std::env::var("ONKEY_SIM_LISTEN").is_ok()
}

/// What one AppleScript call to the music app costs OnKey (an osascript process), spent where the real app would
/// make one while the simulated player stands in for it (ONKEY_SIM_OSA=1).
pub fn sim_osa_cost() {
    if std::env::var("ONKEY_SIM_OSA").is_ok() {
        let _ = std::process::Command::new("osascript").args(["-e", "return 1"]).output();
    }
}

/// The live pitch's latency and cadence (ONKEY_PERF): when each stretch of microphone reached the take, and when
/// the frames tracked from it reached the window. Logged every 10 s: the newest frame's age as each update leaves
/// (from the moment its last sample reached the take), the same for a voice's first frame after a quiet stretch
/// (the dot appearing), and how often new frames went out.
#[derive(Default)]
pub struct DotClock {
    on: Option<bool>,
    pushed: std::collections::VecDeque<(usize, Instant)>,
    newest: Vec<f64>,
    onset: Vec<f64>,
    gaps: Vec<f64>,
    last_emit: Option<Instant>,
    since: Option<Instant>,
}

impl DotClock {
    fn on(&mut self) -> bool {
        *self.on.get_or_insert_with(|| std::env::var("ONKEY_PERF").is_ok())
    }

    /// The take's cleaned microphone is now `len` samples (16 kHz) long.
    pub fn pushed(&mut self, len: usize) {
        if !self.on() {
            return;
        }
        if self.pushed.back().is_some_and(|(l, _)| *l > len) {
            self.pushed.clear();        // a new take
        }
        self.pushed.push_back((len, Instant::now()));
        while self.pushed.len() > 2000 {
            self.pushed.pop_front();
        }
    }

    fn arrived(&self, end: usize) -> Option<Instant> {
        self.pushed.iter().find(|(l, _)| *l >= end).map(|(_, t)| *t)
    }

    /// Frames `from..to` of `usr` (NaN: no dot) are going to the window now.
    pub fn emitted(&mut self, usr: &[f32], from: usize, to: usize) {
        if !self.on() || to <= from {
            return;
        }
        let now = Instant::now();
        let end = |j: usize| j * crate::pitch::HOP + crate::pitch::WIN / 2;
        if let Some(t) = self.arrived(end(to - 1)) {
            self.newest.push((now - t).as_secs_f64() * 1000.0);
        }
        for j in from.max(1)..to {
            if !usr[j].is_nan() && usr[j - 1].is_nan() {
                if let Some(t) = self.arrived(end(j)) {
                    self.onset.push((now - t).as_secs_f64() * 1000.0);
                }
            }
        }
        if let Some(l) = self.last_emit.replace(now) {
            self.gaps.push((now - l).as_secs_f64() * 1000.0);
        }
        let since = *self.since.get_or_insert(now);
        if now - since >= Duration::from_secs(10) {
            let q = |v: &mut Vec<f64>, p: f64| -> f64 {
                if v.is_empty() { return f64::NAN; }
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                v[((v.len() - 1) as f64 * p).round() as usize]
            };
            crate::engine::log(&format!("perf: dot latency median {:.0} ms p95 {:.0} ms; onset median {:.0} ms max {:.0} ms ({} onsets); updates every {:.0} ms (p95 {:.0} ms); recognitions asked {:?}",
                q(&mut self.newest, 0.5), q(&mut self.newest, 0.95), q(&mut self.onset, 0.5), q(&mut self.onset, 1.0), self.onset.len(),
                q(&mut self.gaps, 0.5), q(&mut self.gaps, 0.95), crate::listen::requests()));
            self.newest.clear();
            self.onset.clear();
            self.gaps.clear();
            self.since = Some(now);
        }
    }
}

/// The simulated player's sound into the capture's ring buffer, [L, R, mic 0] frames at the song's own pace on
/// its own thread, until the song ends or the flag returned is set. (The rate, and that flag.)
pub fn sim_capture(mut producer: rtrb::Producer<f32>) -> (f64, Arc<AtomicBool>) {
    let stop = Arc::new(AtomicBool::new(false));
    let Some((mix, frames, starts)) = player().and_then(|g| g.as_ref().map(|p| (p.mix.clone(), p.frames, p.starts))) else {
        stop.store(true, Ordering::SeqCst);
        return (48_000.0, stop);
    };
    let flag = stop.clone();
    let first = ((Instant::now().saturating_duration_since(starts)).as_secs_f64() * 48_000.0) as usize;
    SIM_CAPTURE_FROM.store(first.min(frames) as i64, Ordering::SeqCst);
    let _ = std::thread::Builder::new().name("onkey-sim-player".into()).spawn(move || {
        use std::io::{Read, Seek};
        let Ok(mut file) = std::fs::File::open(&mix) else { return };
        let at = || ((Instant::now().saturating_duration_since(starts)).as_secs_f64() * 48_000.0) as usize;
        let mut sent = first.min(frames);
        let mut bytes = Vec::new();
        while !flag.load(Ordering::SeqCst) && sent < frames {
            std::thread::sleep(Duration::from_millis(10));
            let want = at().min(frames);
            if want > sent {
                bytes.resize((want - sent) * 8, 0);
                if file.seek(std::io::SeekFrom::Start(sent as u64 * 8)).is_err() || file.read_exact(&mut bytes).is_err() {
                    return;
                }
                let sample = |i: usize| f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
                if let Ok(chunk) = producer.write_chunk_uninit((want - sent) * crate::capture::CHANNELS) {
                    chunk.fill_from_iter((0..want - sent).flat_map(|i| [sample(2 * i), sample(2 * i + 1), 0.0]));
                }
                sent = want;
            }
        }
    });
    (48_000.0, stop)
}

pub fn init() {
    if let Ok(p) = std::env::var("ONKEY_EVENTS_LOG") {
        *EVENTS.lock().unwrap() = std::fs::File::create(p).ok();
    }
}

/// An event for the window: into the event log, and on to the live mirror's tabs (see mirror.rs).
pub fn record(ev: &serde_json::Value) {
    let s = ev.to_string();
    crate::mirror::send(ev["type"].as_str().unwrap_or(""), &s);
    if let Some(f) = EVENTS.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{s}");
    }
}

pub struct SimSinger {
    mix_env: Vec<[f32; FP_BANDS]>,
    mic: Vec<f32>,
    heard16: Vec<f32>,
    rs: Resampler,
    /// mix sample index of the capture's sample 0, once found
    offset: Option<i64>,
    n: i64,
    tried_at: i64,
}

impl SimSinger {
    pub fn from_env(rate: f64) -> Option<SimSinger> {
        let dir = std::path::PathBuf::from(std::env::var("ONKEY_SIM").ok()?);
        let kind = std::env::var("ONKEY_SIM_KIND").unwrap_or("perfect".into());
        let read = |p: std::path::PathBuf| -> Vec<f32> {
            std::fs::read(&p).unwrap_or_default().chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect()
        };
        let mix = read(dir.join("mix.f32"));
        let mono: Vec<f32> = mix.chunks_exact(2).map(|c| 0.5 * (c[0] + c[1])).collect();
        let mut m16 = Vec::new();
        Resampler::new(48_000.0, 16_000.0).process(&mono, &mut m16);
        let mic = read(dir.join(format!("mic_{kind}.f32")));
        crate::engine::log(&format!("simulated singer: {} ({kind}), {:.0} s", dir.display(), mic.len() as f64 / 48_000.0));
        assert!((rate - 48_000.0).abs() < 1.0, "the simulated singer needs a 48 kHz capture");
        // with the simulated player, where the capture began in the song is known: the singer sings from there at
        // once (finding it by ear took minutes in listening mode, where the music before the song is named isn't
        // in the take)
        let from = SIM_CAPTURE_FROM.load(Ordering::SeqCst);
        if sim_player_on() && from >= 0 {
            crate::engine::log(&format!("simulated singer: the capture began {:.2} s into the song", from as f64 / 48_000.0));
            return Some(SimSinger { mix_env: Vec::new(), mic, heard16: Vec::new(), rs: Resampler::new(rate, 16_000.0),
                                    offset: Some(from), n: 0, tried_at: 0 });
        }
        Some(SimSinger { mix_env: onset_envelope(&m16), mic, heard16: Vec::new(), rs: Resampler::new(rate, 16_000.0),
                         offset: None, n: 0, tried_at: 0 })
    }

    /// Replace the mic channel of interleaved [L, R, mic] frames with the simulated singer.
    pub fn apply(&mut self, frames: &mut [f32]) {
        let k = frames.len() / 3;
        if self.offset.is_none() {
            let music: Vec<f32> = frames.chunks_exact(3).map(|f| 0.5 * (f[0] + f[1])).collect();
            self.rs.process(&music, &mut self.heard16);
            let keep = 16_000 * 10;
            if self.heard16.len() > keep {
                let cut = self.heard16.len() - keep;
                self.heard16.drain(..cut);
            }
            if self.n - self.tried_at > 48_000 && self.heard16.len() >= 16_000 * 8 {
                self.tried_at = self.n;
                let tail = &self.heard16[self.heard16.len() - 16_000 * 8..];
                let env = onset_envelope(tail);
                let (f, conf) = locate(&env, &self.mix_env, 0, None);
                if conf >= 1.3 {
                    // env[0] is the mix frame f; it was heard 8 s before the newest capture sample
                    let start_n = self.n + k as i64 - 8 * 48_000;
                    self.offset = Some(f * 480 - start_n);
                    crate::engine::log(&format!("simulated singer: found the song at {:.2} s (confidence {conf:.2})", f as f64 / 100.0));
                }
            }
        }
        for (j, fr) in frames.chunks_exact_mut(3).enumerate() {
            fr[2] = match self.offset {
                Some(o) => {
                    let i = self.n + j as i64 + o;
                    if i >= 0 && (i as usize) < self.mic.len() { self.mic[i as usize] } else { 0.0 }
                }
                None => 0.0,
            };
        }
        self.n += k as i64;
    }
}


}

/// Release builds: the same calls, doing nothing.
#[cfg(not(feature = "devtest"))]
mod stub {
    pub fn init() {}
    pub fn record(_ev: &serde_json::Value) {}
    pub fn sim_player_on() -> bool { false }
    pub fn sim_player() -> Option<Option<crate::source::State>> { None }
    pub fn sim_sounding() -> Option<bool> { None }
    pub fn force_battery() -> Option<bool> { None }
    pub fn sim_app_state() -> Option<Option<crate::source::State>> { None }
    pub fn sim_app() -> bool { false }
    pub fn sim_osa_cost() {}
    #[derive(Default)]
    pub struct DotClock;
    impl DotClock {
        pub fn pushed(&mut self, _len: usize) {}
        pub fn emitted(&mut self, _usr: &[f32], _from: usize, _to: usize) {}
    }
    pub fn sim_capture(_producer: rtrb::Producer<f32>) -> (f64, std::sync::Arc<std::sync::atomic::AtomicBool>) {
        (0.0, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)))
    }
    pub struct SimSinger;
    impl SimSinger {
        pub fn from_env(_rate: f64) -> Option<SimSinger> { None }
        pub fn apply(&mut self, _frames: &mut [f32]) {}
    }
}
