//! Scoring maths and the rhythm fingerprint, ported line for line from the Python engine (live.py).
//! Pitch is in MIDI note numbers per 10 ms frame, NaN where there is no voice.
use realfft::RealFftPlanner;

use crate::pitch::HOP;

pub const FRAME_S: f64 = 0.01;
/// frames: a note sung up to 80 ms early/late still counts as on the singer's pitch
pub const TOL: usize = 8;
pub const FP_BANDS: usize = 8;

/// Signed cents error with octave errors forgiven, in [-600, 600).
pub fn fold(c: f32) -> f32 {
    (c + 600.0).rem_euclid(1200.0) - 600.0
}

/// Score credit for a cents error: full within 50 cents, none beyond 150 (Relaxed, the default level).
pub fn credit(err: f32) -> f32 {
    ((150.0 - err.abs()) / 100.0).clamp(0.0, 1.0)
}

/// How strictly a take is scored, chosen in Settings ("scoring_level") and kept with each take ("level").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Level {
    /// the rules OnKey always had: forgiving, for beginners and children
    #[default]
    Relaxed,
    /// tighter pitch and timing, for singers who are mostly in tune
    Steady,
    /// tight, for trained singers
    Pro,
}

/// One level's rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// full credit within this many cents of the note
    pub full: f32,
    /// no credit from this many cents off; in between it falls in a straight line
    pub zero: f32,
    /// timing slack (frames): each half-second may shift this far to line you up with the singer
    pub tol: usize,
    /// judge your pitch by its centre over this many frames, not frame by frame, so vibrato isn't marked down;
    /// 0: exact pitch. See `centre`.
    pub centre: usize,
}

/// Every level's numbers, in one place.
///   Relaxed: today's rules exactly (existing scores stay as they were): ±50 cents full, nothing at 150, 80 ms slack,
///            exact pitch only (a ±80 cent vibrato already scores ~90% here).
///   Steady:  ±35 full, nothing at 100, 60 ms slack.
///   Pro:     ±25 full, nothing at 70, 50 ms slack.
/// Steady and Pro judge your pitch by its centre over 170 ms: one cycle of a 6 Hz vibrato, the middle of singers'
/// usual 5-7 Hz (trained singers measure ~5.8-6.3 Hz and ±65-70 cents on average, up to ±100), so a regular wobble
/// around the note averages out while a held wrong note stays wrong. See `centre` for why it's a median-guided mean.
pub const LEVELS: [(Level, &str, Rules); 3] = [
    (Level::Relaxed, "relaxed", Rules { full: 50.0, zero: 150.0, tol: TOL, centre: 0 }),
    (Level::Steady, "steady", Rules { full: 35.0, zero: 100.0, tol: 6, centre: 17 }),
    (Level::Pro, "pro", Rules { full: 25.0, zero: 70.0, tol: 5, centre: 17 }),
];

/// `centre`: frames this far (cents) from the window's median are another note (a step either side), left out of
/// the mean; wider than any vibrato's swing from its middle (±100 cents)
const CENTRE_KEEP_CENTS: f32 = 150.0;

impl Level {
    pub fn rules(self) -> Rules {
        LEVELS.iter().find(|l| l.0 == self).unwrap().2
    }

