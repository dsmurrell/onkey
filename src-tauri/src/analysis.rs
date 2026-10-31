//! The full score after a take, port of the Python engine's analyze(): your whole recording is pitch-tracked
//! again (with smoothing the live view can't afford), your latency is found from the melody's shape, and you
//! get a score, notes hit, and a result per lyric line. A first play also yields the song's learned line.
use serde_json::{json, Value};

use crate::dsp::{self, fold, shape_hits, FP_BANDS, FRAME_S, TOL};
use crate::notes::NoteTracker;
use crate::source::Track;

pub enum Reference {
    /// a learned song: the singer's line in take frames (NaN = silent or unknown)
    Learned(Vec<f32>),
    /// a first play: the separated vocal (16 kHz) and its pitch (midi, periodicity) per frame
    Separated { vocal16: Vec<f32>, midi: Vec<f32>, periodicity: Vec<f32> },
}

pub struct Input {
    pub track: Track,
    pub recorded: String,
    pub mode: &'static str,
    pub blind: bool,
    pub lyrics: Vec<(f64, String)>,
    pub song_offset: f64,
    /// a first play's stretches between jumps, each at its own place (Take::stretches: [from, to) take frames,
    /// song offset in frames), for what's learned; empty: `song_offset` throughout
    pub places: Vec<(i64, i64, i64)>,
    pub reference: Reference,
    /// your pitch for the whole take: (midi, periodicity, rms) from pitch::track
    pub usr: (Vec<f32>, Vec<f32>),
    pub mic16: Vec<f32>,
    /// when you were singing, as judged live (on the echo-cancelled microphone); None: judged here from
    /// mic16's loudness (the Python prototype's rule, for comparisons)
    pub voiced: Option<Vec<bool>>,
    pub live: Value,
    /// first play: the take's rhythm fingerprint and which frames had Spotify audio
    pub fingerprint: Option<Vec<[f32; FP_BANDS]>>,
    pub audible: Option<Vec<bool>>,
    /// the notes to score (take frames), e.g. syllable bars; None: from the singer's line
    pub bars: Option<Vec<crate::notes::Bar>>,
    /// how strictly to score (dsp::LEVELS); kept with the take
    pub level: dsp::Level,
}

pub struct Output {
    pub result: Value,
    /// first play: (song line, covered, fingerprint) in song frames, to merge into the learned song
    pub learned: Option<(Vec<f32>, Vec<bool>, Vec<[f32; FP_BANDS]>)>,
}

/// Loudness per 10 ms frame (dB), over the 20 ms around the frame.
fn frame_rms_db(x: &[f32], n: usize) -> Vec<f32> {
    (0..n).map(|i| dsp::frame_db(x, i)).collect()
}

