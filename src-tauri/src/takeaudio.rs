//! Your voice in each take: takes/<time>-<song>/mic.m4a, AAC (mono, ~64 kbps, about 0.5 MB a minute), encoded
//! by macOS itself (native/audiofile.swift: Audio Toolbox's ExtAudioFile; no third-party encoder). Takes saved
//! before were 16-bit WAV (mic.wav, 5.8 MB a minute): they're converted once, in the background (`start`), and
//! every reader takes either (`recording`).
//!
//! Kept: every take's recording, under a safety cap of `AUDIO_CAP` for all of them together (`retain`): past it the
//! oldest takes' recordings go first, never a favourite song's or a song's best take (the scores stay in History).
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;

use crate::platform::native::*;

/// A take's recording, as saved now.
pub const M4A: &str = "mic.m4a";
/// A take's recording as saved before AAC (converted to `M4A` by `start`).
pub const WAV: &str = "mic.wav";
/// All the takes' recordings together stay under this (`retain`): ~75 hours of singing in AAC.
pub const AUDIO_CAP: u64 = 2 << 30;
/// A converted recording is kept only when it's this close to the WAV's length (seconds).
const SAME_LENGTH_S: f64 = 0.05;

/// The take's recording in `dir`, if it has one: the AAC, or a WAV not converted yet.
pub fn recording(dir: &Path) -> Option<PathBuf> {
    [M4A, WAV].iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

/// The type to serve a recording as.
pub fn mime(path: &Path) -> &'static str {
    if path.extension().is_some_and(|e| e == "m4a") { "audio/mp4" } else { "audio/wav" }
}

fn cpath(p: &Path) -> std::io::Result<CString> {
    CString::new(p.as_os_str().as_encoded_bytes()).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path with a NUL"))
}

fn core_audio(st: i32, what: &str) -> std::io::Result<()> {
    if st == 0 {
        return Ok(());
    }
    let code = st.to_be_bytes();
    let four = if code.iter().all(|b| b.is_ascii_graphic()) { format!(" '{}'", String::from_utf8_lossy(&code)) } else { String::new() };
    Err(std::io::Error::other(format!("{what}: Core Audio error {st}{four}")))
}

/// Written beside `path` under a temp name (as `store::write_atomic`), flushed, then renamed over it.
fn atomically(path: &Path, write: impl FnOnce(&Path) -> std::io::Result<()>) -> std::io::Result<()> {
    let tmp = crate::store::temp_beside(path)?;
    let done = (|| {
        write(&tmp)?;
        // (opened for writing: Windows refuses to flush a file opened only for reading, "Access is denied")
        std::fs::OpenOptions::new().write(true).open(&tmp)?.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}

/// A take's recording, in bits/s: small, and clear enough for a voice heard once.
pub const TAKE_BIT_RATE: u32 = 64_000;

/// 16-bit mono samples at `rate` to AAC at `path` (mono, about `bit_rate` bits/s; resampled to 48 kHz when AAC
/// doesn't take the rate), atomically.
pub fn write_m4a(path: &Path, samples: &[i16], rate: f64, bit_rate: u32) -> std::io::Result<()> {
    atomically(path, |tmp| {
        let c = cpath(tmp)?;
        core_audio(unsafe { onkey_aac_write(samples.as_ptr(), samples.len() as i64, rate, bit_rate, c.as_ptr()) }, "encoding AAC")
    })
}

/// What decoding a whole file found: its length, rate and channels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decoded {
    pub frames: i64,
    pub rate: f64,
    pub channels: i32,
}

impl Decoded {
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / self.rate
    }
}

/// Decodes all of `path` (so it's known to decode, not just to open): its length, rate and channels.
pub fn probe(path: &Path) -> std::io::Result<Decoded> {
    let c = cpath(path)?;
    let mut d = Decoded { frames: 0, rate: 0.0, channels: 0 };
    core_audio(unsafe { onkey_audio_decode(c.as_ptr(), std::ptr::null_mut(), 0, &mut d.frames, &mut d.rate, &mut d.channels) }, "decoding")?;
    Ok(d)
}

