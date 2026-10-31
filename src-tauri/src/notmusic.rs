//! Is a minute of sound music, or someone talking (a YouTube video that isn't a song)? Judged from the music as
//! captured (16 kHz mono: what a take has 60 s in; the singer isn't separated until the song ends).
//!
//! Three things tell them apart: music holds tones steady (a note, a chord: the same spectral peaks for 128 ms and
//! more; a speaking voice glides), has a beat (the onsets repeat), and rarely stops, where talk pauses between
//! phrases. Talk is called only when there are few held tones AND no beat AND gaps.
//! Calibrated on every 60 s window (every 30 s, the last one shorter) of 244 kept songs against real and synthetic
//! speech, alone and over a music bed: no song window is called talk; see the margin in
//! no_kept_song_is_talk (the nearest songs: a cappella and spoken theatre).

use realfft::RealFftPlanner;

pub const SR: usize = 16_000;
/// Held tones: share of the spectral peaks still there 128 ms on (songs 0.097 and up; talk 0.06-0.15).
pub const PERSIST_TALK: f64 = 0.13;
/// Beat: the onset envelope's autocorrelation at 0.25-1.5 s (songs 0.05 and up, but never with few held tones).
pub const BEAT_TALK: f64 = 0.22;
/// Gaps: share of 20 ms frames 15 dB under the loud ones (talk 0.195 and up; most songs far less). Added after the
/// last minute of Can't Stop (few held tones, weak beat, but no gaps: 0.059) was called talk without it.
pub const QUIET_TALK: f64 = 0.12;

#[derive(Debug, Clone, Copy)]
pub struct Verdict {
    pub persist: f64,
    pub beat: f64,
    pub quiet: f64,
}

impl Verdict {
    pub fn talk(&self) -> bool {
        self.persist < PERSIST_TALK && self.beat < BEAT_TALK && self.quiet >= QUIET_TALK
    }

    /// How far from talk it is: 1 at the line, more inside music (the thresholds loosened this much would call it talk).
    pub fn margin(&self) -> f64 {
        (self.persist / PERSIST_TALK).max(self.beat / BEAT_TALK).max(QUIET_TALK / self.quiet.max(1e-9))
    }
}

/// numpy's percentile (linear interpolation).
fn percentile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));     // (NaN audio sorts last, never panics)
    let pos = q / 100.0 * (s.len() - 1) as f64;
    let (i, f) = (pos.floor() as usize, pos - pos.floor());
    if i + 1 < s.len() { s[i] + (s[i + 1] - s[i]) * f } else { s[i] }
}