pub fn analyze(inp: Input) -> Output {
    let (ref_m, ref_v, from_cache): (Vec<f32>, Vec<bool>, bool) = match &inp.reference {
        Reference::Learned(line) => (line.iter().map(|v| if v.is_nan() { 0.0 } else { *v }).collect(),
                                     line.iter().map(|v| !v.is_nan()).collect(), true),
        Reference::Separated { midi, periodicity, vocal16 } => {
            let db = frame_rms_db(vocal16, midi.len());
            let top = db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            (midi.clone(), periodicity.iter().zip(&db).map(|(p, d)| *p > 0.45 && *d > top - 35.0).collect(), false)
        }
    };
    let n = ref_m.len().min(inp.usr.0.len());
    let ref_m = &ref_m[..n];
    let ref_v = &ref_v[..n];
    let mut usr_m = inp.usr.0[..n].to_vec();
    let usr_p = &inp.usr.1[..n];
    let usr_db = frame_rms_db(&inp.mic16, n);
    let gate = dsp::singing_gate(&usr_db);
    // singing: confident pitch, and (as live) sound above the background; the live judgement is on the
    // echo-cancelled microphone, so it holds up where music from speakers or a noisy room raises the
    // background (the loudness rule here then threw away real singing: final scores far under live ones)
    let mut usr_v: Vec<bool> = match &inp.voiced {
        Some(v) => (0..n).map(|i| usr_p[i] > 0.45 && (i.saturating_sub(2)..(i + 3).min(v.len())).any(|j| v[j])).collect(),
        None => (0..n).map(|i| usr_p[i] > 0.45 && usr_db[i] > gate).collect(),
    };

    // latency: shift your track so it best lines up with the singer (you hear the song late)
    let (mut best, mut best_lag) = (-1i64, 0i64);
    for k in -30i64..=150 {
        let (a0, b0, len) = if k >= 0 { (0, k as usize, n.saturating_sub(k as usize)) } else { ((-k) as usize, 0, n.saturating_sub((-k) as usize)) };
        let (mut u, mut r) = (Vec::new(), Vec::new());
        for j in 0..len {
            if ref_v[a0 + j] && usr_v[b0 + j] {
                u.push(usr_m[b0 + j]);
                r.push(ref_m[a0 + j]);
            }
        }
        let hits = shape_hits(&u, &r, 100.0) as i64;
        if hits > best {
            best = hits;
            best_lag = k;
        }
    }
    if best < 100 {
        best_lag = 20;   // under a second of matching singing: the estimate is noise, assume typical
    }
    roll(&mut usr_m, best_lag);
    roll(&mut usr_v, best_lag);
    // (a take with fewer frames than the lag: none of it lines up)
    if best_lag > 0 {
        usr_v[n.saturating_sub(best_lag as usize)..].iter_mut().for_each(|v| *v = false);
    } else if best_lag < 0 {
        usr_v[..((-best_lag) as usize).min(n)].iter_mut().for_each(|v| *v = false);
    }

    let mut ext = vec![f32::NAN; TOL];
    ext.extend((0..n).map(|i| if ref_v[i] { ref_m[i] } else { f32::NAN }));
    ext.extend(vec![f32::NAN; TOL]);
    let usr_line: Vec<f32> = (0..n).map(|i| if usr_v[i] { usr_m[i] } else { f32::NAN }).collect();
    // each moment scores against the closer of the singer's line and the note bar over it
    let bars_for_err = inp.bars.clone().unwrap_or_else(|| NoteTracker::bars(&(0..n).map(|i| if ref_v[i] { ref_m[i] } else { f32::NAN }).collect::<Vec<_>>()));
    let mut bar_ext = vec![f32::NAN; n + 2 * TOL];
    for &(s, e, m) in &bars_for_err {
        for f in s.max(0)..e.min(n as i64) {
            bar_ext[f as usize + TOL] = m as f32;
        }
    }
    let level = inp.level;
    let err_line = level.error(&ext, &usr_line);
    let err_bar = level.error(&bar_ext, &usr_line);
    let err: Vec<f32> = err_line.iter().zip(&err_bar)
        .map(|(l, b)| if !b.is_nan() && (l.is_nan() || b.abs() < l.abs()) { *b } else { *l })
        .map(|e| if e.is_nan() { 0.0 } else { e }).collect();
    let both: Vec<bool> = (0..n).map(|i| ref_v[i] && usr_v[i]).collect();
    let nb = both.iter().filter(|b| **b).count();
    let nr = ref_v.iter().filter(|b| **b).count();
    let mean_over = |f: &dyn Fn(usize) -> f32| -> f32 {
        if nb == 0 { 0.0 } else { (0..n).filter(|&i| both[i]).map(|i| f(i) as f64).sum::<f64>() as f32 / nb as f32 }
    };
    let accuracy = mean_over(&|i| level.credit(err[i]));
    let coverage = nb as f32 / nr.max(1) as f32;
    let score = dsp::score(accuracy, coverage);
    let mut eb: Vec<f32> = (0..n).filter(|&i| both[i]).map(|i| err[i]).collect();
    let bias = if nb > 0 { dsp::median(&mut eb) } else { 0.0 };
    let within50 = mean_over(&|i| if err[i].abs() <= 50.0 { 1.0 } else { 0.0 });

    // per lyric line, in recording time
    let mut lines = Vec::new();
    let off = inp.song_offset;
    for (i, (t, text)) in inp.lyrics.iter().enumerate() {
        let t0 = t - off;
        let t1 = inp.lyrics.get(i + 1).map_or(n as f64 * FRAME_S, |l| l.0 - off);
        let f0 = ((t0 / FRAME_S) as i64).max(0) as usize;
        let f1 = ((t1 / FRAME_S) as i64).clamp(0, n as i64) as usize;
        if f1 <= f0 || !text.chars().any(|c| c.is_alphanumeric()) {
            continue;
        }
        let rv = ref_v[f0..f1].iter().filter(|b| **b).count();
        let idx: Vec<usize> = (f0..f1).filter(|&k| both[k]).collect();
        let mut e: Vec<f32> = idx.iter().map(|&k| err[k]).collect();
        let acc = (!idx.is_empty()).then(|| idx.iter().map(|&k| level.credit(err[k])).sum::<f32>() / idx.len() as f32);
        lines.push(json!({"t0": t0, "t1": t1, "text": text, "ref_frames": rv,
            "coverage": idx.len() as f32 / rv.max(1) as f32, "accuracy": acc,
            "bias": (!e.is_empty()).then(|| dsp::median(&mut e))}));
    }

    // note by note: a note is hit when you hold it within a semitone for at least half its length (its first
    // 80 ms, where singers scoop up, don't count), forgiving up to 80 ms early/late
    let bars = inp.bars.clone().unwrap_or_else(|| NoteTracker::bars(&(0..n).map(|i| if ref_v[i] { ref_m[i] } else { f32::NAN }).collect::<Vec<_>>()));
    let mut notes = Vec::new();
    for (a, b, m) in bars {
        if b - a < 15 {
            continue;
        }
        let mut held = 0f32;
        for sh in (-8i64..=8).step_by(2) {
            let (a2, b2) = (a + 8 + sh, b + sh);
            if a2 < 0 || b2 > n as i64 {
                continue;
            }
            let ok = (a2..b2).filter(|&k| usr_v[k as usize] && fold(100.0 * (usr_m[k as usize] - m as f32)).abs() <= 100.0).count();
            held = held.max(ok as f32 / (b2 - a2) as f32);
        }
        notes.push(json!([a, b, m, (held * 100.0).round() / 100.0]));
    }
    let notes_hit = notes.iter().filter(|nt| nt[3].as_f64().unwrap_or(0.0) >= 0.5).count();

    let r2 = |v: f32| (v * 100.0).round() / 100.0;
    let result = json!({
        "track": inp.track, "recorded": inp.recorded, "mode": inp.mode, "blind": inp.blind,
        "notes": notes, "notes_hit": notes_hit, "notes_total": notes.len(),
        "score": score.round() as i64, "accuracy": accuracy, "coverage": coverage,
        "within50": within50, "bias_cents": bias, "latency_ms": best_lag * 10,
        "frame_s": FRAME_S, "lines": lines, "level": level.name(),
        "ref": (0..n).map(|i| ref_v[i].then(|| r2(ref_m[i]))).collect::<Vec<_>>(),
        // your contour moved by whole octaves to the note bar over it (else the singer's line), as the live lane
        // shows it: the singer's line can sit an octave off the bars, which put you an octave away from them
        "usr": (0..n).map(|i| usr_v[i].then(|| {
            let b = bar_ext[i + TOL];
            let base = if b.is_nan() { ref_m[i] } else { b };
            r2(if base.is_nan() { usr_m[i] } else { base + fold(100.0 * (usr_m[i] - base)) / 100.0 })
        })).collect::<Vec<_>>(),
        "err": (0..n).map(|i| both[i].then(|| err[i].round())).collect::<Vec<_>>(),
        "live": inp.live,
    });

    // remember the singer's line in song time, so the next sing of this track is scored live from it
    let learned = (!from_cache).then(|| {
        let k0 = (off / FRAME_S).round() as i64;
        let place = |i: usize| inp.places.iter().find(|p| (p.0..p.1).contains(&(i as i64))).map_or(k0, |p| p.2);
        let total = (inp.track.duration / FRAME_S) as usize + 1;
        let mut song = vec![f32::NAN; total];
        let mut covered = vec![false; total];
        let mut fp = vec![[0f32; FP_BANDS]; total];
        for i in 0..n {
            let j = i as i64 + place(i);
            if j >= 0 && (j as usize) < total {
                song[j as usize] = if ref_v[i] { ref_m[i] } else { f32::NAN };
                // learned only where Spotify's audio was actually heard
                covered[j as usize] = inp.audible.as_ref().map_or(true, |a| a.get(i).copied().unwrap_or(false));
            }
        }
        if let Some(f) = &inp.fingerprint {
            for (i, row) in f.iter().enumerate() {
                let j = i as i64 + place(i);
                if j >= 0 && (j as usize) < total {
                    fp[j as usize] = *row;
                }
            }
        }
        (song, covered, if inp.fingerprint.is_some() { fp } else { Vec::new() })
    });
    Output { result, learned }
}

