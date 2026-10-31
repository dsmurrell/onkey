//! Your vocal range, from the takes you've sung, and how each learned song sits against it.
//!
//! Each take keeps how long you sang on each semitone (`Voice`: confident, loud-enough frames of your own pitch,
//! before any octave is forgiven). A take scored since this was added has it in its result.json ("voice"); an
//! older one is measured once from its recording (mic.m4a), in the background while OnKey is idle (`start`).
//! Both land in one small cache, voice.json in the data folder, so the range is a sum of a few hundred rows.
//!
//! Your range is the 5th to 95th percentile of all those frames (a stray note or a pitch-tracker slip doesn't
//! move it), most comfortable the 25th to 75th. It is the range you've *used*, not the most you could reach:
//! it widens as you sing more songs. A song's fit compares its notes (its syllable bars, by how long each is
//! held) with it; as octaves are forgiven, a song mostly above (or below) you is fine an octave down (or up).
//! It reads data only: scoring is unchanged.
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::dsp;

/// The lowest semitone counted (MIDI 24 = C1); the histogram has `BINS` from here (up to B6).
pub const LO: i32 = 24;
pub const BINS: usize = 72;
/// CREPE's confidence over which a frame's pitch is trusted for the range (scoring trusts 0.45 and up; the range
/// wants fewer slips at the edges).
pub const CONFIDENT: f32 = 0.6;
/// Less singing than this in all (10 ms frames: 30 s) and there's no range to show yet.
pub const MIN_FRAMES: u64 = 3000;

/// How long you sang on each semitone (10 ms frames), MIDI `LO` up.
#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    pub counts: Vec<u64>,
}

impl Default for Voice {
    fn default() -> Voice {
        Voice { counts: vec![0; BINS] }
    }
}

impl Voice {
    /// Your frames from a take's pitch (MIDI, NaN unvoiced), CREPE confidence and raw mic loudness (dB): a
    /// confident pitch with the mic over the singing check's bar (`dsp::singing_gate`: you, not the song leaking
    /// in), as the after-song score counts singing.
    pub fn from_frames(midi: &[f32], conf: &[f32], db: &[f32]) -> Voice {
        let gate = dsp::singing_gate(db);
        let mut v = Voice::default();
        for i in 0..midi.len().min(conf.len()).min(db.len()) {
            if conf[i] > CONFIDENT && db[i] > gate && midi[i].is_finite() {
                v.add_note(midi[i], 1);
            }
        }
        v
    }

    /// `frames` more on the semitone nearest `midi` (outside C1-B6: not counted).
    pub fn add_note(&mut self, midi: f32, frames: u64) {
        let k = midi.round() as i32 - LO;
        if (0..BINS as i32).contains(&k) {
            self.counts[k as usize] += frames;
        }
    }

    pub fn add(&mut self, other: &Voice) {
        for (a, b) in self.counts.iter_mut().zip(&other.counts) {
            *a += b;
        }
    }

    pub fn frames(&self) -> u64 {
        self.counts.iter().sum()
    }

    /// The semitone (MIDI) under which `p` percent of the frames fall: the first whose running total reaches it.
    pub fn percentile(&self, p: f64) -> Option<i32> {
        let total = self.frames();
        if total == 0 {
            return None;
        }
        let want = (p / 100.0 * total as f64).max(1.0);
        let mut run = 0u64;
        for (k, c) in self.counts.iter().enumerate() {
            run += c;
            if run as f64 >= want {
                return Some(LO + k as i32);
            }
        }
        Some(LO + BINS as i32 - 1)
    }

    /// The share of the frames above `hi` and below `lo` (MIDI) with every note moved `shift` semitones.
    fn outside(&self, lo: i32, hi: i32, shift: i32) -> (f64, f64) {
        let total = self.frames().max(1) as f64;
        let (mut above, mut below) = (0u64, 0u64);
        for (k, c) in self.counts.iter().enumerate() {
            let m = LO + k as i32 + shift;
            if m > hi {
                above += c;
            } else if m < lo {
                below += c;
            }
        }
        (above as f64 / total, below as f64 / total)
    }

