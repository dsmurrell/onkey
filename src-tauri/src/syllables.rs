//! Syllable bars: one bar per sung syllable, made when a song is learned. Each lyric line (already lined up
//! with the singer) is forced-aligned on the separated lead vocal with a speech model (wav2vec2, English
//! characters), words are split into syllables (CMU pronouncing dictionary counts, hyphenation for the text),
//! each syllable starts where its first letter was heard, and gets the singer's pitch over it. Port of an
//! earlier Python prototype. [lyrics-ml] Spanish, Portuguese, Italian, French and German lyrics (lang.rs detects
//! the language from the text) have their syllables counted and split by their own spelling rules; a song in
//! another script (Han, Kana, Hangul, Devanagari...) is left with LRCLIB's lines and no bars, and the word timer
//! isn't run on it at all (lang::alignable).
use std::collections::HashMap;

use hyphenation::{Hyphenator, Language, Load, Standard};
use ort::session::{builder::GraphOptimizationLevel, Session};

const LABELS: [char; 29] = ['-', '|', 'E', 'T', 'A', 'O', 'N', 'I', 'H', 'S', 'R', 'D', 'L', 'U', 'M', 'W', 'C', 'F', 'G', 'Y',
                            'P', 'B', 'V', 'K', '\'', 'X', 'J', 'Q', 'Z'];

pub type SyllableBar = (i64, i64, i32, String); // song frames, MIDI note, syllable

pub struct Syllabifier {
    counts: HashMap<String, u8>,
    hyph: Standard,
}

impl Syllabifier {
    pub fn new() -> Syllabifier {
        let counts = include_str!("../data/syllables-en.txt").lines().filter_map(|l| {
            let (w, k) = l.split_once(' ')?;
            Some((w.to_string(), k.parse().ok()?))
        }).collect();
        Syllabifier { counts, hyph: Standard::from_embedded(Language::EnglishUS).expect("hyphenation patterns") }
    }

    /// A word's syllables in `lang`: English as `split` (unchanged); the other languages by lang::syllables.
    pub fn split_in(&self, word: &str, lang: crate::lang::Lang) -> Vec<String> {
        match lang {
            crate::lang::Lang::En => self.split(word),
            other => crate::lang::syllables(word, other),
        }
    }

    /// "beautiful" -> ["beau", "ti", "ful"]: the number of syllables from the dictionary (or vowel groups),
    /// the split points from hyphenation, merged or split to match the count. (English.)
    pub fn split(&self, word: &str) -> Vec<String> {
        let w: String = word.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || *c == '\'').collect();
        let k = self.counts.get(&w).map(|k| *k as usize).unwrap_or_else(|| {
            let groups = w.split(|c: char| !"aeiouy".contains(c)).filter(|g| !g.is_empty()).count();
            (groups - usize::from(w.ends_with('e') && w.len() > 3 && groups > 1)).max(1)
        });
        let lower = word.to_lowercase();
        let mut parts: Vec<String> = if lower.len() == word.len() {
            let h = self.hyph.hyphenate(&lower);
            let mut v = Vec::new();
            let mut last = 0;
            for &b in &h.breaks {
                v.push(word[last..b].to_string());
                last = b;
            }
            v.push(word[last..].to_string());
            v
        } else {
            vec![word.to_string()]
        };
        while parts.len() > k {
            let last = parts.pop().unwrap();
            parts.last_mut().unwrap().push_str(&last);
        }
        while parts.len() < k {
            let i = (0..parts.len()).max_by_key(|&j| parts[j].chars().count()).unwrap();
            let p: Vec<char> = parts[i].chars().collect();
            if p.len() < 2 {
                break;
            }
            let h = p.len() / 2;
            let (a, b): (String, String) = (p[..h].iter().collect(), p[h..].iter().collect());
            parts.splice(i..=i, [a, b]);
        }
        parts
    }
}

pub struct WordAligner {
    session: Session,
    /// on DirectML (Windows): how to go to the CPU should a run fail there
    gpu: Option<crate::gpu::Fallback>,
}

impl WordAligner {
    pub fn load(path: &std::path::Path, threads: usize) -> Result<WordAligner, String> {
        let options = move |b: ort::session::builder::SessionBuilder| b.with_optimization_level(GraphOptimizationLevel::Level3)
            .and_then(|b| b.with_intra_threads(threads)).and_then(crate::pitch::lean);
        let loading = |e: ort::Error| format!("loading {}: {e}", path.display());
        // (on Windows on DirectML when it can be: gpu.rs)
        let (session, gpu) = crate::gpu::load("words", path, options).map_err(loading)?;
        Ok(WordAligner { session, gpu })
    }

    /// Log-probabilities (frames × 29 characters) for 16 kHz audio, and seconds per frame.
    fn logprobs(&mut self, audio: &[f32]) -> Result<(Vec<f32>, f64), String> {
        let r = self.logprobs_here(audio);
        // (on the CPU from now on, if it failed on DirectML)
        if let Err(e) = &r {
            if let Some(f) = self.gpu.take() {
                self.session = f.to_cpu(e).map_err(|e| e.to_string())?;
                return self.logprobs_here(audio);
            }
        }
        r
    }

    fn logprobs_here(&mut self, audio: &[f32]) -> Result<(Vec<f32>, f64), String> {
        let x = ndarray::Array2::from_shape_vec((1, audio.len()), audio.to_vec()).map_err(|e| e.to_string())?;
        let out = self.session.run(ort::inputs!["audio" => ort::value::Tensor::from_array(x).map_err(|e| e.to_string())?])
            .map_err(|e| e.to_string())?;
        let (shape, data) = out["logprobs"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let frames = shape[1] as usize;
        Ok((data.to_vec(), audio.len() as f64 / 16_000.0 / frames.max(1) as f64))
    }
}

pub struct Inputs<'a> {
    /// separated lead vocal, 16 kHz, from the take's start
    pub vocal16: &'a [f32],
    /// song time at the take's start (seconds)
    pub song_offset: f64,
    /// lyric lines lined up with the singer (song seconds)
    pub lyrics: &'a [(f64, String)],
    /// the cleaned learned line (song frames, MIDI or NaN)
    pub line: &'a [f32],
    /// every frame's pitch and confidence on the vocal (take frames), for syllables the line misses
    pub raw_midi: &'a [f32],
    pub raw_per: &'a [f32],
    /// per lyric line, when the whole-song alignment heard each letter: (word, letter, start s, end s); None
    /// for lines it didn't place (they're aligned on their own)
    pub heard: Option<&'a [Option<Vec<(usize, usize, f64, f64)>>]>,
}

/// A lyric line's words without bracketed parts (backing vocals sung over the lead).
fn lead_words(text: &str) -> Vec<String> {
    lead_words_with(text, fold_on())
}

fn lead_words_with(text: &str, fold: bool) -> Vec<String> {
    let mut clean = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c { '(' => depth += 1, ')' => { depth = 0usize.max(depth - 1) }, _ if depth == 0 => clean.push(c), _ => {} }
    }
    clean.split_whitespace().filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()) || (fold && fold_upper(w).chars().any(|c| c.is_ascii_uppercase())))
        .map(str::to_string).collect()
}

/// [latin-timing] Accented Latin letters are folded to A-Z before the English aligner sees them ("corazón" ->
/// CORAZON, "Straße" -> STRASSE, "cœur" -> COEUR); the lab measured this harmless for Spanish and German and a
/// little better for French (bench/align/ml_score.py). English lyrics are untouched (an occasional "café" becomes
/// CAFE rather than CAF). ONKEY_FOLD=0 turns it off: accented letters are dropped (CORAZN), as before 1 Nov 2026.
fn fold_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| crate::devtest::knob!("ONKEY_FOLD").as_deref() != Some("0"))   // (a dev-build knob: always on in a release)
}

/// A word in upper case with diacritics folded to A-Z (other characters kept as they are).
fn fold_upper(w: &str) -> String {
    let mut out = String::with_capacity(w.len());
    for c in w.to_uppercase().chars() {
        match c {
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' | 'Ă' | 'Ą' => out.push('A'),
            'Æ' => out.push_str("AE"),
            'Ç' | 'Ć' | 'Č' => out.push('C'),
            'Ď' | 'Đ' => out.push('D'),
            'É' | 'È' | 'Ê' | 'Ë' | 'Ē' | 'Ė' | 'Ę' | 'Ě' => out.push('E'),
            'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' | 'Į' => out.push('I'),
            'Ł' => out.push('L'),
            'Ñ' | 'Ń' | 'Ň' => out.push('N'),
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' | 'Ō' | 'Ő' => out.push('O'),
            'Œ' => out.push_str("OE"),
            'Ř' => out.push('R'),
            'Ś' | 'Š' | 'Ş' => out.push('S'),
            'ẞ' => out.push_str("SS"),
            'Ť' | 'Ţ' => out.push('T'),
            'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű' => out.push('U'),
            'Ý' | 'Ÿ' => out.push('Y'),
            'Ź' | 'Ż' | 'Ž' => out.push('Z'),
            c => out.push(c),
        }
    }
    out
}

/// A word as the aligner's letters see it: upper case, accents folded to A-Z (unless ONKEY_FOLD=0).
fn upper(w: &str) -> String {
    if fold_on() { fold_upper(w) } else { w.to_uppercase() }
}

/// The letters a syllable gives the aligner (counted like the tokens: A-Z and the apostrophe).
fn letter_count(p: &str) -> usize {
    if fold_on() {
        fold_upper(p).chars().filter(|c| c.is_ascii_uppercase() || *c == '\'').count()
    } else {
        p.chars().filter(|c| c.is_ascii_alphabetic() || *c == '\'').count()
    }
}

