//! Development only (the `devtest` feature; release builds don't contain it): the ear. While "Play with the song"
//! plays, it measures what actually comes out of the Mac, with no microphone and no speakers: Core Audio process
//! taps on the music app and on OnKey's own WebKit process (the window's Web Audio is played by WebKit's GPU process,
//! which macOS holds OnKey responsible for: native/ear.swift), one aggregate device, so both are on one sample clock.
//! Every 2 s it finds:
//!   - the voice: where in the take's recording the window's output is (its last 4 s cross-correlated with the take's
//!     mic recording, sample-exact), so in song time (+ song_offset - latency, the place the take was scored at)
//!   - the music: where in the song the music app's output is, by up to three measures, best first:
//!       ref   the music captured while the take was sung (lab/recorded/mics/<take>/ref16.f32, kept by ./run's
//!             ONKEY_KEEP_AUDIO), cross-correlated with the music app's output: no learned timeline involved
//!       fp    the learned song's rhythm fingerprint (.bin onset rows) matched against the music app's output
//!       app   the app's own position (Spotify's), and listening's `heard` (what "Play with the song" follows)
//! and says the gap between them, which is what you hear (both taps are taken where the apps hand their sound to
//! Core Audio; the output device after that is the same for both, `outputs` says which and its latency).
//! It logs "ear: …" lines into $TMPDIR/onkey.log, a summary when it stops, and keeps them for the mirror's /api/ear.
//!
//! The maths (cross-correlation, the readings and their summary) is here, unit-tested on synthetic signals; the
//! capture and the loop are in `live`.
use realfft::RealFftPlanner;

use crate::dsp::{locate_peaks, onset_envelope, FP_BANDS, FRAME_S};

/// The rate everything is matched at.
pub const RATE: f64 = 16_000.0;

/// The best match of `needle` in `hay` (a lag: needle[0] lines up with hay[lag]), for lags in [lo, hi).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    pub lag: usize,
    /// normalised correlation at the lag (-1..1; 1: the same waveform, at any level)
    pub score: f32,
    /// the best score more than `guard` samples away from it (a clear match is well above this)
    pub second: f32,
}

/// Normalised cross-correlation of `needle` against `hay` for every lag in [lo, hi) (clipped to the lags where the
/// needle fits), by FFT. Silent stretches of the hay score 0.
pub fn ncc(needle: &[f32], hay: &[f32], lo: usize, hi: usize) -> Vec<f32> {
    let n = needle.len();
    if n == 0 || hay.len() < n {
        return Vec::new();
    }
    let hi = hi.min(hay.len() - n + 1);
    if hi <= lo {
        return Vec::new();
    }
    let seg = &hay[lo..hi - 1 + n];
    let lags = hi - lo;
    let size = (seg.len() + n).next_power_of_two();
    let mut planner = RealFftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(size);
    let inv = planner.plan_fft_inverse(size);
    let mut a = vec![0f64; size];
    for (d, s) in a.iter_mut().zip(seg) {
        *d = *s as f64;
    }
    let mut b = vec![0f64; size];
    for (d, s) in b.iter_mut().zip(needle) {
        *d = *s as f64;
    }
    let mut fa = fwd.make_output_vec();
    let mut fb = fwd.make_output_vec();
    fwd.process(&mut a, &mut fa).unwrap();
    fwd.process(&mut b, &mut fb).unwrap();
    for (x, y) in fa.iter_mut().zip(&fb) {
        *x *= y.conj();
    }
    let mut c = vec![0f64; size];
    inv.process(&mut fa, &mut c).unwrap();
    let en: f64 = needle.iter().map(|v| (*v as f64).powi(2)).sum();
    let mut csum = vec![0f64; seg.len() + 1];
    for (i, v) in seg.iter().enumerate() {
        csum[i + 1] = csum[i] + (*v as f64).powi(2);
    }
    (0..lags).map(|k| {
        let ew = (csum[k + n] - csum[k]).max(0.0);
        let norm = (en * ew).sqrt();
        // (the FFT's correlation is size times too big; quiet stretches, below the needle's level by 80 dB, are none)
        if norm > 0.0 && ew > en * 1e-8 { (c[k] / size as f64 / norm) as f32 } else { 0.0 }
    }).collect()
}

/// The best lag of `needle` in `hay` within [lo, hi), and the best score more than `guard` samples from it.
pub fn best_lag(needle: &[f32], hay: &[f32], lo: usize, hi: usize, guard: usize) -> Option<Match> {
    let c = ncc(needle, hay, lo, hi);
    let (i, &score) = c.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let second = c.iter().enumerate().filter(|(j, _)| j.abs_diff(i) > guard).map(|(_, v)| *v).fold(0f32, f32::max);
    Some(Match { lag: lo + i, score, second })
}

/// A signed time, "+1.29" or "-0.35" ("+0.00", never "-0.00").
pub fn signed(x: f64) -> String {
    let x = if x.abs() < 0.005 { 0.0 } else { x };
    format!("{}{x:.2}", if x >= 0.0 { "+" } else { "" })
}

/// Level in dBFS (RMS); -120 for silence.
pub fn db(x: &[f32]) -> f32 {
    if x.is_empty() {
        return -120.0;
    }
    let e = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64;
    if e <= 1e-12 { -120.0 } else { (10.0 * e.log10()) as f32 }
}

