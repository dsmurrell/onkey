//! Your output's delay, checked by tapping along (Settings → Your output → Check timing).
//!
//! OnKey hears the music app's audio as the app plays it (capture.rs's process tap), before it reaches your ears,
//! so the window shows the song that much behind what's captured (the take's `heard`, the window's clock). Core
//! Audio says how long that is (capture::output_latency_ms), but over Bluetooth it's often well short (it leaves
//! out the radio and the headphones' own buffering): lyrics and bars then come early on headphones.
//!
//! The check plays a steady click in the window (WebKit's sound goes to the same output as the music) and you tap
//! with each click you hear. The median of tap minus click, the first clicks and the stray taps left out, is the
//! delay you hear things with. As rhythm games do, it's taken as it is, with no allowance for tapping a little
//! early (most people do): you sing along with the same bias.
//!
//! The window's clicks are on the AudioContext's clock and the taps on the page's (performance.now): the clicks
//! are put on the page's clock by the output's own timestamps (getOutputTimestamp), checked to stay put through
//! the run, and a tap is its event time only when that's just before its handler ran (`on_one_clock`). A check
//! is kept only when it's believable (no less than Core Audio's figure less BELOW_REPORTED_MS, no more than
//! MAX_MS) and two in a row agree within AGREE_MS: two checks on AirPods once came out 512 ms apart, one of them
//! hearing the click before it was played.
//!
//! It's kept per output device (its Core Audio uid) as a correction on top of Core Audio's figure (measured minus
//! what Core Audio said then), not instead of it: Core Audio's own changes on the same device still come through
//! (AirPods whose mic is in use move to another Bluetooth profile, and Core Audio's figure moves with it), while
//! what it leaves out, steady for a device, is added. A device never checked is allowed for exactly as before.
use serde_json::{json, Value};

/// The first clicks of a check aren't counted: finding the beat.
pub const SKIP: usize = 4;
/// A tap counts for the click before it when it's this far from it (s): a little early (people tap ahead of the
/// beat), up to well past Bluetooth's delay. Clicks must be further apart than the span (100 bpm: 0.6 s).
const EARLY_S: f64 = 0.2;
const LATE_S: f64 = 0.4;
/// Taps needed to believe a check, and the share of the taps counted that must agree.
const MIN_TAPS: usize = 6;
/// A tap further than three of the taps' median deviations from their median is a stray, but never one within
/// STRAY_S, and always one past STRAY_MAX_S (s). Taps that still spread more than SPREAD_MAX_S are no measurement.
const STRAY_S: f64 = 0.04;
const STRAY_MAX_S: f64 = 0.08;
const SPREAD_MAX_S: f64 = 0.045;

const UNEVEN: &str = "Your taps were too uneven to measure. Try again, tapping along in rhythm, like clapping to a beat.";
pub const UNBELIEVABLE: &str = "That doesn’t look right: did you wait for each click? Try again, tapping along in rhythm, like clapping to a beat.";
pub const REACTING: &str = "It looks like you waited for each click. Try tapping along in rhythm instead.";
const FEW_TAPS: &str = "Not enough taps in time with the clicks. Tap along in rhythm, once on each click.";
const UNTIMED: &str = "The check couldn’t time its clicks. Try again.";
const STALLED: &str = "The sound stalled or restarted during the check, so it can’t be timed. Try again.";
const OUTPUT_CHANGED: &str = "The output changed during the check. Try again.";

/// A refused check's stable code, for the window to say it in its own language (`error` stays the English words;
/// src-tauri/messages/KINDS.md lists them).
pub fn error_code(why: &str) -> &'static str {
    match why {
        UNEVEN => "uneven",
        UNBELIEVABLE => "unbelievable",
        REACTING => "reacting",
        FEW_TAPS => "few_taps",
        UNTIMED => "untimed",
        STALLED => "stalled",
        OUTPUT_CHANGED => "output_changed",
        _ => "other",
    }
}
/// Tapping along in rhythm is tight (a standard deviation of 10-20 ms) and steady; waiting for each click and
/// reacting to it is looser (a reaction varies by 35-60 ms), throws more strays, and falls behind as the run goes
/// on. Any one of these is reacting (ms, share of taps). On a check's 20 taps counted, in synthetic runs (the
/// tests): in rhythm at sd 15 ms refused under 1% of the time, at 20 ms ~3%; reacting at sd 40 ms refused ~75%, at
/// 60 ms ~93%, and two checks must also agree.
const REACT_SD_MS: f64 = 30.0;
const REACT_SPREAD_MS: f64 = 22.0;
const REACT_STRAY_SPREAD_MS: f64 = 15.0;
const REACT_KEPT_SHARE: f64 = 0.8;
const REACT_TREND_MS: f64 = 35.0;

/// Whether a check's taps look like reacting to each click rather than tapping along.
pub fn reacting(e: &Estimate) -> bool {
    e.sd_ms > REACT_SD_MS
        || e.spread_ms > REACT_SPREAD_MS
        || (e.spread_ms > REACT_STRAY_SPREAD_MS && (e.used as f64) < REACT_KEPT_SHARE * e.counted as f64)
        || e.trend_ms > REACT_TREND_MS
}

