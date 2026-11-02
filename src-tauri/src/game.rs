//! Note transcription with GAME (Generative Adaptive MIDI Extractor, OpenVPI, MIT licence; the small ONNX
//! export): the separated lead vocal → notes (start, end, MIDI with cents). On sung test tracks it finds notes
//! far better than the pitch line + note tracker (note F1 0.71 vs 0.42, a third of the stray notes); syllable
//! bars take their notes from it. The vocal is cut at silences as GAME's own slicer does (slicer2.py), into
//! pieces of at most 30 s, each run through encoder → segmenter (8 steps) → estimator.
use ndarray::{Array1, Array2, Array3};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

const SR: usize = 44_100;

pub struct Game {
    encoder: Session,
    segmenter: Session,
    estimator: Session,
    dur2bd: Session,
    bd2dur: Session,
    /// the stretches (s, from the vocal's start) the last `notes` couldn't transcribe: no notes there isn't silence
    pub failed: Vec<(f64, f64)>,
    /// the five on DirectML (Windows): how to go to the CPU should a piece fail there
    gpu: Vec<Option<crate::gpu::Fallback>>,
}

/// Each of the five on the CPU, on every OS, as on a Mac. Not on DirectML: on a Windows laptop (2 Nov) GAME ran
/// there without an error but gave wrong notes, long runs of one note, so every syllable of a line got the same bar
/// (the pitch line, from CREPE on the same GPU, matched the Mac's); a wrong answer isn't a failure, so the fall-back
/// to the CPU never came. GAME is small and runs once per learned song: the CPU costs seconds, not minutes.
fn load(dir: &std::path::Path, name: &str, threads: usize, gpu: &mut Vec<Option<crate::gpu::Fallback>>) -> Result<Session, String> {
    let s = Session::builder()
        .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
        .and_then(|b| b.with_intra_threads(threads))
        .and_then(crate::pitch::lean)
        .and_then(|b| b.commit_from_file(dir.join(format!("{name}.onnx"))))
        .map_err(|e| format!("loading GAME {name}: {e}"))?;
    gpu.push(None);
    Ok(s)
}

impl Game {
    pub fn load(dir: &std::path::Path, threads: usize) -> Result<Game, String> {
        let mut gpu = Vec::new();
        Ok(Game { encoder: load(dir, "encoder", threads, &mut gpu)?, segmenter: load(dir, "segmenter", threads, &mut gpu)?,
                  estimator: load(dir, "estimator", threads, &mut gpu)?, dur2bd: load(dir, "dur2bd", threads, &mut gpu)?,
                  bd2dur: load(dir, "bd2dur", threads, &mut gpu)?, failed: Vec::new(), gpu })
    }

    /// `piece`, with all five on the CPU from now on if it failed on DirectML (whichever failed: they're used together).
    fn piece_or_cpu(&mut self, w: &[f32]) -> Result<Vec<(f64, f64, f32)>, String> {
        let r = self.piece(w);
        if let Err(e) = &r {
            if self.gpu.iter().any(|f| f.is_some()) {
                let gpu = std::mem::take(&mut self.gpu);
                let sessions = [&mut self.encoder, &mut self.segmenter, &mut self.estimator, &mut self.dur2bd, &mut self.bd2dur];
                for (s, f) in sessions.into_iter().zip(gpu) {
                    if let Some(f) = f {
                        *s = f.to_cpu(e).map_err(|e| e.to_string())?;
                    }
                }
                return self.piece(w);
            }
        }
        r
    }

    /// Notes (start s, end s, MIDI) in a 16 kHz vocal, times from its start.
    pub fn notes(&mut self, vocal16: &[f32]) -> Result<Vec<(f64, f64, f32)>, String> {
        // up to 44.1 kHz (linear), as GAME expects
        // a quiet singer brought up to a usual level (peak 0.5, as separated from music at full volume): GAME's
        // slicer takes -40 dB as silence, and a singer separated from music played quietly (peak -49 dBFS, live)
        // was all "silence", no notes at all. (Usual levels are left as they are.)
        let n = vocal16.len() * SR / 16_000;
        let peak = vocal16.iter().fold(0f32, |m, v| m.max(v.abs()));
        let gain = if peak > 1e-6 && peak < 0.25 { 0.5 / peak } else { 1.0 };
        let x: Vec<f32> = (0..n).map(|i| {
            let p = i as f64 * 16_000.0 / SR as f64;
            let k = p as usize;
            let f = (p - k as f64) as f32;
            gain * (vocal16[k] * (1.0 - f) + vocal16.get(k + 1).copied().unwrap_or(0.0) * f)
        }).collect();
        let mut out = Vec::new();
        let (mut tried, mut failed) = (0, 0);
        self.failed.clear();
        for (off, a, b) in slice(&x) {
            let mut k = a;
            while k < b {
                let e = (k + 30 * SR).min(b);
                if e - k >= SR / 10 {
                    tried += 1;
                    let t0 = off + (k - a) as f64 / SR as f64;
                    // one piece the model can't handle loses only that piece's notes, not the whole song's
                    match self.piece_or_cpu(&x[k..e]) {
                        Ok(notes) => out.extend(notes.into_iter().map(|(s, f, m)| (t0 + s, t0 + f, m))),
                        Err(err) => {
                            failed += 1;
                            self.failed.push((t0, t0 + (e - k) as f64 / SR as f64));
                            crate::engine::log(&format!("GAME: piece at {t0:.1} s ({:.2} s long) skipped: {err}", (e - k) as f64 / SR as f64));
                        }
                    }
                }
                k = e;
                crate::progress::advance(e, x.len());     // (the window's progress bar)
            }
        }
        if tried > 0 && failed == tried {
            return Err("every piece failed".into());
        }
        Ok(out)
    }