    /// As kept: {"lo": first semitone, "n": counts from it}, trimmed of empty ends.
    pub fn to_json(&self) -> Value {
        let first = self.counts.iter().position(|c| *c > 0);
        let last = self.counts.iter().rposition(|c| *c > 0);
        match (first, last) {
            (Some(a), Some(b)) => json!({"lo": LO + a as i32, "n": self.counts[a..=b]}),
            _ => json!({"lo": LO, "n": []}),
        }
    }

    pub fn from_json(v: &Value) -> Option<Voice> {
        let lo = v["lo"].as_i64()? as i32;
        let mut out = Voice::default();
        for (i, c) in v["n"].as_array()?.iter().enumerate() {
            let k = lo - LO + i as i32;
            if (0..BINS as i32).contains(&k) {
                out.counts[k as usize] = c.as_u64()?;
            }
        }
        Some(out)
    }
}

/// Your range (MIDI): low and high (5th and 95th percentile), most comfortable `mid_lo`-`mid_hi` (25th-75th).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub low: i32,
    pub high: i32,
    pub mid_lo: i32,
    pub mid_hi: i32,
    pub frames: u64,
}

impl Range {
    pub fn of(v: &Voice) -> Option<Range> {
        let frames = v.frames();
        if frames < MIN_FRAMES {
            return None;
        }
        Some(Range { low: v.percentile(5.0)?, high: v.percentile(95.0)?, mid_lo: v.percentile(25.0)?, mid_hi: v.percentile(75.0)?, frames })
    }

    pub fn to_json(&self) -> Value {
        json!({"low": self.low, "high": self.high, "mid_lo": self.mid_lo, "mid_hi": self.mid_hi,
               "low_name": note_name(self.low), "high_name": note_name(self.high),
               "mid_lo_name": note_name(self.mid_lo), "mid_hi_name": note_name(self.mid_hi),
               "seconds": (self.frames as f64 * dsp::FRAME_S).round()})
    }
}

/// A song against your range. Octaves are forgiven when scoring, so a song mostly out of reach is fine sung an
/// octave down (or up) when that puts it within you.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Fits,
    FewAbove,
    FewBelow,
    OctaveLower,
    OctaveHigher,
    Above,
    Below,
    Wider,
}

/// Out of range for at most this share of the song's sung time and it fits (a note or two at the edges).
const FITS: f64 = 0.05;
/// Over this share out of range on one side, the song is "mostly" there (else "a few notes").
const MOSTLY: f64 = 0.25;

impl Fit {
    /// The song's notes (`song`: frames held per semitone) against `you`.
    pub fn of(song: &Voice, you: &Range) -> Fit {
        let (a, b) = song.outside(you.low, you.high, 0);
        if a + b <= FITS {
            return Fit::Fits;
        }
        // an octave down (or up) that fits is the better advice even when only a few notes are out: a song sitting
        // high in your range is easier an octave lower (Daniel sang Scar Tissue, G3-D4, as G2-A3 against his F#2-C#4)
        let fits_at = |s: i32| {
            let (a2, b2) = song.outside(you.low, you.high, s);
            a2 + b2 <= FITS
        };
        if a > b && fits_at(-12) {
            return Fit::OctaveLower;
        }
        if b > a && fits_at(12) {
            return Fit::OctaveHigher;
        }
        // or at least half as far out as it was and within MOSTLY
        let better = |s: i32| {
            let (a2, b2) = song.outside(you.low, you.high, s);
            a2 + b2 <= FITS || (a2 + b2 <= MOSTLY && a2 + b2 <= 0.5 * (a + b))
        };
        if a > MOSTLY && better(-12) {
            return Fit::OctaveLower;
        }
        if b > MOSTLY && better(12) {
            return Fit::OctaveHigher;
        }
        if a > FITS && b > FITS {
            return Fit::Wider;
        }
        match (a >= b, a.max(b) > MOSTLY) {
            (true, false) => Fit::FewAbove,
            (true, true) => Fit::Above,
            (false, false) => Fit::FewBelow,
            (false, true) => Fit::Below,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Fit::Fits => "fits",
            Fit::FewAbove => "few_above",
            Fit::FewBelow => "few_below",
            Fit::OctaveLower => "octave_lower",
            Fit::OctaveHigher => "octave_higher",
            Fit::Above => "above",
            Fit::Below => "below",
            Fit::Wider => "wider",
        }
    }

