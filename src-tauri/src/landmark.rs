//! Landmark fingerprints: which learned song a few seconds of music are, and where in it they start, from the
//! loudest points of the music's spectrogram (the "constellation" method of Wang 2003, as in Shazam, audfprint
//! and Dejavu; our own implementation). OnKey's own counterpart of ShazamKit's custom catalog, for systems
//! without ShazamKit (recognize.rs uses it on Windows).
//!
//!   - A signature is the spectrogram's peaks: points louder than everything near them in time and pitch. They
//!     survive noise, a level change, re-encoding and a voice on top (most of them, and only some are needed).
//!   - Peaks are paired (an anchor with a few peaks just after it) and each pair hashed by its two frequencies
//!     and the time between them: a hash that's rare across songs and doesn't depend on where the pair is.
//!   - A query's hashes are looked up in the catalog; every hit votes for (song, reference time - query time).
//!     The right song collects many votes at one time difference, which is where the query starts in it. Chance
//!     hits scatter over every difference.
//!
//! Signatures keep the peaks, not the hashes: smaller on disk, and how peaks are paired can change without
//! making stored signatures useless. The peak picking is what the version in the header stands for.
use std::collections::VecDeque;
use std::sync::Arc;

use realfft::RealFftPlanner;

/// Mono samples per second the signature is made from.
pub const SR: usize = 16_000;
/// Spectrogram frame (64 ms) and hop (8 ms: the step offsets are measured in, before refining between frames).
const N: usize = 1024;
const HOP: usize = 128;
/// Bins peaks are looked for in: ~62 Hz to ~5 kHz (where music's energy is, and what survives lossy codecs).
const BIN_LO: usize = 4;
const BIN_HI: usize = 320;
/// A peak is the loudest point within this many frames (±80 ms) and bins (±125 Hz) of it.
const NEAR_T: usize = 10;
const NEAR_F: usize = 8;
/// ...and at most this many per second (the loudest), so a dense mix doesn't swamp the catalog.
const MAX_PER_S: usize = 40;
/// Quieter than this (~-80 dBFS, in ln power) isn't music: digital silence and dither make no peaks.
const FLOOR: f32 = -2.0;
/// Pairs: each anchor with the next `FAN` peaks within `DT_MAX` frames (~0.5 s) and `DF_MAX` bins after it.
const FAN: usize = 6;
const DT_MAX: u32 = 63;
const DF_MAX: i32 = 100;
/// What a match must have to be believed: this many hashes agreeing on the time (±1 frame), and that many times
/// more than the best other song. Measured on 2 s clips of 50 real songs (lab/research/landmark/FINDINGS.md): the
/// right song scores 77 or more 95% of the time (median 203); chance agreements, from other songs or songs not
/// in the catalog, never passed 8.
pub const MIN_SCORE: u32 = 20;
const MIN_LEAD: f32 = 2.0;

const MAGIC: &[u8; 4] = b"OKLM";
const VERSION: u8 = 1;

/// A spectrogram peak: frame (hop `HOP` from the audio's start) and frequency bin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Peak {
    t: u32,
    f: u16,
}

/// Log power spectrogram of bins BIN_LO..BIN_HI, frame-major.
fn spectrogram(x: &[f32]) -> (Vec<f32>, usize) {
    if x.len() < N {
        return (Vec::new(), 0);
    }
    let frames = (x.len() - N) / HOP + 1;
    let bins = BIN_HI - BIN_LO;
    let fft: Arc<dyn realfft::RealToComplex<f32>> = RealFftPlanner::<f32>::new().plan_fft_forward(N);
    let win: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N as f32).cos()).collect();
    let mut buf = fft.make_input_vec();
    let mut spec = fft.make_output_vec();
    let mut out = vec![0f32; frames * bins];
    for i in 0..frames {
        let a = i * HOP;
        for (k, b) in buf.iter_mut().enumerate() {
            *b = x[a + k] * win[k];
        }
        let _ = fft.process(&mut buf, &mut spec);
        let row = &mut out[i * bins..(i + 1) * bins];
        for (r, c) in row.iter_mut().zip(&spec[BIN_LO..BIN_HI]) {
            *r = (c.norm_sqr() + 1e-10).ln();
        }
    }
    (out, frames)
}