    pub fn name(self) -> &'static str {
        LEVELS.iter().find(|l| l.0 == self).unwrap().1
    }

    /// From its name as kept ("relaxed", "steady", "pro"); anything else (an older take with none): Relaxed.
    pub fn parse(s: Option<&str>) -> Level {
        LEVELS.iter().find(|l| Some(l.1) == s).map_or(Level::Relaxed, |l| l.0)
    }

    /// The level chosen in Settings (development builds: ONKEY_LEVEL for the tools when nothing's chosen).
    pub fn current() -> Level {
        let set = crate::settings::get_str("scoring_level");
        Level::parse(set.or_else(|| crate::devtest::knob!("ONKEY_LEVEL")).as_deref())
    }

    /// For the lab's tools (examples): ONKEY_LEVEL, else Relaxed.
    pub fn from_env() -> Level {
        Level::parse(std::env::var("ONKEY_LEVEL").ok().as_deref())
    }

    /// Credit for a cents error at this level (Relaxed: exactly `credit`).
    pub fn credit(self, err: f32) -> f32 {
        if self == Level::Relaxed {
            return credit(err);
        }
        let r = self.rules();
        ((r.zero - err.abs()) / (r.zero - r.full)).clamp(0.0, 1.0)
    }

    /// Per-frame signed cents error of `usr` against `ref_ext` (padded by TOL frames either side, as
    /// `tolerant_error` wants with TOL), at this level's timing slack, on your pitch's centre where the level
    /// says so. Relaxed: exactly `tolerant_error(ref_ext, usr, TOL, 50)`. (Taking the closer of the exact pitch and
    /// the centre was tried: it let a sharp singer with vibrato earn credit each time the swing crossed the note.)
    pub fn error(self, ref_ext: &[f32], usr: &[f32]) -> Vec<f32> {
        let r = self.rules();
        let ext = &ref_ext[TOL - r.tol..ref_ext.len() - (TOL - r.tol)];
        if r.centre == 0 {
            return tolerant_error(ext, usr, r.tol, 50);
        }
        tolerant_error(ext, &centre(usr, r.centre), r.tol, 50)
    }
}

/// Your pitch's centre: per frame, the mean of the sung frames within `win` frames around it (NaN where this one
/// isn't sung), leaving out frames over CENTRE_KEEP_CENTS from their median, so a step to the next note or two
/// doesn't drag it. A plain mean blurred every note change (a perfect singer lost ~20% at Pro in simulation); a
/// plain median leaves a ±10-15 cent ripple of a vibrato whose cycle isn't the window's length; the mean
/// around the median cancels a 5-7 Hz vibrato to within a few cents and keeps note steps sharp.
pub fn centre(usr: &[f32], win: usize) -> Vec<f32> {
    let h = win / 2;
    let n = usr.len();
    let mut w: Vec<f32> = Vec::with_capacity(win + 1);
    (0..n).map(|i| {
        if usr[i].is_nan() {
            return f32::NAN;
        }
        w.clear();
        w.extend(usr[i.saturating_sub(h)..(i + h + 1).min(n)].iter().copied().filter(|v| !v.is_nan()));
        let m = median(&mut w);
        let (mut s, mut k) = (0f64, 0usize);
        for v in &w {
            if 100.0 * (v - m).abs() < CENTRE_KEEP_CENTS {
                s += *v as f64;
                k += 1;
            }
        }
        if k == 0 { usr[i] } else { (s / k as f64) as f32 }
    }).collect()
}

/// Signed cents error of `usr` vs the singer, forgiving timing but not pitch.
/// Each half-second window gets the one shift (within ±tol frames) that lines you up best with the singer,
/// and every frame in that window is measured at that shift. `ref_ext` covers the usr frames plus tol frames
/// either side.
pub fn tolerant_error(ref_ext: &[f32], usr: &[f32], tol: usize, win: usize) -> Vec<f32> {
    let n = usr.len();
    let shifts = 2 * tol + 1;
    let e = |k: usize, i: usize| fold(100.0 * (usr[i] - ref_ext[k + i]));
    let mut out = vec![f32::NAN; n];
    let mut w0 = 0;
    while w0 < n {
        let w1 = (w0 + win).min(n);
        let mut best = (f64::INFINITY, tol);
        for k in 0..shifts {
            let mut cost = 0.0f64;
            for i in w0..w1 {
                let v = e(k, i);
                cost += if v.is_nan() { 300.0 } else { v.abs().min(300.0) as f64 };
            }
            let c = cost / (w1 - w0) as f64 - if k == tol { 1e-6 } else { 0.0 };   // ties go to no shift
            if c < best.0 {
                best = (c, k);
            }
        }
        for i in w0..w1 {
            let v = e(best.1, i);
            // where the shifted singer line has a gap, measure against the unshifted frame
            out[i] = if v.is_nan() { e(tol, i) } else { v };
        }
        w0 = w1;
    }
    out
}