    fn piece(&mut self, w: &[f32]) -> Result<Vec<(f64, f64, f32)>, String> {
        let e = |e: ort::Error| e.to_string();
        let dur = w.len() as f32 / SR as f32;
        let wave = Tensor::from_array(Array2::from_shape_vec((1, w.len()), w.to_vec()).map_err(|e| e.to_string())?).map_err(e)?;
        let enc = self.encoder.run(ort::inputs!["waveform" => wave, "duration" => Tensor::from_array(Array1::from_vec(vec![dur])).map_err(e)?]).map_err(e)?;
        let (sh, xs) = enc["x_seg"].try_extract_tensor::<f32>().map_err(e)?;
        let t = sh[1] as usize;
        let x_seg = Array3::from_shape_vec((1, t, 128), xs.to_vec()).map_err(|e| e.to_string())?;
        let (_, xe) = enc["x_est"].try_extract_tensor::<f32>().map_err(e)?;
        let x_est = Array3::from_shape_vec((1, t, 128), xe.to_vec()).map_err(|e| e.to_string())?;
        let (_, mt) = enc["maskT"].try_extract_tensor::<bool>().map_err(e)?;
        let mask_t = Array2::from_shape_vec((1, t), mt.to_vec()).map_err(|e| e.to_string())?;
        drop(enc);
        let known = {
            let o = self.dur2bd.run(ort::inputs!["durations" => Tensor::from_array(Array2::from_shape_vec((1, 1), vec![dur]).unwrap()).map_err(e)?,
                                                  "maskT" => Tensor::from_array(mask_t.clone()).map_err(e)?]).map_err(e)?;
            let (_, b) = o["boundaries"].try_extract_tensor::<bool>().map_err(e)?;
            Array2::from_shape_vec((1, t), b.to_vec()).map_err(|e| e.to_string())?
        };
        let mut bd = known.clone();
        for step in 0..8 {
            let o = self.segmenter.run(ort::inputs![
                "x_seg" => Tensor::from_array(x_seg.clone()).map_err(e)?,
                "language" => Tensor::from_array(Array1::from_vec(vec![0i64])).map_err(e)?,
                "known_boundaries" => Tensor::from_array(known.clone()).map_err(e)?,
                "prev_boundaries" => Tensor::from_array(bd.clone()).map_err(e)?,
                "t" => Tensor::from_array(Array1::from_vec(vec![step as f32 / 8.0])).map_err(e)?,
                "maskT" => Tensor::from_array(mask_t.clone()).map_err(e)?,
                "threshold" => Tensor::from_array(ndarray::arr0(0.2f32)).map_err(e)?,
                "radius" => Tensor::from_array(ndarray::arr0(2i64)).map_err(e)?]).map_err(e)?;
            let (_, b) = o["boundaries"].try_extract_tensor::<bool>().map_err(e)?;
            bd = Array2::from_shape_vec((1, t), b.to_vec()).map_err(|e| e.to_string())?;
        }
        let o = self.bd2dur.run(ort::inputs!["boundaries" => Tensor::from_array(bd.clone()).map_err(e)?,
                                              "maskT" => Tensor::from_array(mask_t.clone()).map_err(e)?]).map_err(e)?;
        let (dsh, durs) = o["durations"].try_extract_tensor::<f32>().map_err(e)?;
        let nn = dsh[1] as usize;
        let durs = durs.to_vec();
        let (_, mn) = o["maskN"].try_extract_tensor::<bool>().map_err(e)?;
        let mask_n = Array2::from_shape_vec((1, nn), mn.to_vec()).map_err(|e| e.to_string())?;
        drop(o);
        let o = self.estimator.run(ort::inputs!["x_est" => Tensor::from_array(x_est).map_err(e)?,
                                                 "boundaries" => Tensor::from_array(bd).map_err(e)?,
                                                 "maskT" => Tensor::from_array(mask_t).map_err(e)?,
                                                 "maskN" => Tensor::from_array(mask_n.clone()).map_err(e)?,
                                                 "threshold" => Tensor::from_array(ndarray::arr0(0.2f32)).map_err(e)?]).map_err(e)?;
        let (_, pres) = o["presence"].try_extract_tensor::<bool>().map_err(e)?;
        let (_, sc) = o["scores"].try_extract_tensor::<f32>().map_err(e)?;
        let mut notes = Vec::new();
        let mut t0 = 0f64;
        for i in 0..nn {
            if !mask_n[[0, i]] {
                continue;
            }
            let d = durs[i] as f64;
            if pres[i] {
                notes.push((t0, t0 + d, sc[i]));
            }
            t0 += d;
        }
        Ok(notes)
    }
}