/// The estimate of a check on an output Core Audio says `reported_ms` for, refused when the taps don't make one,
/// look like reacting, or give a delay that can't be right (Err: what to say). Reacting to each click can also give
/// a delay that can't be right: a reaction lands 400-600 ms after a click once Bluetooth's delay is added, which is
/// just before the next click (taps then read as heard before played: AirPods, 18 Oct, -152 ms).
pub fn judge(clicks: &[f64], taps: &[f64], reported_ms: i32) -> Result<Estimate, &'static str> {
    let e = estimate(clicks, taps)?;
    if reacting(&e) {
        Err(REACTING)
    } else if !believable(e.ms, reported_ms) {
        Err(UNBELIEVABLE)
    } else {
        Ok(e)
    }
}

/// How far behind the clicks you tapped.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    /// median of tap minus click over the taps that agree (ms)
    pub ms: f64,
    /// how far those taps were from it, typically (their median deviation, ms)
    pub spread_ms: f64,
    /// taps that agree, and taps counted (one per click after the first SKIP)
    pub used: usize,
    pub counted: usize,
    /// how much later the second half's taps were than the first half's (ms): reacting falls behind
    pub trend_ms: f64,
    /// how far all the taps counted spread, strays and all: their median deviation scaled to a standard deviation (ms)
    pub sd_ms: f64,
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n == 0 { f64::NAN } else if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) }
}

/// The delay you hear the clicks with, from when they were played (`clicks`, s, in order) and when you tapped
/// (`taps`, s, the same clock). Each tap goes with the click before it (EARLY_S..LATE_S of it); a click with two
/// taps keeps the one nearer the rest; the first SKIP clicks aren't counted; taps far from the median are strays.
/// Err: what to say when the taps don't make a measurement.
pub fn estimate(clicks: &[f64], taps: &[f64]) -> Result<Estimate, &'static str> {
    // each click's taps (offsets from it)
    let mut per: Vec<Vec<f64>> = vec![Vec::new(); clicks.len()];
    for &t in taps {
        if let Some(i) = clicks.iter().rposition(|&c| t - c >= -EARLY_S) {
            let d = t - clicks[i];
            if d < LATE_S {
                per[i].push(d);
            }
        }
    }
    let mut all: Vec<f64> = per.iter().skip(SKIP).flatten().copied().collect();
    if all.len() < MIN_TAPS {
        return Err(FEW_TAPS);
    }
    let mid = median(&mut all);
    // one tap per click: the one nearest the rest (a double tap or a slip)
    let mut one: Vec<f64> = per.iter().skip(SKIP).filter_map(|d| d.iter().copied().min_by(|a, b| (a - mid).abs().total_cmp(&(b - mid).abs()))).collect();
    let counted = one.len();
    let trend_ms = {
        let (mut a, mut b) = (one[..counted / 2].to_vec(), one[counted / 2..].to_vec());
        (median(&mut b) - median(&mut a)) * 1000.0
    };
    let mid = median(&mut one);
    let mut dev: Vec<f64> = one.iter().map(|d| (d - mid).abs()).collect();
    let mad = median(&mut dev);
    let sd_ms = 1.4826 * mad * 1000.0;
    let tol = (3.0 * mad).clamp(STRAY_S, STRAY_MAX_S);
    let mut kept: Vec<f64> = one.into_iter().filter(|d| (d - mid).abs() <= tol).collect();
    if kept.len() < MIN_TAPS || kept.len() * 3 < counted * 2 {
        return Err(UNEVEN);
    }
    let ms = median(&mut kept) * 1000.0;
    let mut dev: Vec<f64> = kept.iter().map(|d| (d * 1000.0 - ms).abs()).collect();
    let spread_ms = median(&mut dev);
    if spread_ms > SPREAD_MAX_S * 1000.0 {
        return Err(UNEVEN);
    }
    Ok(Estimate { ms, spread_ms, used: kept.len(), counted, trend_ms, sd_ms })
}

/// A measured delay is believed only when it's no shorter than Core Audio's figure less this (ms): Bluetooth adds to
/// what Core Audio says, it doesn't take away, and a tap a little ahead of the beat is tens of ms, not hundreds.
/// A correction kept below it (an earlier check, before this guard) is ignored when loaded.
pub const BELOW_REPORTED_MS: i32 = 80;
/// ... and no longer than this (ms): no output is that slow, so it's a mistimed check.
pub const MAX_MS: f64 = 800.0;
/// Two checks in a row must agree within this (ms) before one is kept.
pub const AGREE_MS: f64 = 50.0;
/// The click's mapping onto the page's clock may move this much during a check (s) before the check is refused:
/// more is the output stalling or restarting (Bluetooth waking, a profile change) mid-check.
const MAX_DRIFT_S: f64 = 0.03;
/// A tap's own time (event.timeStamp) is used when it's this recent when its handler runs (ms); otherwise when
/// the handler ran (WebKit's event times have been seen far off the page's clock).
const TAP_LAG_MAX_MS: f64 = 50.0;