/// Frames that line up once your overall offset from the singer is removed (melody shape, for latency).
pub fn shape_hits(usr: &[f32], reference: &[f32], tol: f32) -> usize {
    let mut e: Vec<f32> = usr.iter().zip(reference).map(|(u, r)| fold(100.0 * (u - r))).collect();
    if e.is_empty() {
        return 0;
    }
    let med = median(&mut e.clone());
    e.iter_mut().filter(|v| fold(**v - med).abs() < tol).count()
}

pub fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) }
}

/// numpy-style percentile (linear interpolation)
pub fn percentile(v: &[f32], q: f32) -> f32 {
    let mut s: Vec<f32> = v.iter().copied().filter(|x| !x.is_nan()).collect();
    if s.is_empty() {
        return f32::NAN;
    }
    s.sort_by(|a, b| a.total_cmp(b));
    let pos = q / 100.0 * (s.len() - 1) as f32;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(s.len() - 1);
    s[lo] + (s[hi] - s[lo]) * (pos - lo as f32)
}

/// Loudness of 10 ms frame `i` (dB), over the 20 ms around it (outside `x`: silence).
pub fn frame_db(x: &[f32], i: usize) -> f32 {
    let (a, b) = (i as i64 * HOP as i64 - HOP as i64, i as i64 * HOP as i64 + HOP as i64);
    let s: f64 = (a.max(0)..b.min(x.len() as i64)).map(|k| (x[k as usize] as f64).powi(2)).sum();
    20.0 * (((s / (2 * HOP) as f64).sqrt() + 1e-9).log10()) as f32
}

/// Frames this quiet (dB) are digital silence: the time a skip or a gap filled in, or past the recording's end.
pub const SILENT_DB: f32 = -150.0;

/// The singing check's bar (dB, raw mic loudness per frame): 10 dB over the take's quietest fifth and within 40 dB
/// of its loudest. Over speakers this is what tells you from the song leaking into the mic (the leak is about as
/// loud all through; you're louder). Digital silence isn't the room's quiet and is left out: counted, a skip over
/// a fifth of the take dropped the bar to 40 dB under the loudest, and the leak counted as singing again (a take
/// with nobody singing and a skip over its intro scored 40).
pub fn singing_gate(db: &[f32]) -> f32 {
    let heard: Vec<f32> = db.iter().copied().filter(|d| *d > SILENT_DB).collect();
    if heard.is_empty() {
        return f32::INFINITY;
    }
    let noise = percentile(&heard, 20.0);
    let top = heard.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    (noise + 10.0).max(top - 40.0)
}

/// A take's score (0-100) from how in tune you were (`accuracy`, 0-1) and how much of the singer's part you sang
/// (`coverage`, 0-1). Singing 40% of the part earns full credit: some of the part is backing vocals, ad-libs or
/// bars that aren't really there, and the singing check is strict over speakers, so asking for more marked people
/// down for parts they couldn't sing. Singing very little still scores little (a tenth of the part for any credit
/// at all to count fully), so one good line isn't a good take.
pub fn score(accuracy: f32, coverage: f32) -> f32 {
    100.0 * accuracy * (0.3 * (coverage / 0.1).min(1.0) + 0.7 * (coverage / 0.4).min(1.0))
}