/// Where `x` is loudest over `len` samples (the start of that stretch), stepping `step`: a stretch of singing to
/// look for, not a gap.
pub fn loudest(x: &[f32], len: usize, step: usize) -> Option<usize> {
    if x.len() < len || len == 0 {
        return None;
    }
    let mut csum = vec![0f64; x.len() + 1];
    for (i, v) in x.iter().enumerate() {
        csum[i + 1] = csum[i] + (*v as f64).powi(2);
    }
    (0..=x.len() - len).step_by(step.max(1)).max_by(|a, b| (csum[a + len] - csum[*a]).total_cmp(&(csum[b + len] - csum[*b])))
}

/// What the take was scored at: the song's time at the recording's start, and the latency scoring allowed for (s).
/// The voice at recording time τ goes with the song at τ + song_offset - latency (the take page's `off`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub song_offset: f64,
    pub latency: f64,
}

impl Frame {
    pub fn voice_song(&self, take_s: f64) -> f64 {
        take_s + self.song_offset - self.latency
    }
    /// The music captured at recording time ρ during the take was the song at ρ + song_offset.
    pub fn ref_song(&self, take_s: f64) -> f64 {
        take_s + self.song_offset
    }
}

/// One measurement, at the end of the window looked at (all s).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// seconds since the ear started (the window's end)
    pub at: f64,
    /// where the voice coming out is in the take's recording, and how well it matched (normalised correlation)
    pub voice_take: Option<(f64, f32)>,
    /// where the music coming out is in the take's own captured music (the recording's time line) and the match
    pub music_ref: Option<(f64, f32)>,
    /// the song time of the music coming out by the learned fingerprint, and the match (normalised, 0..1)
    pub music_fp: Option<(f64, f32)>,
    /// the music app's own position at that moment, and listening's `heard` (song s)
    pub app: Option<f64>,
    pub heard: Option<f64>,
    /// levels (dBFS) of the two outputs over the window
    pub voice_db: f32,
    pub music_db: f32,
}

/// A voice match this good is the recording playing (a 4 s waveform match: noise scores ~0.05).
pub const VOICE_MIN: f32 = 0.5;
/// A music match against the take's own captured music this good is it.
pub const REF_MIN: f32 = 0.4;
/// A fingerprint match this good is the song there (normalised onset correlation).
pub const FP_MIN: f32 = 0.3;

impl Reading {
    pub fn voice_song(&self, f: &Frame) -> Option<f64> {
        self.voice_take.filter(|v| v.1 >= VOICE_MIN).map(|v| f.voice_song(v.0))
    }
    /// The music's song time by the best measure there is: (time, which).
    pub fn music_song(&self, f: &Frame) -> Option<(f64, &'static str)> {
        if let Some((t, s)) = self.music_ref {
            if s >= REF_MIN {
                return Some((f.ref_song(t), "ref"));
            }
        }
        if let Some((t, s)) = self.music_fp {
            if s >= FP_MIN {
                return Some((t, "fp"));
            }
        }
        self.app.map(|t| (t, "app"))
    }
    /// What you hear: how far the music is ahead of the voice (s, + music ahead, - music behind).
    pub fn gap(&self, f: &Frame) -> Option<(f64, &'static str)> {
        let v = self.voice_song(f)?;
        self.music_song(f).map(|(m, how)| (m - v, how))
    }
    /// Each measure's gap: (name, gap), for the summary.
    pub fn gaps(&self, f: &Frame) -> Vec<(&'static str, f64)> {
        let Some(v) = self.voice_song(f) else { return Vec::new() };
        let mut g = Vec::new();
        if let Some((t, s)) = self.music_ref {
            if s >= REF_MIN { g.push(("ref", f.ref_song(t) - v)); }
        }
        if let Some((t, s)) = self.music_fp {
            if s >= FP_MIN { g.push(("fp", t - v)); }
        }
        if let Some(t) = self.app { g.push(("app", t - v)); }
        if let Some(t) = self.heard { g.push(("heard", t - v)); }
        g
    }

    /// The log line: "ear: voice out at take 12.31 = song 11.43; music out at song 12.72 (ref 12.70, fp 12.72,
    /// app 12.74, heard 12.71); audible gap +1.29 s (music ahead, by ref)".
    pub fn line(&self, f: &Frame, app_name: &str) -> String {
        let s2 = |x: f64| format!("{x:.2}");
        let voice = match self.voice_take {
            Some((t, s)) if s >= VOICE_MIN => format!("voice out at take {} = song {} (match {s:.2})", s2(t), s2(f.voice_song(t))),
            Some((t, s)) => format!("voice unsure (best take {} match {s:.2}, {:.0} dB)", s2(t), self.voice_db),
            None if self.voice_db < -70.0 => "no voice out (silent: not playing, or a silent stretch of the recording)".to_string(),
            None => format!("voice not found ({:.0} dB)", self.voice_db),
        };
        let mut parts = Vec::new();
        match self.music_ref {
            Some((t, s)) if s >= REF_MIN => parts.push(format!("ref {} ({s:.2})", s2(f.ref_song(t)))),
            Some((_, s)) => parts.push(format!("ref unsure ({s:.2})")),
            None => {}
        }
        match self.music_fp {
            Some((t, s)) if s >= FP_MIN => parts.push(format!("fp {} ({s:.2})", s2(t))),
            Some((_, s)) => parts.push(format!("fp unsure ({s:.2})")),
            None => {}
        }
        if let Some(t) = self.app { parts.push(format!("{app_name} {}", s2(t))); }
        if let Some(t) = self.heard { parts.push(format!("heard {}", s2(t))); }
        let music = match self.music_song(f) {
            Some((t, _)) => format!("music out at song {}", s2(t)),
            None if self.music_db < -70.0 => "no music out (silent)".to_string(),
            None => format!("music not placed ({:.0} dB)", self.music_db),
        };
        let gap = match self.gap(f) {
            Some((g, how)) => format!("; audible gap {} s ({}, by {how})", signed(g),
                                      if g.abs() < 0.03 { "together" } else if g > 0.0 { "music ahead" } else { "voice ahead" }),
            None => String::new(),
        };
        let more = if parts.is_empty() { String::new() } else { format!(" ({})", parts.join(", ")) };
        format!("ear: {voice}; {music}{more}{gap}")
    }
}