/// Where each lyric line is really sung: the whole song's lyrics forced-aligned at once on the lead vocal.
/// The given times (from LRCLIB, made for some recording of the song) can be seconds off, or belong to a
/// different version with another structure; this only uses the text. Lines without words, and lines the
/// aligner barely heard, keep their given time moved by the neighbouring lines' correction.
pub fn place_lines(aligner: &mut WordAligner, vocal16: &[f32], song_offset: f64, lyrics: &[(f64, String)]) -> Option<Vec<(f64, String)>> {
    place_lines_heard(aligner, vocal16, song_offset, lyrics).map(|p| p.0)
}

/// Letter times per lyric line from the whole-song alignment (see Inputs::heard).
pub type Heard = Vec<Option<Vec<(usize, usize, f64, f64)>>>;

/// place_lines, and when each letter of the lines it placed was heard.
pub fn place_lines_heard(aligner: &mut WordAligner, vocal16: &[f32], song_offset: f64, lyrics: &[(f64, String)]) -> Option<(Vec<(f64, String)>, Heard)> {
    let (logp, times) = song_logprobs(aligner, vocal16, song_offset)?;
    let times: Vec<f64> = times.into_iter().map(|t| t - HEARD_LAG_S).collect();
    place_on(&logp, &times, (song_offset, song_offset + vocal16.len() as f64 / 16_000.0), lyrics).map(|(l, h, _)| (l, h))
}

/// The word timer's stretch (s), see song_logprobs.
const WORDS_CHUNK_S: usize = 10;

/// How late the word timer hears a letter after it's sung (s): a learned song's line-up moves every frame's time back
/// by this (not a first play's live line-up: its words are lit by a pace spread from the line's start that is made to
/// run early already, and 0.05 s earlier lines cost JamendoLyrics' first plays 2.4 points of the right word lit). Measured
/// [align-night] 26 Oct on the align eval: line starts heard by the whole-song alignment sit a median +0.04 s
/// (JamendoLyrics' hand-marked onsets, 287 lines) and +0.055 s (the library's silver truth, 43 lines) after the
/// singing, words +0.05 / +0.07 s; 0.05 s centres them (more starts lines early, which is fine, but words drift
/// early: JamendoLyrics' word AAE grows past 0.08 s).
const HEARD_LAG_S: f64 = 0.05;

/// The word aligner's log-probabilities for the whole vocal, 10 s at a time, and each frame's song time.
/// (10 s, not 20: the word timer's activations grow with the stretch it's given, and they were the learning's peak
/// on an 8 GB Mac. memprobe words on Rolling in the Deep: peak footprint 1.36 GB at 20 s, 0.98 GB at 10 s, 0.82 GB
/// at 5 s, the time the same; lyric lines (evals' "exact") 95.0% within 0.3 s at both 20 and 10 s, median 0.045 ->
/// 0.046 s; the align eval (align-dev, --quick) as good or better but the library's lines within 0.3 s, 94.0 -> 92.8%
/// (within 0.5 s 94.0 -> 95.2%, worst stretch 6.0 -> 0 s), first plays with LRCLIB 8 s off 40.9 -> 64.5% within 1 s;
/// CSD bars within run-to-run noise. ONKEY_WORDS_CHUNK_S: another length, for measuring)
fn song_logprobs(aligner: &mut WordAligner, vocal16: &[f32], song_offset: f64) -> Option<(Vec<f32>, Vec<f64>)> {
    let chunk = crate::devtest::knob!("ONKEY_WORDS_CHUNK_S").and_then(|v| v.parse::<usize>().ok()).unwrap_or(WORDS_CHUNK_S) * 16_000;
    let mut logp = Vec::new();
    let mut times = Vec::new();         // song seconds of each frame (each chunk has its own frame length)
    for (ci, c) in vocal16.chunks(chunk).enumerate() {
        if c.len() < 1600 {
            break;
        }
        let (lp, r) = aligner.logprobs(c).ok()?;
        crate::progress::advance(ci + 1, vocal16.len().div_ceil(chunk));     // (the window's progress bar)
        let base = song_offset + (ci * chunk) as f64 / 16_000.0;
        times.extend((0..lp.len() / LABELS.len()).map(|k| base + k as f64 * r));
        logp.extend(lp);
    }
    Some((logp, times))
}

/// Lines placed, when each letter was heard, and per line how well its letters were heard (mean probability along
/// the alignment; None: not aligned).
type Placed = (Vec<(f64, String)>, Heard, Vec<Option<f32>>);

/// place_lines_heard on log-probabilities already made (song_logprobs) of the vocal between song seconds `t0` and `t1`;
/// also how well each line's letters were heard.
fn place_on(logp: &[f32], times: &[f64], (t0, t1): (f64, f64), lyrics: &[(f64, String)]) -> Option<Placed> {
    let idx: HashMap<char, usize> = LABELS.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    // only lines this recording covers (a learning play may be part of the song): the rest keep their time,
    // moved with the nearest placed line
    let inside = |t: f64| t >= t0 - 8.0 && t < t1 - 2.0;
    let mut toks = Vec::new();
    let mut owner: Vec<Option<(usize, usize, usize)>> = Vec::new();     // per token: (line, word, letter)
    let mut first: Vec<Option<(usize, usize)>> = Vec::new();   // per line: its tokens [from, to)
    for (i, (t, text)) in lyrics.iter().enumerate() {
        if !inside(*t) {
            first.push(None);
            continue;
        }
        let from = toks.len();
        for (wi, w) in lead_words(text).iter().enumerate() {
            let cs: Vec<usize> = upper(w).chars().filter_map(|c| idx.get(&c).copied()).filter(|&c| c > 1).collect();
            if cs.is_empty() {
                continue;
            }
            if !toks.is_empty() {
                toks.push(1);
                owner.push(None);
            }
            for (li, c) in cs.into_iter().enumerate() {
                toks.push(c);
                owner.push(Some((i, wi, li)));
            }
        }
        first.push((toks.len() > from).then(|| (if from == 0 { 0 } else { from + 1 }, toks.len())));
    }
    let spans = crate::ctc::align(logp, LABELS.len(), &toks, 0)?;
    let conf: Vec<Option<f32>> = first.iter().map(|f| f.map(|(a, b)| spans[a..b].iter().map(|s| s.2).sum::<f32>() / (b - a) as f32)).collect();
    let mut cs: Vec<f32> = conf.iter().flatten().copied().collect();
    let typical = median(&mut cs);
    // the aligner's start for each line it heard well; the others follow their neighbours' correction
    let mut fixed: Vec<Option<f64>> = first.iter().zip(&conf).map(|(f, c)| {
        let (a, _) = (*f)?;
        let t = times[spans[a].0];
        (c.unwrap_or(0.0) >= 0.25 * typical).then_some(t)
    }).collect();
    // a line placed more than 30 s from its given time is a mismatch, not a correction
    for (i, f) in fixed.iter_mut().enumerate() {
        if f.is_some_and(|t| (t - lyrics[i].0).abs() > 30.0) {
            *f = None;
        }
    }
    // the given timing, corrected by how far off it is around each line (the median over the 7 nearest placed
    // lines: a lyric file made for another recording can be off by different amounts in different sections)
    let placed: Vec<(usize, f64)> = fixed.iter().enumerate().filter_map(|(j, t)| t.map(|t| (j, t - lyrics[j].0))).collect();
    let local = section_moves(&placed, lyrics.len());
    // a heard start more than HYBRID_TOL from the corrected given time is the aligner going wrong there
    // (repeated words, a backing vocal): the corrected given time is safer
    let tol = hybrid_tol();
    let mut accepted = vec![false; lyrics.len()];
    for i in 0..lyrics.len() {
        let expect = lyrics[i].0 + local[i];
        fixed[i] = Some(match fixed[i] {
            Some(t) if tol <= 0.0 || (t - expect).abs() <= tol => { accepted[i] = true; t }
            _ => expect,
        });
    }
    let mut heard: Heard = vec![None; lyrics.len()];
    for (k, o) in owner.iter().enumerate() {
        if let Some((i, wi, li)) = *o {
            if accepted[i] {
                let (sa, sb, _) = spans[k];
                let end = times[sb.min(times.len() - 1)];
                heard[i].get_or_insert_with(Vec::new).push((wi, li, times[sa], end));
            }
        }
    }
    let mut out: Vec<(f64, String)> = lyrics.iter().zip(&fixed).map(|((_, s), t)| (t.unwrap().max(0.0), s.clone())).collect();
    for i in 1..out.len() {
        if out[i].0 < out[i - 1].0 + 0.05 {
            out[i].0 = out[i - 1].0 + 0.05;
        }
    }
    Some((out, heard, conf))
}

/// How far the given timing is off around each of `n` lines, from the lines placed by the words heard (`placed`:
/// line, heard start minus given time): the median over the 7 nearest placed lines (a lyric file made for another
/// recording can be off by different amounts in different sections), unless most lines agree on one move for the
/// whole song (half of them, 4 or more, within 0.6 s of the song's median move) and a section's move is more than
/// 1.5 s from it: that section is the aligner hearing a run of lines where they aren't sung (The Darkness's guitar
/// intro, left in the separated singer, heard as the first verse: its first 7 lines stored 5 s early while every
/// later line was right), and it takes the song's move.
fn section_moves(placed: &[(usize, f64)], n: usize) -> Vec<f64> {
    if placed.is_empty() {
        return vec![0.0; n];
    }
    let mut all: Vec<f32> = placed.iter().map(|p| p.1 as f32).collect();
    let whole = median(&mut all) as f64;
    let agree = placed.iter().filter(|p| (p.1 - whole).abs() <= 0.6).count();
    let consensus = agree >= 4 && 2 * agree >= placed.len();
    (0..n).map(|i| {
        let mut near: Vec<(usize, f64)> = placed.to_vec();
        near.sort_by_key(|(j, _)| j.abs_diff(i));
        let mut d: Vec<f32> = near.iter().take(7).map(|(_, d)| *d as f32).collect();
        let m = median(&mut d) as f64;
        if consensus && (m - whole).abs() > 1.5 { whole } else { m }
    }).collect()
}