/// Rhythm fingerprint, (frames × 8) at 10 ms: how much the sound just changed in each of 8 frequency bands
/// (positive spectral flux, slow trend removed, normalised). Enough to place a recording in the song; not audio.
pub fn onset_envelope(x16: &[f32]) -> Vec<[f32; FP_BANDS]> {
    if x16.len() < 2048 {
        return Vec::new();
    }
    let n = 1 + x16.len() / 160;
    let mut padded = vec![0f32; 512];
    padded.extend_from_slice(x16);
    padded.extend(std::iter::repeat(0.0).take(512));
    let hann: Vec<f32> = (0..1024).map(|k| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * k as f32 / 1023.0).cos()).collect();
    const EDGES: [usize; 9] = [4, 7, 13, 23, 42, 76, 137, 248, 448];   // ~60 Hz to 7 kHz at 16 kHz
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(1024);
    let mut inbuf = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    let mut loud = vec![[0f32; FP_BANDS]; n];
    for (f, row) in loud.iter_mut().enumerate() {
        let a = f * 160;
        if a + 1024 > padded.len() {
            break;
        }
        for i in 0..1024 {
            inbuf[i] = padded[a + i] * hann[i];
        }
        fft.process(&mut inbuf, &mut spec).unwrap();
        for b in 0..FP_BANDS {
            let s: f32 = spec[EDGES[b]..EDGES[b + 1]].iter().map(|c| c.norm()).sum();
            row[b] = (1000.0 * s).ln_1p();
        }
    }
    let mut env = vec![[0f32; FP_BANDS]; n];
    for b in 0..FP_BANDS {
        let flux: Vec<f32> = (0..n).map(|f| if f == 0 { 0.0 } else { (loud[f][b] - loud[f - 1][b]).max(0.0) }).collect();
        // slow trend: mean over the 100 frames [f-50, f+49] (numpy convolve "same" with a box of 100)
        let mut csum = vec![0f64; n + 1];
        for f in 0..n {
            csum[f + 1] = csum[f] + flux[f] as f64;
        }
        for f in 0..n {
            let lo = f.saturating_sub(50);
            let hi = (f + 50).min(n);
            env[f][b] = flux[f] - ((csum[hi] - csum[lo]) / 100.0) as f32;
        }
        let mean = env.iter().map(|r| r[b] as f64).sum::<f64>() / n as f64;
        let std = (env.iter().map(|r| (r[b] as f64 - mean).powi(2)).sum::<f64>() / n as f64).sqrt() as f32;
        for r in env.iter_mut() {
            r[b] /= std + 1e-6;
        }
    }
    env
}

/// The fewest fingerprint frames `locate` works from (3 s): it gives (0, 0.0) for less.
pub const LOCATE_MIN_FRAMES: usize = 300;

/// The fewest 16 kHz samples whose `onset_envelope` has LOCATE_MIN_FRAMES frames (1 + samples / 160 of them):
/// a placement tried on less audio is wasted.
pub const LOCATE_MIN_SAMPLES: usize = (LOCATE_MIN_FRAMES - 1) * 160;

/// Where in the song (frame) a stretch of live audio starts, searching starts in [lo, hi). Returns
/// (frame, confidence): the peak correlation over the next best one more than 0.3 s away.
pub fn locate(live: &[[f32; FP_BANDS]], song: &[[f32; FP_BANDS]], lo: i64, hi: Option<i64>) -> (i64, f32) {
    let n = live.len() as i64;
    if n < LOCATE_MIN_FRAMES as i64 || (song.len() as i64) < n {
        return (0, 0.0);
    }
    let max_hi = song.len() as i64 - n + 1;
    let hi = hi.map_or(max_hi, |h| h.min(max_hi));
    let lo = lo.max(0);
    if hi <= lo {
        return (0, 0.0);
    }
    let c: Vec<f32> = (lo..hi).map(|k| {
        let mut acc = 0f32;
        for (t, l) in live.iter().enumerate() {
            let s = &song[(k as usize) + t];
            for b in 0..FP_BANDS {
                acc += s[b] * l[b];
            }
        }
        acc
    }).collect();
    let (i, &peak) = c.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
    if peak <= 0.0 {
        return (lo + i as i64, 0.0);
    }
    let second = c.iter().enumerate()
        .filter(|(j, _)| (*j as i64 - i as i64).abs() > 30)
        .map(|(_, v)| *v)
        .fold(f32::NEG_INFINITY, f32::max);
    let second = if second.is_finite() { second } else { 0.0 };
    (lo + i as i64, if second > 0.0 { peak / second } else { 9.0 })
}