/// Median and interquartile range of `x` (sorted copy). None when empty.
pub fn median_iqr(x: &[f64]) -> Option<(f64, f64)> {
    if x.is_empty() {
        return None;
    }
    let mut v = x.to_vec();
    v.sort_by(f64::total_cmp);
    let q = |p: f64| {
        let i = p * (v.len() - 1) as f64;
        let (a, b) = (i.floor() as usize, i.ceil() as usize);
        v[a] + (v[b] - v[a]) * (i - a as f64)
    };
    Some((q(0.5), q(0.75) - q(0.25)))
}

/// The summary at stop: per measure, the median gap, its spread and how many readings had one.
pub fn summary(rs: &[Reading], f: &Frame, app_name: &str) -> String {
    let voiced = rs.iter().filter(|r| r.voice_song(f).is_some()).count();
    let mut out = format!("ear summary: {} readings, voice found in {voiced}", rs.len());
    for name in ["ref", "fp", "app", "heard"] {
        let g: Vec<f64> = rs.iter().flat_map(|r| r.gaps(f)).filter(|(n, _)| *n == name).map(|(_, g)| g).collect();
        if let Some((m, iqr)) = median_iqr(&g) {
            let label = if name == "app" { app_name } else { name };
            out.push_str(&format!("; gap by {label} {} s (spread {iqr:.2}, n {})", signed(m), g.len()));
        }
    }
    let best: Vec<f64> = rs.iter().filter_map(|r| r.gap(f)).map(|g| g.0).collect();
    match median_iqr(&best) {
        Some((m, _)) => out.push_str(&format!("; heard: {}", if m.abs() < 0.03 { "together".to_string() }
            else if m > 0.0 { format!("music {m:.2} s ahead") } else { format!("voice {:.2} s ahead", -m) })),
        None => out.push_str("; no gap measured"),
    }
    out
}

/// The take's music kept while it was sung (development builds with ONKEY_KEEP_AUDIO: lab/recorded/mics/<time>-<song>/,
/// ref16.f32 and mic16.f32 at 16 kHz, sample-aligned), found by its track and time and checked against the take's
/// recording: the folder whose mic, or echo-cancelled mic, has the take's loudest 5 s of singing in it. Returns
/// (ref16, offset): ref16[i + offset] is the music captured at the take's recording sample i.
pub fn find_ref(mics: &std::path::Path, track_id: &str, take_time: &str, take16: &[f32]) -> Option<(Vec<f32>, i64, f32)> {
    let probe_len = (5.0 * RATE) as usize;
    let at = loudest(take16, probe_len, 1600)?;
    let probe = &take16[at..at + probe_len];
    // "20261026-140201" (the take's id) → "20261026T140201" (the folder's), and minutes either side
    let stamp = |s: &str| -> Option<i64> {
        let d: String = s.chars().filter(|c| c.is_ascii_digit()).take(14).collect();
        if d.len() < 14 { return None; }
        let n = |a: usize, b: usize| d[a..b].parse::<i64>().ok();
        Some((((n(0, 8)? * 24 + n(8, 10)?) * 60 + n(10, 12)?) * 60) + n(12, 14)?)
    };
    let want = stamp(take_time)?;
    let mut best: Option<(Vec<f32>, i64, f32)> = None;
    for e in std::fs::read_dir(mics).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        // (days roll over: a 23:59 take in a folder of the next day is rare, and only costs a missed reference)
        if stamp(&name).is_none_or(|t| (t - want).abs() > 15 * 60) {
            continue;
        }
        let dir = e.path();
        let meta: serde_json::Value = match std::fs::read(dir.join("meta.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
            Some(m) => m,
            None => continue,
        };
        if meta["track"]["id"].as_str() != Some(track_id) {
            continue;
        }
        for mic in ["mic16.f32", "clean16.f32"] {
            let Some(m) = read_f32(&dir.join(mic)) else { continue };
            let Some(hit) = best_lag(probe, &m, 0, m.len(), 800) else { continue };
            if hit.score >= VOICE_MIN && best.as_ref().is_none_or(|b| hit.score > b.2) {
                let Some(r) = read_f32(&dir.join("ref16.f32")) else { continue };
                best = Some((r, hit.lag as i64 - at as i64, hit.score));
            }
        }
    }
    best
}