/// How the words heard moved the lines when a song was learned: one offset for the song plus a steady drift (a
/// version at another speed), and how many of the lines heard agree with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// s, at the song's start
    pub offset: f64,
    /// s per s of song (within 2%)
    pub drift: f64,
    /// lines heard that move with the fit (within 0.6 s), and lines heard
    pub agree: usize,
    pub heard: usize,
    /// whether the fit was believed (else LRCLIB's times are kept as given)
    pub believed: bool,
}

/// Lines placed (song s), when each letter was heard (where believed), and the fit that decided.
pub type Placement = (Vec<(f64, String)>, Heard, Fit);

/// place_lines_heard, believed only where it's consistent, for the times kept with a learned song.
///
/// The whole song's alignment can go badly wrong: Dancing Queen's lines were stored 7-20 s early, four packed into
/// one second, and 12 songs learned on 15 Oct moved a median of over 3 s. So the lines heard are believed only
/// together: one offset plus a steady drift (within 2%: another version's speed) that at least half the lines
/// heard (4 or more) agree with within 0.6 s, and two thirds when it moves them more than 3 s. Those lines go
/// where they were heard; every other line moves with the fit. Without such a fit LRCLIB's times are kept
/// (the live line-up and sing mode refine them). Lines keep LRCLIB's order and at least half its spacing (up
/// to 0.5 s): never bunched.
pub fn place_lines_checked(aligner: &mut WordAligner, vocal16: &[f32], song_offset: f64, lyrics: &[(f64, String)]) -> Option<Placement> {
    place_lines_checked_and_heard(aligner, vocal16, song_offset, lyrics).map(|p| p.0)
}

/// place_lines_checked, and every letter the alignment heard (believed or not), kept with the song (store::save_words).
pub fn place_lines_checked_and_heard(aligner: &mut WordAligner, vocal16: &[f32], song_offset: f64, lyrics: &[(f64, String)]) -> Option<(Placement, Heard)> {
    let (placed, heard) = place_lines_heard(aligner, vocal16, song_offset, lyrics)?;
    let all = heard.clone();
    Some((check_placement(lyrics, placed, heard), all))
}

/// The consistency check of place_lines_checked, on its placement.
pub fn check_placement(lyrics: &[(f64, String)], placed: Vec<(f64, String)>, mut heard: Heard) -> Placement {
    let pts: Vec<(usize, f64, f64)> = (0..lyrics.len()).filter(|&i| heard[i].is_some() && lyrics[i].1.chars().any(|c| c.is_alphanumeric()))
        .map(|i| (i, lyrics[i].0, placed[i].0 - lyrics[i].0)).collect();
    let mut best = (0usize, f64::MAX, 0.0f64, 0.0f64);           // (agree, their error, offset, drift)
    for k in -20..=20 {
        let b = k as f64 * 0.001;
        let mut r: Vec<f32> = pts.iter().map(|p| (p.2 - b * p.1) as f32).collect();
        if r.is_empty() {
            break;
        }
        let a = median(&mut r) as f64;
        let near: Vec<f64> = pts.iter().map(|p| (p.2 - (a + b * p.1)).abs()).filter(|e| *e <= 0.6).collect();
        let (n, err) = (near.len(), near.iter().sum::<f64>());
        if n > best.0 || (n == best.0 && err < best.1 - 1e-9) {
            best = (n, err, a, b);
        }
    }
    let (agree, _, a, b) = best;
    let mid = pts.iter().map(|p| p.1).sum::<f64>() / pts.len().max(1) as f64;
    let big = (a + b * mid).abs() > 3.0;
    let believed = agree >= 4 && 2 * agree >= pts.len() && (!big || 3 * agree >= 2 * pts.len());
    let (a, b) = if believed { (a, b) } else { (0.0, 0.0) };
    let mut out: Vec<(f64, String)> = Vec::with_capacity(lyrics.len());
    for (i, (g, text)) in lyrics.iter().enumerate() {
        let fit = g + a + b * g;
        let inlier = heard[i].is_some() && (placed[i].0 - fit).abs() <= 0.6;
        if !inlier {
            heard[i] = None;
        }
        // a line that opens on a sung vowel ("oh oh ...", "whoa"): the aligner hears the vowel late, or not at all and
        // starts the line at its first word, so the line goes no later than the fit puts it
        let mut t = if inlier && opens_on_vowel(text) { placed[i].0.min(fit) } else if inlier { placed[i].0 } else { fit };
        if let (Some(prev), Some(pg)) = (out.last(), i.checked_sub(1).map(|j| lyrics[j].0)) {
            let gap = (0.5 * (g - pg)).clamp(0.0, 0.5);
            t = t.max(prev.0 + gap);
        }
        out.push((t.max(0.0), text.clone()));
    }
    (out, heard, Fit { offset: a, drift: b, agree, heard: pts.len(), believed })
}

/// Whether a lyric line opens on a sung vowel or filler ("Oh, you and I", "whoa oh oh", "aw ah"), letters doubled or
/// not. The whole-song alignment starts those lines late: on the align eval [align-night] 29% of them heard more
/// than 0.3 s late (Kinematic's "oh oh ..." lines by 0.8-1.1 s, Daniel Powter, Ben E. King, Ridgway), against 2.7% of
/// other lines.
fn opens_on_vowel(text: &str) -> bool {
    let Some(w) = lead_words(text).into_iter().next() else { return false };
    let mut d = String::new();
    for c in w.chars().skip_while(|c| !c.is_alphabetic()).take_while(|c| c.is_alphabetic()).flat_map(|c| c.to_lowercase()) {
        if !d.ends_with(c) {
            d.push(c);
        }
    }
    matches!(d.as_str(), "oh" | "ah" | "aw" | "whoa" | "woah" | "yeah" | "hey" | "na" | "la" | "m" | "uh" | "ha" | "eh")
}

/// How well a line's words are heard in the singer between song seconds `a` and `b` (the mean probability of its
/// letters along their forced alignment, 0..1): to compare two candidate times for a line (lab checks).
pub fn line_confidence(aligner: &mut WordAligner, vocal16: &[f32], song_offset: f64, a: f64, b: f64, text: &str) -> Option<f32> {
    let idx: HashMap<char, usize> = LABELS.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let (s0, s1) = (((a - song_offset) * 16_000.0).max(0.0) as usize, (((b - song_offset) * 16_000.0) as usize).min(vocal16.len()));
    if s1 < s0 + 8000 {
        return None;
    }
    let mut toks = Vec::new();
    for w in lead_words(text) {
        let cs: Vec<usize> = upper(&w).chars().filter_map(|c| idx.get(&c).copied()).filter(|&c| c > 1).collect();
        if cs.is_empty() {
            continue;
        }
        if !toks.is_empty() {
            toks.push(1);
        }
        toks.extend(cs);
    }
    if toks.is_empty() {
        return None;
    }
    let (logp, _) = aligner.logprobs(&vocal16[s0..s1]).ok()?;
    let spans = crate::ctc::align(&logp, LABELS.len(), &toks, 0)?;
    let letters: Vec<f32> = spans.iter().zip(&toks).filter(|(_, t)| **t > 1).map(|(s, _)| s.2).collect();
    Some(letters.iter().sum::<f32>() / letters.len().max(1) as f32)
}

/// A first play's lyrics lined up with the singer while it plays: the lines to show and how they were moved.
pub struct LiveLineUp {
    pub lyrics: Vec<(f64, String)>,
    /// lines heard in the window that agree on the move
    pub heard: usize,
    /// median move of the lines heard from LRCLIB's time (s; negative: LRCLIB's were late)
    pub moved: f64,
    /// how fast the singer gets through the words: seconds per syllable from one word's start to the next's
    /// (a low quantile, so a word filled by it comes early rather than late); None if too few words were heard
    pub pace: Option<f64>,
}

/// The least a live line-up's lines must be heard (the mean probability of their letters along the alignment) to be
/// shown. Forced alignment puts every word it's given somewhere, so lines can agree on a move where they aren't sung:
/// on the align eval [align-night] every line-up believed with a wrong move was heard 0.02-0.21 (Third Eye Blind's
/// first play: 3 lines "agreeing" on -8.3 s at 0.19, every line after it then 9-10 s early all song; Zara Larsson
/// -8.1 s at 0.17; Tom Odell -11.8 s at 0.07; Coldplay's Clocks -4.4 s at 0.02), every right one 0.25 or more.
const LIVE_MIN_HEARD: f32 = 0.23;

/// Seconds of the most recent separated singer a live line-up listens to (bounded work, however long the play).
pub const LIVE_WINDOW_S: f64 = 40.0;

