//! The flight recorder: enough kept on this Mac that most reports ("Bad Day felt late", "it froze at Starting
//! OnKey", "Shazam took a minute", "90% CPU while singing") can be diagnosed from the records alone, without
//! playing the song again. Everything stays in OnKey's data folder, under records/:
//!   records/logs/onkey-<day>.log   the app's log, one file per day (the live one is still $TMPDIR/onkey.log)
//!   records/takes/<time>-<song>.json   one summary per play (~10-50 KB): which build, which Mac, how the song was
//!                                  followed, every placement decision, the lyrics shown (their entry check, pinned
//!                                  or not, their times) and how far each line was from where the singer was heard,
//!                                  recognition and Shazam requests, macOS permission calls and Apple events with
//!                                  their durations, capture restarts, the learner's stages, and once a second CPU,
//!                                  memory, the engine loop's longest pause, live-pitch lag and the window's frames
//!   records/builds.json            the builds that have run on this Mac (an older one than before is warned about)
//! Numbers and ids only: never lyrics text, never audio (song titles and artists, which the log has too, find a
//! play). Kept 30 days, at most `RECORDS_CAP` in all (see `sweep`); take recordings (takes/*/mic.m4a) are all kept,
//! under a cap for them together (takeaudio::retain, run by the same daily clean-up).
//!
//! Cheap by design: a note is a push under a lock nobody holds for long, the sampler wakes once a second, and a
//! summary is written once per play, after it's scored.
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SHA: &str = match option_env!("ONKEY_GIT_SHA") { Some(s) => s, None => "unknown" };
const COMMIT_TIME: &str = match option_env!("ONKEY_COMMIT_TIME") { Some(s) => s, None => "0" };

/// Logs and take summaries are kept this long.
const KEEP_DAYS: u64 = 30;
/// All of records/ together stays under this (the oldest files go first).
pub const RECORDS_CAP: u64 = 200 << 20;
/// A day's log stops growing past this (a runaway loop logging can't fill the disk).
const DAY_LOG_CAP: u64 = 20 << 20;
/// Events kept in memory (the last hour or so), and perf samples (two hours at one a second).
const EVENTS_MAX: usize = 20_000;
const PERF_MAX: usize = 7_200;
/// A summary keeps at most this many events of one kind (the first and last halves).
const KIND_MAX: usize = 300;
/// A take's summary includes the global events (recognition, Apple events…) from this long before it started.
const PRE_S: f64 = 30.0;

static DIR: OnceLock<PathBuf> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

/// Seconds since OnKey started (the records' clock).
pub fn now() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------- which build, which Mac

/// A fact about this computer by its sysctl name (the platform's: the registry on Windows).
fn sysctl_str(name: &str) -> Option<String> {
    crate::platform::sys::machine(name)
}

/// This build: version, commit and when that commit was made.
pub fn build() -> Value {
    json!({"version": VERSION, "sha": SHA, "commit_time": COMMIT_TIME.parse::<u64>().unwrap_or(0)})
}

/// This Mac: model, chip, memory and macOS version.
pub fn mac() -> Value {
    static MAC: OnceLock<Value> = OnceLock::new();
    MAC.get_or_init(|| json!({
        "model": sysctl_str("hw.model"), "chip": sysctl_str("machdep.cpu.brand_string"),
        "ram_gb": crate::lowmem::physical_memory().map(|b| (b as f64 / (1u64 << 30) as f64).round()),
        "macos": sysctl_str("kern.osproductversion"), "cores": std::thread::available_parallelism().map(|n| n.get()).ok(),
        "low_memory_mode": crate::lowmem::low_memory()})).clone()
}

/// "0.1.0 (sha, 2026-10-16 12:00)".
fn build_words(b: &Value) -> String {
    let t = b["commit_time"].as_u64().unwrap_or(0);
    let when = if t > 0 { crate::store::format_time(SystemTime::UNIX_EPOCH + Duration::from_secs(t), "%Y-%m-%d %H:%M") } else { "?".into() };
    format!("{} ({}, committed {when})", b["version"].as_str().unwrap_or("?"), b["sha"].as_str().unwrap_or("?"))
}