/// The audio of `path`, mixed to mono, and its rate.
pub fn decode_mono(path: &Path) -> std::io::Result<(Vec<f32>, f64)> {
    let d = probe(path)?;
    let mut out = vec![0f32; d.frames.max(0) as usize];
    let c = cpath(path)?;
    let mut e = d;
    core_audio(unsafe { onkey_audio_decode(c.as_ptr(), out.as_mut_ptr(), out.len() as i64, &mut e.frames, &mut e.rate, &mut e.channels) }, "decoding")?;
    out.truncate(e.frames.max(0) as usize);
    Ok((out, e.rate))
}

/// Is `m4a` a faithful copy of `wav`: it decodes to the end, mono, the same length within `SAME_LENGTH_S`.
fn verified(wav: &Path, m4a: &Path) -> Result<Decoded, String> {
    let w = probe(wav).map_err(|e| format!("the WAV doesn't decode ({e})"))?;
    let a = probe(m4a).map_err(|e| format!("the AAC doesn't decode ({e})"))?;
    if a.channels != 1 {
        return Err(format!("the AAC has {} channels", a.channels));
    }
    if w.frames > 0 && (a.rate <= 0.0 || (a.seconds() - w.seconds()).abs() > SAME_LENGTH_S) {
        return Err(format!("the AAC is {:.3} s, the WAV {:.3} s", a.seconds(), w.seconds()));
    }
    Ok(a)
}

/// What converting one take did.
#[derive(Debug, PartialEq)]
pub enum Converted {
    /// mic.wav (this many bytes) is now mic.m4a (this many)
    Done { wav: u64, m4a: u64 },
    /// nothing to convert (no mic.wav)
    Nothing,
}

/// A take's mic.wav to mic.m4a: encoded beside it under a temp name, checked (`verified`), renamed into place,
/// and only then the WAV deleted. Resumable at every step: a temp file left by a kill is just a temp file (swept
/// by `start`), and a take found with both (killed between the rename and the delete) has its AAC checked again
/// before the WAV goes; one that fails the check is encoded afresh. On any failure the WAV stays.
pub fn convert(dir: &Path) -> Result<Converted, String> {
    let (wav, m4a) = (dir.join(WAV), dir.join(M4A));
    let Ok(wav_len) = std::fs::metadata(&wav).map(|m| m.len()) else { return Ok(Converted::Nothing) };
    if !(m4a.is_file() && verified(&wav, &m4a).is_ok()) {
        let src = cpath(&wav).map_err(|e| e.to_string())?;
        atomically(&m4a, |tmp| {
            let dst = cpath(tmp)?;
            core_audio(unsafe { onkey_aac_convert(src.as_ptr(), dst.as_ptr()) }, "encoding AAC")?;
            verified(&wav, tmp).map(|_| ()).map_err(std::io::Error::other)
        }).map_err(|e| e.to_string())?;
    }
    std::fs::remove_file(&wav).map_err(|e| format!("couldn't delete the WAV: {e}"))?;
    Ok(Converted::Done { wav: wav_len, m4a: std::fs::metadata(&m4a).map(|m| m.len()).unwrap_or(0) })
}

/// Temp files left in a take's folder by a write that never finished: the store's sweep, so another OnKey (or a lab
/// tool) writing there now never has one taken from under it (only another run's, over ten minutes old).
fn sweep_temp(dir: &Path) {
    crate::store::sweep_temp_files(dir);
}

/// Every take folder, oldest first (their names start with the time).
fn take_dirs(data: &Path) -> Vec<PathBuf> {
    let mut d: Vec<PathBuf> = std::fs::read_dir(data.join("takes")).into_iter().flatten().flatten()
        .map(|e| e.path()).filter(|p| p.is_dir()).collect();
    d.sort();
    d
}

/// Set once this run's conversion has been through every take: until then `retain` would count WAVs about to
/// shrink twelvefold, so the daily clean-up leaves the recordings alone.
static CONVERTED: AtomicBool = AtomicBool::new(false);