/// One run of the check as the window recorded it, each part on its own clock: the clicks on the AudioContext's,
/// the taps and the output's timestamps on the page's (performance.now, ms). `on_one_clock` puts them together.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Run {
    /// when each click was scheduled (ctx.currentTime, s)
    pub clicks: Vec<f64>,
    /// ctx.getOutputTimestamp() through the run: [contextTime s, performanceTime ms], which context time was
    /// going out to the output when (none where the WebView doesn't have it)
    #[serde(default)]
    pub stamps: Vec<[f64; 2]>,
    /// [ctx.currentTime s, performance.now() ms] through the run: the mapping when there are no output timestamps
    #[serde(default)]
    pub now: Vec<[f64; 2]>,
    /// each tap: [event.timeStamp ms, performance.now() ms when its handler ran]
    pub taps: Vec<[f64; 2]>,
    /// how each tap was made ("key", "pointer"), for the log
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub base_latency: Option<f64>,
    #[serde(default)]
    pub output_latency: Option<f64>,
    #[serde(default)]
    pub sample_rate: Option<f64>,
}

/// A run's clicks and taps on the page's clock (s).
#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    pub clicks: Vec<f64>,
    pub taps: Vec<f64>,
    /// "output timestamps" or "currentTime" (the fallback)
    pub how: &'static str,
    /// click on the page's clock = its context time + this (s)
    pub offset_s: f64,
    /// how far that moved during the run (ms)
    pub drift_ms: f64,
    /// taps timed by their own event time (the rest: when their handler ran)
    pub by_event_time: usize,
}

fn valid(p: &[f64; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite() && p[0] > 0.0 && p[1] > 0.0
}

/// When a tap was made on the page's clock (ms), and whether by its own event time: event.timeStamp when it's a
/// little before its handler ran (0..TAP_LAG_MAX_MS), else when the handler ran.
pub fn tap_time(t: [f64; 2]) -> (f64, bool) {
    let [ts, now] = t;
    if ts.is_finite() && (0.0..=TAP_LAG_MAX_MS).contains(&(now - ts)) { (ts, true) } else { (now, false) }
}

/// From currentTime against performance.now(): currentTime moves in steps (a block rendered at a time), so the
/// least page-minus-context seen is when a step was just taken: that, and how far it differs between the run's
/// halves (s). None: too few samples.
fn by_current_time(now: &[[f64; 2]]) -> Option<(f64, f64)> {
    let offs: Vec<f64> = now.iter().filter(|p| valid(p)).map(|p| p[1] / 1000.0 - p[0]).collect();
    if offs.len() < 10 {
        return None;
    }
    let least = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let (a, b) = (least(&offs[..offs.len() / 2]), least(&offs[offs.len() / 2..]));
    Some((a.min(b), (a - b).abs()))
}

/// The run's clicks and taps on one clock, the page's (s). A click is when the output timestamps say its context
/// time went out to the output (Core Audio's IO time: what Core Audio's figure, and the process tap, count from);
/// without them, when it was rendered (currentTime) plus the context's own buffering (baseLatency). Err: what to
/// say (the mapping moved during the run: the sound stalled or restarted).
pub fn on_one_clock(run: &Run) -> Result<Mapped, &'static str> {
    let mut offs: Vec<f64> = run.stamps.iter().filter(|p| valid(p)).map(|p| p[1] / 1000.0 - p[0]).collect();
    let (how, offset_s, drift_s) = if offs.len() >= 5 {
        offs.sort_by(|a, b| a.total_cmp(b));
        let k = offs.len() / 10;
        let drift = offs[offs.len() - 1 - k] - offs[k];
        ("output timestamps", median(&mut offs), drift)
    } else {
        let Some((o, d)) = by_current_time(&run.now) else { return Err(UNTIMED) };
        ("currentTime", o + run.base_latency.unwrap_or(0.0).clamp(0.0, 0.2), d)
    };
    if drift_s > MAX_DRIFT_S {
        return Err(STALLED);
    }
    let taps: Vec<(f64, bool)> = run.taps.iter().map(|&t| tap_time(t)).collect();
    Ok(Mapped { clicks: run.clicks.iter().map(|c| c + offset_s).collect(), taps: taps.iter().map(|t| t.0 / 1000.0).collect(),
                how, offset_s, drift_ms: drift_s * 1000.0, by_event_time: taps.iter().filter(|t| t.1).count() })
}

/// Whether a measured delay (ms) can be right on an output Core Audio says `reported_ms` for.
pub fn believable(ms: f64, reported_ms: i32) -> bool {
    ms >= (reported_ms - BELOW_REPORTED_MS).max(0) as f64 && ms <= MAX_MS
}

/// After a check measured `ms`, with the one before it on this output (`first`, if any): keep the two's mean, or
/// ask for one more.
#[derive(Debug, Clone, PartialEq)]
pub enum Confirm {
    Keep(f64),
    Again,
}