    /// The one line shown on the song's card.
    pub fn label(self) -> &'static str {
        match self {
            Fit::Fits => "Fits your range",
            Fit::FewAbove => "A few notes above your range",
            Fit::FewBelow => "A few notes below your range",
            Fit::OctaveLower => "Mostly above you: sing it an octave lower",
            Fit::OctaveHigher => "Mostly below you: sing it an octave higher",
            Fit::Above => "Mostly above your range",
            Fit::Below => "Mostly below your range",
            Fit::Wider => "Wider than your range",
        }
    }

    /// Singable as it is or an octave away (the Library's "In your range").
    pub fn suits(self) -> bool {
        matches!(self, Fit::Fits | Fit::OctaveLower | Fit::OctaveHigher)
    }
}

/// A song's notes from its bars (start and end in 10 ms frames, MIDI): frames held per semitone.
pub fn song_notes(bars: &[(i64, i64, i64)]) -> Voice {
    let mut v = Voice::default();
    for b in bars {
        v.add_note(b.2 as f32, (b.1 - b.0).max(0) as u64);
    }
    v
}

/// A song's sung range (MIDI), 5th to 95th percentile of its notes by time held.
pub fn song_range(v: &Voice) -> Option<(i32, i32)> {
    Some((v.percentile(5.0)?, v.percentile(95.0)?))
}

pub fn note_name(m: i32) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[m.rem_euclid(12) as usize], m.div_euclid(12) - 1)
}

// ---- the cache: one row per take ----

const CACHE: &str = "voice.json";
static LOCK: Mutex<()> = Mutex::new(());

/// A take's row: its voice; nothing to give (no recording, or it couldn't be read); or still to be measured from
/// its recording (kept, so a take from before isn't read again on every look).
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Voice(Voice),
    Nothing,
    Pending,
}

type Rows = BTreeMap<String, Row>;