/// A first play's lyrics lined up with the singer separated so far, while the song plays (engine.rs, each time a
/// new stretch of singer is separated). `vocal16` is the lead vocal from the take's start, `song_offset` the song
/// time there; `given` LRCLIB's lines and `shown` the lines shown now (the same lines, maybe moved already);
/// `now` the song time now. The last `LIVE_WINDOW_S` of singer are forced-aligned with the words, as when the
/// song is learned: lines heard there go where they were sung, lines still to come move by how far the recent
/// ones were off. Lines already shown (starting before `now` + 0.3 s) keep their time, so the line on screen
/// never jumps. None unless at least 2 lines were heard and two thirds of them agree (moves within 0.4 s of
/// their median): a few lines heard by chance in the wrong place mustn't move the rest.
pub fn line_up_live(aligner: &mut WordAligner, vocal16: &[f32], song_offset: f64, given: &[(f64, String)],
                    shown: &[(f64, String)], now: f64, prior: f64) -> Option<LiveLineUp> {
    if shown.len() != given.len() {
        return None;
    }
    let from = vocal16.len().saturating_sub((LIVE_WINDOW_S * 16_000.0) as usize);
    let off = song_offset + from as f64 / 16_000.0;
    let end = song_offset + vocal16.len() as f64 / 16_000.0;
    // (LINEUP_TRACE: why each line-up is refused, for the lab)
    let trace = |why: String| {
        #[cfg(feature = "devtest")]
        if crate::devtest::knob!("LINEUP_TRACE").is_some() {
            eprintln!("    line-up at {now:.1} (heard to {end:.1}): {why}");
        }
        let _ = &why;
    };
    // nothing to hear yet (an intro: no line due within 12 s of the window, however far off LRCLIB is): no work
    if !given.iter().any(|(t, s)| !s.trim().is_empty() && t + prior > off - 12.0 && t + prior < end + 12.0) {
        trace("no line due in the window".into());
        return None;
    }
    // lyrics in another script (Mandarin, Japanese, Korean...): the English word timer can't hear them, and
    // LRCLIB's times are kept as they are, so no model pass (lang::alignable)
    if !crate::lang::alignable(given) {
        trace("lyrics aren't in Latin letters".into());
        return None;
    }
    let Some((logp, times)) = song_logprobs(aligner, &vocal16[from..], off) else {
        trace("no word timings".into());
        return None;
    };
    // only the words of lines expected to be sung inside the window, by LRCLIB's time moved as far as it's
    // thought to be off: forced alignment puts every word it's given somewhere, so a line sung before the window
    // pushes the lines after it later (one test song's lines all moved 6 s that way) and one not sung yet pulls
    // the lines before it earlier. So the lines inside are chosen by a guess of the move, aligned, and chosen
    // again by the move found, until the two agree (within 0.5 s; 3 tries): a move found from the wrong lines
    // doesn't agree with the guess that chose them. Guesses: the last move believed (`prior`) and none (LRCLIB as
    // given); when both settle (a repeated chorus can fit another place), the one whose letters were heard best
    // wins. (With LRCLIB 8 s early, the first lines sung were found -5.6 s off, the lines not sung yet pulled in,
    // and believed: 3 of 20 test songs then kept their lyrics 9-20 s early all song.)
    let mut found: Option<(f32, LiveLineUp)> = None;
    for start in [Some(prior), (prior.abs() > 0.5).then_some(0.0)].into_iter().flatten() {
        let mut guess = start;
        for _ in 0..3 {
            let inside: Vec<(f64, String)> = given.iter().map(|(t, s)| {
                let at = t + guess;
                (at, if at >= off + 1.0 { s.clone() } else { String::new() })
            }).collect();
            let Some((mut placed, heard, conf)) = place_on(&logp, &times, (off, end), &inside) else {
                trace(format!("guess {guess:+.2}: nothing placed"));
                break;
            };
            for (p, g) in placed.iter_mut().zip(given) {
                p.1.clone_from(&g.1);
            }
            let moves: Vec<f32> = (0..given.len()).filter(|&i| heard[i].is_some()).map(|i| (placed[i].0 - given[i].0) as f32).collect();
            if moves.len() < 2 {
                trace(format!("guess {guess:+.2}: {} line(s) heard", moves.len()));
                // (one line heard, well off the guess: not believed, but where to look again. With LRCLIB late, the
                // lines sung at the end of the window are due after it and left out, and one heard line held the first
                // line-up back 5 s on a test song, 22-27 s after the first sung line; the search from it must still
                // find two lines or more that agree, and settle)
                if moves.len() == 1 && guess == start && ((moves[0] as f64) - guess).abs() >= 0.5 {
                    guess = moves[0] as f64;
                    continue;
                }
                break;
            }
            let moved = median(&mut moves.clone()) as f64;
            let agree = moves.iter().filter(|m| (**m as f64 - moved).abs() <= 0.4).count();
            if agree * 3 < moves.len() * 2 {
                trace(format!("guess {guess:+.2}: {agree} of {} lines agree on {moved:+.2} (moves {:?})", moves.len(),
                              moves.iter().map(|m| (m * 10.0).round() / 10.0).collect::<Vec<_>>()));
                break;
            }
            if (moved - guess).abs() < 0.5 {
                let heard_conf: Vec<f32> = (0..given.len()).filter(|&i| heard[i].is_some()).filter_map(|i| conf[i]).collect();
                let c = heard_conf.iter().sum::<f32>() / heard_conf.len().max(1) as f32;
                trace(format!("guess {guess:+.2}: {agree} of {} lines agree on {moved:+.2}, heard {c:.2}", moves.len()));
                if c < LIVE_MIN_HEARD {
                    trace(format!("guess {guess:+.2}: heard {c:.2}, under {LIVE_MIN_HEARD}: words forced onto singing that isn't them"));
                    break;
                }
                if !found.as_ref().is_some_and(|f| c <= f.0) {
                    found = Some((c, LiveLineUp { lyrics: keep_shown(shown, &placed, now), heard: agree, moved, pace: pace(given, &heard) }));
                }
                break;
            }
            trace(format!("guess {guess:+.2}: found {moved:+.2} ({agree} of {} lines), tried again", moves.len()));
            guess = moved;
        }
    }
    found.map(|f| f.1)
}

/// Which live line-ups to believe during a play. The first needs 3 lines heard, or 2 and a small move (1.5 s);
/// after that a line-up may follow a drift of up to 1 s from the last one believed. Anything else (near the end
/// of a song a few outro words can be heard 5-12 s away, as in 5 of 10 test songs) is believed only once the
/// next line-up agrees with it (within 0.5 s).
#[derive(Default)]
pub struct LineUps {
    believed: Option<f64>,
    pending: Option<f64>,
    settled: bool,
}

impl LineUps {
    /// The move of the last line-up believed (0 before any).
    pub fn believed(&self) -> f64 {
        self.believed.unwrap_or(0.0)
    }

    /// Whether the lyrics are where the singer is: a line-up from 3 lines heard or more believed, within 0.3 s of the
    /// one believed before it. (Then a first play stops separating the singer as it plays: learn::chunk_s. One
    /// line-up alone left a test song's lyrics 0.4 s early all song.)
    pub fn settled(&self) -> bool {
        self.settled
    }

    /// Whether to show this line-up (`moved`, from `heard` lines that agree).
    pub fn accept(&mut self, moved: f64, heard: usize) -> bool {
        let ok = match self.believed {
            None => heard >= 3 || moved.abs() <= 1.5,
            Some(m) => (moved - m).abs() <= 1.0,
        } || self.pending.is_some_and(|p| (moved - p).abs() <= 0.5);
        if ok {
            self.settled |= heard >= 3 && self.believed.is_some_and(|b| (moved - b).abs() <= 0.3);
            self.believed = Some(moved);
            self.pending = None;
        } else {
            self.pending = Some(moved);
        }
        ok
    }
}

/// Seconds per syllable between word starts over the lines heard: the 30th percentile (most words start no
/// sooner than this predicts, so a fill made with it is early rather than late), from 8 word steps or more, kept
/// within 0.12-0.6 s.
pub fn pace(lyrics: &[(f64, String)], heard: &Heard) -> Option<f64> {
    static SYL: std::sync::Mutex<Option<Syllabifier>> = std::sync::Mutex::new(None);
    let mut g = SYL.lock().unwrap();
    let syl = g.get_or_insert_with(Syllabifier::new);
    let langs = crate::lang::line_langs(lyrics);
    let mut steps: Vec<f32> = Vec::new();
    for (i, h) in heard.iter().enumerate() {
        let Some(h) = h else { continue };
        let words = lead_words(&lyrics[i].1);
        let mut starts: Vec<(usize, f64)> = Vec::new();
        for &(wi, _, t0, _) in h {
            match starts.iter_mut().find(|w| w.0 == wi) {
                Some(w) => w.1 = w.1.min(t0),
                None => starts.push((wi, t0)),
            }
        }
        starts.sort_by_key(|w| w.0);
        for p in starts.windows(2) {
            let n = words.get(p[0].0).map_or(1, |w| syl.split_in(w, langs[i]).len().max(1));
            let d = p[1].1 - p[0].1;
            if p[1].0 == p[0].0 + 1 && d > 0.0 && d < 3.0 {
                steps.push((d / n as f64) as f32);
            }
        }
    }
    if steps.len() < 8 {
        return None;
    }
    steps.sort_by(|a, b| a.total_cmp(b));
    Some((steps[steps.len() * 3 / 10] as f64).clamp(0.12, 0.6))
}