/// The best few places (song frame, normalised correlation 0..1) where a stretch of live audio matches the song,
/// starts searched in [lo, hi): the local peaks of the correlation, at least 0.3 s apart, best first. Unlike
/// `locate`, the runners-up are kept: a repetitive intro matches one beat off nearly as well as in the right place,
/// and the tracker (tracker.rs) keeps both until the music tells them apart.
pub fn locate_peaks(live: &[[f32; FP_BANDS]], song: &[[f32; FP_BANDS]], lo: i64, hi: Option<i64>, k: usize) -> Vec<(i64, f32)> {
    let n = live.len();
    if n < LOCATE_MIN_FRAMES || song.len() < n {
        return Vec::new();
    }
    let max_hi = (song.len() - n + 1) as i64;
    let hi = hi.map_or(max_hi, |h| h.min(max_hi));
    let lo = lo.max(0);
    if hi <= lo {
        return Vec::new();
    }
    let live_norm = live.iter().flat_map(|r| r.iter()).map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
    // the song's energy over each window, from a running sum
    let mut csum = vec![0f64; song.len() + 1];
    for (i, r) in song.iter().enumerate() {
        csum[i + 1] = csum[i] + r.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
    }
    let c: Vec<f32> = (lo..hi).map(|k0| {
        let k = k0 as usize;
        let mut acc = 0f32;
        for (t, l) in live.iter().enumerate() {
            let s = &song[k + t];
            for b in 0..FP_BANDS {
                acc += s[b] * l[b];
            }
        }
        let norm = live_norm * (csum[k + n] - csum[k]).sqrt();
        if norm > 0.0 { (acc as f64 / norm) as f32 } else { 0.0 }
    }).collect();
    let mut order: Vec<usize> = (0..c.len()).collect();
    order.sort_by(|a, b| c[*b].total_cmp(&c[*a]));
    let mut peaks: Vec<(i64, f32)> = Vec::new();
    for i in order {
        if peaks.len() >= k || c[i] <= 0.0 {
            break;
        }
        let f = lo + i as i64;
        if peaks.iter().all(|(p, _)| (p - f).abs() > 30) {
            peaks.push((f, c[i]));
        }
    }
    peaks
}

/// How alike the song is to itself `lag` frames later, over `n` frames from frame `at` (normalised correlation
/// 0..1): high where a passage repeats at that distance (a repetitive intro a beat on, a chorus a verse on).
pub fn self_similarity(song: &[[f32; FP_BANDS]], at: i64, n: usize, lag: i64) -> f32 {
    let (a, b) = (at.max(0) as usize, (at + lag).max(0) as usize);
    if a + n > song.len() || b + n > song.len() {
        return 0.0;
    }
    let (mut ab, mut aa, mut bb) = (0f64, 0f64, 0f64);
    for t in 0..n {
        for k in 0..FP_BANDS {
            let (x, y) = (song[a + t][k] as f64, song[b + t][k] as f64);
            ab += x * y;
            aa += x * x;
            bb += y * y;
        }
    }
    if aa > 0.0 && bb > 0.0 { (ab / (aa * bb).sqrt()) as f32 } else { 0.0 }
}