pub fn converted() -> bool {
    CONVERTED.load(Ordering::SeqCst)
}

/// Wait at least this long after launch, and for this long with no take, no music and no song being learned,
/// before converting (checked again before each take).
const AFTER_LAUNCH: Duration = Duration::from_secs(120);
const QUIET_FOR: Duration = Duration::from_secs(60);

fn quiet() -> bool {
    crate::engine::quiet_for() >= QUIET_FOR && crate::learn::running() == 0
}

/// The one-time conversion of the takes saved as WAV, on its own thread at background priority (QoS background:
/// the efficiency cores, throttled I/O), one take at a time and only while OnKey is idle (`quiet`), newest first
/// (the ones most likely played back). Progress goes to the log. Then the recordings are kept under their cap.
pub fn start(data: PathBuf) {
    std::thread::Builder::new().name("onkey-take-audio".into()).spawn(move || {
        crate::platform::sys::background_thread();
        std::thread::sleep(AFTER_LAUNCH);
        let mut todo: Vec<PathBuf> = take_dirs(&data);
        for d in &todo {
            sweep_temp(d);
        }
        todo.retain(|d| d.join(WAV).is_file());
        todo.reverse();
        let total = todo.len();
        let log = crate::engine::log;
        if total > 0 {
            log(&format!("take audio: {total} recording{} saved as WAV to convert to AAC (in the background, while OnKey is idle)",
                         if total == 1 { "" } else { "s" }));
        }
        let (mut done, mut failed, mut wav_bytes, mut m4a_bytes) = (0usize, 0usize, 0u64, 0u64);
        let started = std::time::Instant::now();
        for (i, dir) in todo.iter().enumerate() {
            while !quiet() {
                std::thread::sleep(Duration::from_secs(15));
            }
            let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            match convert(dir) {
                Ok(Converted::Done { wav, m4a }) => {
                    done += 1;
                    wav_bytes += wav;
                    m4a_bytes += m4a;
                }
                Ok(Converted::Nothing) => {}
                Err(e) => {
                    failed += 1;
                    log(&format!("take audio: {name} not converted, its WAV kept: {e}"));
                }
            }
            if (i + 1) % 20 == 0 || i + 1 == total {
                log(&format!("take audio: {}/{total} converted to AAC ({:.0} MB of WAV now {:.0} MB){}", i + 1,
                             wav_bytes as f64 / 1e6, m4a_bytes as f64 / 1e6,
                             if failed > 0 { format!(", {failed} failed") } else { String::new() }));
            }
        }
        if total > 0 {
            log(&format!("take audio: conversion finished in {:.0} s: {done} converted, {failed} failed", started.elapsed().as_secs_f64()));
        }
        CONVERTED.store(true, Ordering::SeqCst);
        log_retained(&retain(&data, AUDIO_CAP));
    }).ok();
}