/// New times for lyric lines, except those already shown at song time `now` (starting before `now` + 0.3 s),
/// which keep theirs: the line on screen never jumps, and none comes up sooner than 0.3 s from now. In order.
pub fn keep_shown(shown: &[(f64, String)], new: &[(f64, String)], now: f64) -> Vec<(f64, String)> {
    let mut out: Vec<(f64, String)> = Vec::with_capacity(shown.len());
    for (i, (t, text)) in shown.iter().enumerate() {
        let mut t = if *t <= now + 0.3 { *t } else { new.get(i).map_or(*t, |n| n.0).max(now + 0.3) };
        if let Some(prev) = out.last() {
            t = t.max(prev.0);
        }
        out.push(((t * 100.0).round() / 100.0, text.clone()));
    }
    out
}

fn median(v: &mut Vec<f32>) -> f32 {
    crate::dsp::median(v)
}

pub fn make(aligner: &mut WordAligner, syl: &Syllabifier, inp: &Inputs) -> Vec<SyllableBar> {
    let idx: HashMap<char, usize> = LABELS.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let k0 = (inp.song_offset * 100.0).round() as i64;
    let voiced: Vec<f32> = inp.line.iter().copied().filter(|v| !v.is_nan()).collect();
    let register = if voiced.is_empty() { 60.0 } else { median(&mut voiced.clone()) };
    let mut bars: Vec<(i64, i64, Option<f32>, String)> = Vec::new();
    // the language of each line (lang.rs): how its words split into syllables (ONKEY_SYL_LANG=en: every line as
    // English, the way it was before [lyrics-ml], for the lab to compare: examples/barsml)
    let langs = match crate::devtest::knob!("ONKEY_SYL_LANG").as_deref() {
        Some("en") => vec![crate::lang::Lang::En; inp.lyrics.len()],
        _ => crate::lang::line_langs(inp.lyrics),
    };
    for (i, (t, text)) in inp.lyrics.iter().enumerate() {
        crate::progress::advance(i, inp.lyrics.len());     // (the window's progress bar)
        // bracketed parts are backing vocals sung over the lead: no bars for them
        let words = lead_words(text);
        if words.is_empty() {
            continue;
        }
        let nxt = inp.lyrics.get(i + 1).map_or(t + 6.0, |l| l.0);
        // when each letter was heard: from the whole-song alignment when this line was placed by it (one
        // alignment for the song keeps every word in order across lines), else this line on its own, in a
        // window that stops just into the next line (a wider one let a late line's last words land among
        // the next line's)
        let heard = inp.heard.and_then(|h| h.get(i)).and_then(|h| h.as_ref());
        let got: Option<(HashMap<usize, (f64, f64)>, HashMap<(usize, usize), f64>)> = match heard {
            Some(h) => {
                let mut wspan: HashMap<usize, (f64, f64)> = HashMap::new();
                let mut lstart: HashMap<(usize, usize), f64> = HashMap::new();
                for &(wi, li, t0, t1) in h {
                    let e = wspan.entry(wi).or_insert((t0, t1));
                    e.0 = e.0.min(t0);
                    e.1 = e.1.max(t1);
                    lstart.insert((wi, li), t0);
                }
                Some((wspan, lstart))
            }
            None => (|| {
                let (a, e) = ((t - 0.4).max(0.0), (nxt + 0.1).min(t + 12.0));
                let (s0, s1) = (((a - inp.song_offset) * 16_000.0) as i64, ((e - inp.song_offset) * 16_000.0) as i64);
                if s0 < 0 || s1 as usize > inp.vocal16.len() || s1 - s0 < 8000 {
                    return None;
                }
                let mut toks = Vec::new();
                let mut owners: Vec<(usize, usize)> = Vec::new();      // (word, letter index within the word)
                for (wi, w) in words.iter().enumerate() {
                    let cs: Vec<usize> = upper(w).chars().filter_map(|c| idx.get(&c).copied()).filter(|&c| c > 1).collect();
                    if cs.is_empty() {
                        continue;
                    }
                    if !toks.is_empty() {
                        toks.push(1);                               // '|' between words
                        owners.push((usize::MAX, 0));
                    }
                    for (li, c) in cs.into_iter().enumerate() {
                        toks.push(c);
                        owners.push((wi, li));
                    }
                }
                // (no letter the timer knows, as in a line in another script: no model run for it)
                if toks.is_empty() {
                    return None;
                }
                let (logp, ratio) = aligner.logprobs(&inp.vocal16[s0 as usize..s1 as usize]).ok()?;
                let spans = crate::ctc::align(&logp, LABELS.len(), &toks, 0)?;
                let mut wspan: HashMap<usize, (f64, f64)> = HashMap::new();
                let mut lstart: HashMap<(usize, usize), f64> = HashMap::new();
                for ((sa, sb, _), &(wi, li)) in spans.iter().zip(&owners) {
                    if wi == usize::MAX {
                        continue;
                    }
                    let (t0, t1) = (a + *sa as f64 * ratio, a + *sb as f64 * ratio);
                    let e = wspan.entry(wi).or_insert((t0, t1));
                    e.0 = e.0.min(t0);
                    e.1 = e.1.max(t1);
                    lstart.insert((wi, li), t0);
                }
                Some((wspan, lstart))
            })(),
        };
        let Some((wspan, lstart)) = got else { continue };
        for (wi, w) in words.iter().enumerate() {
            let Some(&(w0, mut w1)) = wspan.get(&wi) else { continue };
            // a word lasts until the next word starts, if that's soon (singers hold the vowel)
            let nxt0 = wspan.iter().filter(|(k, _)| **k > wi).map(|(_, v)| v.0).fold(w1 + 0.4, f64::min);
            w1 = w1.max(nxt0.min(w1 + 0.8));
            let parts = syl.split_in(w, langs[i]);
            let f0 = (w0 * 100.0) as i64;
            let f1 = (w1 * 100.0) as i64;
            let seg: Vec<f32> = (f0..f1).map(|f| inp.line.get(f as usize).copied().unwrap_or(f32::NAN)).collect();
            let len = seg.len() as i64;
            // where each later syllable starts: when the aligner heard its first letter; else even
            let letters: Vec<usize> = parts.iter().map(|p| letter_count(p)).collect();
            let mut cuts = Vec::new();
            let mut pos = 0;
            for k in 1..parts.len() {
                pos += letters[k - 1];
                if let Some(&ts) = lstart.get(&(wi, pos)) {
                    if ts > w0 && ts < w1 {
                        cuts.push(((ts - w0) * 100.0).round() as i64);
                    }
                }
            }
            let mut edges = vec![0];
            let ok = cuts.len() == parts.len() - 1 && {
                let mut all = vec![0];
                all.extend(&cuts);
                all.push(len);
                all.windows(2).all(|p| p[1] - p[0] >= 4)
            };
            if ok {
                edges.extend(cuts);
            } else {
                edges.extend((1..parts.len()).map(|k| len * k as i64 / parts.len() as i64));
            }
            edges.push(len);
            for (k, sy) in parts.iter().enumerate() {
                let (a1, b1) = (edges[k], edges[k + 1]);
                let mut v: Vec<f32> = seg[a1 as usize..b1 as usize].iter().copied().filter(|x| !x.is_nan()).collect();
                let pitch = if v.len() as f32 >= 5f32.max(0.3 * (b1 - a1) as f32) {
                    Some(median(&mut v))
                } else {
                    // the syllable's middle 60% on the raw track, fairly confident frames, in the song's register
                    let (m0, m1) = (f0 + a1 + (b1 - a1) / 5 - k0, f0 + b1 - (b1 - a1) / 5 - k0);
                    // only pitches near the singer's local register (the cleaned line over ±3 s): rejects an
                    // instrument the separation let through
                    let mut lw: Vec<f32> = ((f0 + a1 - 300).max(0)..(f0 + b1 + 300)).filter_map(|f| inp.line.get(f as usize).copied())
                        .filter(|v| !v.is_nan()).collect();
                    let reg = if lw.len() >= 20 { median(&mut lw) } else { register };
                    let mut c: Vec<f32> = (m0.max(0)..m1.max(0)).filter_map(|j| {
                        let j = j as usize;
                        let mut m = *inp.raw_midi.get(j)?;
                        if (m - reg).abs() > 9.0 {
                            m -= 12.0 * ((m - reg) / 12.0).round();
                        }
                        (inp.raw_per.get(j).copied().unwrap_or(0.0) > 0.25 && (m - reg).abs() <= 7.0).then_some(m)
                    }).collect();
                    if c.len() >= 3 {
                        Some(median(&mut c))
                    } else if let Some(last) = bars.last().filter(|b| b.2.is_some() && f0 + a1 - b.1 < 100) {
                        last.2                                  // nothing to measure: hold the last note
                    } else {
                        None
                    }
                };
                bars.push((f0 + a1, f0 + b1, pitch, sy.clone()));
            }
        }
    }
    // a bar ends where the voice fades (15 dB below the bar's loudest) and at the first pause in singing
    let nfv = inp.vocal16.len() / 160;
    let vdb: Vec<f32> = (0..nfv).map(|i| {
        let f = &inp.vocal16[i * 160..(i + 1) * 160];
        10.0 * (f.iter().map(|x| x * x).sum::<f32>() / 160.0 + 1e-10).log10()
    }).collect();
    for b in bars.iter_mut() {
        let (a0, b0) = (b.0 - k0, b.1 - k0);
        if b0 - a0 < 8 || a0 < 0 || b0 as usize > nfv {
            continue;
        }
        let seg = &vdb[a0 as usize..b0 as usize];
        let top = seg.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if let Some(last) = seg.iter().rposition(|d| *d > top - 15.0) {
            b.1 = b.1.min(b.0 + last as i64 + 5);
        }
        // where the singing first stops, read from the voice itself (confident pitch near the singer's register)
        let mut lw: Vec<f32> = ((b.0 - 300).max(0)..b.1 + 300).filter_map(|f| inp.line.get(f as usize).copied()).filter(|v| !v.is_nan()).collect();
        let reg = if lw.len() >= 20 { median(&mut lw) } else { register };
        let v: Vec<bool> = (a0..b0).map(|j| {
            let j = j as usize;
            let Some(&m) = inp.raw_midi.get(j) else { return false };
            let m = if (m - reg).abs() > 9.0 { m - 12.0 * ((m - reg) / 12.0).round() } else { m };
            inp.raw_per.get(j).copied().unwrap_or(0.0) > 0.3 && (m - reg).abs() <= 9.0
        }).collect();
        if let Some(i) = v.iter().position(|x| *x) {
            let (mut last, mut gap) = (i, 0);
            for (j, &x) in v.iter().enumerate().skip(i) {
                if x {
                    last = j;
                    gap = 0;
                } else {
                    gap += 1;
                    if gap > 25 {
                        break;
                    }
                }
            }
            b.1 = b.1.min(b.0 + last as i64 + 5);
        }
    }
    // a syllable sung across several steady notes (a run) becomes one bar per note: the first keeps the
    // syllable, the others are "~" (its continuation); notes from the voice (confident pitch near the bar's)
    let mut split = Vec::with_capacity(bars.len());
    for b in bars {
        let Some(p) = b.2 else { split.push(b); continue };
        let (j0, j1) = (b.0 - k0, b.1 - k0);
        if b.1 - b.0 < 40 || j0 < 0 || j1 as usize > inp.raw_midi.len() {
            split.push(b);
            continue;
        }
        let m: Vec<f32> = (j0..j1).map(|j| {
            let j = j as usize;
            let mut m = inp.raw_midi[j];
            if (m - p).abs() > 9.0 {
                m -= 12.0 * ((m - p) / 12.0).round();
            }
            if inp.raw_per.get(j).copied().unwrap_or(0.0) < 0.3 || (m - p).abs() > 7.0 { f32::NAN } else { m }
        }).collect();
        let mut tr = crate::notes::NoteTracker::with(8, 70.0, 10, 6, 15);
        tr.feed(&m, 0);
        tr.finish();
        let notes: Vec<_> = tr.notes.into_iter().filter(|n| n.1 - n.0 >= 15).collect();
        if notes.len() < 2 {
            split.push(b);
            continue;
        }
        for (q, n) in notes.iter().enumerate() {
            let s2 = if q == 0 { b.0 } else { b.0 + n.0 };
            let e2 = b.0 + notes.get(q + 1).map_or(m.len() as i64, |x| x.0);
            split.push((s2, e2, Some(n.2 as f32), if q == 0 { b.3.clone() } else { "~".to_string() }));
        }
    }
    let mut bars = split;
    // one syllable at a time
    bars.sort_by_key(|b| b.0);
    for k in 0..bars.len().saturating_sub(1) {
        bars[k].1 = bars[k].1.min(bars[k + 1].0);
    }
    bars.into_iter().filter(|b| b.1 - b.0 >= 3).filter_map(|b| Some((b.0, b.1, b.2?.round() as i32, b.3))).collect()
}