/// Is build `a` older than build `b` (by version, then by when its commit was made)?
pub fn older(a: &Value, b: &Value) -> bool {
    let v = |x: &Value| x["version"].as_str().unwrap_or("0").split(['.', '-']).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (va, vb) = (v(a), v(b));
    if va != vb {
        return va < vb;
    }
    let (ta, tb) = (a["commit_time"].as_u64().unwrap_or(0), b["commit_time"].as_u64().unwrap_or(0));
    ta > 0 && tb > 0 && ta < tb
}

/// The builds seen on this Mac (records/builds.json): the newest, and whether this one is older than it. Returns a
/// warning when it is.
fn note_build(dir: &Path) -> Option<String> {
    let path = dir.join("builds.json");
    let mut seen: Value = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| json!({}));
    let me = build();
    let newest = seen["newest"].clone();
    let warn = (!newest.is_null() && older(&me, &newest)).then(|| format!(
        "WARNING: this is an older build than one that ran on this Mac before: {} is running, {} ran on {}",
        build_words(&me), build_words(&newest), seen["newest_seen"].as_str().unwrap_or("?")));
    if newest.is_null() || older(&newest, &me) || newest["sha"] == me["sha"] {
        seen["newest"] = me.clone();
        seen["newest_seen"] = json!(crate::store::now_iso());
    }
    let mut runs: Vec<Value> = seen["runs"].as_array().cloned().unwrap_or_default();
    runs.retain(|r| r["sha"] != me["sha"]);
    runs.push(json!({"version": me["version"], "sha": me["sha"], "commit_time": me["commit_time"], "last_run": crate::store::now_iso()}));
    let n = runs.len();
    seen["runs"] = json!(runs.split_off(n.saturating_sub(20)));
    let _ = crate::store::write_atomic(&path, seen.to_string().as_bytes());
    warn
}

/// At launch: the records folder, the build and Mac at the top of this log session (and a warning if this build
/// is older than one that ran here before), the once-a-second sampler and the daily clean-up.
pub fn init(data: &Path) {
    let dir = set_dir(data);
    let m = mac();
    // (on Windows "macos" holds "Windows 11 24H2 (build …)": see sysctl_str)
    let os = crate::platform::sys::OS_NAME;
    crate::engine::log(&format!("==== OnKey {} on {} ({}), {} GB, {os}{}{}", build_words(&build()),
        m["model"].as_str().unwrap_or("?"), m["chip"].as_str().unwrap_or("?"), m["ram_gb"], m["macos"].as_str().unwrap_or("?"),
        if crate::lowmem::low_memory() { ", low-memory mode" } else { "" }));
    if let Some(w) = note_build(&dir) {
        crate::engine::log(&w);
    }
    std::thread::Builder::new().name("onkey-records".into()).spawn(sampler).ok();
    let data = data.to_path_buf();
    std::thread::Builder::new().name("onkey-records-sweep".into()).spawn(move || {
        std::thread::sleep(Duration::from_secs(60));      // not while OnKey is starting
        loop {
            let s = sweep(&data, SystemTime::now());
            if s.removed_files > 0 {
                crate::engine::log(&format!("records: kept tidy: {} old log/summary files removed ({:.0} MB freed); records now {:.1} MB",
                                            s.removed_files, s.freed as f64 / 1e6, s.records_bytes as f64 / 1e6));
            }
            // the takes' recordings under their cap (not while the WAVs from before AAC are still being converted:
            // the conversion does it when it's through)
            if crate::takeaudio::converted() {
                crate::takeaudio::log_retained(&crate::takeaudio::retain(&data, crate::takeaudio::AUDIO_CAP));
            }
            std::thread::sleep(Duration::from_secs(24 * 3600));
        }
    }).ok();
}

/// Where the records go (<data>/records), with no sampler, clean-up or log header: what `init` starts with, and
/// all a lab tool needs to write summaries.
pub fn set_dir(data: &Path) -> PathBuf {
    START.get_or_init(Instant::now);
    let dir = data.join("records");
    let _ = std::fs::create_dir_all(dir.join("takes"));
    let _ = std::fs::create_dir_all(dir.join("logs"));
    let _ = DIR.set(dir.clone());
    dir
}