pub fn confirm(first: Option<f64>, ms: f64) -> Confirm {
    match first {
        Some(f) if (f - ms).abs() <= AGREE_MS => Confirm::Keep(0.5 * (f + ms)),
        _ => Confirm::Again,
    }
}

/// The last believable check not yet confirmed: output uid, measured (ms), spread (ms), when.
static FIRST: std::sync::Mutex<Option<(String, f64, f64, std::time::Instant)>> = std::sync::Mutex::new(None);
/// A first check waits this long for its confirmation (s).
const FIRST_KEPT_S: u64 = 600;

/// What OnKey allows for (ms): Core Audio's figure for the device, plus your check's correction when there is one.
pub fn apply(reported_ms: i32, correction_ms: Option<i32>) -> i32 {
    (reported_ms.max(0) + correction_ms.unwrap_or(0)).max(0)
}

/// The correction kept for device `uid` in the settings' output_timing (`all`), when it can be right with Core
/// Audio's figure now (`reported_ms`): one that takes more than BELOW_REPORTED_MS off it, or brings it below
/// nothing, is from a mistimed check (before the checks were guarded) and is ignored.
/// The tap-along check is retired (18 Oct 2026): people tap 75-115 ms ahead of the beat and drift earlier through
/// a run, so a tap can't measure an output's delay better than about 40 ms, and on Daniel's speakers and Sony
/// headphones Core Audio's figure was already within that. Kept corrections are no longer applied: Core Audio's
/// figure is used. The check's code stays for a possible return; the evidence is the tap-along runs summed up above.
const CHECK_RETIRED: bool = true;

fn correction_in(all: &Value, uid: &str, reported_ms: i32) -> Option<i32> {
    if uid.is_empty() {
        return None;
    }
    let c = all.get(uid)?.get("correction_ms")?.as_i64()? as i32;
    if c < -BELOW_REPORTED_MS || reported_ms.max(0) + c < 0 {
        ignored(uid, c, reported_ms);
        return None;
    }
    Some(c)
}

/// Says once per device and correction that a kept correction is ignored.
fn ignored(uid: &str, c: i32, reported_ms: i32) {
    static SAID: std::sync::Mutex<Vec<(String, i32)>> = std::sync::Mutex::new(Vec::new());
    let mut said = SAID.lock().unwrap();
    if said.iter().any(|(u, k)| u == uid && *k == c) {
        return;
    }
    said.push((uid.to_string(), c));
    if !cfg!(test) {
        crate::engine::log(&format!("output timing: the kept check of this output ({c:+} ms on Core Audio's {reported_ms} ms) can't be right, \
                                     so it's ignored (Core Audio's figure is used; Reset or check again in Settings)"));
    }
}

/// A check kept for device `uid` in `all` (the settings' output_timing): `ms` measured, kept as the correction on
/// top of Core Audio's figure then (`reported_ms`). Returns the correction (ms).
fn save_in(all: &mut Value, uid: &str, ms: f64, spread_ms: f64, taps: usize, reported_ms: i32, at: u64) -> i32 {
    let correction = (ms - reported_ms.max(0) as f64).round() as i32;
    if !all.is_object() {
        *all = json!({});
    }
    all[uid] = json!({"correction_ms": correction, "measured_ms": ms.round(), "reported_ms": reported_ms,
                      "spread_ms": spread_ms.round(), "taps": taps, "at": at});
    correction
}

/// The correction allowed for on the output now.
fn correction_now(reported_ms: i32) -> Option<i32> {
    if CHECK_RETIRED {
        return None;
    }
    correction_in(&crate::settings::get_value("output_timing"), &crate::capture::output_uid(), reported_ms)
}

/// How long after the music app plays you hear it on the output now (ms), as OnKey allows for it.
pub fn heard_ms() -> i32 {
    let reported = crate::capture::output_latency_ms();
    apply(reported, correction_now(reported))
}

/// Core Audio's figure, and what's allowed for, on the output now: in frames of 10 ms (the take's `heard`).
pub fn heard_frames() -> (i64, i64) {
    let reported = crate::capture::output_latency_ms();
    let heard = apply(reported, correction_now(reported));
    (frames(heard), frames(reported))
}

fn frames(ms: i32) -> i64 {
    (ms.max(0) as f64 / 10.0).round() as i64
}

/// The output now, for Settings and the window: its uid and name, Core Audio's figure and what's allowed for
/// (ms), your check's correction (null: never checked), and `suggest`: a check is worth suggesting (never checked,
/// Core Audio says over 100 ms, so likely Bluetooth, and the suggestion wasn't dismissed for it).
pub fn status() -> Value {
    let uid = crate::capture::output_uid();
    let reported = crate::capture::output_latency_ms();
    let all = crate::settings::get_value("output_timing");
    status_of(&all, &uid, &crate::capture::output_name(), reported)
}