/// How far (s) a heard line start may be from the corrected given time (ONKEY_HYBRID_TOL overrides it, for
/// testing; 0 trusts the aligner everywhere).
fn hybrid_tol() -> f64 {
    crate::devtest::knob!("ONKEY_HYBRID_TOL").and_then(|v| v.parse().ok()).unwrap_or(1.0)
}

/// Syllable bars refined with a note transcription (GAME): each bar takes the note that covers most of it
/// (if it covers 40%), a bar spanning several notes of 0.15 s or more splits at their boundaries (the later
/// parts "~", the syllable carried on), and a bar hardly any note covers (under 20%) is dropped: nobody is
/// singing there (a stray bar is worse than a missing one). GAME's octave is trusted. Notes in song frames:
/// (start, end, MIDI). On 18 sung test songs: right note 77.5% -> 85.9%, stray bars 4.3% -> 2.2%.
pub fn refine_with_notes(bars: &[SyllableBar], notes: &[(f64, f64, f32)]) -> Vec<SyllableBar> {
    let ov = |a0: f64, a1: f64, b0: f64, b1: f64| (a1.min(b1) - a0.max(b0)).max(0.0);
    let mut out = Vec::with_capacity(bars.len());
    for b in bars {
        let (s, e) = (b.0 as f64, b.1 as f64);
        let mut inside: Vec<&(f64, f64, f32)> = notes.iter().filter(|g| ov(s, e, g.0, g.1) > 0.0).collect();
        let cover: f64 = inside.iter().map(|g| ov(s, e, g.0, g.1)).sum::<f64>() / (e - s).max(1.0);
        if cover < 0.2 {
            continue;
        }
        let best = inside.iter().max_by(|x, y| ov(s, e, x.0, x.1).total_cmp(&ov(s, e, y.0, y.1)));
        let pa = match best {
            Some(g) if ov(s, e, g.0, g.1) >= 0.4 * (e - s) => g.2.round() as i32,
            _ => b.2,
        };
        inside.retain(|g| ov(s, e, g.0, g.1) >= 15.0);
        inside.sort_by(|x, y| x.0.total_cmp(&y.0));
        if inside.len() >= 2 && b.3 != "~" {
            for (q, g) in inside.iter().enumerate() {
                let s2 = if q == 0 { b.0 } else { s.max(g.0).round() as i64 };
                let e2 = if q == inside.len() - 1 { b.1 } else { e.min(inside[q + 1].0).round() as i64 };
                if e2 - s2 >= 3 {
                    out.push((s2, e2, g.2.round() as i32, if q == 0 { b.3.clone() } else { "~".to_string() }));
                }
            }
        } else {
            out.push((b.0, b.1, pa, b.3.clone()));
        }
    }
    if same_note_hold() {
        // a held note: a sung note on the same pitch (within a semitone) carrying straight on from a bar
        // (starting within 0.1 s of its end, before the next bar) is the syllable still held: the bar runs on
        for k in 0..out.len() {
            let next = out.get(k + 1).map_or(f64::MAX, |n| n.0 as f64);
            let (mut end, p) = (out[k].1 as f64, out[k].2 as f32);
            let mut chain: Vec<&(f64, f64, f32)> = notes.iter().filter(|g| g.1 > end && g.0 < next).collect();
            chain.sort_by(|x, y| x.0.total_cmp(&y.0));
            for g in chain {
                if g.0 > end + 10.0 || (g.2 - p).abs() > 1.0 {
                    break;
                }
                end = g.1.min(next);
            }
            out[k].1 = out[k].1.max(end.round() as i64);
        }
    }
    out
}

/// Run bars on along same-pitch sung notes (ONKEY_SAME_HOLD=1 while being measured).
fn same_note_hold() -> bool {
    crate::devtest::knob!("ONKEY_SAME_HOLD").is_some()
}

/// refine_with_notes, except where the transcription failed (`failed`: song frames): no notes there means GAME
/// couldn't say, not that nobody sings, so those bars stay as they were. (Learning Don't Stop Believin' while
/// OnKey quit, GAME's last two pieces failed and every bar after 3:30 was dropped: no bars in the outro.)
pub fn refine_with_notes_where_heard(bars: &[SyllableBar], notes: &[(f64, f64, f32)], failed: &[(f64, f64)]) -> Vec<SyllableBar> {
    let unheard = |b: &SyllableBar| failed.iter().any(|f| (b.0 as f64) < f.1 && (b.1 as f64) > f.0);
    let (keep, refine): (Vec<SyllableBar>, Vec<SyllableBar>) = bars.iter().cloned().partition(unheard);
    let mut out = refine_with_notes(&refine, notes);
    out.extend(keep);
    out.sort_by_key(|b| b.0);
    out
}

/// Put words back on syllable bars kept without them (their lyrics' check didn't match: LRCLIB answered with
/// another entry, for a song kept before each line had its own check; or a syllable wasn't found where it sits):
/// each line with bars missing words has its syllables (as made, see `make`) put on the bars that start in that
/// line's time (`lyrics` lined up with the singer), in order, where there are exactly as many unheld bars as
/// syllables; elsewhere the bars stay as they are.
pub fn relabel(bars: &mut [(i64, i64, i32, String)], lyrics: &[(f64, String)]) {
    let syl = Syllabifier::new();
    let langs = crate::lang::line_langs(lyrics);
    for (i, (t, text)) in lyrics.iter().enumerate() {
        let parts: Vec<String> = lead_words(text).iter().flat_map(|w| syl.split_in(w, langs[i])).collect();
        if parts.is_empty() {
            continue;
        }
        let (a, b) = (((t - 0.3) * 100.0) as i64, lyrics.get(i + 1).map_or(i64::MAX, |n| ((n.0 - 0.3) * 100.0) as i64));
        let idx: Vec<usize> = (0..bars.len()).filter(|&k| bars[k].0 >= a && bars[k].0 < b && bars[k].3.trim() != "~").collect();
        if idx.len() == parts.len() && idx.iter().any(|&k| bars[k].3.trim().is_empty()) {
            for (k, p) in idx.into_iter().zip(parts) {
                bars[k].3 = p;
            }
        }
    }
}