/// The records folder (None before `init`, and in tests and lab tools).
pub fn dir() -> Option<&'static Path> {
    DIR.get().map(|p| p.as_path())
}

/// A log line, also into today's file under records/logs (30 days of them are kept).
pub fn log_line(line: &str) {
    let Some(dir) = DIR.get() else { return };
    use std::io::Write;
    let day = line.get(..10).filter(|d| d.len() == 10 && d.as_bytes()[4] == b'-').map(|d| d.replace('-', "")).unwrap_or_else(|| "unknown".into());
    let path = dir.join("logs").join(format!("onkey-{day}.log"));
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= DAY_LOG_CAP) {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

// ---------------------------------------------------------------- events

struct Ev {
    t: f64,
    take: u64,
    kind: &'static str,
    data: Value,
}

static EVENTS: Mutex<VecDeque<Ev>> = Mutex::new(VecDeque::new());

/// Something that happened, not tied to one take (recognition, an Apple event, a capture restart…): a take's
/// summary takes those from its time. `data` is numbers and ids only.
pub fn note(kind: &'static str, data: Value) {
    note_for(0, kind, data);
}

/// Something that happened to take `take` (the engine's take version): placement, its lyrics.
pub fn note_for(take: u64, kind: &'static str, data: Value) {
    let ev = Ev { t: now(), take, kind, data };
    let mut e = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
    if e.len() >= EVENTS_MAX {
        e.pop_front();
    }
    e.push_back(ev);
}

/// Run `f`, noting how long it took as `kind` ("perm": a macOS permission call), and in the log when it's slow.
pub fn timed<T>(kind: &'static str, what: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    note(kind, json!({"what": what, "ms": r1(ms)}));
    if ms > 1000.0 {
        crate::engine::log(&format!("slow macOS call: {what} took {:.1} s", ms / 1000.0));
    }
    out
}

/// One Apple event (osascript) to `app`: how long, and whether it got through ("ok", "denied", "error").
pub fn apple_event(app: &str, ms: f64, outcome: &str) {
    note("ae", json!({"app": app, "ms": r1(ms), "outcome": outcome}));
    if ms > 2000.0 {
        crate::engine::log(&format!("slow Apple event to {app}: {:.1} s ({outcome})", ms / 1000.0));
    }
}

/// Level of mono audio in dBFS (RMS), for "was the audio sent silent?".
pub fn rms_db(x: &[f32]) -> f64 {
    if x.is_empty() {
        return -120.0;
    }
    let ms = x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64;
    r1((10.0 * ms.max(1e-12).log10()).max(-120.0))
}

// ---------------------------------------------------------------- performance, once a second

struct Sample {
    t: f64,
    cpu: f32,
    mb: f32,
    loop_ms: f32,
    pitch_ms: f32,
}

static PERF: Mutex<VecDeque<Sample>> = Mutex::new(VecDeque::new());
/// the engine loop's longest iteration since the last sample (µs)
static LOOP_MAX_US: AtomicU64 = AtomicU64::new(0);
/// live pitch: how far the pitch tracked lags the audio taken in (ms, f32 bits; NaN: no take)
static PITCH_LAG: AtomicU32 = AtomicU32::new(0x7fc0_0000);
static PITCH_AT: AtomicU64 = AtomicU64::new(0);

/// The engine loop went round once, taking `d` (its sleep included).
pub fn engine_tick(d: Duration) {
    LOOP_MAX_US.fetch_max(d.as_micros() as u64, Relaxed);
}

/// The take playing: its pitch is tracked this far behind the audio it has (ms).
pub fn pitch_lag(ms: f64) {
    PITCH_LAG.store((ms as f32).to_bits(), Relaxed);
    PITCH_AT.store((now() * 1000.0) as u64, Relaxed);
}

fn cpu_seconds() -> f64 {
    crate::platform::sys::cpu_seconds()
}

/// OnKey's memory as Activity Monitor shows it (the physical footprint; Windows: private memory, as Task Manager
/// shows it), MB.
pub fn footprint_mb() -> f64 {
    crate::platform::sys::footprint_mb()
}

fn sampler() {
    let (mut last_cpu, mut last_t) = (cpu_seconds(), Instant::now());
    let mut minute: Vec<f32> = Vec::new();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let (cpu, t) = (cpu_seconds(), Instant::now());
        let pct = 100.0 * (cpu - last_cpu) / t.duration_since(last_t).as_secs_f64().max(1e-3);
        (last_cpu, last_t) = (cpu, t);
        let loop_ms = LOOP_MAX_US.swap(0, Relaxed) as f32 / 1000.0;
        let fresh = (now() * 1000.0) as u64 - PITCH_AT.load(Relaxed) < 2000;
        let pitch_ms = if fresh { f32::from_bits(PITCH_LAG.load(Relaxed)) } else { f32::NAN };
        let s = Sample { t: now(), cpu: pct as f32, mb: footprint_mb() as f32, loop_ms, pitch_ms };
        // a line a minute in the log while a take plays (or while OnKey is busy): enough to see a slow-down there too
        crate::capture::note_mic_channel();
        minute.push(s.cpu);
        if minute.len() >= 60 {
            let busy = minute.iter().cloned().fold(0f32, f32::max) > 50.0;
            if fresh || busy {
                let mut m = minute.clone();
                m.sort_by(|a, b| a.total_cmp(b));
                crate::engine::log(&format!("perf: CPU {:.0}% median, {:.0}% max over the last minute; {:.0} MB", m[m.len() / 2], m[m.len() - 1], s.mb));
            }
            minute.clear();
        }
        let mut p = PERF.lock().unwrap_or_else(|e| e.into_inner());
        if p.len() >= PERF_MAX {
            p.pop_front();
        }
        p.push_back(s);
    }
}