fn status_of(all: &Value, uid: &str, name: &str, reported: i32) -> Value {
    let correction = if CHECK_RETIRED { None } else { correction_in(all, uid, reported) };
    let kept = all.get(uid).filter(|_| !uid.is_empty());
    let dismissed = kept.is_some_and(|k| k["dismissed"].as_bool() == Some(true));
    let first = FIRST.lock().unwrap().as_ref().filter(|f| f.0 == uid && f.3.elapsed().as_secs() < FIRST_KEPT_S).map(|f| f.1.round());
    json!({"uid": uid, "name": name, "reported_ms": reported, "heard_ms": apply(reported, correction), "correction_ms": correction,
           "measured_ms": correction.and(kept.and_then(|k| k.get("measured_ms")).cloned()),
           "checked_at": correction.and(kept.and_then(|k| k.get("at")).cloned()), "first_ms": first,
           "suggest": !uid.is_empty() && correction.is_none() && !dismissed && reported > 100})
}

fn update(f: impl FnOnce(&mut Value)) {
    let mut all = crate::settings::get_value("output_timing");
    if !all.is_object() {
        all = json!({});
    }
    f(&mut all);
    crate::settings::set_value("output_timing", all);
}

/// The run in the log, to check a run from it later: how the clicks were put on the page's clock, the context's
/// latencies, each click in both clocks, each tap's event time and how long before its handler that was.
fn log_run(run: &Run, m: Option<&Mapped>) {
    let r3 = |v: f64| format!("{v:.3}");
    let mut line = String::from("output timing run: ");
    if let Some(m) = m {
        line += &format!("clicks by {} (page = context + {:.4} s, moved {:.1} ms during the run); ", m.how, m.offset_s, m.drift_ms);
        if m.how == "output timestamps" {
            if let Some((o, _)) = by_current_time(&run.now) {
                line += &format!("currentTime runs {:.0} ms ahead of the output; ", (m.offset_s - o) * 1000.0);
            }
        }
    }
    line += &format!("{} output timestamps, {} currentTime samples; baseLatency {}, outputLatency {}, {} Hz; ",
                     run.stamps.iter().filter(|p| valid(p)).count(), run.now.len(),
                     run.base_latency.map_or("none".into(), r3), run.output_latency.map_or("none".into(), r3),
                     run.sample_rate.map_or("?".into(), |r| format!("{r:.0}")));
    let clicks: Vec<String> = run.clicks.iter().map(|c| match m {
        Some(m) => format!("{c:.3}→{:.1}", (c + m.offset_s) * 1000.0),
        None => format!("{c:.3}"),
    }).collect();
    line += &format!("clicks (context s→page ms) [{}]; ", clicks.join(" "));
    let taps: Vec<String> = run.taps.iter().enumerate().map(|(i, t)| {
        let (_, own) = tap_time(*t);
        format!("{}{:.1}({:+.1}{})", run.kinds.get(i).map_or("", |k| if k == "pointer" { "p" } else { "" }), t[0], t[1] - t[0], if own { "" } else { "!" })
    }).collect();
    line += &format!("taps (event ms, handler later by; ! = handler time used) [{}]", taps.join(" "));
    crate::engine::log(&line);
}

/// A check done on device `uid` (the one Settings showed). A believable one is kept, and allowed for from now on
/// (a take playing follows within 3 s), when it agrees with the one before it; the first asks for one more.
/// Returns what to show: the measurement and correction, `confirm` (one more to go), or `error` (what to say).
pub fn check(uid: &str, run: &Run) -> Value {
    if uid.is_empty() || uid != crate::capture::output_uid() {
        return json!({"error": OUTPUT_CHANGED, "code": error_code(OUTPUT_CHANGED)});
    }
    let mapped = on_one_clock(run);
    log_run(run, mapped.as_ref().ok());
    let refuse = |why: &str, measured: Option<f64>| {
        crate::engine::log(&format!("output timing check refused: {why}"));
        crate::records::note("output_timing", json!({"what": "check", "ok": false, "why": why, "measured_ms": measured.map(f64::round),
                                                     "clicks": run.clicks.len(), "taps": run.taps.len()}));
        json!({"error": why, "code": error_code(why)})
    };
    let m = match mapped {
        Ok(m) => m,
        Err(why) => return refuse(why, None),
    };
    let reported = crate::capture::output_latency_ms();
    if let Ok(e) = estimate(&m.clicks, &m.taps) {
        crate::engine::log(&format!("output timing measured: you hear it {:.0} ms after it's played (±{:.0} ms, {} of {} taps, {:+.0} ms later by the end); \
                                     Core Audio says {reported} ms", e.ms, e.spread_ms, e.used, e.counted, e.trend_ms));
    }
    let est = match judge(&m.clicks, &m.taps, reported) {
        Ok(e) => e,
        Err(why) => {
            if why == UNBELIEVABLE || why == REACTING {
                *FIRST.lock().unwrap() = None;
            }
            return refuse(why, None);
        }
    };
    let mut first = FIRST.lock().unwrap();
    let before = first.as_ref().filter(|f| f.0 == uid && f.3.elapsed().as_secs() < FIRST_KEPT_S).map(|f| (f.1, f.2));
    let ms = match confirm(before.map(|b| b.0), est.ms) {
        Confirm::Keep(ms) => ms,
        Confirm::Again => {
            *first = Some((uid.to_string(), est.ms, est.spread_ms, std::time::Instant::now()));
            crate::engine::log(&match before {
                Some((b, _)) => format!("output timing: {:.0} ms doesn't agree with the check before ({b:.0} ms): one more to confirm", est.ms),
                None => format!("output timing: {:.0} ms, one more to confirm", est.ms),
            });
            return json!({"confirm": true, "measured_ms": est.ms.round(), "reported_ms": reported, "spread_ms": est.spread_ms.round(),
                          "before_ms": before.map(|b| b.0.round())});
        }
    };
    *first = None;
    drop(first);
    let spread = before.map_or(est.spread_ms, |b| b.1.max(est.spread_ms));
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut correction = 0;
    update(|all| correction = save_in(all, uid, ms, spread, est.used, reported, at));
    crate::engine::log(&format!("output timing checked: two checks agree, you hear it {ms:.0} ms after it's played; \
                                 Core Audio says {reported} ms, so {correction:+} ms is added"));
    crate::records::note("output_timing", json!({"what": "check", "ok": true, "measured_ms": ms.round(), "reported_ms": reported,
                                                 "correction_ms": correction, "spread_ms": spread.round(), "taps": est.used}));
    json!({"ok": true, "measured_ms": ms.round(), "reported_ms": reported, "correction_ms": correction, "spread_ms": spread.round()})
}