/// Running maximum over ±r of `n` values read with `get`, written with `put` (a monotonic queue: O(n)).
fn running_max(n: usize, r: usize, get: impl Fn(usize) -> f32, mut put: impl FnMut(usize, f32)) {
    let mut q: VecDeque<usize> = VecDeque::new();
    let mut next = 0;
    for i in 0..n {
        while next < n && next <= i + r {
            let v = get(next);
            while q.back().is_some_and(|&j| get(j) <= v) {
                q.pop_back();
            }
            q.push_back(next);
            next += 1;
        }
        while q.front().is_some_and(|&j| j + r < i) {
            q.pop_front();
        }
        put(i, get(*q.front().unwrap()));
    }
}

/// The spectrogram's peaks, in time order.
fn peaks(x: &[f32]) -> Vec<Peak> {
    let (s, frames) = spectrogram(x);
    let bins = BIN_HI - BIN_LO;
    if frames == 0 {
        return Vec::new();
    }
    // the neighbourhood's maximum: along frequency, then along time
    let mut mf = vec![0f32; s.len()];
    for t in 0..frames {
        let row = &s[t * bins..(t + 1) * bins];
        let out = &mut mf[t * bins..(t + 1) * bins];
        running_max(bins, NEAR_F, |i| row[i], |i, v| out[i] = v);
    }
    let mut m = vec![0f32; s.len()];
    for f in 0..bins {
        running_max(frames, NEAR_T, |i| mf[i * bins + f], |i, v| m[i * bins + f] = v);
    }
    // candidates, then the loudest MAX_PER_S in each second
    let mut found: Vec<(f32, Peak)> = Vec::new();
    for t in 0..frames {
        for f in 0..bins {
            let v = s[t * bins + f];
            if v == m[t * bins + f] && v > FLOOR {
                found.push((v, Peak { t: t as u32, f: (f + BIN_LO) as u16 }));
            }
        }
    }
    let per = SR / HOP; // frames a second
    let mut kept = Vec::with_capacity(found.len());
    for block in found.chunk_by_mut(|a, b| a.1.t as usize / per == b.1.t as usize / per) {
        block.sort_by(|a, b| b.0.total_cmp(&a.0));
        kept.extend(block.iter().take(MAX_PER_S).map(|p| p.1));
    }
    kept.sort();
    kept
}

/// A signature of mono `SR` Hz audio: a short header, then its peaks (time steps as varints, bins as u16).
pub fn signature(x16: &[f32]) -> Vec<u8> {
    let p = peaks(x16);
    let mut b = Vec::with_capacity(12 + p.len() * 3);
    b.extend_from_slice(MAGIC);
    b.push(VERSION);
    b.extend_from_slice(&(p.len() as u32).to_le_bytes());
    let mut last = 0u32;
    for q in &p {
        let mut d = q.t - last;
        last = q.t;
        loop {
            let byte = (d & 0x7f) as u8;
            d >>= 7;
            if d == 0 {
                b.push(byte);
                break;
            }
            b.push(byte | 0x80);
        }
        b.extend_from_slice(&q.f.to_le_bytes());
    }
    b
}

/// A signature's peaks. None: not one of these (another kind of signature, or another version of this one).
fn parse(sig: &[u8]) -> Option<Vec<Peak>> {
    if sig.len() < 9 || &sig[..4] != MAGIC || sig[4] != VERSION {
        return None;
    }
    let n = u32::from_le_bytes(sig[5..9].try_into().ok()?) as usize;
    let mut p = Vec::with_capacity(n.min(sig.len()));
    let (mut i, mut t) = (9usize, 0u32);
    for _ in 0..n {
        let mut d = 0u32;
        let mut shift = 0;
        loop {
            let byte = *sig.get(i)?;
            i += 1;
            d |= ((byte & 0x7f) as u32).checked_shl(shift)?;
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        t = t.checked_add(d)?;
        let f = u16::from_le_bytes(sig.get(i..i + 2)?.try_into().ok()?);
        i += 2;
        p.push(Peak { t, f });
    }
    (i == sig.len()).then_some(p)
}

/// Each anchor paired with the next few peaks: (hash, anchor's frame). Hash: f1 (9 bits), f2 (9), dt (6).
fn hashes(p: &[Peak]) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(p.len() * FAN);
    for (i, a) in p.iter().enumerate() {
        let mut n = 0;
        for b in &p[i + 1..] {
            let dt = b.t - a.t;
            if dt > DT_MAX || n == FAN {
                break;
            }
            if dt == 0 || (b.f as i32 - a.f as i32).abs() > DF_MAX {
                continue;
            }
            out.push((((a.f as u32) << 15) | ((b.f as u32) << 6) | dt, a.t));
            n += 1;
        }
    }
    out
}