/// The window's own frame stats (sent every few seconds while the lane draws): numbers only are kept.
pub fn window_frames(report: &Value) {
    let Some(o) = report.as_object() else { return };
    let clean: serde_json::Map<String, Value> = o.iter().filter(|(k, v)| v.is_number() && k.len() <= 24)
        .map(|(k, v)| (k.clone(), v.clone())).take(16).collect();
    note("frames", Value::Object(clean));
}

// ---------------------------------------------------------------- takes

struct Open {
    version: u64,
    start_t: f64,
    end_t: Option<f64>,
    started: SystemTime,
    /// the song's ids (it can be kept under another: its ISRC)
    tracks: Vec<String>,
    info: serde_json::Map<String, Value>,
}

static OPEN: Mutex<Vec<Open>> = Mutex::new(Vec::new());

/// A take started (the engine's version for it): what it is and how it was placed, as an object.
pub fn take_begin(version: u64, track_id: &str, info: Value) {
    let mut o = OPEN.lock().unwrap_or_else(|e| e.into_inner());
    // takes never finished (quitting) aren't kept for ever
    if o.len() > 8 {
        o.remove(0);
    }
    o.push(Open { version, start_t: now(), end_t: None, started: SystemTime::now(), tracks: vec![track_id.to_string()],
                  info: info.as_object().cloned().unwrap_or_default() });
}

/// More about take `version` (set, or merged into an object already there).
pub fn take_set(version: u64, key: &str, value: Value) {
    if let Some(o) = OPEN.lock().unwrap_or_else(|e| e.into_inner()).iter_mut().find(|o| o.version == version) {
        match (o.info.get_mut(key), value) {
            (Some(Value::Object(had)), Value::Object(new)) => had.extend(new),
            (_, v) => { o.info.insert(key.to_string(), v); }
        }
    }
}

/// The song turned out to be kept under another id too.
pub fn take_track(version: u64, id: &str) {
    if let Some(o) = OPEN.lock().unwrap_or_else(|e| e.into_inner()).iter_mut().find(|o| o.version == version) {
        if !o.tracks.iter().any(|t| t == id) {
            o.tracks.push(id.to_string());
        }
    }
}