/// numpy.roll(x, -k)
fn roll<T: Clone>(x: &mut [T], k: i64) {
    let n = x.len() as i64;
    if n == 0 {
        return;
    }
    let k = k.rem_euclid(n) as usize;
    x.rotate_left(k);
}

/// Song stretches (s) as take frames [from, to), by the take's places (Take::stretches: [from, to) take frames and
/// each one's song offset in frames). A take kept without its places has one: its song offset throughout.
pub fn take_frames(song_s: &[(f64, f64)], places: &[(i64, i64, i64)]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for &(a, b) in song_s {
        let (a, b) = ((a / FRAME_S).round() as i64, (b / FRAME_S).round() as i64);
        for &(lo, hi, k0) in places {
            let (s, e) = ((a - k0).max(lo).max(0), (b - k0).min(hi));
            if s < e {
                out.push((s, e));
            }
        }
    }
    out.sort();
    out
}

/// The figures a take's score is made of, kept as first scored when a stretch is first left out (to give back
/// exactly when nothing is left out any more).
const SCORED: [&str; 8] = ["score", "accuracy", "coverage", "within50", "bias_cents", "notes_hit", "notes_total", "lines"];

/// A kept take scored at `level` instead of the level it was sung at (Settings' level changed since), for showing:
/// its pitch errors worked out again from what it keeps (your line, the singer's line and the notes), then its
/// score, lines and any part left out as "Not the melody", as `leave_out` scores them. The level it was sung at and
/// its score then stay in "sung_level" and "sung_score". Nothing changes when it's at `level` already, or when it
/// keeps no lines to score from.
pub fn at_level(r: &mut Value, level: dsp::Level) {
    if dsp::Level::parse(r["level"].as_str()) == level {
        return;
    }
    let (sung, score) = (dsp::Level::parse(r["level"].as_str()).name(), r["score"].clone());
    if rescore_at(r, level) {
        r["sung_level"] = json!(sung);
        r["sung_score"] = score;
    }
}