/// One play's lyric times and syllable bars merged into what the song already had, by the song frames this play
/// heard (`covered`). A play is often part of a song (from where you joined, up to a pause or a skip), and on its
/// own it would leave the song with lyrics and bars for that part only.
///  - lines: where the play heard the song, the play's time; elsewhere the song's, unless no play has placed the
///    song's lyrics yet (`placed_before` false: still LRCLIB's times), when the play's placement (its fit moves
///    the whole song) is the better guess everywhere
///  - bars: all of the play's, and the song's where the play didn't hear it and none of the play's overlaps
///    (a play that made none leaves the song's as they are)
/// Different lyrics (other lines): the play's, as they are.
pub fn merge_play(old_lyrics: &[(f64, String)], new_lyrics: &[(f64, String)], placed_before: bool,
                  old_bars: &[SyllableBar], new_bars: &[SyllableBar], covered: &[bool]) -> (Vec<(f64, String)>, Vec<SyllableBar>) {
    if old_lyrics.len() != new_lyrics.len() || old_lyrics.iter().zip(new_lyrics).any(|(a, b)| a.1 != b.1) {
        return (new_lyrics.to_vec(), new_bars.to_vec());
    }
    let heard = |f: i64| f >= 0 && covered.get(f as usize).copied().unwrap_or(false);
    let lyrics: Vec<(f64, String)> = old_lyrics.iter().zip(new_lyrics).map(|(o, n)| {
        let f = (o.0 / crate::dsp::FRAME_S).round() as i64;
        if heard(f) || !placed_before { n.clone() } else { o.clone() }
    }).collect();
    // a play that made no bars (a poor separation, an instrumental stretch) takes none of the song's away
    if new_bars.is_empty() {
        return (lyrics, old_bars.to_vec());
    }
    let mid = |b: &SyllableBar| (b.0 + b.1) / 2;
    let mut bars: Vec<SyllableBar> = new_bars.to_vec();
    let kept: Vec<SyllableBar> = old_bars.iter()
        .filter(|o| !heard(mid(o)) && !new_bars.iter().any(|n| o.0 < n.1 && n.0 < o.1)).cloned().collect();
    bars.extend(kept);
    bars.sort_by_key(|b| (b.0, b.1));
    (lyrics, bars)
}

#[cfg(test)]
mod tests {
    use super::keep_shown;