/// GAME's slicer (slicer2.py with threshold -40 dB, min_length 1000 ms, min_interval 200 ms, hop 20 ms,
/// max_sil_kept 100 ms): pieces of sound between silences, as (offset s, first sample, end sample).
fn slice(x: &[f32]) -> Vec<(f64, usize, usize)> {
    let hop = (SR as f64 * 0.020).round() as usize;
    let min_interval_s = SR as f64 * 0.200;
    let win = (min_interval_s.round() as usize).min(4 * hop);
    let min_length = (SR as f64 * 1.000 / hop as f64).round() as usize;
    let min_interval = (min_interval_s / hop as f64).round() as usize;
    let max_sil = (SR as f64 * 0.100 / hop as f64).round() as usize;
    let threshold = 10f32.powf(-40.0 / 20.0);
    if x.len().div_ceil(hop) <= min_length {
        return vec![(0.0, 0, x.len())];
    }
    // librosa-style RMS: frames centred on k*hop, zero padded
    let half = win / 2;
    let nf = 1 + x.len() / hop;
    let rms: Vec<f32> = (0..nf).map(|f| {
        let c = f * hop;
        let s: f32 = (0..win).map(|j| {
            let i = (c + j) as i64 - half as i64;
            if i >= 0 && (i as usize) < x.len() { x[i as usize].powi(2) } else { 0.0 }
        }).sum();
        (s / win as f32).sqrt()
    }).collect();
    let argmin = |a: usize, b: usize| -> usize {
        let b = b.min(rms.len());
        (a..b).min_by(|i, j| rms[*i].total_cmp(&rms[*j])).unwrap_or(a)
    };
    let mut tags: Vec<(usize, usize)> = Vec::new();
    let (mut sil_start, mut clip_start): (Option<usize>, usize) = (None, 0);
    for (i, &r) in rms.iter().enumerate() {
        if r < threshold {
            sil_start.get_or_insert(i);
            continue;
        }
        let Some(ss) = sil_start else { continue };
        let leading = ss == 0 && i > max_sil;
        let middle = i - ss >= min_interval && i - clip_start >= min_length;
        if !leading && !middle {
            sil_start = None;
            continue;
        }
        if i - ss <= max_sil {
            let pos = argmin(ss, i + 1);
            tags.push(if ss == 0 { (0, pos) } else { (pos, pos) });
            clip_start = pos;
        } else if i - ss <= max_sil * 2 {
            let pos = argmin(i - max_sil, ss + max_sil + 1);
            let pl = argmin(ss, ss + max_sil + 1);
            let pr = argmin(i - max_sil, i + 1);
            if ss == 0 {
                tags.push((0, pr));
                clip_start = pr;
            } else {
                tags.push((pl.min(pos), pr.max(pos)));
                clip_start = pr.max(pos);
            }
        } else {
            let pl = argmin(ss, ss + max_sil + 1);
            let pr = argmin(i - max_sil, i + 1);
            tags.push(if ss == 0 { (0, pr) } else { (pl, pr) });
            clip_start = pr;
        }
        sil_start = None;
    }
    let total = rms.len();
    if let Some(ss) = sil_start {
        if total - ss >= min_interval {
            let pos = argmin(ss, (ss + max_sil).min(total) + 1);
            tags.push((pos, total + 1));
        }
    }
    let cut = |a: usize, b: usize| (a as f64 * hop as f64 / SR as f64, a * hop, (b * hop).min(x.len()));
    if tags.is_empty() {
        return vec![(0.0, 0, x.len())];
    }
    let mut out = Vec::new();
    if tags[0].0 > 0 {
        out.push(cut(0, tags[0].0));
    }
    for w in tags.windows(2) {
        out.push(cut(w[0].1, w[1].0));
    }
    if tags.last().unwrap().1 < total {
        out.push(cut(tags.last().unwrap().1, total));
    }
    out.into_iter().filter(|(_, a, b)| b > a).collect()
}