/// `at_level`'s work, at any level (at its own, it should give the score it has: the test that it's exact).
pub fn rescore_at(r: &mut Value, level: dsp::Level) -> bool {
    let frames = |k: &str| -> Option<Vec<Option<f32>>> { r[k].as_array().map(|a| a.iter().map(|v| v.as_f64().map(|x| x as f32)).collect()) };
    let (Some(rf), Some(us)) = (frames("ref"), frames("usr")) else { return false };
    let n = rf.len().min(us.len());
    if n == 0 {
        return false;
    }
    let mut ext = vec![f32::NAN; TOL];
    ext.extend(rf[..n].iter().map(|v| v.unwrap_or(f32::NAN)));
    ext.extend(vec![f32::NAN; TOL]);
    let usr_line: Vec<f32> = us[..n].iter().map(|v| v.unwrap_or(f32::NAN)).collect();
    let mut bar_ext = vec![f32::NAN; n + 2 * TOL];
    for nt in r["notes"].as_array().into_iter().flatten() {
        let (Some(s), Some(e), Some(m)) = (nt[0].as_i64(), nt[1].as_i64(), nt[2].as_f64()) else { continue };
        for f in s.max(0)..e.min(n as i64) {
            bar_ext[f as usize + TOL] = m as f32;
        }
    }
    let err_line = level.error(&ext, &usr_line);
    let err_bar = level.error(&bar_ext, &usr_line);
    let err: Vec<Option<f32>> = (0..n).map(|i| {
        let (l, b) = (err_line[i], err_bar[i]);
        let e = if !b.is_nan() && (l.is_nan() || b.abs() < l.abs()) { b } else { l };
        (rf[i].is_some() && us[i].is_some()).then(|| if e.is_nan() { 0.0 } else { e.round() })
    }).collect();
    // scored as leaving out what was left out (or nothing), at the new level
    let left: Vec<(i64, i64)> = r["left_out"].as_array().map(|a| a.iter().filter_map(|p| Some((p[0].as_i64()?, p[1].as_i64()?))).collect()).unwrap_or_default();
    if let Some(first) = r.get("scored_first").cloned() {
        for k in SCORED {
            r[k] = first[k].clone();
        }
    }
    if let Some(o) = r.as_object_mut() {
        o.remove("scored_first");
        o.remove("left_out");
    }
    r["err"] = json!(err);
    r["level"] = json!(level.name());
    if left.is_empty() {
        leave_out(r, &[(-2, -1)]);       // nothing in the take: scores it all
        if let Some(o) = r.as_object_mut() {
            o.remove("scored_first");
            o.remove("left_out");
        }
    } else {
        leave_out(r, &left);
    }
    true
}