/// What a query matched.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    /// seconds into the reference's audio where the query's audio starts
    pub offset: f64,
    /// hashes agreeing on that place (±1 frame)
    pub score: u32,
}

/// Songs to match against. Every song's hashes in one sorted list: (hash, song << 20 | frame).
#[derive(Default)]
pub struct Catalog {
    ids: Vec<String>,
    entries: Vec<(u32, u32)>,
    sorted: bool,
}

/// Bits of an entry for the frame (2^20 frames of 8 ms: 2.3 hours) and so for the song (4096 songs).
const T_BITS: u32 = 20;

impl Catalog {
    pub fn new() -> Catalog {
        Catalog::default()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Add a song's signature under `id`. False: not a signature of this kind (a ShazamKit one, say), or the
    /// catalog is full.
    pub fn add(&mut self, id: &str, sig: &[u8]) -> bool {
        let Some(p) = parse(sig) else { return false };
        let song = self.ids.len() as u32;
        if song >= 1 << (32 - T_BITS) {
            return false;
        }
        self.ids.push(id.to_string());
        self.entries.extend(hashes(&p).into_iter().filter(|&(_, t)| t < 1 << T_BITS).map(|(h, t)| (h, song << T_BITS | t)));
        self.sorted = false;
        true
    }

    /// Bytes the catalog holds in memory.
    pub fn bytes(&self) -> usize {
        self.entries.len() * std::mem::size_of::<(u32, u32)>()
    }

    /// Which song a query signature is from and where in it it starts, when sure enough (`MIN_SCORE`, and well
    /// ahead of any other song).
    pub fn find(&mut self, sig: &[u8]) -> Option<Hit> {
        self.best(sig).filter(|(h, second)| h.score >= MIN_SCORE && h.score as f32 >= MIN_LEAD * *second as f32).map(|(h, _)| h)
    }

    /// The best-voted song and place for a query signature however few votes it has, and the best score of any
    /// other song (for measuring how sure a match is).
    pub fn best(&mut self, sig: &[u8]) -> Option<(Hit, u32)> {
        let p = parse(sig)?;
        if !self.sorted {
            self.entries.sort_unstable();
            self.sorted = true;
        }
        // votes: (song, reference frame - query frame), offset so it's never negative
        const BIAS: i64 = 1 << 24;
        let mut votes: Vec<u64> = Vec::new();
        for (h, tq) in hashes(&p) {
            let a = self.entries.partition_point(|e| e.0 < h);
            for &(eh, v) in &self.entries[a..] {
                if eh != h {
                    break;
                }
                let d = (v & ((1 << T_BITS) - 1)) as i64 - tq as i64 + BIAS;
                votes.push(((v >> T_BITS) as u64) << 32 | d as u64);
            }
        }
        votes.sort_unstable();
        let runs: Vec<(u64, u32)> = votes.chunk_by(|a, b| a == b).map(|r| (r[0], r.len() as u32)).collect();
        // each place's score counts its neighbours too (a peak a frame early or late under noise)
        let count = |k: u64| runs.binary_search_by_key(&k, |r| r.0).map_or(0, |i| runs[i].1);
        let mut best: Option<(u32, u64)> = None;
        let mut best_song_score = vec![0u32; self.ids.len()];
        for &(k, c) in &runs {
            let s = c + count(k.wrapping_sub(1)) + count(k + 1);
            let song = (k >> 32) as usize;
            best_song_score[song] = best_song_score[song].max(s);
            if best.map_or(true, |(bs, _)| s > bs) {
                best = Some((s, k));
            }
        }
        let (score, k) = best?;
        let song = (k >> 32) as usize;
        let second = best_song_score.iter().enumerate().filter(|&(i, _)| i != song).map(|(_, &s)| s).max().unwrap_or(0);
        // between frames: the votes' centre over the three
        let (c0, c1, c2) = (count(k - 1) as f64, count(k) as f64, count(k + 1) as f64);
        let d = (k & 0xffff_ffff) as i64 - BIAS;
        let frac = (c2 - c0) / (c0 + c1 + c2);
        Some((Hit { id: self.ids[song].clone(), offset: (d as f64 + frac) * HOP as f64 / SR as f64, score }, second))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> f32 {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
        }
    }

    /// A minute of "music": random notes (a few partials each, random pitch and length) over soft noise.
    fn song(seed: u64, secs: usize) -> Vec<f32> {
        let mut r = rng(seed);
        let mut x: Vec<f32> = (0..secs * SR).map(|_| r() * 0.01).collect();
        let mut at = 0usize;
        while at < x.len() {
            let len = (0.1 + 0.3 * (r() + 1.0)) as f64;
            let n = (len * SR as f64) as usize;
            let f0 = 110.0 * 2f64.powf((r() as f64 + 1.0) * 2.5);
            let amp = 0.1 + 0.1 * r().abs();
            for k in 0..n.min(x.len() - at) {
                let t = k as f64 / SR as f64;
                let env = (1.0 - k as f64 / n as f64) as f32;
                let mut v = 0.0;
                for h in 1..4 {
                    v += (2.0 * std::f64::consts::PI * f0 * h as f64 * t).sin() / h as f64;
                }
                x[at + k] += amp * env * v as f32;
            }
            at += (n as f32 * (0.3 + 0.4 * (r() + 1.0))) as usize;
        }
        x
    }

    fn catalog(songs: &[(&str, &[f32])]) -> Catalog {
        let mut c = Catalog::new();
        for (id, x) in songs {
            assert!(c.add(id, &signature(x)));
        }
        c
    }

    #[test]
    fn a_signature_reads_back_as_the_peaks_it_was_made_of() {
        let x = song(1, 20);
        let p = peaks(&x);
        assert!(p.len() > 20 * 10, "{} peaks in 20 s", p.len());
        assert_eq!(parse(&signature(&x)).unwrap(), p);
        // not ours, or cut short: refused, not misread
        assert!(parse(b"SHS1\x01\x02\x03\x04\x05\x06").is_none());
        let s = signature(&x);
        assert!(parse(&s[..s.len() - 1]).is_none());
        let mut c = Catalog::new();
        assert!(!c.add("x", b"\x00\x01\x02\x03 a ShazamKit signature"));
        assert!(c.is_empty());
    }

    #[test]
    fn a_clip_is_named_and_placed_to_within_a_few_ms() {
        let songs: Vec<Vec<f32>> = (0..5).map(|i| song(100 + i, 60)).collect();
        let named: Vec<(String, &[f32])> = songs.iter().enumerate().map(|(i, x)| (format!("s{i}"), &x[..])).collect();
        let mut c = catalog(&named.iter().map(|(n, x)| (n.as_str(), *x)).collect::<Vec<_>>());
        for (i, x) in songs.iter().enumerate() {
            for start in [1.234, 17.0, 40.5039] {
                let a = (start * SR as f64) as usize;
                let sig = signature(&x[a..a + 2 * SR]);
                let h = c.find(&sig).unwrap_or_else(|| panic!("s{i} {start}: {:?}", c.best(&sig)));
                assert_eq!(h.id, format!("s{i}"));
                assert!((h.offset - a as f64 / SR as f64).abs() < 0.006, "{start}: {}", h.offset);
            }
        }
    }

    #[test]
    fn a_quieter_noisier_clip_is_still_found() {
        let x = song(7, 60);
        let mut c = catalog(&[("a", &x), ("b", &song(8, 60))]);
        let mut r = rng(99);
        let a = 23 * SR + 77;
        // 8 s: these few-note test songs make far fewer peaks than real music (which scores 77+ from 2 s)
        let clip: Vec<f32> = x[a..a + 8 * SR].iter().map(|v| v * 0.3 + r() * 0.02).collect();
        let h = c.find(&signature(&clip)).expect("found");
        assert_eq!(h.id, "a");
        assert!((h.offset - a as f64 / SR as f64).abs() < 0.01);
    }

    #[test]
    fn unrelated_audio_and_silence_match_nothing() {
        let mut c = catalog(&[("a", &song(1, 60)), ("b", &song(2, 60)), ("c", &song(3, 60))]);
        for seed in 50..60 {
            let other = song(seed, 5);
            assert_eq!(c.find(&signature(&other)), None, "seed {seed}");
        }
        let mut r = rng(5);
        let noise: Vec<f32> = (0..5 * SR).map(|_| r() * 0.3).collect();
        assert_eq!(c.find(&signature(&noise)), None);
        assert!(peaks(&vec![0.0; 5 * SR]).is_empty());
        assert_eq!(c.find(&signature(&vec![0.0; 5 * SR])), None);
    }
}