pub fn log_retained(r: &Retained) {
    if r.removed > 0 {
        crate::engine::log(&format!("take audio: over the {} GB cap: {} of the oldest takes' recordings removed ({:.0} MB freed; \
                                     favourites and each song's best take kept); recordings now {:.0} MB",
                                    AUDIO_CAP >> 30, r.removed, r.freed as f64 / 1e6, r.total as f64 / 1e6));
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Retained {
    /// takes whose recording was removed
    pub removed: usize,
    pub freed: u64,
    /// all the takes' recordings after
    pub total: u64,
}

/// Every take keeps its recording until all of them together pass `cap`; then the oldest takes' recordings go
/// until they're under it, except a favourite song's takes and each song's best take (the highest score of the
/// takes sung through at least 10%, as the Library shows; else its highest score), which are never removed. The
/// takes themselves (scores, details) stay in History.
pub fn retain(data: &Path, cap: u64) -> Retained {
    let size = |d: &Path| [M4A, WAV].iter().filter_map(|n| std::fs::metadata(d.join(n)).ok()).map(|m| m.len()).sum::<u64>();
    let takes: Vec<(PathBuf, u64)> = take_dirs(data).into_iter().map(|d| { let s = size(&d); (d, s) }).filter(|t| t.1 > 0).collect();
    let mut r = Retained { total: takes.iter().map(|t| t.1).sum(), ..Default::default() };
    if r.total <= cap {
        return r;
    }
    let store = crate::store::Store::new(data.to_path_buf());
    let result = |d: &Path| -> Option<Value> { serde_json::from_slice(&std::fs::read(d.join("result.json")).ok()?).ok() };
    // each take's song (as its heart is kept), whether it's a favourite, and its rank for "best"
    let mut fav: std::collections::HashMap<String, bool> = Default::default();
    let mut best: std::collections::HashMap<String, ((bool, i64), PathBuf)> = Default::default();
    let mut keep: std::collections::HashSet<PathBuf> = Default::default();
    for (d, _) in &takes {
        let Some(res) = result(d) else { continue };
        let s = |k: &str| res["track"][k].as_str().unwrap_or("").to_string();
        let track = crate::source::Track { id: s("id"), name: s("name"), artist: s("artist"), ..Default::default() };
        if track.id.is_empty() {
            continue;
        }
        let key = store.song_key(&track);
        if *fav.entry(key.clone()).or_insert_with(|| store.is_favorite(&track)) {
            keep.insert(d.clone());
        }
        let rank = (res["coverage"].as_f64().unwrap_or(0.0) >= 0.1, res["score"].as_i64().unwrap_or(i64::MIN));
        let e = best.entry(key).or_insert_with(|| (rank, d.clone()));
        if rank > e.0 {
            *e = (rank, d.clone());
        }
    }
    keep.extend(best.into_values().map(|(_, d)| d));
    for (d, len) in &takes {
        if r.total <= cap {
            break;
        }
        if keep.contains(d) {
            continue;
        }
        let gone: Vec<bool> = [M4A, WAV].iter().map(|n| d.join(n)).filter(|p| p.exists()).map(|p| std::fs::remove_file(p).is_ok()).collect();
        if gone.iter().all(|g| *g) {
            r.removed += 1;
            r.freed += len;
            r.total -= len;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("onkey-takeaudio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A voice-like test signal: a gliding tone with vibrato, words of 0.6 s and gaps.
    fn voice(rate: f64, seconds: f64) -> Vec<i16> {
        (0..(rate * seconds) as usize).map(|i| {
            let t = i as f64 / rate;
            let on = (t % 0.8) < 0.6;
            let f = 220.0 * (1.0 + 0.2 * (t * 0.3).sin()) + 4.0 * (t * 6.0 * std::f64::consts::TAU).sin();
            if on { (6000.0 * (std::f64::consts::TAU * f * t).sin() + 1500.0 * (std::f64::consts::TAU * 2.0 * f * t).sin()) as i16 } else { 0 }
        }).collect()
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "no AAC encoder on Linux yet")]
    fn a_take_encodes_to_mono_aac_of_the_same_length_that_decodes() {
        let d = tmp("roundtrip");
        // the rate it decodes at, and the bit rate it's at: Core Audio's AAC takes 8-48 kHz and 64 kbps; Microsoft's
        // takes 44.1 or 48 kHz only (the rest resampled to 48) and 96 kbps at the least
        #[cfg(not(windows))]
        let (aac_rate, bit_rate) = (|rate: f64| if rate > 48_000.0 { 48_000.0 } else { rate }, 64_000);
        #[cfg(windows)]
        let (aac_rate, bit_rate) = (crate::platform::windows::mf::aac_rate, 96_000);
        for rate in [48_000.0, 44_100.0, 16_000.0, 96_000.0] {
            let pcm = voice(rate, 20.0);
            let p = d.join(format!("mic-{rate}.m4a"));
            write_m4a(&p, &pcm, rate, TAKE_BIT_RATE).unwrap();
            let got = probe(&p).unwrap();
            assert_eq!(got.channels, 1, "{rate}");
            assert!((got.seconds() - 20.0).abs() < 0.05, "{rate}: {got:?}");
            let (x, r) = decode_mono(&p).unwrap();
            assert_eq!(r, aac_rate(rate));
            // the sound is the sound: same loudness (within 1 dB), and in step (silent where the input is)
            let src: Vec<f32> = pcm.iter().map(|s| *s as f32 / 32768.0).collect();
            let db = 20.0 * (rms(&x) / rms(&src)).log10();
            assert!(db.abs() < 1.0, "{rate}: {db:.2} dB");
            let gap = (0.62 * r) as usize..(0.78 * r) as usize;      // the first gap between words (0.6-0.8 s): AAC's 44 ms of priming would show
            assert!(rms(&x[gap]) < 0.01, "{rate}: sound in a gap (out of step?)");
            let bytes = std::fs::metadata(&p).unwrap().len();
            assert!(bytes < 20 * bit_rate / 8 * 13 / 10, "{rate}: {bytes} bytes for 20 s");
            assert!(std::fs::read_dir(&d).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with('.')), "no temp file left");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_that_isnt_audio_doesnt_decode() {
        let d = tmp("bad");
        std::fs::write(d.join(M4A), b"not audio at all").unwrap();
        assert!(probe(&d.join(M4A)).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "no AAC encoder on Linux yet")]
    fn a_wav_is_converted_checked_then_deleted_and_a_bad_one_is_kept() {
        let d = tmp("convert");
        let t = d.join("20261001-120000-Song");
        std::fs::create_dir_all(&t).unwrap();
        crate::engine::write_wav(&t.join(WAV), &voice(48_000.0, 12.0), 48_000).unwrap();
        let wav_len = std::fs::metadata(t.join(WAV)).unwrap().len();
        // a temp file left by a conversion that was killed
        std::fs::write(t.join(".mic.m4a.tmp-1-0"), b"half").unwrap();
        // one just written (another OnKey's, still converting) is left alone; one an hour old is swept
        std::fs::write(t.join(".mic.m4a.tmp-2-0"), b"busy").unwrap();
        std::fs::File::options().write(true).open(t.join(".mic.m4a.tmp-1-0")).unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600)).unwrap();
        sweep_temp(&t);
        assert!(!t.join(".mic.m4a.tmp-1-0").exists());
        assert!(t.join(".mic.m4a.tmp-2-0").exists());
        std::fs::remove_file(t.join(".mic.m4a.tmp-2-0")).unwrap();
        let Converted::Done { wav, m4a } = convert(&t).unwrap() else { panic!("not converted") };
        assert_eq!(wav, wav_len);
        // (a 48 kHz 16-bit WAV is 768 kbps: at least 8 times a 64 kbps AAC, 6 times Windows' 96 kbps one)
        let smaller = if cfg!(windows) { 6 } else { 8 };
        assert!(m4a > 0 && m4a * smaller < wav, "{m4a} vs {wav}");
        assert!(!t.join(WAV).exists() && t.join(M4A).exists());
        assert!((probe(&t.join(M4A)).unwrap().seconds() - 12.0).abs() < 0.05);
        assert_eq!(recording(&t), Some(t.join(M4A)));
        assert_eq!(convert(&t).unwrap(), Converted::Nothing, "done once");

        // killed between the rename and the delete: both there; the AAC is checked again, then the WAV goes
        crate::engine::write_wav(&t.join(WAV), &voice(48_000.0, 12.0), 48_000).unwrap();
        assert!(matches!(convert(&t).unwrap(), Converted::Done { .. }));
        assert!(!t.join(WAV).exists());

        // an AAC that doesn't match its WAV (another length) is made again, not trusted
        crate::engine::write_wav(&t.join(WAV), &voice(48_000.0, 5.0), 48_000).unwrap();
        assert!(matches!(convert(&t).unwrap(), Converted::Done { .. }));
        assert!((probe(&t.join(M4A)).unwrap().seconds() - 5.0).abs() < 0.05);

        // a WAV that doesn't decode: an error, and the WAV stays (nothing else left behind)
        let b = d.join("20261002-120000-Broken");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join(WAV), b"RIFF....WAVEfmt nonsense").unwrap();
        assert!(convert(&b).is_err());
        assert!(b.join(WAV).exists() && !b.join(M4A).exists());
        assert_eq!(std::fs::read_dir(&b).unwrap().count(), 1);
        assert_eq!(recording(&b), Some(b.join(WAV)), "an unconverted WAV is still played");
        assert_eq!(mime(&b.join(WAV)), "audio/wav");
        assert_eq!(mime(&t.join(M4A)), "audio/mp4");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "no AAC encoder on Linux yet")]
    fn a_wrong_length_aac_is_not_verified() {
        let d = tmp("verify");
        crate::engine::write_wav(&d.join(WAV), &voice(48_000.0, 3.0), 48_000).unwrap();
        write_m4a(&d.join("long.m4a"), &voice(48_000.0, 3.1), 48_000.0, TAKE_BIT_RATE).unwrap();
        write_m4a(&d.join("same.m4a"), &voice(48_000.0, 3.0), 48_000.0, TAKE_BIT_RATE).unwrap();
        assert!(verified(&d.join(WAV), &d.join("long.m4a")).is_err());
        assert!(verified(&d.join(WAV), &d.join("same.m4a")).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A take of `song` at day `day` with `score`, its recording `kb` KB.
    fn take(d: &Path, day: u32, song: &str, score: i64, kb: usize) -> PathBuf {
        let t = d.join(format!("takes/202610{day:02}-120000-{song}"));
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join(M4A), vec![0u8; kb * 1000]).unwrap();
        std::fs::write(t.join("result.json"), json!({"track": {"id": format!("isrc:{song}"), "name": song, "artist": "A"},
            "score": score, "coverage": 0.9}).to_string()).unwrap();
        t
    }

    #[test]
    fn recordings_are_all_kept_under_the_cap_then_the_oldest_go_but_never_favourites_or_bests() {
        let d = tmp("retain");
        // under the cap: everything kept
        for day in 1..=10 {
            take(&d, day, "Plain", day as i64, 100);
        }
        assert_eq!(retain(&d, 2_000_000), Retained { removed: 0, freed: 0, total: 1_000_000 });
        // a favourite song (oldest takes), and Plain's best take is its oldest one
        take(&d, 1, "Plain", 99, 100);
        std::fs::rename(d.join("takes/20261001-120000-Plain"), d.join("takes/20260901-120000-Plain")).unwrap();
        for day in 11..=13 {
            take(&d, day - 10, "Loved", 10, 100);
        }
        std::fs::create_dir_all(d.join("songs")).unwrap();
        std::fs::write(d.join("songs/isrc_Loved.json"), json!({"track": {"id": "isrc:Loved", "name": "Loved", "artist": "A"}, "favorite": true}).to_string()).unwrap();
        let store = crate::store::Store::new(d.clone());
        assert!(store.is_favorite(&crate::source::Track { id: "isrc:Loved".into(), name: "Loved".into(), artist: "A".into(), ..Default::default() }),
                "the test's favourite is one");
        // 13 recordings of 100 KB, a cap of 1 MB: three of Plain's go, the oldest of them, not its best or Loved's
        let r = retain(&d, 1_000_000);
        assert_eq!(r.removed, 3, "{r:?}");
        assert!(r.total <= 1_000_000);
        let has = |name: &str| d.join(format!("takes/{name}")).join(M4A).exists();
        assert!(has("20260901-120000-Plain"), "the song's best take stays");
        for day in 1..=3 {
            assert!(has(&format!("202610{day:02}-120000-Loved")), "favourites stay");
        }
        for day in 2..=4 {
            assert!(!has(&format!("202610{day:02}-120000-Plain")), "day {day}: the oldest go first");
            assert!(d.join(format!("takes/202610{day:02}-120000-Plain/result.json")).exists(), "the take stays in History");
        }
        assert!(has("20261005-120000-Plain"));
        // a cap nothing can meet: only unprotected recordings go, the rest stay
        let r = retain(&d, 0);
        assert_eq!(r.removed, 6);
        assert_eq!(r.total, 400_000, "Loved's three and Plain's best");
        let _ = std::fs::remove_dir_all(&d);
    }
}