/// The verdict on `x` (16 kHz mono, ideally 60 s); None when it's (nearly) silent or too short to judge.
pub fn judge(x: &[f32]) -> Option<Verdict> {
    const N: usize = 1024;
    const HOP: usize = 256;
    if x.len() < 10 * SR {
        return None;
    }
    // loudness in 20 ms frames: silence isn't judged
    let db: Vec<f64> = x.chunks_exact(320)
        .map(|f| 10.0 * (f.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / 320.0 + 1e-12).log10()).collect();
    let reference = percentile(&db, 90.0);
    if reference < -60.0 {
        return None;
    }
    // gaps: the share of frames 15 dB or more under the loud ones (talk pauses between phrases; music rarely does)
    let quiet = db.iter().filter(|d| **d < reference - 15.0).count() as f64 / db.len() as f64;
    // spectrum, 80 Hz - 4 kHz, in dB: 64 ms frames every 16 ms
    let (lo, hi) = (80 * N / SR, 4000 * N / SR);
    let nb = hi - lo;
    let win: Vec<f64> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (N - 1) as f64).cos()).collect();
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(N);
    let mut inp = fft.make_input_vec();
    let mut out = fft.make_output_vec();
    let t = 1 + (x.len() - N) / HOP;
    let mut p = vec![0f64; t * nb];
    for k in 0..t {
        for (i, v) in inp.iter_mut().enumerate() {
            *v = x[k * HOP + i] as f64 * win[i];
        }
        fft.process(&mut inp, &mut out).ok()?;
        for j in 0..nb {
            p[k * nb + j] = 10.0 * (out[lo + j].norm_sqr() + 1e-12).log10();
        }
    }
    let row = |k: usize| &p[k * nb..(k + 1) * nb];
    let maxes: Vec<f64> = (0..t).map(|k| row(k).iter().cloned().fold(f64::MIN, f64::max)).collect();
    let loud_from = percentile(&maxes, 90.0) - 30.0;
    // tonal peaks: above both neighbours and 12 dB over the frame's median
    let mut pk = vec![false; t * nb];
    for k in 0..t {
        let r = row(k);
        let mut s = r.to_vec();
        s.sort_by(|a, b| a.total_cmp(b));     // (NaN audio sorts last, never panics)
        let med = if nb % 2 == 1 { s[nb / 2] } else { (s[nb / 2 - 1] + s[nb / 2]) / 2.0 };
        for j in 1..nb - 1 {
            pk[k * nb + j] = r[j] > r[j - 1] && r[j] > r[j + 1] && r[j] > med + 12.0;
        }
    }
    let near = |k: usize, j: usize| pk[k * nb + j] || (j > 0 && pk[k * nb + j - 1]) || (j + 1 < nb && pk[k * nb + j + 1]);
    let (mut peaks, mut held) = (0usize, 0usize);
    for k in (0..t).filter(|k| maxes[*k] > loud_from) {
        for j in 0..nb {
            if pk[k * nb + j] {
                peaks += 1;
                if (1..=8).all(|d| k + d < t && near(k + d, j)) {
                    held += 1;
                }
            }
        }
    }
    let persist = held as f64 / peaks.max(1) as f64;
    // beat: the onset envelope (rises in the spectrum) repeating at 0.25-1.5 s
    let mut flux: Vec<f64> = (0..t - 1).map(|k| (0..nb).map(|j| (p[(k + 1) * nb + j] - p[k * nb + j]).max(0.0)).sum()).collect();
    let mean = flux.iter().sum::<f64>() / flux.len() as f64;
    flux.iter_mut().for_each(|v| *v -= mean);
    let ac = |l: usize| flux.iter().zip(&flux[l..]).map(|(a, b)| a * b).sum::<f64>();
    let a0 = ac(0) + 1e-9;
    let fps = SR as f64 / HOP as f64;
    let (a, b) = ((0.25 * fps) as usize, (1.5 * fps) as usize);
    let beat = (a..b.min(flux.len())).map(|l| ac(l) / a0).fold(f64::MIN, f64::max);
    Some(Verdict { persist, beat, quiet })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_audio_is_judged_without_a_panic() {
        let mut x = tone_song(12);
        x[1000..1100].iter_mut().for_each(|v| *v = f32::NAN);
        let _ = judge(&x);
        assert_eq!(percentile(&[3.0, f64::NAN, 1.0, 2.0], 0.0), 1.0);
    }

    fn tone_song(secs: usize) -> Vec<f32> {
        // a chord held for half a second at a time, with a click on every beat: music
        (0..secs * SR).map(|i| {
            let t = i as f64 / SR as f64;
            let chord = [220.0, 277.2, 329.6].iter().map(|f| (2.0 * std::f64::consts::PI * f * (1.0 + (t * 2.0).floor() * 0.06) * t).sin()).sum::<f64>();
            let click = if (t * 2.0).fract() < 0.01 { 1.5 } else { 0.0 };
            (0.1 * chord + click * 0.2) as f32
        }).collect()
    }

    fn babble(secs: usize) -> Vec<f32> {
        // a voice-like buzz gliding in pitch, in syllables, with pauses: talk
        let mut ph = 0.0f64;
        (0..secs * SR).map(|i| {
            let t = i as f64 / SR as f64;
            let f0 = 140.0 + 50.0 * (t * 14.5).sin() + 30.0 * (t * 32.0).sin();
            ph += 2.0 * std::f64::consts::PI * f0 / SR as f64;
            let syl = ((t * 4.3).sin().max(0.0)) * if (t * 0.37).fract() < 0.25 { 0.0 } else { 1.0 };
            let buzz: f64 = (1..12).map(|h| (ph * h as f64).sin() / h as f64).sum();
            (0.2 * syl * buzz) as f32
        }).collect()
    }

    #[test]
    fn held_chords_on_a_beat_are_music() {
        let v = judge(&tone_song(60)).unwrap();
        assert!(!v.talk(), "{v:?}");
    }

    #[test]
    fn gliding_syllables_with_pauses_are_talk() {
        let v = judge(&babble(60)).unwrap();
        assert!(v.talk(), "{v:?}");
    }

    #[test]
    fn silence_and_short_bits_are_not_judged() {
        assert!(judge(&vec![0.0; 60 * SR]).is_none());
        assert!(judge(&tone_song(5)).is_none());
    }

    /// Kept recordings (each a folder with the song's mix as `mix16.f32`, 16 kHz mono f32), from ONKEY_RECORDED_DIR;
    /// the two tests below skip without it.
    fn recorded_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("ONKEY_RECORDED_DIR").map(std::path::PathBuf::from)
    }

    /// Against the calibration's own numbers on kept audio, when it's there.
    #[test]
    #[ignore]
    fn matches_the_lab_features() {
        let Some(lab) = recorded_dir() else { return };
        for name in ["Comedy-Cake-When-Comedians-Make-Hecklers-Lose-It", "Adele-Rolling-in-the-Deep---Acapella", "Taylor-Swift-Shake-It-Off"] {
            let b = std::fs::read(lab.join(name).join("mix16.f32")).unwrap();
            let x: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            let v = judge(&x[..60 * SR]).unwrap();
            println!("{name}: persist {:.4} beat {:.4} quiet {:.4} talk {}", v.persist, v.beat, v.quiet, v.talk());
        }
    }

    /// Every kept song, every 60 s window (every 30 s): none is talk; the closest to it, for the margin.
    #[test]
    #[ignore]
    fn no_kept_song_is_talk() {
        let Some(lab) = recorded_dir() else { return };
        let (mut n, mut talk, mut closest) = (0, Vec::new(), (f64::MAX, String::new()));
        for d in std::fs::read_dir(&lab).unwrap().flatten() {
            let name = d.file_name().to_string_lossy().into_owned();
            let Ok(b) = std::fs::read(d.path().join("mix16.f32")) else { continue };
            if name.starts_with("Comedy-Cake") {
                continue;
            }
            let x: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            let mut s = 0;
            while s + 40 * SR <= x.len() {
                if let Some(v) = judge(&x[s..(s + 60 * SR).min(x.len())]) {
                    n += 1;
                    // how far inside "music" it is: the larger of the two ratios to its threshold
                    let m = v.margin();
                    if m < closest.0 {
                        closest = (m, format!("{name} at {} s: {v:?}", s / SR));
                    }
                    if v.talk() {
                        talk.push(format!("{name} at {} s", s / SR));
                    }
                }
                s += 30 * SR;
            }
        }
        println!("{n} windows; closest to talk x{:.2}: {}", closest.0, closest.1);
        assert!(talk.is_empty(), "{talk:?}");
    }
}