/// A scored take (analyze's result) with stretches of it (take frames) left out of its score: the song's learned
/// notes there follow a backing vocal or a harmony, not the melody (the "Not the melody" stretches the take page
/// marks, kept with the song). Their frames count towards neither accuracy nor coverage, nor their notes or lines,
/// worked out again from the take's own frames (`ref`, `err`, `notes`), which stay as they were; nothing left
/// out gives back the score as first scored. Credit is given at the take's own level (`level` in its result; an
/// older take with none: Relaxed), the level its `err` frames were worked out at. Returns whether the score's
/// figures changed.
pub fn leave_out(r: &mut Value, out: &[(i64, i64)]) -> bool {
    let before: Vec<Value> = SCORED.iter().map(|k| r[*k].clone()).collect();
    let out: Vec<(i64, i64)> = out.iter().copied().filter(|(a, b)| a < b).collect();
    if out.is_empty() {
        if let Some(first) = r.get("scored_first").cloned() {
            for k in SCORED {
                r[k] = first[k].clone();
            }
        }
        if let Some(o) = r.as_object_mut() {
            o.remove("scored_first");
            o.remove("left_out");
        }
        return SCORED.iter().zip(&before).any(|(k, b)| r[*k] != *b);
    }
    if r["scored_first"].is_null() {
        r["scored_first"] = Value::Object(SCORED.iter().map(|k| (k.to_string(), r[*k].clone())).collect());
    }
    let first = r["scored_first"].clone();
    let level = dsp::Level::parse(r["level"].as_str());
    let kept = |i: i64| !out.iter().any(|&(a, b)| (a..b).contains(&i));
    let frames = |k: &str| -> Vec<Option<f32>> { r[k].as_array().map(|a| a.iter().map(|v| v.as_f64().map(|x| x as f32)).collect()).unwrap_or_default() };
    let (rf, err) = (frames("ref"), frames("err"));
    let n = rf.len().min(err.len());
    // over frames [f0, f1): (singer's frames, your errors where you both sang)
    let part = |f0: usize, f1: usize| -> (usize, Vec<f32>) {
        let (f0, f1) = (f0.min(n), f1.min(n));
        let nr = (f0..f1).filter(|&i| kept(i as i64) && rf[i].is_some()).count();
        (nr, (f0..f1).filter(|&i| kept(i as i64) && rf[i].is_some()).filter_map(|i| err[i]).collect())
    };
    let (nr, e) = part(0, n);
    let nb = e.len();
    let mean = |f: &dyn Fn(f32) -> f32| if nb == 0 { 0.0 } else { e.iter().map(|x| f(*x) as f64).sum::<f64>() as f32 / nb as f32 };
    let accuracy = mean(&|x| level.credit(x));
    let coverage = nb as f32 / nr.max(1) as f32;
    let mut eb = e.clone();
    r["score"] = json!(dsp::score(accuracy, coverage).round() as i64);
    r["accuracy"] = json!(accuracy);
    r["coverage"] = json!(coverage);
    r["within50"] = json!(mean(&|x| if x.abs() <= 50.0 { 1.0 } else { 0.0 }));
    r["bias_cents"] = json!(if nb > 0 { dsp::median(&mut eb) } else { 0.0 });
    // a note is left out when its middle is
    let notes: Vec<f64> = r["notes"].as_array().map(|a| a.iter().filter(|nt| {
        let (a, b) = (nt[0].as_i64().unwrap_or(0), nt[1].as_i64().unwrap_or(0));
        kept((a + b) / 2)
    }).map(|nt| nt[3].as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
    r["notes_total"] = json!(notes.len());
    r["notes_hit"] = json!(notes.iter().filter(|h| **h >= 0.5).count());
    let lines: Vec<Value> = first["lines"].as_array().cloned().unwrap_or_default().into_iter().map(|mut l| {
        let (t0, t1) = (l["t0"].as_f64().unwrap_or(0.0), l["t1"].as_f64().unwrap_or(0.0));
        let f0 = ((t0 / FRAME_S) as i64).max(0) as usize;
        let f1 = ((t1 / FRAME_S) as i64).max(0) as usize;
        let (rv, mut e) = part(f0, f1);
        l["ref_frames"] = json!(rv);
        l["coverage"] = json!(e.len() as f32 / rv.max(1) as f32);
        l["accuracy"] = json!((!e.is_empty()).then(|| e.iter().map(|x| level.credit(*x)).sum::<f32>() / e.len() as f32));
        l["bias"] = json!((!e.is_empty()).then(|| dsp::median(&mut e)));
        l
    }).collect();
    r["lines"] = json!(lines);
    r["left_out"] = json!(out.iter().map(|&(a, b)| [a, b]).collect::<Vec<_>>());
    SCORED.iter().zip(&before).any(|(k, b)| r[*k] != *b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::Level;

    /// A melody of held notes 0.3-0.7 s long, stepping 1-4 semitones (the singer's line), 30 s.
    fn melody() -> Vec<f32> {
        let (mut v, mut seed, mut note) = (Vec::new(), 7u32, 62.0f32);
        while v.len() < 3000 {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let len = 30 + (seed >> 16) as usize % 40;
            let step = 1.0 + ((seed >> 8) % 4) as f32;
            note = if (seed >> 4) % 2 == 0 && note < 67.0 || note < 57.0 { note + step } else { note - step };
            v.extend(std::iter::repeat(note).take(len));
        }
        v.truncate(3000);
        v
    }

    /// The after-song score of a singer whose pitch is `sung(frame, singer's note)`, against `melody` learned.
    fn score(level: Level, sung: impl Fn(usize, f32) -> f32) -> (i64, f32) {
        let line = melody();
        let n = line.len();
        let usr: Vec<f32> = line.iter().enumerate().map(|(i, m)| sung(i, *m)).collect();
        let mic16: Vec<f32> = (0..n * 160).map(|k| 0.1 * (k as f32 * 0.05).sin()).collect();
        let out = analyze(Input {
            track: Track { id: "test".into(), name: "test".into(), duration: n as f64 * FRAME_S, ..Default::default() },
            recorded: "test".into(), mode: "sing", blind: false, lyrics: Vec::new(), song_offset: 0.0, places: Vec::new(),
            reference: Reference::Learned(line), usr: (usr, vec![0.9; n]), mic16, voiced: Some(vec![true; n]),
            live: Value::Null, fingerprint: None, audible: None, bars: None, level,
        });
        assert_eq!(out.result["level"], level.name());
        (out.result["score"].as_i64().unwrap(), out.result["accuracy"].as_f64().unwrap() as f32)
    }

    fn vibrato(i: usize, m: f32) -> f32 {
        m + 0.8 * (2.0 * std::f32::consts::PI * 6.0 * i as f32 * FRAME_S as f32).sin()   // 6 Hz, ±80 cents
    }

    #[test]
    fn a_take_shorter_than_the_assumed_lag_is_analysed_without_a_panic() {
        // under 20 frames tracked (0.2 s): no lag found, 20 assumed, more than the take has
        for n in [0usize, 5, 19] {
            let line = vec![60.0f32; n];
            let out = analyze(Input {
                track: Track { id: "test".into(), name: "test".into(), duration: 1.0, ..Default::default() },
                recorded: "test".into(), mode: "sing", blind: false, lyrics: Vec::new(), song_offset: 0.0, places: Vec::new(),
                reference: Reference::Learned(line.clone()), usr: (line, vec![0.9; n]), mic16: vec![0.1; n * 160], voiced: Some(vec![true; n]),
                live: Value::Null, fingerprint: None, audible: None, bars: None, level: Level::Relaxed,
            });
            assert!(out.result.is_object());
        }
    }

    #[test]
    fn a_perfect_singer_with_vibrato_scores_near_full_at_every_level() {
        for level in [Level::Relaxed, Level::Steady, Level::Pro] {
            let (s, acc) = score(level, vibrato);
            eprintln!("{level:?}: score {s}, accuracy {acc:.3}");
            // Relaxed is today's rules exactly: on the exact pitch, a ±80 cent swing spends half its time past
            // ±50 and scores about 90; Steady and Pro judge the pitch's centre too
            let need = if level == Level::Relaxed { 0.85 } else { 0.93 };
            assert!(acc >= need, "{level:?}: {acc}");
            // (at Pro a semitone step blurs the centre for a frame or two: 99)
            assert!(score(level, |_, m| m).0 >= 99, "{level:?}: a perfect singer without vibrato");
        }
    }

    #[test]
    fn a_steady_sharp_singer_scores_lower_the_stricter_the_level() {
        let sharp = |_: usize, m: f32| m + 0.4;
        let (r, s, p) = (score(Level::Relaxed, sharp).0, score(Level::Steady, sharp).0, score(Level::Pro, sharp).0);
        eprintln!("40 cents sharp: relaxed {r}, steady {s}, pro {p}");
        assert_eq!(r, 100, "within Relaxed's 50 cents");
        assert!(s < r && p < s, "{r} {s} {p}");
        // a wrong note held with vibrato on it is still wrong: the centre doesn't help a singer 40 cents sharp
        let (sv, pv) = (score(Level::Steady, |i, m| vibrato(i, m) + 0.4).0, score(Level::Pro, |i, m| vibrato(i, m) + 0.4).0);
        eprintln!("40 sharp with vibrato: steady {sv}, pro {pv}");
        assert!(sv <= s + 2 && pv <= p + 2, "{sv} {pv}");
    }

    #[test]
    fn relaxed_scores_are_as_they_were() {
        // as before levels: the old rule (dsp::credit: full within 50 cents, none at 150, 80 ms slack, exact pitch)
        assert_eq!(score(Level::Relaxed, |_, m| m), (100, 1.0));
        assert_eq!(score(Level::Relaxed, |_, m| m + 0.4).0, 100);
        // 70 cents sharp throughout: credit (150 - 70) / 100 = 0.8 on most frames; near a step the timing slack
        // finds the next note closer on a few (81, as before levels)
        let (s, acc) = score(Level::Relaxed, |_, m| m + 0.7);
        assert_eq!(s, 81);
        assert!(acc > 0.8 && acc < 0.82, "{acc}");
        // a 6 Hz ±80 cent vibrato on the exact pitch: half its time is past ±50 cents (score 89 before levels too)
        let (s, acc) = score(Level::Relaxed, vibrato);
        assert_eq!(s, 89);
        assert!((acc - 0.890).abs() < 5e-4, "{acc}");
    }

    /// A scored take of `n` frames by hand: the singer throughout, you at `err` cents (None: not singing).
    fn scored(n: usize, err: impl Fn(usize) -> Option<f32>) -> Value {
        let e: Vec<Option<f32>> = (0..n).map(&err).collect();
        let both: Vec<f32> = e.iter().flatten().copied().collect();
        let acc = both.iter().map(|x| dsp::credit(*x)).sum::<f32>() / both.len().max(1) as f32;
        let cov = both.len() as f32 / n as f32;
        json!({"score": dsp::score(acc, cov).round() as i64, "accuracy": acc, "coverage": cov, "within50": 0.5, "bias_cents": 0.0,
               "notes": [[0, 100, 60, 1.0], [150, 250, 62, 0.0]], "notes_hit": 1, "notes_total": 2,
               "lines": [{"t0": 0.0, "t1": 1.5, "text": "a", "ref_frames": 150}, {"t0": 1.5, "t1": 3.0, "text": "b", "ref_frames": 150}],
               "ref": vec![Some(60.0); n], "err": e})
    }

    #[test]
    fn a_stretch_left_out_counts_towards_neither_accuracy_nor_coverage() {
        // right for 200 frames, then 100 frames a tone and a half off (the notes there a backing vocal's)
        let mut r = scored(300, |i| Some(if i < 200 { 0.0 } else { 300.0 }));
        assert!((r["accuracy"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-3);
        let first = r.clone();
        assert!(leave_out(&mut r, &[(200, 300)]));
        assert_eq!(r["score"], 100);
        assert_eq!(r["accuracy"].as_f64(), Some(1.0));
        assert_eq!(r["coverage"].as_f64(), Some(1.0));
        // the note whose middle is left out isn't counted; the line all left out has nothing to score
        assert_eq!((r["notes_hit"].as_i64(), r["notes_total"].as_i64()), (Some(1), Some(1)));
        assert_eq!(r["lines"][1]["ref_frames"], 50);
        assert_eq!(r["lines"][0]["accuracy"].as_f64(), Some(1.0));
        assert_eq!(r["left_out"], json!([[200, 300]]));
        // put back: exactly as first scored
        assert!(leave_out(&mut r, &[]));
        assert_eq!(r, first);
        assert!(!leave_out(&mut r, &[]));
    }

    #[test]
    fn a_stretch_left_out_unsung_doesnt_lower_coverage() {
        // you sat out the last third (the harmony there isn't yours to sing)
        let mut r = scored(300, |i| (i < 200).then_some(0.0));
        assert!((r["coverage"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-3);
        leave_out(&mut r, &[(200, 300)]);
        assert_eq!(r["coverage"].as_f64(), Some(1.0));
        // left out again with another stretch: worked out from the take as first scored, not from the last time
        leave_out(&mut r, &[(0, 100)]);
        assert!((r["coverage"].as_f64().unwrap() - 0.5).abs() < 1e-3);
        assert_eq!(r["scored_first"]["coverage"], scored(300, |i| (i < 200).then_some(0.0))["coverage"]);
    }

    #[test]
    fn worked_out_again_it_scores_as_after_the_song() {
        // a take scored by analyze, its score worked out again from its frames with a stretch past its end left out
        let n = 3000;
        let line: Vec<f32> = (0..n).map(|i| if (i / 50) % 4 == 3 { f32::NAN } else { 60.0 + ((i / 200) % 5) as f32 }).collect();
        let usr: Vec<f32> = (0..n).map(|i| if i % 700 < 500 { line[(i + n - 20) % n] + 0.3 * ((i as f32) * 0.05).sin() } else { f32::NAN }).collect();
        let out = analyze(Input {
            track: Track::default(), recorded: String::new(), mode: "sing", blind: false, lyrics: vec![(0.0, "one".into()), (12.0, "two".into())],
            song_offset: 0.0, places: Vec::new(), reference: Reference::Learned(line), usr: (usr.iter().map(|u| if u.is_nan() { 0.0 } else { *u }).collect(),
            usr.iter().map(|u| if u.is_nan() { 0.0 } else { 0.9 }).collect()), mic16: vec![0.0; n * 160], voiced: Some(usr.iter().map(|u| !u.is_nan()).collect()),
            live: Value::Null, fingerprint: None, audible: None, bars: None, level: Level::Relaxed,
        });
        let mut r = out.result.clone();
        leave_out(&mut r, &[(n as i64 + 10, n as i64 + 20)]);
        assert!(r["score"].as_i64().unwrap() > 30);
        assert!((r["score"].as_i64().unwrap() - out.result["score"].as_i64().unwrap()).abs() <= 1);
        assert!((r["accuracy"].as_f64().unwrap() - out.result["accuracy"].as_f64().unwrap()).abs() < 0.005);
        assert!((r["coverage"].as_f64().unwrap() - out.result["coverage"].as_f64().unwrap()).abs() < 1e-6);
        assert_eq!(r["notes_hit"], out.result["notes_hit"]);
    }

    #[test]
    fn song_stretches_land_in_each_part_of_the_take_at_its_own_place() {
        // one place throughout: the take starts 30 s into the song
        assert_eq!(take_frames(&[(40.0, 45.0)], &[(0, i64::MAX, 3000)]), vec![(1000, 1500)]);
        // a stretch from before the take started only covers its start
        assert_eq!(take_frames(&[(25.0, 31.0)], &[(0, i64::MAX, 3000)]), vec![(0, 100)]);
        // a skip 20 s ahead at take frame 1000 (song 40 s): after it the take is 20 s further into the song, so 40-60 s
        // was never heard
        let places = [(0, 1000, 3000), (1000, i64::MAX, 5000)];
        assert_eq!(take_frames(&[(38.0, 75.0)], &places), vec![(800, 1000), (1000, 2500)]);
        assert_eq!(take_frames(&[(45.0, 55.0)], &places), vec![]);
    }

    #[test]
    fn a_take_shows_at_the_level_chosen_now() {
        // the same singing scored after the song at each level, and a Relaxed take shown at each level instead:
        // within a point of it, and the level it was sung at kept with its score
        let line = melody();
        let n = line.len();
        let usr: Vec<f32> = line.iter().enumerate().map(|(i, m)| m + if i < n / 2 { 0.4 } else { 0.9 }).collect();
        let sung = |level| analyze(Input {
            track: Track { id: "test".into(), name: "test".into(), duration: n as f64 * FRAME_S, ..Default::default() },
            recorded: "test".into(), mode: "sing", blind: false, lyrics: Vec::new(), song_offset: 0.0, places: Vec::new(),
            reference: Reference::Learned(line.clone()), usr: (usr.clone(), vec![0.9; n]),
            mic16: (0..n * 160).map(|k| 0.1 * (k as f32 * 0.05).sin()).collect(), voiced: Some(vec![true; n]),
            live: Value::Null, fingerprint: None, audible: None, bars: None, level,
        }).result;
        let relaxed = sung(Level::Relaxed);
        let mut same = relaxed.clone();
        at_level(&mut same, Level::Relaxed);
        assert_eq!(same, relaxed, "at its own level it's left as it is");
        for level in [Level::Steady, Level::Pro] {
            let want = sung(level)["score"].as_i64().unwrap();
            let mut r = relaxed.clone();
            at_level(&mut r, level);
            eprintln!("{level:?}: sung at it {want}, shown at it {}, sung at Relaxed {}", r["score"], relaxed["score"]);
            assert!((r["score"].as_i64().unwrap() - want).abs() <= 1, "{level:?}: {} against {want}", r["score"]);
            assert_eq!((r["level"].as_str(), r["sung_level"].as_str(), &r["sung_score"]), (Some(level.name()), Some("relaxed"), &relaxed["score"]));
        }
    }

    #[test]
    fn a_stretch_left_out_is_rescored_at_the_takes_own_level() {
        // (test-all: scoring-levels with exclude-stretch) 40 cents sharp for the first half, then a tone and a half
        // off on a backing vocal's notes, scored after the song at each level, then that half left out
        let line = melody();
        let n = line.len();
        let usr: Vec<f32> = line.iter().enumerate().map(|(i, m)| m + if i < n / 2 { 0.4 } else { 1.5 }).collect();
        let sharp = |_: usize, m: f32| m + 0.4;
        for level in [Level::Relaxed, Level::Steady, Level::Pro] {
            let out = analyze(Input {
                track: Track { id: "test".into(), name: "test".into(), duration: n as f64 * FRAME_S, ..Default::default() },
                recorded: "test".into(), mode: "sing", blind: false, lyrics: Vec::new(), song_offset: 0.0, places: Vec::new(),
                reference: Reference::Learned(line.clone()), usr: (usr.clone(), vec![0.9; n]),
                mic16: (0..n * 160).map(|k| 0.1 * (k as f32 * 0.05).sin()).collect(), voiced: Some(vec![true; n]),
                live: Value::Null, fingerprint: None, audible: None, bars: None, level,
            });
            let first = out.result["score"].as_i64().unwrap();
            // nothing of the take left out (a stretch past its end): worked out again it scores as after the song,
            // at its own level
            let mut r = out.result.clone();
            leave_out(&mut r, &[(n as i64 + 10, n as i64 + 20)]);
            assert!((r["score"].as_i64().unwrap() - first).abs() <= 1, "{level:?}: {} against {first}", r["score"]);
            // the off half left out: what's left is scored at the take's level, like a singer 40 cents sharp
            let mut r = out.result.clone();
            assert!(leave_out(&mut r, &[(n as i64 / 2, n as i64)]));
            let (got, want) = (r["score"].as_i64().unwrap(), score(level, sharp).0);
            eprintln!("{level:?}: first {first}, left out {got}, 40 sharp throughout {want}");
            assert!(got > first, "{level:?}: {got} {first}");
            assert!((got - want).abs() <= 3, "{level:?}: {got} against {want}");
            // and put back, as first scored
            leave_out(&mut r, &[]);
            assert_eq!(r["score"].as_i64(), Some(first));
        }
    }
}