fn load(data: &Path) -> Rows {
    let v: Value = std::fs::read(data.join(CACHE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let row = |r: &Value| match Voice::from_json(r) {
        Some(v) => Row::Voice(v),
        None if r["pending"] == true => Row::Pending,
        None => Row::Nothing,
    };
    v["takes"].as_object().map(|m| m.iter().map(|(k, r)| (k.clone(), row(r))).collect()).unwrap_or_default()
}

fn save(data: &Path, rows: &Rows) {
    let takes: serde_json::Map<String, Value> = rows.iter().map(|(k, r)| (k.clone(), match r {
        Row::Voice(v) => v.to_json(),
        Row::Nothing => Value::Null,
        Row::Pending => json!({"pending": true}),
    })).collect();
    let _ = crate::store::write_atomic(&data.join(CACHE), json!({"version": 1, "takes": takes}).to_string().as_bytes());
}

fn take_dirs(data: &Path) -> Vec<PathBuf> {
    let mut d: Vec<PathBuf> = std::fs::read_dir(data.join("takes")).into_iter().flatten().flatten()
        .map(|e| e.path()).filter(|p| p.is_dir() && p.join("result.json").is_file()).collect();
    d.sort();
    d
}

fn name_of(d: &Path) -> String {
    d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// The takes' rows, with any take new since added (scored with its voice in result.json, else to be measured);
/// and the takes still to be measured, oldest first.
fn refresh(data: &Path) -> (Rows, Vec<PathBuf>) {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut rows = load(data);
    let dirs = take_dirs(data);
    let before = rows.clone();
    for d in &dirs {
        let name = name_of(d);
        if rows.contains_key(&name) {
            continue;
        }
        let r: Value = std::fs::read(d.join("result.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
        let row = match Voice::from_json(&r["voice"]) {
            Some(v) => Row::Voice(v),
            None if crate::takeaudio::recording(d).is_some() => Row::Pending,
            None => Row::Nothing,
        };
        rows.insert(name, row);
    }
    // deleted takes leave the range too
    let live: std::collections::HashSet<String> = dirs.iter().map(|d| name_of(d)).collect();
    rows.retain(|k, _| live.contains(k));
    if rows != before {
        save(data, &rows);
    }
    let pending = dirs.iter().filter(|d| rows.get(&name_of(d)) == Some(&Row::Pending)).cloned().collect();
    (rows, pending)
}

fn keep(data: &Path, name: String, v: Option<Voice>) {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut rows = load(data);
    rows.insert(name, v.map_or(Row::Nothing, Row::Voice));
    save(data, &rows);
}

/// All your takes' voice summed, and how many takes are still to be measured.
pub fn yours(data: &Path) -> (Voice, usize, usize) {
    let (rows, pending) = refresh(data);
    let mut all = Voice::default();
    let mut takes = 0;
    for v in rows.values().filter_map(|r| if let Row::Voice(v) = r { Some(v) } else { None }).filter(|v| v.frames() > 0) {
        all.add(v);
        takes += 1;
    }
    (all, takes, pending.len())
}

/// For the window: your range (null until there's enough singing), from how many takes, and how many are still
/// being measured.
pub fn summary(data: &Path) -> Value {
    let (v, takes, pending) = yours(data);
    json!({"range": Range::of(&v).map(|r| r.to_json()), "takes": takes, "pending": pending})
}

/// The Library's rows given each song's sung range and its fit against yours ("range": [lo, hi], "fit", "fit_label").
pub fn annotate(rows: &mut [Value], store: &crate::store::Store, data: &Path) {
    let (v, _, _) = yours(data);
    let you = Range::of(&v);
    for r in rows.iter_mut() {
        let Some(id) = r["track"]["id"].as_str() else { continue };
        let Some(bars) = store.song_bars(id) else { continue };
        if bars.len() < 20 {
            continue;
        }
        let notes = song_notes(&bars);
        if let Some((lo, hi)) = song_range(&notes) {
            r["range"] = json!([lo, hi]);
            r["range_names"] = json!([note_name(lo), note_name(hi)]);
        }
        if let Some(you) = &you {
            let f = Fit::of(&notes, you);
            r["fit"] = json!(f.key());
            r["fit_label"] = json!(f.label());
            r["fit_suits"] = json!(f.suits());
        }
    }
}

/// One take's voice from its recording: decoded, to 16 kHz, pitch-tracked as after a song (`pitch::track`).
pub fn measure(crepe: &mut crate::pitch::Crepe, rec: &Path) -> Result<Voice, String> {
    let (x, rate) = crate::takeaudio::decode_mono(rec).map_err(|e| e.to_string())?;
    let x16 = if (rate - 16_000.0).abs() < 1.0 {
        x
    } else {
        let mut out = Vec::with_capacity((x.len() as f64 * 16_000.0 / rate) as usize + 16);
        let mut r = crate::resample::Resampler::new(rate, 16_000.0);
        r.process(&x, &mut out);
        r.process(&[0.0; 4096], &mut out);
        out
    };
    let (midi, conf, _) = crate::pitch::track(crepe, &x16).map_err(|e| e.to_string())?;
    let db: Vec<f32> = (0..midi.len()).map(|i| dsp::frame_db(&x16, i)).collect();
    Ok(Voice::from_frames(&midi, &conf, &db))
}

const AFTER_LAUNCH: Duration = Duration::from_secs(150);
const QUIET_FOR: Duration = Duration::from_secs(60);

fn quiet() -> bool {
    crate::engine::quiet_for() >= QUIET_FOR && crate::learn::running() == 0
}

/// The takes from before the range was kept, measured once from their recordings: on their own thread at
/// background priority, newest first, one at a time and only while OnKey is idle (as takeaudio::start).
pub fn start(data: PathBuf) {
    std::thread::Builder::new().name("onkey-range".into()).spawn(move || {
        crate::platform::sys::background_thread();
        std::thread::sleep(AFTER_LAUNCH);
        let (_, mut todo) = refresh(&data);
        if todo.is_empty() {
            return;
        }
        todo.reverse();
        let log = crate::engine::log;
        log(&format!("range: {} older take{} to measure for your range (in the background, while OnKey is idle)",
                     todo.len(), if todo.len() == 1 { "" } else { "s" }));
        let mut crepe: Option<crate::pitch::Crepe> = None;
        let started = std::time::Instant::now();
        let (mut done, mut failed) = (0, 0);
        for d in &todo {
            if !quiet() {
                crepe = None;   // (nothing of it held while you sing)
                while !quiet() {
                    std::thread::sleep(Duration::from_secs(15));
                }
            }
            if !d.is_dir() {
                continue;
            }
            if crepe.is_none() {
                match crate::pitch::Crepe::new() {
                    Ok(c) => crepe = Some(c),
                    Err(e) => {
                        log(&format!("range: no pitch tracker ({e}); older takes not measured"));
                        return;
                    }
                }
            }
            let Some(rec) = crate::takeaudio::recording(d) else { continue };
            let v = measure(crepe.as_mut().unwrap(), &rec);
            if let Err(e) = &v {
                failed += 1;
                log(&format!("range: {} not measured: {e}", name_of(d)));
            } else {
                done += 1;
            }
            keep(&data, name_of(d), v.ok());
        }
        log(&format!("range: {done} older takes measured in {:.0} s ({failed} failed)", started.elapsed().as_secs_f64()));
    }).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `secs` seconds on each of `notes` (MIDI), as a voice.
    fn sung(notes: &[(i32, u64)]) -> Voice {
        let mut v = Voice::default();
        for (m, secs) in notes {
            v.add_note(*m as f32, secs * 100);
        }
        v
    }

    #[test]
    fn percentiles_ignore_stray_notes() {
        // 10 s on each of A2..E4 (45..64), plus 1 s of a pitch slip an octave up and one down
        let mut notes: Vec<(i32, u64)> = (45..=64).map(|m| (m, 10)).collect();
        notes.push((76, 1));
        notes.push((33, 1));
        let v = sung(&notes);
        let r = Range::of(&v).unwrap();
        assert_eq!((r.low, r.high), (45, 64), "5th/95th: the slips ignored");
        assert_eq!((r.mid_lo, r.mid_hi), (49, 60));
        assert_eq!(note_name(r.low), "A2");
        assert_eq!(note_name(r.high), "E4");
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(45), "A2");
    }

    #[test]
    fn frames_count_only_confident_and_loud() {
        // 200 frames sung at C3 (loud, confident), 200 of the song leaking in at G4 (quiet), 200 unsure at E2
        let mut midi = vec![48.0; 200];
        midi.extend(vec![67.0; 200]);
        midi.extend(vec![40.0; 200]);
        let mut conf = vec![0.9; 400];
        conf.extend(vec![0.3; 200]);
        let mut db = vec![-20.0; 200];
        db.extend(vec![-60.0; 200]);
        db.extend(vec![-20.0; 200]);
        // the room's quiet, for the gate to stand over
        midi.extend(vec![f32::NAN; 400]);
        conf.extend(vec![0.0; 400]);
        db.extend(vec![-70.0; 400]);
        let v = Voice::from_frames(&midi, &conf, &db);
        assert_eq!(v.frames(), 200);
        assert_eq!(v.percentile(50.0), Some(48));
    }

    #[test]
    fn too_little_singing_has_no_range() {
        assert!(Range::of(&sung(&[(50, 29)])).is_none());
        assert!(Range::of(&sung(&[(50, 30)])).is_some());
    }

    #[test]
    fn json_round_trip() {
        let v = sung(&[(45, 3), (50, 7), (64, 1)]);
        assert_eq!(Voice::from_json(&v.to_json()), Some(v));
        assert_eq!(Voice::from_json(&Voice::default().to_json()), Some(Voice::default()));
    }

    fn you() -> Range {
        // A2-E4, comfortable C3-A3
        Range { low: 45, high: 64, mid_lo: 48, mid_hi: 57, frames: 100_000 }
    }

    #[test]
    fn fit_labels() {
        let you = you();
        let even = |lo: i32, hi: i32| sung(&(lo..=hi).map(|m| (m, 10)).collect::<Vec<_>>());
        assert_eq!(Fit::of(&even(48, 62), &you), Fit::Fits);
        // one high note held briefly: still fits
        let mut v = even(48, 62);
        v.add_note(66.0, 30);
        assert_eq!(Fit::of(&v, &you), Fit::Fits);
        // a chorus reaching up two semitones past you: a few notes above (an octave down is too low)
        assert_eq!(Fit::of(&even(50, 66), &you), Fit::FewAbove);
        // high in your range with a note or two over, and all of it fits an octave down: that's the advice
        assert_eq!(Fit::of(&even(57, 66), &you), Fit::OctaveLower);
        assert_eq!(Fit::of(&even(43, 58), &you), Fit::FewBelow);
        // a soprano line, G4-E5: mostly above you, fine an octave down (G3-E4)
        assert_eq!(Fit::of(&even(67, 76), &you), Fit::OctaveLower);
        // a low line, A1-G#2: mostly below you, fine an octave up (A2-G#3)
        assert_eq!(Fit::of(&even(33, 44), &you), Fit::OctaveHigher);
        // lower still, F1-C2: an octave up (F2-C3) is still half below you, so just "below"
        assert_eq!(Fit::of(&even(29, 36), &you), Fit::Below);
        // wider than you both ways
        assert_eq!(Fit::of(&even(40, 70), &you), Fit::Wider);
        // way up and too wide to fit an octave down either
        assert_eq!(Fit::of(&even(60, 90), &you), Fit::Above);
    }

    #[test]
    fn suits_and_labels() {
        assert!(Fit::Fits.suits() && Fit::OctaveLower.suits() && Fit::OctaveHigher.suits());
        assert!(!Fit::FewAbove.suits() && !Fit::Wider.suits());
        assert_eq!(Fit::OctaveLower.label(), "Mostly above you: sing it an octave lower");
    }

    #[test]
    fn song_notes_weigh_by_time_held() {
        // a long low note and many short high ones
        let bars = vec![(0, 1000, 50), (1000, 1010, 70), (1010, 1020, 70)];
        let v = song_notes(&bars);
        assert_eq!(v.frames(), 1020);
        assert_eq!(song_range(&v), Some((50, 50)));
    }

    #[test]
    fn cache_follows_the_takes() {
        let data = std::env::temp_dir().join(format!("onkey-range-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let t = |n: &str| data.join("takes").join(n);
        // a new take with its voice, an old one with a recording to measure, one with neither
        for (n, r) in [("20261020-120000-A", json!({"voice": sung(&[(50, 40)]).to_json()})), ("20261019-120000-B", json!({})),
                       ("20261018-120000-C", json!({}))] {
            std::fs::create_dir_all(t(n)).unwrap();
            std::fs::write(t(n).join("result.json"), r.to_string()).unwrap();
        }
        std::fs::write(t("20261019-120000-B").join(crate::takeaudio::M4A), b"x").unwrap();
        let (v, takes, pending) = yours(&data);
        assert_eq!((v.frames(), takes, pending), (4000, 1, 1));
        keep(&data, "20261019-120000-B".into(), Some(sung(&[(60, 10)])));
        let (v, takes, pending) = yours(&data);
        assert_eq!((v.frames(), takes, pending), (5000, 2, 0));
        // a deleted take leaves the range
        std::fs::remove_dir_all(t("20261020-120000-A")).unwrap();
        let (v, takes, _) = yours(&data);
        assert_eq!((v.frames(), takes), (1000, 1));
        let s = summary(&data);
        assert!(s["range"].is_null(), "10 s of singing is too little");
        let _ = std::fs::remove_dir_all(&data);
    }
}