/// Median filter as torchcrepe.filter.median: over the values inside the signal and not NaN, taking the
/// lower middle value; NaN where the window has none.
pub fn nan_median_filter(x: &[f32], win: usize) -> Vec<f32> {
    let h = (win / 2) as i64;
    let n = x.len() as i64;
    (0..n).map(|i| {
        let mut w: Vec<f32> = ((i - h)..=(i + h)).filter(|&k| k >= 0 && k < n).map(|k| x[k as usize])
            .filter(|v| !v.is_nan()).collect();
        if w.is_empty() {
            return f32::NAN;
        }
        w.sort_by(|a, b| a.total_cmp(b));
        w[(w.len() - 1) / 2]
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skip_leaves_the_singing_check_where_it_was() {
        // the song leaking from speakers at -40 dB, you singing over it at -20 dB a third of the time; then the
        // silence of a skip over a quarter of the take
        let mut db: Vec<f32> = (0..3000).map(|i| if i % 3 == 0 { -20.0 } else { -40.0 }).collect();
        let bar = singing_gate(&db);
        assert!(bar > -40.0 && bar < -20.0, "{bar}");
        db.extend(std::iter::repeat(frame_db(&[], 0)).take(1000));
        assert_eq!(singing_gate(&db), bar);
        assert_eq!(singing_gate(&db[3000..]), f32::INFINITY, "nothing heard: nothing passes");
    }

    #[test]
    fn folding() {
        assert_eq!(fold(1250.0), 50.0);
        assert_eq!(fold(-650.0), 550.0);
        assert_eq!(credit(40.0), 1.0);
        assert_eq!(credit(-200.0), 0.0);
    }

    #[test]
    fn tolerant_error_forgives_timing_not_pitch() {
        // the singer holds 60 then 62; you sing the same 5 frames late, then 70 cents sharp
        let mut reference = vec![f32::NAN; 8];
        reference.extend(std::iter::repeat(60.0).take(50));
        reference.extend(std::iter::repeat(62.0).take(50));
        reference.extend(vec![f32::NAN; 8]);
        let mut usr = vec![60.0f32; 5];
        usr.extend(std::iter::repeat(60.0).take(45));
        usr.extend(std::iter::repeat(62.7).take(50));
        let e = tolerant_error(&reference, &usr, TOL, 50);
        assert!(e[..50].iter().all(|v| v.abs() < 1e-3));
        assert!(e[60..].iter().all(|v| (v - 70.0).abs() < 1e-2));
    }
}

#[cfg(test)]
mod level_tests {
    use super::*;

    #[test]
    fn relaxed_is_the_rules_as_they_were() {
        assert_eq!(Level::default(), Level::Relaxed);
        assert_eq!(Level::parse(None), Level::Relaxed);
        assert_eq!(Level::parse(Some("pro")), Level::Pro);
        assert_eq!(Level::parse(Some("nonsense")), Level::Relaxed);
        for e in (-2000..2000).map(|c| c as f32 * 0.37) {
            assert_eq!(Level::Relaxed.credit(e).to_bits(), credit(e).to_bits());
        }
        // the error, frame for frame, is tolerant_error's with 80 ms slack on the exact pitch, gaps and all
        let (mut seed, mut next) = (1u32, 0.0f32);
        let mut rnd = || { seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345); (seed >> 8) as f32 / 16_777_216.0 };
        let ext: Vec<f32> = (0..1000 + 2 * TOL).map(|i| if i % 97 < 9 { f32::NAN } else { 60.0 + (i / 40) as f32 % 5.0 }).collect();
        let usr: Vec<f32> = (0..1000).map(|i| { next = 60.0 + (i / 40) as f32 % 5.0 + rnd() - 0.5; if rnd() < 0.1 { f32::NAN } else { next } }).collect();
        let a = Level::Relaxed.error(&ext, &usr);
        let b = tolerant_error(&ext, &usr, TOL, 50);
        assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
    }

    #[test]
    fn stricter_levels_give_less_credit_off_the_note() {
        for e in [0.0, 20.0, 30.0, 40.0, 60.0, 90.0, 120.0] {
            let (r, s, p) = (Level::Relaxed.credit(e), Level::Steady.credit(e), Level::Pro.credit(e));
            assert!(r >= s && s >= p, "{e}: {r} {s} {p}");
        }
        assert_eq!(Level::Steady.credit(35.0), 1.0);
        assert_eq!(Level::Steady.credit(100.0), 0.0);
        assert_eq!(Level::Pro.credit(25.0), 1.0);
        assert_eq!(Level::Pro.credit(70.0), 0.0);
    }

    #[test]
    fn the_centre_cancels_vibrato_and_keeps_note_steps() {
        // 6 Hz, ±80 cents around 60, then a step to 62
        let usr: Vec<f32> = (0..400).map(|i| (if i < 200 { 60.0 } else { 62.0 }) + 0.8 * (2.0 * std::f32::consts::PI * 0.06 * i as f32).sin()).collect();
        let c = centre(&usr, 17);
        for (rate, hz) in [(5.0f32, "5"), (7.0, "7")] {
            let u: Vec<f32> = (0..300).map(|i| 60.0 + 0.8 * (2.0 * std::f32::consts::PI * rate * 0.01 * i as f32).sin()).collect();
            let worst = centre(&u, 17)[20..280].iter().map(|v| (100.0 * (v - 60.0)).abs()).fold(0.0, f32::max);
            assert!(worst < 25.0, "{hz} Hz: {worst}");
        }
        assert!(c[20..190].iter().all(|v| (v - 60.0).abs() < 0.1), "within the first note");
        assert!(c[210..380].iter().all(|v| (v - 62.0).abs() < 0.1), "within the second");
        // a held wrong note stays wrong
        assert!(centre(&[60.4; 50], 17).iter().all(|v| (v - 60.4).abs() < 1e-4));
    }
}