    /// [lyrics-ml] The English syllable splitter as it was before the languages work (1 Nov 2026), kept here so
    /// the regression below can show the English path is byte-identical over the whole dictionary.
    fn old_split(syl: &super::Syllabifier, word: &str) -> Vec<String> {
        use hyphenation::Hyphenator;
        let w: String = word.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || *c == '\'').collect();
        let k = syl.counts.get(&w).map(|k| *k as usize).unwrap_or_else(|| {
            let groups = w.split(|c: char| !"aeiouy".contains(c)).filter(|g| !g.is_empty()).count();
            (groups - usize::from(w.ends_with('e') && w.len() > 3 && groups > 1)).max(1)
        });
        let lower = word.to_lowercase();
        let mut parts: Vec<String> = if lower.len() == word.len() {
            let h = syl.hyph.hyphenate(&lower);
            let mut v = Vec::new();
            let mut last = 0;
            for &b in &h.breaks {
                v.push(word[last..b].to_string());
                last = b;
            }
            v.push(word[last..].to_string());
            v
        } else {
            vec![word.to_string()]
        };
        while parts.len() > k {
            let last = parts.pop().unwrap();
            parts.last_mut().unwrap().push_str(&last);
        }
        while parts.len() < k {
            let i = (0..parts.len()).max_by_key(|&j| parts[j].chars().count()).unwrap();
            let p: Vec<char> = parts[i].chars().collect();
            if p.len() < 2 {
                break;
            }
            let h = p.len() / 2;
            let (a, b): (String, String) = (p[..h].iter().collect(), p[h..].iter().collect());
            parts.splice(i..=i, [a, b]);
        }
        parts
    }

    #[test]
    fn english_syllables_are_as_they_were_over_the_whole_dictionary() {
        let syl = super::Syllabifier::new();
        let mut n = 0;
        for w in include_str!("../data/syllables-en.txt").lines().filter_map(|l| l.split_once(' ').map(|p| p.0)) {
            let want = old_split(&syl, w);
            assert_eq!(syl.split(w), want, "{w}");
            assert_eq!(syl.split_in(w, crate::lang::Lang::En), want, "{w}");
            n += 1;
        }
        assert!(n > 100_000, "{n} words");
        // and words as lyrics have them (capitals, punctuation, accents, contractions)
        for w in ["Beautiful,", "DON'T", "café", "whoa-oh", "I'm", "ev'ry", "Yeah!", "x", "---", "corazón"] {
            assert_eq!(syl.split(w), old_split(&syl, w), "{w}");
        }
    }

    #[test]
    fn other_languages_split_by_their_own_rules() {
        use crate::lang::Lang;
        let syl = super::Syllabifier::new();
        // the English path has these wrong (one syllable: the dictionary's "come", a final e dropped, accents lost)
        assert_eq!(syl.split("come").len(), 1);
        assert_eq!(syl.split("vive").len(), 1);
        assert_eq!(syl.split_in("come", Lang::Es), vec!["co", "me"]);
        assert_eq!(syl.split_in("vive", Lang::Es), vec!["vi", "ve"]);
        assert_eq!(syl.split_in("amore", Lang::It), vec!["a", "mo", "re"]);
        assert_eq!(syl.split_in("Liebe", Lang::De), vec!["Lie", "be"]);
        assert_eq!(syl.split_in("coração", Lang::Pt), vec!["co", "ra", "ção"]);
        assert_eq!(syl.split_in("chanson", Lang::Fr), vec!["chan", "son"]);
        // the letters a syllable gives the aligner add up to the word's, however it's split (folding included)
        for (w, l) in [("Straße", Lang::De), ("cœur,", Lang::Fr), ("corazón", Lang::Es), ("você", Lang::Pt), ("perché", Lang::It)] {
            let parts = syl.split_in(w, l);
            assert_eq!(parts.iter().map(|p| super::letter_count(p)).sum::<usize>(), super::letter_count(w), "{w}");
            assert_eq!(parts.concat(), w);
        }
    }

    #[test]
    fn folds_accented_latin_letters() {
        use super::fold_upper;
        assert_eq!(fold_upper("corazón"), "CORAZON");
        assert_eq!(fold_upper("Niño"), "NINO");
        assert_eq!(fold_upper("Straße"), "STRASSE");
        assert_eq!(fold_upper("über"), "UBER");
        assert_eq!(fold_upper("garçon"), "GARCON");
        assert_eq!(fold_upper("cœur"), "COEUR");
        assert_eq!(fold_upper("Æble"), "AEBLE");
        assert_eq!(fold_upper("smørbrød"), "SMORBROD");
        assert_eq!(fold_upper("Åse"), "ASE");
        assert_eq!(fold_upper("être,"), "ETRE,");
        assert_eq!(fold_upper("l'été"), "L'ETE");
        assert_eq!(fold_upper("plain"), "PLAIN");
        // every folded letter is one the English aligner has
        let all = "áàâäãåāăąæçćčďđéèêëēėęěíìîïīįłñńňóòôöõøōőœřśšşßťţúùûüūůűýÿźżž";
        assert!(fold_upper(all).chars().all(|c| c.is_ascii_uppercase()), "{}", fold_upper(all));
        assert!(fold_upper(&all.to_uppercase()).chars().all(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn folded_lead_words_keep_accent_only_words() {
        use super::lead_words_with;
        assert_eq!(lead_words_with("voy à la mer (ó)", false), vec!["voy", "la", "mer"]);
        assert_eq!(lead_words_with("voy à la mer (ó)", true), vec!["voy", "à", "la", "mer"]);
        assert_eq!(lead_words_with("¡Óyeme tiburón!", true), vec!["¡Óyeme", "tiburón!"]);
        assert_eq!(lead_words_with("- 123 é", true), vec!["é"]);
    }

    #[test]
    fn folding_is_on_unless_turned_off() {
        // the default (ONKEY_FOLD unset, or anything but "0"): on
        if crate::devtest::knob!("ONKEY_FOLD").as_deref() != Some("0") {
            assert!(super::fold_on());
            assert_eq!(super::upper("corazón"), "CORAZON");
            assert_eq!(super::letter_count("ñón"), 3);
        }
    }

    #[test]
    fn folding_leaves_english_as_it_was() {
        use super::{fold_upper, lead_words_with};
        // plain English (A-Z, apostrophes, punctuation): folded and unfolded are the same letters, words and counts
        let line = "Oh, you and I don't know (hey) what's goin' on - it's 2 AM, baby!";
        for w in line.split_whitespace() {
            assert_eq!(fold_upper(w), w.to_uppercase(), "{w}");
            assert_eq!(fold_upper(w).chars().filter(|c| c.is_ascii_uppercase() || *c == '\'').count(),
                       w.chars().filter(|c| c.is_ascii_alphabetic() || *c == '\'').count(), "{w}");
        }
        assert_eq!(lead_words_with(line, true), lead_words_with(line, false));
        // the odd accented word in English keeps all its letters rather than losing the accented one
        assert_eq!(fold_upper("café"), "CAFE");
        assert_eq!(fold_upper("naïve"), "NAIVE");
        assert_eq!(fold_upper("fiancée"), "FIANCEE");
        assert_eq!(lead_words_with("at the café, naïve", true), vec!["at", "the", "café,", "naïve"]);
    }

    #[test]
    fn lines_opening_on_a_sung_vowel() {
        for l in ["oh oh and the clothes that you wear", "Oh, you and I", "Whoa-oh", "Ooh, baby", "aw ah aw ah", "(Hey) yeah yeah"] {
            assert!(super::opens_on_vowel(l), "{l}");
        }
        for l in ["Only you", "Hello", "Ahead of time", "", "(oh) only you"] {
            assert!(!super::opens_on_vowel(l), "{l}");
        }
    }

    #[test]
    fn a_live_line_up_never_moves_a_line_already_shown() {
        let l = |v: &[f64]| v.iter().enumerate().map(|(i, t)| (*t, format!("line {i}"))).collect::<Vec<_>>();
        let times = |v: Vec<(f64, String)>| v.into_iter().map(|x| x.0).collect::<Vec<_>>();
        let shown = l(&[10.0, 14.0, 18.0, 22.0]);
        // the singer is 1.5 s ahead of these lyrics; it's 14.5 s now: lines 0 and 1 have been shown
        assert_eq!(times(keep_shown(&shown, &l(&[8.5, 12.5, 16.5, 20.5]), 14.5)), vec![10.0, 14.0, 16.5, 20.5]);
        // a line now due before "now" comes up 0.3 s from now, not in the past; order is kept
        assert_eq!(times(keep_shown(&shown, &l(&[8.5, 12.5, 14.0, 20.5]), 14.5)), vec![10.0, 14.0, 14.8, 20.5]);
    }

    #[test]
    fn a_section_heard_far_from_the_rest_takes_the_songs_move() {
        use super::section_moves;
        // The Darkness: the first 7 lines heard 5 s before they're sung (the intro's guitar), the other 20 lines
        // all agree that the lyrics run 1.7 s late
        let placed: Vec<(usize, f64)> = (0..27).map(|i| (i, if i < 7 { -7.0 } else { -1.7 + 0.1 * (i % 3) as f64 })).collect();
        let m = section_moves(&placed, 27);
        assert!(m.iter().all(|d| (d + 1.7).abs() < 0.25), "{m:?}");
        // no song-wide agreement (another version, sections off by different amounts): each section keeps its own
        let placed: Vec<(usize, f64)> = (0..24).map(|i| (i, [-4.0, 0.0, 3.0][i / 8])).collect();
        let m = section_moves(&placed, 24);
        assert_eq!((m[0], m[12], m[23]), (-4.0, 0.0, 3.0));
        // sections a little apart (within 1.5 s) keep their own
        let placed: Vec<(usize, f64)> = (0..20).map(|i| (i, if i < 8 { -2.5 } else { -1.5 })).collect();
        assert_eq!(section_moves(&placed, 20)[0], -2.5);
    }

    #[test]
    fn a_learned_placement_is_believed_only_where_consistent() {
        use super::check_placement;
        let lyrics: Vec<(f64, String)> = (0..12).map(|i| (10.0 + 4.0 * i as f64, format!("line {i}"))).collect();
        let heard_all = || -> super::Heard { (0..12).map(|_| Some(vec![(0usize, 0usize, 0.0, 0.0)])).collect() };
        let check = |moves: &[f64]| {
            let placed = lyrics.iter().zip(moves).map(|(l, m)| (l.0 + m, l.1.clone())).collect();
            check_placement(&lyrics, placed, heard_all())
        };
        // a version 1.2 s late throughout, one line misheard: all but that one where heard, it moves with them
        let mut m = vec![-1.2; 12];
        m[5] = -9.0;
        let (out, heard, fit) = check(&m);
        assert!(fit.believed && fit.agree == 11);
        assert!((out[5].0 - (30.0 - 1.2)).abs() < 0.01 && heard[5].is_none());
        assert!((out[0].0 - 8.8).abs() < 1e-9);
        // a drifting version (0.8% faster): followed
        let m: Vec<f64> = lyrics.iter().map(|l| -0.008 * l.0).collect();
        let (_, _, fit) = check(&m);
        assert!(fit.believed && fit.agree == 12 && (fit.drift + 0.008).abs() < 0.0015, "{fit:?}");
        // Dancing Queen: sections thrown 4-20 s early, lines packed: not believed, LRCLIB's times kept, none bunched
        let m = [-1.1, -1.1, -1.1, -7.1, -11.2, -11.6, -11.6, -12.0, -15.0, -15.9, -17.6, -4.8];
        let (out, _, fit) = check(&m);
        assert!(!fit.believed, "{fit:?}");
        for (o, l) in out.iter().zip(&lyrics) {
            assert_eq!(o.0, l.0);
        }
        // a big move (6 s, an intro) by just over half the lines isn't enough; by most of them it is
        let mut m = vec![6.0; 12];
        for x in m.iter_mut().take(5) {
            *x = 0.0;
        }
        assert!(!check(&m).2.believed);
        let mut m = vec![6.0; 12];
        m[0] = 0.0;
        m[1] = 0.0;
        assert!(check(&m).2.believed);
        // lines heard bunched together keep at least half LRCLIB's spacing (0.5 s at most)
        let m = [0.0, 0.0, 0.0, 0.0, 0.0, -3.9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (out, _, _) = check(&m);
        assert!(out[5].0 >= out[4].0 + 0.5 - 1e-9);
    }

    #[test]
    fn a_line_up_far_from_the_last_needs_a_second_that_agrees() {
        let mut u = super::LineUps::default();
        assert!(!u.accept(-2.3, 2));        // a big first move from 2 lines: wait for another
        assert!(u.accept(-2.2, 2));         // which agrees
        assert!(!u.settled());
        assert!(u.accept(-1.6, 5));         // a drift is followed
        assert!(!u.settled());              // (not yet settled: 0.6 s from the last)
        assert!(u.accept(-1.5, 4));
        assert!(u.settled());               // 4 lines, and agrees with the last: settled
        assert!(!u.accept(11.8, 5));        // a stray far one isn't
        assert!(u.accept(-1.5, 4));
        assert!(!u.accept(4.0, 3));
        assert!(u.accept(4.3, 6));          // unless the next agrees with it
    }

    #[test]
    fn a_partial_play_keeps_the_rest_of_the_song() {
        let line = |t: f64, w: &str| (t, w.to_string());
        let old = vec![line(2.0, "a"), line(50.0, "b"), line(100.0, "c")];
        let new = vec![line(8.0, "a"), line(52.0, "b"), line(103.0, "c")];
        // this play heard 40-120 s of the song
        let covered: Vec<bool> = (0..15_000).map(|f| (4_000..12_000).contains(&f)).collect();
        let old_bars = vec![(200, 260, 60, "a".to_string()), (5_000, 5_050, 62, "b".to_string()), (10_000, 10_060, 64, "c".to_string())];
        let new_bars = vec![(5_200, 5_260, 61, "b".to_string()), (10_300, 10_340, 65, "c".to_string())];
        let (l, b) = crate::syllables::merge_play(&old, &new, true, &old_bars, &new_bars, &covered);
        // the line it didn't hear keeps its time; the two it heard take the play's
        assert_eq!(l.iter().map(|x| x.0).collect::<Vec<_>>(), vec![2.0, 52.0, 103.0]);
        // the bar before the play is kept; the old bars where it heard the song give way to its own
        assert_eq!(b, vec![(200, 260, 60, "a".to_string()), (5_200, 5_260, 61, "b".to_string()), (10_300, 10_340, 65, "c".to_string())]);
    }

    #[test]
    fn a_play_that_heard_nothing_changes_nothing_and_other_lyrics_are_the_plays() {
        let line = |t: f64, w: &str| (t, w.to_string());
        let old = vec![line(2.0, "a"), line(50.0, "b")];
        let new = vec![line(9.0, "a"), line(59.0, "b")];
        let old_bars = vec![(200, 260, 60, "a".to_string())];
        let (l, b) = crate::syllables::merge_play(&old, &new, true, &old_bars, &[], &vec![false; 10_000]);
        assert_eq!((l, b), (old.clone(), old_bars.clone()));
        // it heard the whole song but made no bars: the song keeps its own
        let (_, b) = crate::syllables::merge_play(&old, &new, true, &old_bars, &[], &vec![true; 10_000]);
        assert_eq!(b, old_bars.clone());
        let other = vec![line(9.0, "x"), line(59.0, "y")];
        let (l, b) = crate::syllables::merge_play(&old, &other, true, &old_bars, &[], &vec![true; 10_000]);
        assert_eq!((l, b), (other, vec![]));
    }

    #[test]
    fn lines_no_play_has_placed_take_the_plays_fit_even_where_it_didnt_hear() {
        let line = |t: f64, w: &str| (t, w.to_string());
        // a first, partial play: the song's times are still LRCLIB's
        let fetched = vec![line(8.0, "a"), line(60.0, "b"), line(120.0, "c")];
        let new = vec![line(2.0, "a"), line(54.0, "b"), line(114.0, "c")];
        let covered: Vec<bool> = (0..15_000).map(|f| f < 7_000).collect();
        let (l, _) = crate::syllables::merge_play(&fetched, &new, false, &[], &[], &covered);
        assert_eq!(l.iter().map(|x| x.0).collect::<Vec<_>>(), vec![2.0, 54.0, 114.0]);
        // a later play that heard only the end: lines placed before keep their times, even though the play
        // started from those same times (a learned song's play does: what made every line look "unplaced")
        let later = vec![line(9.0, "a"), line(61.0, "b"), line(115.0, "c")];
        let covered: Vec<bool> = (0..15_000).map(|f| f > 10_000).collect();
        let (l, _) = crate::syllables::merge_play(&new, &later, true, &[], &[], &covered);
        assert_eq!(l.iter().map(|x| x.0).collect::<Vec<_>>(), vec![2.0, 54.0, 115.0]);
    }
}