/// The take's play is over (it may still be learned and scored).
pub fn take_end(version: u64) {
    if let Some(o) = OPEN.lock().unwrap_or_else(|e| e.into_inner()).iter_mut().find(|o| o.version == version) {
        o.end_t.get_or_insert_with(now);
    }
}

/// The take is done with (scored, failed or dropped): its summary is written. `outcome` says how it ended. The
/// file's path, if written.
pub fn take_finish(version: u64, outcome: Value, store: Option<&crate::store::Store>) -> Option<PathBuf> {
    let open = {
        let mut o = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        let i = o.iter().position(|o| o.version == version)?;
        o.remove(i)
    };
    let words = store.and_then(|s| open.tracks.iter().find_map(|id| s.words(id)));
    let mut sum = summary(&open, outcome, words.as_ref());
    let dir = DIR.get()?.join("takes");
    let slug: String = open.info.get("track").and_then(|t| t["name"].as_str()).unwrap_or("take").chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>()
        .split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
    let mut name = format!("{}-{}", crate::store::format_time(open.started, "%Y%m%d-%H%M%S"), &slug[..slug.len().min(40)]);
    if dir.join(format!("{name}.json")).exists() {
        name = format!("{name}-{version}");
    }
    sum["id"] = json!(name);
    let path = dir.join(format!("{name}.json"));
    crate::store::write_atomic(&path, sum.to_string().as_bytes()).ok()?;
    Some(path)
}

/// Percentile of `v` (sorted in place), `p` in 0..1.
fn pct(v: &mut [f64], p: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[((v.len() - 1) as f64 * p).round() as usize])
}

/// The heard words kept with a song (store's .words): the first word heard in each line, by line (song s),
/// if they were heard on lyrics checked as `check`.
pub fn heard_line_starts(words: &Value, check: &str) -> Option<Vec<Option<f64>>> {
    if words["check"].as_str() != Some(check) {
        return None;
    }
    let n = words["lines"].as_u64()? as usize;
    let mut out = vec![None; n];
    for w in words["words"].as_array()? {
        let (Some(line), Some(t)) = (w[0].as_u64(), w[2].as_f64()) else { continue };
        if let Some(o) = out.get_mut(line as usize) {
            *o = Some(o.map_or(t, |x: f64| x.min(t)));
        }
    }
    Some(out)
}

/// Each lyric line as shown (`times`) against where the singer was heard starting it: shown minus heard (s;
/// positive: the line showed late), and the summary of them.
pub fn line_offsets(times: &[f64], heard: &[Option<f64>]) -> Value {
    let offs: Vec<Option<f64>> = times.iter().zip(heard.iter().chain(std::iter::repeat(&None))).map(|(s, h)| h.map(|h| r2(s - h))).collect();
    let mut all: Vec<f64> = offs.iter().flatten().copied().collect();
    let mut abs: Vec<f64> = all.iter().map(|x| x.abs()).collect();
    json!({"by_line": offs, "lines_heard": all.len(), "median": pct(&mut all, 0.5),
           "abs_p90": pct(&mut abs, 0.9), "over_0_5s": abs.iter().filter(|x| **x > 0.5).count()})
}