/// A raw little-endian f32 file.
pub fn read_f32(p: &std::path::Path) -> Option<Vec<f32>> {
    let b = std::fs::read(p).ok()?;
    Some(b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

/// Finds the voice and the music, reading after reading: each search starts near where the last one found it (2 s
/// on), and searches everything when that fails.
#[derive(Default)]
pub struct Tracker {
    /// (take sample at the window's end, `total` then)
    last_voice: Option<(usize, usize)>,
    last_ref: Option<(usize, usize)>,
}

/// The voice and music windows are this long (s); the fingerprint's, longer (the fingerprint needs 3 s or more).
pub const WIN_S: f64 = 4.0;
pub const FP_WIN_S: f64 = 6.0;

/// Search `needle` (ending at sample `total` of the ear's clock) in `hay`, near where it was last time.
fn follow(needle: &[f32], hay: &[f32], last: &mut Option<(usize, usize)>, total: usize, min: f32) -> Option<Match> {
    let (win, slack) = (needle.len(), (2.0 * RATE) as usize);
    let (lo, hi) = match *last {
        Some((at, then)) => {
            let guess = at + total.saturating_sub(then);
            (guess.saturating_sub(win + slack), guess.saturating_sub(win) + slack)
        }
        None => (0, hay.len()),
    };
    let mut hit = best_lag(needle, hay, lo, hi, 320);
    if last.is_some() && hit.is_none_or(|h| h.score < min) {
        hit = best_lag(needle, hay, 0, hay.len(), 320);
    }
    *last = hit.filter(|h| h.score >= min).map(|h| (h.lag + win, total));
    hit
}

impl Tracker {
    /// One reading from the last seconds of both outputs (16 kHz, sample-aligned; `total`: samples so far), with the
    /// app's position and listening's `heard` at the moment they end. `reference`: the take's own music and where
    /// it lines up (ref[i + offset] is take sample i); `onsets`: the learned song's fingerprint.
    #[allow(clippy::too_many_arguments)]
    pub fn measure(&mut self, f: &Frame, take16: &[f32], reference: Option<&(Vec<f32>, i64, f32)>, onsets: &[[f32; FP_BANDS]],
                   music: &[f32], voice: &[f32], total: usize, app: Option<f64>, heard: Option<f64>, at: f64) -> Reading {
        let win = (WIN_S * RATE) as usize;
        let mut r = Reading { at, app, heard, ..Default::default() };
        let vw = &voice[voice.len().saturating_sub(win)..];
        let mw = &music[music.len().saturating_sub(win)..];
        r.voice_db = db(vw);
        r.music_db = db(mw);
        // the voice, in the take's recording
        if vw.len() == win && r.voice_db > -70.0 {
            if let Some(h) = follow(vw, take16, &mut self.last_voice, total, VOICE_MIN) {
                r.voice_take = Some(((h.lag + win) as f64 / RATE, h.score));
            }
        } else {
            self.last_voice = None;
        }
        // the music, in the take's own captured music (the recording's time line)
        if let Some((rf, off, _)) = reference {
            if mw.len() == win && r.music_db > -70.0 {
                if let Some(h) = follow(mw, rf, &mut self.last_ref, total, REF_MIN) {
                    r.music_ref = Some(((h.lag + win) as f64 / RATE - *off as f64 / RATE, h.score));
                }
            }
        }
        // the music by the learned fingerprint, within 5 s of where the voice, or else the app, says it is
        let fw = (FP_WIN_S * RATE) as usize;
        if !onsets.is_empty() && music.len() >= fw && r.music_db > -70.0 {
            let env = onset_envelope(&music[music.len() - fw..]);
            let (lo, hi) = match r.voice_song(f).or(app).or(heard) {
                Some(s) => {
                    let start = (s - FP_WIN_S) / FRAME_S;
                    ((start - 500.0) as i64, Some((start + 500.0) as i64))
                }
                None => (0, None),
            };
            if let Some(&(k, c)) = locate_peaks(&env, onsets, lo, hi, 2).first() {
                r.music_fp = Some((k as f64 * FRAME_S + FP_WIN_S, c));
            }
        }
        r
    }
}

#[cfg(feature = "devtest")]
pub use live::{latest, start, stop, Take};

#[cfg(feature = "devtest")]
mod live {
    use std::ffi::{c_char, c_void, CString};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use rtrb::{Producer, RingBuffer};
    use serde_json::{json, Value};

    use super::*;
    use crate::dsp::FP_BANDS;
    use crate::resample::Resampler;

    // the Swift library on macOS; a stand-in that says it isn't there on Windows (src/platform)
    use crate::platform::native::{onkey_ear_outputs, onkey_ear_start, onkey_ear_stop};

    /// Interleaved frames from the ear's taps: [music, voice] (each mixed to mono).
    const CH: usize = 2;
    static PRODUCER: Mutex<Option<Producer<f32>>> = Mutex::new(None);

    extern "C" fn on_audio(data: *const f32, frames: i32, _ctx: *mut c_void) {
        let n = frames.max(0) as usize * CH;
        let samples = unsafe { std::slice::from_raw_parts(data, n) };
        if let Ok(mut g) = PRODUCER.try_lock() {
            if let Some(p) = g.as_mut() {
                if let Ok(chunk) = p.write_chunk_uninit(n) {
                    chunk.fill_from_iter(samples.iter().copied());
                }
            }
        }
    }

    /// What the ear needs to know about the take playing.
    pub struct Take {
        pub id: String,
        /// the track's id, as keep_mic kept it
        pub track_id: String,
        /// the ids the player may name it by (player_now's track): the take's track, its app's id, the learned song's
        pub ids: Vec<String>,
        pub frame: Frame,
        /// the take's recording (mic.m4a)
        pub mic: std::path::PathBuf,
        /// the learned song's rhythm fingerprint (10 ms rows), empty when none
        pub onsets: Vec<[f32; FP_BANDS]>,
        /// the music app's bundle id (prefix) and name
        pub bundle: String,
        pub app_name: String,
        pub shared: std::sync::Arc<std::sync::Mutex<crate::engine::Shared>>,
    }

    struct Running {
        take: String,
        stop: Arc<AtomicBool>,
        thread: std::thread::JoinHandle<()>,
    }

    static RUN: Mutex<Option<Running>> = Mutex::new(None);
    /// The latest lines and the last summary, for the mirror's /api/ear.
    static LATEST: Mutex<Option<Value>> = Mutex::new(None);

    fn say(line: &str) {
        crate::engine::log(line);
        let mut g = LATEST.lock().unwrap_or_else(|e| e.into_inner());
        let v = g.get_or_insert_with(|| json!({"lines": []}));
        if let Some(a) = v["lines"].as_array_mut() {
            a.push(json!(format!("{} {line}", crate::store::now_iso())));
            if a.len() > 300 { a.remove(0); }
        }
    }

    /// The ear's lines so far (this run and the last), its last summary and whether it's running.
    pub fn latest() -> Value {
        let mut v = LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| json!({"lines": []}));
        v["running"] = json!(RUN.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|r| r.take.clone()));
        v
    }

    /// Start listening to the take playing with its song (stops one already running for another take).
    pub fn start(t: Take) {
        let mut g = RUN.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_ref().is_some_and(|r| r.take == t.id && !r.thread.is_finished()) {
            return;
        }
        if let Some(r) = g.take() {
            r.stop.store(true, Ordering::SeqCst);
            let _ = r.thread.join();
        }
        *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(json!({"lines": [], "take": t.id}));
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let take = t.id.clone();
        let thread = std::thread::Builder::new().name("ear".into()).spawn(move || run(t, s)).expect("ear thread");
        *g = Some(Running { take, stop, thread });
    }

    /// Stop it: its summary goes to the log.
    pub fn stop() {
        let r = RUN.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(r) = r {
            r.stop.store(true, Ordering::SeqCst);
            let _ = r.thread.join();
        }
    }

    fn to16(x: &[f32], rate: f64) -> Vec<f32> {
        if (rate - RATE).abs() < 1.0 {
            return x.to_vec();
        }
        let mut out = Vec::with_capacity((x.len() as f64 * RATE / rate) as usize + 1);
        let mut r = Resampler::new(rate, RATE);
        r.process(x, &mut out);
        r.process(&vec![0.0; (rate / 100.0) as usize], &mut out);
        out
    }

    fn outputs() -> String {
        let mut buf = vec![0 as c_char; 16 * 1024];
        unsafe { onkey_ear_outputs(buf.as_mut_ptr(), buf.len() as i32) };
        unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
    }

    fn run(t: Take, stop: Arc<AtomicBool>) {
        let f = t.frame;
        say(&format!("ear: listening to take {} ({}; song_offset {:.3} s, latency {:.0} ms: voice at take τ goes with song τ{:+.3})",
                     t.id, t.app_name, f.song_offset, f.latency * 1000.0, f.song_offset - f.latency));
        let take16 = match crate::takeaudio::decode_mono(&t.mic) {
            Ok((x, rate)) => to16(&x, rate),
            Err(e) => return say(&format!("ear: can't decode the take's recording ({e}); not listening")),
        };
        // the take's own music, when ./run kept it (the best truth: no learned time line in it)
        let kept = std::env::var("ONKEY_KEEP_AUDIO").ok().map(|d| std::path::PathBuf::from(d).join("mics"));
        let reference = kept.as_deref().and_then(|d| find_ref(d, &t.track_id, &t.id, &take16));
        match &reference {
            Some((r, off, s)) => say(&format!("ear: the take's own music found ({:.0} s, lined up with the recording at {:+.3} s, match {s:.2})",
                                             r.len() as f64 / RATE, *off as f64 / RATE)),
            None => say(&format!("ear: no kept music for this take ({}); measuring by fingerprint and the app's clock",
                                 if kept.is_none() { "ONKEY_KEEP_AUDIO isn't set" } else { "none in lab/recorded/mics matches it" })),
        }
        if t.onsets.is_empty() {
            say("ear: the song has no learned fingerprint");
        }
        // the taps: the music app's and OnKey's WebKit process (it may only appear once the window's sound starts)
        let (producer, mut consumer) = RingBuffer::<f32>::new(48_000 * CH * 10);
        *PRODUCER.lock().unwrap_or_else(|e| e.into_inner()) = Some(producer);
        let bundle = CString::new(t.bundle.clone()).unwrap_or_default();
        let began = Instant::now();
        let mut rate = 0.0;
        let mut last_code = -1;
        loop {
            if stop.load(Ordering::SeqCst) {
                *PRODUCER.lock().unwrap_or_else(|e| e.into_inner()) = None;
                return say("ear: stopped before it could listen");
            }
            let code = unsafe { onkey_ear_start(bundle.as_ptr(), on_audio, std::ptr::null_mut(), &mut rate) };
            if code == 0 {
                break;
            }
            if code != last_code {
                say(&format!("ear: can't tap yet: {}", match code {
                    1 => format!("{} isn't sending sound to Core Audio", t.app_name),
                    9 => "the ear's taps aren't on Windows".into(),
                    5 => "OnKey's WebKit process isn't sending sound (no Core Audio process object OnKey is responsible for)".into(),
                    2 => "macOS refused the tap (audio capture permission)".into(),
                    3 => "the aggregate device couldn't be made".into(),
                    _ => "the aggregate device wouldn't start".into(),
                }));
                last_code = code;
            }
            if began.elapsed() > Duration::from_secs(30) {
                *PRODUCER.lock().unwrap_or_else(|e| e.into_inner()) = None;
                return say(&format!("ear: gave up after 30 s ({}): {}", last_code, outputs()));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        say(&format!("ear: tapping at {rate:.0} Hz; outputs: {}", outputs()));

        let keep = (30.0 * RATE) as usize;
        let (mut music, mut voice) = (Vec::<f32>::new(), Vec::<f32>::new());
        let (mut rm, mut rv) = (Resampler::new(rate, RATE), Resampler::new(rate, RATE));
        let mut total = 0usize;          // 16 kHz samples taken since the start (music and voice alike)
        let mut raw = Vec::new();
        let (mut ms, mut vs) = (Vec::new(), Vec::new());
        let mut next = 2.0 * RATE;
        let mut readings: Vec<Reading> = Vec::new();
        let mut tracker = Tracker::default();
        let mut said_outputs = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(20));
            let n = consumer.slots();
            if n < CH { continue; }
            raw.clear();
            if let Ok(chunk) = consumer.read_chunk(n - n % CH) {
                let (a, b) = chunk.as_slices();
                raw.extend_from_slice(a);
                raw.extend_from_slice(b);
                chunk.commit_all();
            }
            let now = Instant::now();
            ms.clear(); vs.clear();
            let m: Vec<f32> = raw.chunks_exact(CH).map(|c| c[0]).collect();
            let v: Vec<f32> = raw.chunks_exact(CH).map(|c| c[1]).collect();
            rm.process(&m, &mut ms);
            rv.process(&v, &mut vs);
            let k = ms.len().min(vs.len());
            music.extend_from_slice(&ms[..k]);
            voice.extend_from_slice(&vs[..k]);
            total += k;
            if music.len() > keep {
                let d = music.len() - keep;
                music.drain(..d);
                voice.drain(..d);
            }
            if (total as f64) < next { continue; }
            next += 2.0 * RATE;
            // the app's clock and listening's, at the moment the window ended (`now`: when its last samples came)
            let (mut app, mut heard) = (None, None);
            if let Some((id, playing, pos, at)) = t.shared.lock().ok().and_then(|g| g.player_now.clone()) {
                if t.ids.contains(&id) {
                    app = Some(if playing { pos + now.saturating_duration_since(at).as_secs_f64() } else { pos });
                    // where Play with the song places the song (the engine's take placement, as player_now says)
                    heard = t.shared.lock().ok().and_then(|g| g.along_place).filter(|_| playing)
                        .map(|(s, at)| s + at.elapsed().as_secs_f64() - now.elapsed().as_secs_f64());
                }
            }
            let r = tracker.measure(&f, &take16, reference.as_ref(), &t.onsets, &music, &voice, total, app, heard, began.elapsed().as_secs_f64());
            say(&r.line(&f, &t.app_name));
            readings.push(r);
            if said_outputs.elapsed() > Duration::from_secs(30) {
                say(&format!("ear: outputs: {}", outputs()));
                said_outputs = Instant::now();
            }
        }
        unsafe { onkey_ear_stop() };
        *PRODUCER.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let s = summary(&readings, &f, &t.app_name);
        say(&s);
        if let Some(v) = LATEST.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            v["summary"] = json!(s);
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// A noise-like test signal (a fixed pseudo-random sequence, lightly smoothed so it isn't white).
    fn noise(n: usize, seed: u64) -> Vec<f32> {
        let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut prev = 0f32;
        (0..n).map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let x = ((s >> 33) as f32 / (1u64 << 31) as f32) - 0.5;
            prev = 0.6 * prev + 0.4 * x;
            prev
        }).collect()
    }

    #[test]
    fn cross_correlation_finds_a_stretch_at_any_level_to_the_sample() {
        let hay = noise(80_000, 1);
        for &at in &[0usize, 1, 12_345, 80_000 - 4_000] {
            let needle: Vec<f32> = hay[at..at + 4_000].iter().map(|v| v * 0.1).collect();
            let m = best_lag(&needle, &hay, 0, hay.len(), 320).unwrap();
            assert_eq!(m.lag, at);
            assert!(m.score > 0.999, "{m:?}");
            assert!(m.second < 0.2, "{m:?}");
        }
        // searched only in part of the hay: the best there, not outside it
        let needle = &hay[30_000..34_000];
        assert_eq!(best_lag(needle, &hay, 29_000, 31_000, 320).unwrap().lag, 30_000);
        assert!(best_lag(needle, &hay, 0, 20_000, 320).unwrap().score < 0.2);
    }

    #[test]
    fn a_noisy_limited_copy_still_matches_and_noise_does_not() {
        let hay = noise(64_000, 2);
        let other = noise(64_000, 3);
        // what comes out of Web Audio: raised, through a limiter (clipped), with a little hiss
        let needle: Vec<f32> = hay[20_000..24_000].iter().zip(&other).map(|(v, n)| (v * 8.0).clamp(-0.5, 0.5) + 0.05 * n).collect();
        let m = best_lag(&needle, &hay, 0, hay.len(), 320).unwrap();
        assert_eq!(m.lag, 20_000);
        assert!(m.score > VOICE_MIN, "{m:?}");
        let none = best_lag(&other[..4_000], &hay, 0, hay.len(), 320).unwrap();
        assert!(none.score < 0.2, "{none:?}");
    }

    #[test]
    fn silence_in_the_hay_scores_nothing() {
        let mut hay = vec![0f32; 10_000];
        hay.extend(noise(10_000, 4));
        let c = ncc(&hay[12_000..13_000], &hay, 0, 20_000);
        assert!(c[..9_000].iter().all(|v| *v == 0.0));
        assert_eq!(best_lag(&hay[12_000..13_000], &hay, 0, 20_000, 100).unwrap().lag, 12_000);
    }

    #[test]
    fn loudest_finds_the_singing_not_the_gap() {
        let mut x = vec![0.001f32; 50_000];
        for (i, v) in noise(8_000, 5).iter().enumerate() { x[30_000 + i] = *v; }
        let at = loudest(&x, 8_000, 100).unwrap();
        assert!((29_500..=30_500).contains(&at), "{at}");
    }

    #[test]
    fn the_gap_is_music_minus_voice_in_song_time() {
        // a take that starts 40 s into the song, scored with 120 ms of latency
        let f = Frame { song_offset: 40.0, latency: 0.12 };
        assert!((f.voice_song(10.0) - 49.88).abs() < 1e-9);
        // the voice is at take 10.00 (song 49.88), the music at song 51.17 by its own captured music: 1.29 s ahead
        let r = Reading { voice_take: Some((10.0, 0.9)), music_ref: Some((11.17, 0.8)), music_fp: Some((51.2, 0.6)),
                          app: Some(51.3), heard: Some(50.0), voice_db: -20.0, music_db: -15.0, ..Default::default() };
        let (g, how) = r.gap(&f).unwrap();
        assert_eq!(how, "ref");
        assert!((g - 1.29).abs() < 1e-9, "{g}");
        let line = r.line(&f, "Spotify");
        assert!(line.contains("voice out at take 10.00 = song 49.88"), "{line}");
        assert!(line.contains("music out at song 51.17"), "{line}");
        assert!(line.contains("Spotify 51.30"), "{line}");
        assert!(line.contains("audible gap +1.29 s (music ahead, by ref)"), "{line}");
        // without the kept music: the fingerprint, then the app's clock
        let r2 = Reading { music_ref: None, ..r.clone() };
        assert_eq!(r2.gap(&f).map(|g| g.1), Some("fp"));
        let r3 = Reading { music_ref: None, music_fp: Some((51.2, 0.1)), ..r.clone() };
        assert_eq!(r3.gap(&f).map(|g| g.1), Some("app"));
        assert!(r3.line(&f, "Spotify").contains("fp unsure"));
        // a voice that didn't match: no gap
        let r4 = Reading { voice_take: Some((10.0, 0.2)), ..r.clone() };
        assert!(r4.gap(&f).is_none());
        assert!(r4.line(&f, "Spotify").contains("voice unsure"));
        // the voice ahead
        let r5 = Reading { music_ref: Some((9.5, 0.8)), ..r };
        assert!(r5.line(&f, "Spotify").contains("audible gap -0.38 s (voice ahead, by ref)"));
    }

    #[test]
    fn the_summary_gives_each_measures_median_gap() {
        let f = Frame { song_offset: 0.0, latency: 0.0 };
        let rs: Vec<Reading> = (0..5).map(|i| Reading {
            voice_take: Some((10.0 + 2.0 * i as f64, 0.9)),
            music_ref: Some((10.0 + 2.0 * i as f64 + [0.30, 0.31, 0.29, 0.30, 0.90][i], 0.8)),
            app: Some(10.0 + 2.0 * i as f64 + 0.5),
            ..Default::default()
        }).collect();
        let s = summary(&rs, &f, "Spotify");
        assert!(s.contains("5 readings, voice found in 5"), "{s}");
        assert!(s.contains("gap by ref +0.30 s"), "{s}");
        assert_eq!(signed(-0.001), "+0.00");
        assert_eq!(signed(-0.35), "-0.35");
        assert!(s.contains("gap by Spotify +0.50 s"), "{s}");
        assert!(s.contains("heard: music 0.30 s ahead"), "{s}");
        assert_eq!(median_iqr(&[]), None);
        assert_eq!(median_iqr(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some((3.0, 2.0)));
    }

    /// A "song" with a rhythm: noise bursts at irregular times (so the fingerprint can place it), 16 kHz.
    fn beats(n: usize, seed: u64) -> Vec<f32> {
        let base = noise(n, seed);
        let mut s = seed;
        let mut out = vec![0f32; n];
        let mut i = 0;
        while i < n {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let len = 800 + (s >> 40) as usize % 2400;
            let gap = 1600 + (s >> 20) as usize % 6000;
            for k in i..(i + len).min(n) { out[k] = base[k] * (1.0 - (k - i) as f32 / len as f32); }
            i += len + gap;
        }
        out
    }

    #[test]
    fn the_tracker_measures_a_known_gap_from_both_outputs() {
        // the take: 60 s of singing, sung 30 s into the song, scored with 150 ms of latency; its music kept, lined
        // up 0.5 s later than the recording (ref[i + 8000] is take sample i)
        let f = Frame { song_offset: 30.0, latency: 0.15 };
        let take = noise(60 * 16_000, 11);
        let song = beats(120 * 16_000, 12);
        let reference: Vec<f32> = song[(29.5 * RATE) as usize..(90.0 * RATE) as usize].to_vec();
        let kept = (reference, 8_000i64, 1.0f32);
        let onsets = crate::dsp::onset_envelope(&song);
        // playing back: the voice from take 20 s on, the music 0.4 s ahead of where it should be (the voice at take
        // τ wants song τ + 29.85; the music comes out at song τ + 30.25)
        let mut tr = Tracker::default();
        let mut last = None;
        for step in 0..4 {
            let end = 20.0 + 2.0 * step as f64;         // take time at the windows' end
            let n = (FP_WIN_S * RATE) as usize;
            let v_end = (end * RATE) as usize;
            let voice: Vec<f32> = take[v_end - n..v_end].iter().map(|x| (x * 6.0).clamp(-0.4, 0.4)).collect();
            let m_end = ((end + 30.25) * RATE) as usize;
            let music: Vec<f32> = song[m_end - n..m_end].to_vec();
            let r = tr.measure(&f, &take, Some(&kept), &onsets, &music, &voice, 100_000 + step * 32_000, Some(end + 30.25 + 0.6), None, end);
            let (vt, vs) = r.voice_take.unwrap();
            assert!((vt - end).abs() < 1e-3 && vs > 0.9, "{r:?}");
            let (g, how) = r.gap(&f).unwrap();
            assert_eq!(how, "ref");
            assert!((g - 0.4).abs() < 0.002, "{g} {r:?}");
            let gaps = r.gaps(&f);
            let fp = gaps.iter().find(|x| x.0 == "fp").expect("a fingerprint gap").1;
            assert!((fp - 0.4).abs() <= 0.021, "fingerprint gap {fp}");
            assert!((gaps.iter().find(|x| x.0 == "app").unwrap().1 - 1.0).abs() < 1e-6);
            last = Some(r);
        }
        // the search followed it (a narrow search the second time on found the same)
        assert!(tr.last_voice.is_some() && tr.last_ref.is_some());
        let line = last.unwrap().line(&f, "Spotify");
        assert!(line.contains("audible gap +0.40 s (music ahead, by ref)"), "{line}");
        // silence from the window: no voice, and the search starts afresh
        let r = tr.measure(&f, &take, Some(&kept), &onsets, &song[..96_000], &[0.0; 96_000], 500_000, None, None, 30.0);
        assert!(r.voice_take.is_none() && tr.last_voice.is_none());
        assert!(r.line(&f, "Spotify").contains("no voice out (silent"));
    }

    #[test]
    fn a_kept_take_is_found_by_its_track_time_and_singing() {
        let dir = std::env::temp_dir().join(format!("onkey-ear-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mics = dir.join("mics");
        let write = |name: &str, track: &str, mic: &[f32], rf: &[f32]| {
            let d = mics.join(name);
            std::fs::create_dir_all(&d).unwrap();
            let raw = |x: &[f32]| x.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
            std::fs::write(d.join("mic16.f32"), raw(mic)).unwrap();
            std::fs::write(d.join("ref16.f32"), raw(rf)).unwrap();
            std::fs::write(d.join("meta.json"), serde_json::json!({"track": {"id": track}}).to_string()).unwrap();
        };
        // the take's recording: 20 s, singing in the middle; the kept mic starts 0.25 s before it
        let mut take = vec![0f32; 320_000];
        for (i, v) in noise(160_000, 6).iter().enumerate() { take[80_000 + i] = *v; }
        let mut kept = vec![0f32; 4_000];
        kept.extend_from_slice(&take);
        let music = noise(kept.len(), 7);
        write("20261026T140201-Some-Song", "spotify:track:x", &kept, &music);
        write("20261026T140230-Other-Song", "spotify:track:y", &kept, &music);       // another track
        write("20261026T120000-Some-Song", "spotify:track:x", &kept, &music);        // two hours before
        write("20261026T140215-Some-Song", "spotify:track:x", &noise(kept.len(), 8), &music);  // other singing
        let (rf, off, score) = find_ref(&mics, "spotify:track:x", "20261026-140201-Some-Song", &take).unwrap();
        assert_eq!(off, 4_000);
        assert!(score > 0.99);
        assert_eq!(rf.len(), music.len());
        assert!(find_ref(&mics, "spotify:track:z", "20261026-140201-Some-Song", &take).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
