//! Landmark fingerprints (landmark.rs) against ShazamKit's custom catalog (on this Mac, no network), on real
//! songs: which song and where, from 2, 3 and 5 s clips at random places, clean and spoilt (noise, quieter, AAC
//! re-encoded, resampled, a voice on top). Results: lab/research/landmark/FINDINGS.md.
//!   ONKEY_BENCH_DIR=<dir of song folders with mix16.f32 (+ vocal16.f32)> [ONKEY_BENCH_SONGS=40] [ONKEY_BENCH_UNKNOWN=10]
//!   [ONKEY_BENCH_CLIPS=5] cargo test --lib --release landmark_bench -- --ignored --nocapture
//! Reads the songs only. AAC uses /usr/bin/afconvert on clips written to a temporary folder.
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::landmark;
use crate::recognize::{signature as shz_signature, SigCatalog, SR};

struct Song {
    id: String,
    mix: Vec<f32>,
    vocal: Option<Vec<f32>>,
}

fn read_f32(p: &Path) -> Option<Vec<f32>> {
    Some(std::fs::read(p).ok()?.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect())
}

fn env(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A WAV of float samples.
fn write_wav(p: &Path, x: &[f32], rate: u32) {
    let mut b = Vec::with_capacity(44 + x.len() * 4);
    let data = (x.len() * 4) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for v in x {
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(p, b).unwrap();
}

/// A float WAV's samples (the "data" chunk, wherever it is).
fn read_wav(p: &Path) -> Vec<f32> {
    let b = std::fs::read(p).unwrap();
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if &b[i..i + 4] == b"data" {
            return b[i + 8..(i + 8 + len).min(b.len())].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        }
        i += 8 + len + (len & 1);
    }
    Vec::new()
}

/// `x` as a 44.1 kHz AAC file at 128 kb/s, decoded back to 16 kHz (afconvert: Core Audio's own codec, no network).
fn aac(x: &[f32], dir: &Path) -> Vec<f32> {
    let (w, m, back) = (dir.join("clip.wav"), dir.join("clip.m4a"), dir.join("back.wav"));
    let mut up = Vec::new();
    crate::resample::Resampler::new(SR as f64, 44_100.0).process(x, &mut up);
    write_wav(&w, &up, 44_100);
    let ok = std::process::Command::new("/usr/bin/afconvert").args(["-f", "m4af", "-d", "aac", "-b", "128000"]).arg(&w).arg(&m).status().unwrap().success()
        && std::process::Command::new("/usr/bin/afconvert").args(["-f", "WAVE", "-d", "LEF32@16000"]).arg(&m).arg(&back).status().unwrap().success();
    assert!(ok, "afconvert");
    read_wav(&back)
}

fn resampled(x: &[f32]) -> Vec<f32> {
    let mut up = Vec::new();
    crate::resample::Resampler::new(SR as f64, 44_100.0).process(x, &mut up);
    let mut down = Vec::new();
    crate::resample::Resampler::new(44_100.0, SR as f64).process(&up, &mut down);
    down
}

#[derive(Default)]
struct Tally {
    n: usize,
    right: usize,
    misplaced: usize,
    wrong: usize,
    none: usize,
    errs: Vec<f64>,
    ms: Vec<f64>,
}

impl Tally {
    fn add(&mut self, want: Option<(&str, f64)>, got: Option<(String, f64)>, ms: f64) {
        self.n += 1;
        self.ms.push(ms);
        match (want, got) {
            (_, None) => self.none += 1,
            (Some((id, at)), Some((g, o))) if g == id => {
                let e = (o - at).abs();
                self.errs.push(e);
                if e < 0.1 { self.right += 1 } else { self.misplaced += 1 }
            }
            (_, Some(_)) => self.wrong += 1,
        }
    }

    fn line(&mut self) -> String {
        let pct = |k: usize| 100.0 * k as f64 / self.n.max(1) as f64;
        self.errs.sort_by(f64::total_cmp);
        self.ms.sort_by(f64::total_cmp);
        let q = |v: &[f64], f: f64| if v.is_empty() { f64::NAN } else { v[((v.len() - 1) as f64 * f).round() as usize] };
        let mean = self.ms.iter().sum::<f64>() / self.ms.len().max(1) as f64;
        format!("{:4} | {:5.1}% | {:4.1}% | {:4.1}% | {:5.1}% | {:5.1} / {:6.1} / {:7.1} | {:6.1} / {:6.1}",
                self.n, pct(self.right), pct(self.misplaced), pct(self.wrong), pct(self.none),
                q(&self.errs, 0.5) * 1000.0, q(&self.errs, 0.95) * 1000.0, self.errs.last().copied().unwrap_or(f64::NAN) * 1000.0,
                mean, q(&self.ms, 0.95))
    }
}

/// Both matchers on one clip: (landmark's answer, ms), (ShazamKit's, ms).
type Answer = (Option<(String, f64)>, f64);
fn ask(lm: &mut landmark::Catalog, shz: &mut SigCatalog, clip: &[f32]) -> (Answer, Answer) {
    let t = Instant::now();
    let a = lm.find(&landmark::signature(clip)).map(|h| (h.id, h.offset));
    let lm_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let b = shz_signature(clip).and_then(|s| shz.find(&s));
    let shz_ms = t.elapsed().as_secs_f64() * 1000.0;
    ((a, lm_ms), (b, shz_ms))
}

#[test]
#[ignore]
fn landmark_bench() {
    let dir = PathBuf::from(std::env::var("ONKEY_BENCH_DIR").expect("ONKEY_BENCH_DIR"));
    let (known_n, unknown_n, clips) = (env("ONKEY_BENCH_SONGS", 40), env("ONKEY_BENCH_UNKNOWN", 10), env("ONKEY_BENCH_CLIPS", 5));
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
    dirs.sort();
    // songs of 2 minutes or more, spread over the folder (not one artist's run of names)
    let mut songs = Vec::new();
    let step = (dirs.len() / (known_n + unknown_n)).max(1);
    for d in dirs.iter().step_by(step).chain(dirs.iter()) {
        if songs.len() == known_n + unknown_n {
            break;
        }
        let id = d.file_name().unwrap().to_string_lossy().into_owned();
        if songs.iter().any(|s: &Song| s.id == id) {
            continue;
        }
        let Some(mix) = read_f32(&d.join("mix16.f32")) else { continue };
        if mix.len() < 120 * SR {
            continue;
        }
        songs.push(Song { id, vocal: read_f32(&d.join("vocal16.f32")), mix });
    }
    assert!(songs.len() > 2, "too few songs in {dir:?}");
    let known_n = known_n.min(songs.len() - 1);
    let (known, unknown) = songs.split_at(known_n);
    let secs: f64 = known.iter().map(|s| s.mix.len() as f64 / SR as f64).sum();
    println!("{} songs in the catalog ({:.0} min), {} not in it", known.len(), secs / 60.0, unknown.len());

    // the catalogs, from whole songs (as a full play makes them in the app)
    let mut lm = landmark::Catalog::new();
    let mut shz = SigCatalog::new();
    let (mut lm_bytes, mut shz_bytes, mut lm_ms, mut shz_ms) = (0usize, 0usize, 0.0, 0.0);
    for s in known {
        let t = Instant::now();
        let a = landmark::signature(&s.mix);
        lm_ms += t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let b = shz_signature(&s.mix).expect("ShazamKit signature");
        shz_ms += t.elapsed().as_secs_f64() * 1000.0;
        lm_bytes += a.len();
        shz_bytes += b.len();
        assert!(lm.add(&s.id, &a));
        assert!(shz.add(&s.id, &b), "ShazamKit catalog add {}", s.id);
    }
    let t = Instant::now();
    let _ = lm.find(&landmark::signature(&known[0].mix[..2 * SR]));
    let sort_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("signature per minute of song: landmark {:.1} KB, ShazamKit {:.1} KB; made in {:.0} / {:.0} ms per song",
             lm_bytes as f64 / 1024.0 / (secs / 60.0), shz_bytes as f64 / 1024.0 / (secs / 60.0), lm_ms / known.len() as f64, shz_ms / known.len() as f64);
    println!("landmark catalog in memory: {:.1} MB ({:.0} KB per song; first query sorts it: {:.0} ms)",
             lm.bytes() as f64 / 1e6, lm.bytes() as f64 / 1024.0 / known.len() as f64, sort_ms);

    let tmp = std::env::temp_dir().join(format!("onkey-landmark-bench-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let conditions = ["clean", "noise 10 dB SNR", "-10 dB quieter", "AAC 128k", "resampled 44.1k", "voice -6 dB (same song, 250 ms late)",
                      "voice -6 dB (another song)"];
    println!("\nlength | condition | method | clips | right song+place | misplaced >0.1s | wrong song | none | offset error ms median/p95/max | ms mean/p95");
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for len in [2.0, 3.0, 5.0] {
        let n = (len * SR as f64) as usize;
        for (ci, cond) in conditions.iter().enumerate() {
            let (mut a, mut b) = (Tally::default(), Tally::default());
            for (si, s) in known.iter().enumerate() {
                for _ in 0..clips {
                    let at = ((s.mix.len() - n - SR) as f64 * rng.next()) as usize;
                    let x = &s.mix[at..at + n];
                    let clip: Vec<f32> = match ci {
                        0 => x.to_vec(),
                        1 => {
                            let g = rms(x) * 10f32.powf(-10.0 / 20.0) * 3f32.sqrt();
                            x.iter().map(|v| v + g * (2.0 * rng.next() as f32 - 1.0)).collect()
                        }
                        2 => x.iter().map(|v| v * 10f32.powf(-10.0 / 20.0)).collect(),
                        3 => {
                            // half a second either side, so the codec's start and end don't fall on the clip
                            let pad = SR / 2;
                            let lo = at.saturating_sub(pad);
                            let y = aac(&s.mix[lo..at + n + pad], &tmp);
                            y[at - lo..(at - lo + n).min(y.len())].to_vec()
                        }
                        4 => resampled(x),
                        _ => {
                            let v = if ci == 5 { s.vocal.as_deref().map(|v| (v, SR / 4)) } else { known[(si + 7) % known.len()].vocal.as_deref().map(|v| (v, 0)) };
                            let Some((v, late)) = v else { continue };
                            let lo = at.saturating_sub(late).min(v.len().saturating_sub(n));
                            let v = &v[lo..lo + n];
                            let g = rms(x) * 10f32.powf(-6.0 / 20.0) / rms(v).max(1e-6);
                            x.iter().zip(v).map(|(a, b)| a + g * b).collect()
                        }
                    };
                    let want = Some((s.id.as_str(), at as f64 / SR as f64));
                    let ((ga, ma), (gb, mb)) = ask(&mut lm, &mut shz, &clip);
                    a.add(want, ga, ma);
                    b.add(want, gb, mb);
                }
            }
            println!("{len} s | {cond} | landmark | {}", a.line());
            println!("{len} s | {cond} | ShazamKit | {}", b.line());
        }
        // songs not in the catalog: every answer is a false one
        let (mut a, mut b) = (Tally::default(), Tally::default());
        for s in unknown {
            for _ in 0..clips * 2 {
                let at = ((s.mix.len() - n - SR) as f64 * rng.next()) as usize;
                let ((ga, ma), (gb, mb)) = ask(&mut lm, &mut shz, &s.mix[at..at + n]);
                a.add(None, ga, ma);
                b.add(None, gb, mb);
            }
        }
        println!("{len} s | song not in catalog | landmark | {}", a.line());
        println!("{len} s | song not in catalog | ShazamKit | {}", b.line());
    }
    let _ = std::fs::remove_dir_all(&tmp);

    // how sure: the best score of right answers against the best of songs not in the catalog
    let mut right = Vec::new();
    let mut other = Vec::new();
    for (pool, into) in [(known, &mut right), (unknown, &mut other)] {
        for s in pool {
            for _ in 0..20 {
                let at = ((s.mix.len() - 2 * SR - SR) as f64 * rng.next()) as usize;
                if let Some((h, second)) = lm.best(&landmark::signature(&s.mix[at..at + 2 * SR])) {
                    into.push(if h.id == s.id { (h.score, second) } else { (0, h.score) });
                }
            }
        }
    }
    let hist = |v: &[u32]| {
        let mut v = v.to_vec();
        v.sort();
        let q = |f: f64| v.get(((v.len().max(1) - 1) as f64 * f) as usize).copied().unwrap_or(0);
        format!("p5 {} p50 {} p95 {} max {}", q(0.05), q(0.5), q(0.95), v.last().copied().unwrap_or(0))
    };
    println!("\n2 s landmark scores: right song {} | best other song for those {} | songs not in catalog {}",
             hist(&right.iter().map(|r| r.0).collect::<Vec<_>>()), hist(&right.iter().map(|r| r.1).collect::<Vec<_>>()),
             hist(&other.iter().map(|r| r.1.max(r.0)).collect::<Vec<_>>()));
}