fn summary(o: &Open, outcome: Value, words: Option<&Value>) -> Value {
    let end = o.end_t.unwrap_or_else(now);
    let rel = |t: f64| r2(t - o.start_t);
    let mut kinds: Vec<(&'static str, Vec<Value>)> = Vec::new();
    {
        let evs = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
        for e in evs.iter() {
            let of_track = e.data["track"].as_str().is_some_and(|t| o.tracks.iter().any(|x| x == t)) && e.t >= o.start_t - PRE_S;
            let mine = e.take == o.version || of_track || (e.take == 0 && e.data["track"].is_null() && e.t >= o.start_t - PRE_S && e.t <= end);
            if !mine {
                continue;
            }
            let mut d = e.data.clone();
            if let Some(m) = d.as_object_mut() {
                m.insert("t".into(), json!(rel(e.t)));
                m.remove("track");
            }
            match kinds.iter_mut().find(|k| k.0 == e.kind) {
                Some(k) => k.1.push(d),
                None => kinds.push((e.kind, vec![d])),
            }
        }
    }
    let mut sum = json!({"v": 1, "build": build(), "mac": mac(), "started": crate::store::chrono_like(o.started),
                         "seconds": r1(end - o.start_t), "outcome": outcome});
    for (k, v) in &o.info {
        sum[k] = v.clone();
    }
    // Apple events: per app (count, how long), and each slow one
    let mut ae = json!({});
    if let Some(i) = kinds.iter().position(|k| k.0 == "ae") {
        let list = kinds.remove(i).1;
        let mut apps: Vec<(String, Vec<f64>, usize)> = Vec::new();
        for e in &list {
            let app = e["app"].as_str().unwrap_or("?").to_string();
            let i = apps.iter().position(|a| a.0 == app).unwrap_or_else(|| { apps.push((app, Vec::new(), 0)); apps.len() - 1 });
            apps[i].1.push(e["ms"].as_f64().unwrap_or(0.0));
            if e["outcome"] != "ok" {
                apps[i].2 += 1;
            }
        }
        for (app, mut ms, failed) in apps {
            let total: f64 = ms.iter().sum();
            ae[app] = json!({"n": ms.len(), "failed": failed, "total_ms": r1(total), "p50_ms": pct(&mut ms, 0.5),
                             "p95_ms": pct(&mut ms, 0.95), "max_ms": pct(&mut ms, 1.0)});
        }
        ae["slow"] = json!(list.into_iter().filter(|e| e["ms"].as_f64().unwrap_or(0.0) > 300.0).take(50).collect::<Vec<_>>());
    }
    sum["apple_events"] = ae;
    // the learner's stages, with how long each took
    if let Some(i) = kinds.iter().position(|k| k.0 == "stage") {
        let list = kinds.remove(i).1;
        let st: Vec<Value> = list.iter().enumerate().map(|(j, e)| {
            let next = list.get(j + 1).and_then(|n| n["t"].as_f64());
            json!({"stage": e["stage"], "t": e["t"], "s": next.map(|n| r1(n - e["t"].as_f64().unwrap_or(0.0)))})
        }).collect();
        sum["learner"] = json!(st);
    }
    for (k, mut list) in kinds {
        let dropped = list.len().saturating_sub(KIND_MAX);
        if dropped > 0 {
            list.drain(KIND_MAX / 2..KIND_MAX / 2 + dropped);
            sum[format!("{k}_dropped")] = json!(dropped);
        }
        sum[k] = json!(list);
    }
    // once a second: CPU (% of one core), footprint (MB), the engine loop's longest pause (ms), live-pitch lag (ms)
    {
        let p = PERF.lock().unwrap_or_else(|e| e.into_inner());
        let s: Vec<&Sample> = p.iter().filter(|s| s.t >= o.start_t && s.t <= end + 1.0).collect();
        let col = |f: &dyn Fn(&Sample) -> f32| s.iter().map(|x| { let v = f(x); if v.is_finite() { json!(r1(v as f64)) } else { Value::Null } }).collect::<Vec<_>>();
        let stat = |f: &dyn Fn(&Sample) -> f32| {
            let mut v: Vec<f64> = s.iter().map(|x| f(x) as f64).filter(|v| v.is_finite()).collect();
            json!({"p50": pct(&mut v, 0.5).map(r1), "p95": pct(&mut v, 0.95).map(r1), "max": pct(&mut v, 1.0).map(r1)})
        };
        sum["perf"] = json!({"t0": s.first().map(|x| rel(x.t)), "every_s": 1,
            "cpu": col(&|x| x.cpu), "mb": col(&|x| x.mb), "loop_ms": col(&|x| x.loop_ms), "pitch_lag_ms": col(&|x| x.pitch_ms),
            "stats": {"cpu": stat(&|x| x.cpu), "mb": stat(&|x| x.mb), "loop_ms": stat(&|x| x.loop_ms), "pitch_lag_ms": stat(&|x| x.pitch_ms)}});
    }
    // each line as shown against where the singer was heard singing it (when the song's heard words are kept and
    // were heard on the same lyrics)
    if let (Some(w), Some(check), Some(times)) = (words, sum["lyrics"]["check"].as_str(), sum["lyrics"]["times"].as_array()) {
        let times: Vec<f64> = times.iter().filter_map(|t| t.as_f64()).collect();
        if let Some(h) = heard_line_starts(w, check) {
            sum["line_offsets"] = line_offsets(&times, &h);
        }
    }
    sum
}

// ---------------------------------------------------------------- keeping it tidy

#[derive(Default, Debug)]
pub struct Swept {
    pub removed_files: usize,
    pub freed: u64,
    pub records_bytes: u64,
}

fn files(dir: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| {
        let m = e.metadata().ok().filter(|m| m.is_file())?;
        Some((e.path(), m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
    }).collect()
}

/// Logs and summaries older than 30 days go, then the oldest until records/ is under `RECORDS_CAP`. Nothing else is
/// touched: take recordings (takeaudio::retain), learned songs, lyrics and the models are never removed here.
pub fn sweep(data: &Path, now: SystemTime) -> Swept {
    sweep_capped(data, now, RECORDS_CAP)
}

fn sweep_capped(data: &Path, now: SystemTime, cap: u64) -> Swept {
    let mut s = Swept::default();
    let rec = data.join("records");
    let age = |t: SystemTime| now.duration_since(t).unwrap_or_default();
    let mut all: Vec<(PathBuf, u64, SystemTime)> = ["logs", "takes"].iter().flat_map(|d| files(&rec.join(d))).collect();
    all.retain(|(p, len, t)| {
        if age(*t) > Duration::from_secs(KEEP_DAYS * 86_400) && std::fs::remove_file(p).is_ok() {
            s.removed_files += 1;
            s.freed += len;
            return false;
        }
        true
    });
    all.sort_by_key(|f| f.2);
    let mut total: u64 = all.iter().map(|f| f.1).sum();
    // over the cap: the oldest go until it's three quarters full (so it isn't trimmed again at once)
    if total > cap {
        for (p, len, _) in &all {
            if total <= cap * 3 / 4 {
                break;
            }
            if std::fs::remove_file(p).is_ok() {
                s.removed_files += 1;
                s.freed += len;
                total -= len;
            }
        }
    }
    s.records_bytes = total;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("onkey-records-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn set_age(p: &Path, days: u64) {
        let t = SystemTime::now() - Duration::from_secs(days * 86_400);
        let f = std::fs::File::options().write(true).open(p).unwrap();
        f.set_modified(t).unwrap();
    }

    #[test]
    fn builds_are_ordered_by_version_then_commit_time() {
        let b = |v: &str, t: u64| json!({"version": v, "sha": "x", "commit_time": t});
        assert!(older(&b("0.1.0", 100), &b("0.2.0", 50)));
        assert!(older(&b("0.1.0", 100), &b("0.1.0", 200)));
        assert!(!older(&b("0.1.0", 200), &b("0.1.0", 100)));
        assert!(!older(&b("0.10.0", 0), &b("0.9.9", 0)));
        assert!(!older(&b("0.1.0", 0), &b("0.1.0", 100)), "an unknown commit time is never called older");
    }

    #[test]
    fn an_older_build_than_one_seen_before_is_warned_about() {
        let d = tmp("builds");
        std::fs::write(d.join("builds.json"), json!({"newest": {"version": "99.0.0", "sha": "abc", "commit_time": 1}}).to_string()).unwrap();
        assert!(note_build(&d).unwrap().contains("older build"));
        let _ = std::fs::remove_file(d.join("builds.json"));
        assert!(note_build(&d).is_none());
        assert!(note_build(&d).is_none(), "the same build again is fine");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn old_records_go_and_take_recordings_are_left_alone() {
        let d = tmp("sweep");
        let logs = d.join("records/logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("onkey-20260901.log"), "old").unwrap();
        set_age(&logs.join("onkey-20260901.log"), 40);
        std::fs::write(logs.join("onkey-20261015.log"), "new").unwrap();
        // 30 takes, all old: every recording stays (takeaudio::retain keeps them under their cap)
        for i in 0..30 {
            let t = d.join(format!("takes/202610{:02}-120000-Song", i + 1));
            std::fs::create_dir_all(&t).unwrap();
            std::fs::write(t.join("mic.m4a"), vec![0u8; 1000]).unwrap();
            std::fs::write(t.join("result.json"), "{}").unwrap();
            set_age(&t.join("mic.m4a"), 60);
        }
        let s = sweep(&d, SystemTime::now());
        assert_eq!(s.removed_files, 1);
        assert!(!logs.join("onkey-20260901.log").exists() && logs.join("onkey-20261015.log").exists());
        assert!((1..=30).all(|i| d.join(format!("takes/202610{i:02}-120000-Song/mic.m4a")).exists()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn records_stay_under_their_cap() {
        let d = tmp("cap");
        let t = d.join("records/takes");
        std::fs::create_dir_all(&t).unwrap();
        let cap = 80_000;
        let big = vec![b'x'; cap / 8];
        for i in 0..10 {
            let p = t.join(format!("2026101{i}-take.json"));
            std::fs::write(&p, &big).unwrap();
            set_age(&p, 10 - i as u64);
        }
        let s = sweep_capped(&d, SystemTime::now(), cap as u64);
        assert!(s.records_bytes <= cap as u64 * 3 / 4, "{s:?}");
        assert!(t.join("20261019-take.json").exists() && !t.join("20261010-take.json").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_take_summary_has_its_events_perf_and_line_offsets_and_no_words() {
        note("ae", json!({"app": "Spotify", "ms": 80.0, "outcome": "ok"}));
        take_begin(9001, "isrc:TEST", json!({"track": {"id": "isrc:TEST", "name": "Test Song"},
            "lyrics": {"check": "abc", "times": [10.0, 20.0, 30.0]}}));
        note_for(9001, "placement", json!({"offset": 1.5, "how": "fingerprint"}));
        note_for(9002, "placement", json!({"offset": 99.0}));
        note("stage", json!({"track": "isrc:TEST", "stage": "separating"}));
        note("ae", json!({"app": "Spotify", "ms": 900.0, "outcome": "ok"}));
        take_end(9001);
        let open = {
            let mut o = OPEN.lock().unwrap();
            let i = o.iter().position(|o| o.version == 9001).unwrap();
            o.remove(i)
        };
        let words = json!({"check": "abc", "lines": 3, "words": [[0, 0, 10.3, 10.6], [0, 1, 10.7, 11.0], [2, 0, 29.5, 30.0]]});
        let s = summary(&open, json!({"status": "done"}), Some(&words));
        assert_eq!(s["placement"].as_array().unwrap().len(), 1);
        assert_eq!(s["placement"][0]["offset"], 1.5);
        assert_eq!(s["learner"][0]["stage"], "separating");
        assert!(s["apple_events"]["Spotify"]["n"].as_u64().unwrap() >= 2);
        assert_eq!(s["apple_events"]["slow"].as_array().unwrap().len(), 1);
        assert_eq!(s["line_offsets"]["by_line"], json!([-0.3, null, 0.5]));
        assert_eq!(s["line_offsets"]["lines_heard"], 2);
        assert!(s["build"]["version"].is_string() && s["mac"]["model"].is_string());
    }

    #[test]
    fn window_frames_keep_numbers_only() {
        window_frames(&json!({"p50_ms": 16.7, "dropped": 2, "note": "some text", "nested": {"a": 1}}));
        let e = EVENTS.lock().unwrap();
        let last = e.iter().rev().find(|e| e.kind == "frames").unwrap();
        assert_eq!(last.data, json!({"p50_ms": 16.7, "dropped": 2}));
    }

    #[test]
    fn rms_of_silence_is_the_floor() {
        assert_eq!(rms_db(&[0.0; 100]), -120.0);
        assert!((rms_db(&[0.5; 100]) + 6.0).abs() < 0.1);
    }
}