/// Forget the check of device `uid`: Core Audio's figure is allowed for again.
pub fn reset(uid: &str) {
    *FIRST.lock().unwrap() = None;
    update(|all| if let Some(o) = all.as_object_mut() {
        o.remove(uid);
    });
    crate::records::note("output_timing", json!({"what": "reset"}));
}

/// Don't suggest checking device `uid` again.
pub fn dismiss(uid: &str) {
    if uid.is_empty() {
        return;
    }
    update(|all| {
        if !all[uid].is_object() {
            all[uid] = json!({});
        }
        all[uid]["dismissed"] = json!(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_refusal_has_its_own_code() {
        let all = [UNEVEN, UNBELIEVABLE, REACTING, FEW_TAPS, UNTIMED, STALLED, OUTPUT_CHANGED];
        let mut codes: Vec<&str> = all.iter().map(|w| error_code(w)).collect();
        assert!(!codes.contains(&"other"));
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), all.len());
        assert_eq!(error_code("something new"), "other");
    }

    /// 16 clicks at 100 bpm from 0.5 s.
    fn clicks() -> Vec<f64> {
        (0..16).map(|i| 0.5 + i as f64 * 0.6).collect()
    }

    #[test]
    fn the_delay_is_the_median_of_the_taps_after_the_first_clicks() {
        let c = clicks();
        // Bluetooth: heard 230 ms late, tapped with some jitter; the first clicks tapped wildly (finding the beat)
        let jitter = [0.012, -0.009, 0.004, -0.015, 0.008, 0.0, -0.004, 0.011, -0.007, 0.003, 0.006, -0.012];
        let mut taps: Vec<f64> = vec![0.95, 1.2, 2.3];
        taps.extend(c[SKIP..].iter().zip(jitter).map(|(c, j)| c + 0.23 + j));
        let e = estimate(&c, &taps).unwrap();
        assert!((e.ms - 231.5).abs() < 3.0, "{e:?}");
        assert_eq!((e.used, e.counted), (12, 12));
        assert!(e.spread_ms < 12.0);
        // speakers, tapped a little early: a small negative delay is taken as it is
        let taps: Vec<f64> = c.iter().map(|c| c - 0.02).collect();
        assert!((estimate(&c, &taps).unwrap().ms + 20.0).abs() < 0.5);
    }

    /// Taps `delay` s after each click (after the first SKIP), jittered by a normal-ish spread of `sd` s, later by
    /// `drift` s over the run; seeded.
    fn taps(c: &[f64], delay: f64, sd: f64, drift: f64, seed: u64) -> Vec<f64> {
        let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut uni = || { x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (x >> 11) as f64 / (1u64 << 53) as f64 };
        let n = c.len() - SKIP;
        c[SKIP..].iter().enumerate().map(|(i, c)| {
            let z = (0..12).map(|_| uni()).sum::<f64>() - 6.0;     // ~normal, sd 1
            c + delay + sd * z + drift * i as f64 / (n - 1) as f64
        }).collect()
    }

    /// A check's 24 clicks (CLICKS in TimingCheck.tsx).
    fn check_clicks() -> Vec<f64> {
        (0..24).map(|i| 0.5 + i as f64 * 0.6).collect()
    }

    /// How many of 400 seeded runs are refused, on AirPods (Core Audio 171 ms).
    fn refused(delay: f64, sd: f64, drift: f64) -> usize {
        let c = check_clicks();
        (0..400).filter(|&seed| judge(&c, &taps(&c, delay, sd, drift, seed), 171).is_err()).count()
    }

    #[test]
    fn tapping_along_is_kept_and_reacting_is_refused() {
        // in rhythm on AirPods: heard 250 ms late, tapped a little ahead of it; and what's kept is right
        assert_eq!(refused(0.23, 0.01, 0.0), 0);
        assert!(refused(0.23, 0.015, 0.0) <= 4);
        assert!(refused(0.23, 0.02, 0.0) <= 20);
        let c = check_clicks();
        for seed in 0..100 {
            if let Ok(e) = judge(&c, &taps(&c, 0.23, 0.015, 0.0, seed), 171) {
                assert!((e.ms - 230.0).abs() < 15.0, "{e:?}");
            }
        }
        // waiting for each click: 170-250 ms heard plus a 150-200 ms reaction varying by 40-60 ms (each run; two
        // runs must also agree)
        assert!(refused(0.34, 0.04, 0.0) >= 260, "{}", refused(0.34, 0.04, 0.0));
        assert!(refused(0.36, 0.06, 0.0) >= 340, "{}", refused(0.36, 0.06, 0.0));
        // falling behind through the run
        assert!(refused(0.27, 0.012, 0.09) >= 340, "{}", refused(0.27, 0.012, 0.09));
        // the run of 18 Oct that was kept: reacting ~450 ms late reads as -150 ms (before the next click), tight
        assert_eq!(refused(0.448, 0.008, 0.0), 400);
        assert_eq!(judge(&c, &taps(&c, 0.448, 0.008, 0.0, 1), 171), Err(UNBELIEVABLE));
    }

    #[test]
    fn stray_and_double_taps_are_left_out() {
        let c = clicks();
        let mut taps: Vec<f64> = c[SKIP..].iter().map(|c| c + 0.18).collect();
        taps[3] += 0.15;                     // a slip: 330 ms
        taps[7] -= 0.3;                      // way early (120 ms before its click)
        taps.push(c[10] + 0.05);             // a double tap on one click: the one nearer the rest is kept
        taps.sort_by(|a, b| a.total_cmp(b));
        let e = estimate(&c, &taps).unwrap();
        assert!((e.ms - 180.0).abs() < 0.5, "{e:?}");
        assert_eq!(e.used, 10);
        assert_eq!(e.counted, 12);
    }

    #[test]
    fn too_few_or_uneven_taps_are_no_measurement() {
        let c = clicks();
        // a few taps only
        let taps: Vec<f64> = c[SKIP..SKIP + 4].iter().map(|c| c + 0.2).collect();
        assert!(estimate(&c, &taps).is_err());
        // taps anywhere: no steady delay
        let taps: Vec<f64> = c[SKIP..].iter().enumerate().map(|(i, c)| c + [-0.15, 0.3, 0.05, 0.22, -0.05, 0.38][i % 6]).collect();
        assert!(estimate(&c, &taps).is_err());
        assert!(estimate(&c, &[]).is_err());
    }

    #[test]
    fn a_check_is_kept_per_device_on_top_of_core_audio() {
        let mut all = json!({});
        // AirPods: Core Audio said 160 ms, you hear it at 241: 81 ms more is allowed for
        assert_eq!(save_in(&mut all, "airpods", 241.0, 9.0, 12, 160, 1), 81);
        assert_eq!(correction_in(&all, "airpods", 160), Some(81));
        assert_eq!(apply(160, correction_in(&all, "airpods", 160)), 241);
        // Core Audio's figure moving on the same device (its mic in use) still comes through
        assert_eq!(apply(190, correction_in(&all, "airpods", 190)), 271);
        // another device, never checked: exactly as before
        assert_eq!(correction_in(&all, "speakers", 28), None);
        assert_eq!(apply(28, correction_in(&all, "speakers", 28)), 28);
        assert_eq!(correction_in(&all, "", 28), None);
        assert_eq!(apply(-1, None), 0);
    }

    #[test]
    fn a_kept_correction_that_cannot_be_right_is_ignored() {
        // the AirPods check of 18 Oct: -152 ms measured on Core Audio's 171, kept as -323 (allowed for: 0 ms)
        let all = json!({"airpods": {"correction_ms": -323, "measured_ms": -152, "reported_ms": 171}});
        assert_eq!(correction_in(&all, "airpods", 171), None);
        assert_eq!(apply(171, correction_in(&all, "airpods", 171)), 171);
        assert_eq!(status_of(&all, "airpods", "AirPods", 171)["correction_ms"], Value::Null);
        // a little less than Core Audio says is believable; never below nothing
        let all = json!({"airpods": {"correction_ms": -60}, "speakers": {"correction_ms": -30}});
        assert_eq!(correction_in(&all, "airpods", 171), Some(-60));
        assert_eq!(correction_in(&all, "speakers", 20), None);
        assert_eq!(correction_in(&all, "speakers", 40), Some(-30));
    }

    #[test]
    fn only_a_believable_delay_is_kept() {
        assert!(!believable(-152.0, 171));          // heard before it was played
        assert!(!believable(80.0, 171));            // well short of Core Audio's figure
        assert!(believable(95.0, 171));
        assert!(believable(360.0, 171));
        assert!(!believable(900.0, 171));
        assert!(believable(0.0, 28));               // speakers: down to nothing
        assert!(!believable(-20.0, 28));
    }

    #[test]
    fn two_checks_must_agree_before_one_is_kept() {
        assert_eq!(confirm(None, 240.0), Confirm::Again);
        assert_eq!(confirm(Some(220.0), 260.0), Confirm::Keep(240.0));
        assert_eq!(confirm(Some(360.0), 230.0), Confirm::Again);
    }

    /// A run as the window records it: clicks every 0.6 s on the context's clock from `t0`; the context's time
    /// `ctx` goes out to the output at page time (ctx + `off`) s; taps `heard` s after each click goes out, each
    /// event handled `lag` ms after it.
    fn run(t0: f64, off: f64, heard: f64, lag: f64) -> Run {
        let clicks: Vec<f64> = (0..16).map(|i| t0 + i as f64 * 0.6).collect();
        // the output's timestamps every 50 ms, a little jitter; currentTime 20 ms ahead of the output, in 10 ms steps
        let stamps = (0..200).map(|i| { let c = t0 - 0.5 + i as f64 * 0.05; [c, (c + off) * 1000.0 + (i % 3) as f64 * 0.3] }).collect();
        let now = (0..200).map(|i| { let p = t0 - 0.5 + off + i as f64 * 0.05 + 0.003 * (i % 4) as f64;
                                     [((p - off + 0.02) / 0.01).floor() * 0.01, p * 1000.0] }).collect();
        let jitter = [0.008, -0.006, 0.003, -0.01, 0.005, 0.0, -0.004, 0.007];
        let taps = clicks.iter().enumerate().map(|(i, c)| { let at = (c + off + heard + jitter[i % 8]) * 1000.0; [at, at + lag] }).collect();
        Run { clicks, stamps, now, taps, base_latency: Some(0.005), output_latency: Some(0.0), sample_rate: Some(48000.0), ..Default::default() }
    }

    #[test]
    fn clicks_and_taps_are_put_on_the_page_clock() {
        // a fresh context per run: the context's clock starts again, the page's goes on; the same delay either way
        for (t0, off) in [(1.3, 812.4), (0.9, 826.75)] {
            let m = on_one_clock(&run(t0, off, 0.24, 4.0)).unwrap();
            assert_eq!((m.how, m.by_event_time), ("output timestamps", 16));
            assert!(m.drift_ms < 1.0, "{m:?}");
            let e = estimate(&m.clicks, &m.taps).unwrap();
            assert!((e.ms - 240.0).abs() < 3.0, "{e:?}");
        }
        // an event time far off the page's clock (or after its handler): the handler's time instead
        let mut r = run(1.3, 812.4, 0.24, 4.0);
        for t in r.taps.iter_mut().step_by(2) {
            t[0] -= 400.0;
        }
        let m = on_one_clock(&r).unwrap();
        assert_eq!(m.by_event_time, 8);
        assert!((estimate(&m.clicks, &m.taps).unwrap().ms - 244.0).abs() < 4.0);
        // no output timestamps: currentTime (20 ms ahead of the output here) and baseLatency, near enough
        let mut r = run(1.3, 812.4, 0.24, 4.0);
        r.stamps.clear();
        let m = on_one_clock(&r).unwrap();
        assert_eq!(m.how, "currentTime");
        assert!((estimate(&m.clicks, &m.taps).unwrap().ms - 240.0).abs() < 30.0);
    }

    #[test]
    fn a_run_whose_sound_stalled_is_refused() {
        // the output stopped for 0.5 s mid-run (Bluetooth waking): its clicks went out later than before
        let mut r = run(1.3, 812.4, 0.24, 4.0);
        for s in r.stamps.iter_mut().skip(100) {
            s[1] += 500.0;
        }
        assert!(on_one_clock(&r).is_err());
        assert!(on_one_clock(&Run::default()).is_err());
    }

    #[test]
    fn a_check_is_suggested_once_for_a_device_that_is_likely_bluetooth() {
        let s = status_of(&json!({}), "airpods", "AirPods", 160);
        assert_eq!((s["suggest"].as_bool(), s["heard_ms"].as_i64(), s["correction_ms"].is_null()), (Some(true), Some(160), true));
        assert_eq!(status_of(&json!({}), "speakers", "Speakers", 28)["suggest"], false);
        assert_eq!(status_of(&json!({"airpods": {"dismissed": true}}), "airpods", "AirPods", 160)["suggest"], false);
        let checked = json!({"airpods": {"correction_ms": 70, "measured_ms": 230}});
        let s = status_of(&checked, "airpods", "AirPods", 160);
        // the check is retired: a kept one is no longer applied, Core Audio's figure is used
        assert_eq!((s["heard_ms"].as_i64(), s["correction_ms"].is_null()), (Some(160), true));
    }
}
