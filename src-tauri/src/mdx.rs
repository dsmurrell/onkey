//! Lead-vocal separation with UVR's MDX-Net Karaoke 2 model (ONNX). The model predicts the music *with* its
//! backing vocals; the lead vocal is what's left when that is taken away, so harmonies stay out of the
//! singer's pitch line. Processing follows python-audio-separator's MDX path: 44.1 kHz stereo, 261,120-sample
//! chunks overlapping by a quarter, STFT 5120/1024 keeping 2048 bins, Hann-weighted overlap-add.
use ndarray::Array4;
use ort::session::{builder::{GraphOptimizationLevel, SessionBuilder}, Session};
use realfft::{num_complex::Complex32, RealFftPlanner};

use crate::learn::Separator;
use crate::resample::Resampler;

const RATE: f64 = 44_100.0;
const N_FFT: usize = 5120;
const HOP: usize = 1024;
const DIM_F: usize = 2048;
const DIM_T: usize = 256;
const CHUNK: usize = HOP * (DIM_T - 1);
const TRIM: usize = N_FFT / 2;
const COMPENSATE: f32 = 1.065;

pub struct Mdx {
    session: Session,
    window: Vec<f32>,
    /// on DirectML (Windows): how to go to the CPU should a run fail there
    gpu: Option<crate::gpu::Fallback>,
}

impl Mdx {
    /// `coreml`: run on Apple's Core ML (GPU / Neural Engine), or on Windows DirectML (gpu.rs), instead of the CPU.
    pub fn load(path: &std::path::Path, threads: usize, coreml: bool) -> Result<Mdx, String> {
        // one chunk a run: the model's free batch dimension is fixed at 1, so Core ML sees static shapes (with an
        // unbounded one the Neural Engine refuses the model, "E5RT ... unbounded dimension", and the GPU runs it)
        let configure = move |b: SessionBuilder| b.with_optimization_level(GraphOptimizationLevel::Level3)?.with_intra_threads(threads)?
            .with_dimension_override("batch_size", 1);
        let loading = |e: ort::Error| format!("loading {}: {e}", path.display());
        let (session, gpu) = if coreml {
            crate::gpu::load("separation", path, configure).map_err(loading)?
        } else {
            (Session::builder().and_then(configure).and_then(|b| b.commit_from_file(path)).map_err(loading)?, None)
        };
        let window = (0..N_FFT).map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / N_FFT as f32).cos()).collect();
        Ok(Mdx { session, window, gpu })
    }

    /// `run_chunk`, on the CPU from now on if it failed on DirectML.
    fn run_chunk_or_cpu(&mut self, ch: [&[f32]; 2], planner: &mut RealFftPlanner<f32>) -> Result<[Vec<f32>; 2], String> {
        let r = self.run_chunk(ch, planner);
        if let Err(e) = &r {
            if let Some(f) = self.gpu.take() {
                self.session = f.to_cpu(e).map_err(|e| e.to_string())?;
                return self.run_chunk(ch, planner);
            }
        }
        r
    }

    /// The model's music estimate for one chunk (2 channels × CHUNK samples).
    fn run_chunk(&mut self, ch: [&[f32]; 2], planner: &mut RealFftPlanner<f32>) -> Result<[Vec<f32>; 2], String> {
        let fft = planner.plan_fft_forward(N_FFT);
        let ifft = planner.plan_fft_inverse(N_FFT);
        let mut spec = Array4::<f32>::zeros((1, 4, DIM_F, DIM_T));
        let mut inbuf = fft.make_input_vec();
        let mut out = fft.make_output_vec();
        for (c, x) in ch.iter().enumerate() {
            for t in 0..DIM_T {
                // centred frames, reflect-padded at the ends (torch.stft center=True)
                for (n, v) in inbuf.iter_mut().enumerate() {
                    let i = (t * HOP + n) as i64 - TRIM as i64;
                    let len = x.len() as i64;
                    let j = if i < 0 { -i } else if i >= len { 2 * (len - 1) - i } else { i };
                    *v = x[j.clamp(0, len - 1) as usize] * self.window[n];
                }
                fft.process(&mut inbuf, &mut out).map_err(|e| e.to_string())?;
                for f in 3..DIM_F {        // the lowest 3 bins are zeroed, as the model expects
                    spec[[0, 2 * c, f, t]] = out[f].re;
                    spec[[0, 2 * c + 1, f, t]] = out[f].im;
                }
            }
        }
        let outputs = self.session.run(ort::inputs!["input" => ort::value::Tensor::from_array(spec).map_err(|e| e.to_string())?])
            .map_err(|e| e.to_string())?;
        let (_, pred) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let at = |k: usize, f: usize, t: usize| pred[(k * DIM_F + f) * DIM_T + t];
        // inverse STFT with window-squared normalisation (torch.istft center=True), length CHUNK
        let mut result = [vec![0f32; CHUNK], vec![0f32; CHUNK]];
        let mut norm = vec![0f32; CHUNK + N_FFT];
        let mut acc = vec![0f32; CHUNK + N_FFT];
        let mut sbuf = ifft.make_input_vec();
        let mut tbuf = ifft.make_output_vec();
        for c in 0..2 {
            acc.iter_mut().for_each(|v| *v = 0.0);
            for t in 0..DIM_T {
                for (f, s) in sbuf.iter_mut().enumerate() {
                    *s = if f < DIM_F && f > 0 { Complex32::new(at(2 * c, f, t), at(2 * c + 1, f, t)) } else if f == 0 { Complex32::new(at(2 * c, 0, t), 0.0) } else { Complex32::new(0.0, 0.0) };
                }
                ifft.process(&mut sbuf, &mut tbuf).map_err(|e| e.to_string())?;
                for n in 0..N_FFT {
                    acc[t * HOP + n] += tbuf[n] / N_FFT as f32 * self.window[n];
                    if c == 0 {
                        norm[t * HOP + n] += self.window[n] * self.window[n];
                    }
                }
            }
            for i in 0..CHUNK {
                let k = i + TRIM;
                result[c][i] = if norm[k] > 1e-11 { acc[k] / norm[k] } else { 0.0 };
            }
        }
        Ok(result)
    }
}

impl Separator for Mdx {
    fn separate(&mut self, stereo: &[f32], rate: f64) -> Result<Vec<f32>, String> {
        // to 44.1 kHz, per channel
        let mut ch: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
        for c in 0..2 {
            let x: Vec<f32> = stereo.iter().skip(c).step_by(2).copied().collect();
            if (rate - RATE).abs() < 1.0 {
                ch[c] = x;
            } else {
                let mut rs = Resampler::with_cutoff(rate, RATE, 20_000.0);
                rs.process(&x, &mut ch[c]);
                rs.process(&vec![0.0; 256], &mut ch[c]);   // flush
            }
        }
        let len = (stereo.len() / 2) as f64 / rate * RATE;
        let len = len.round() as usize;
        for c in &mut ch {
            c.resize(len, 0.0);
        }
        // the model hears the music at one level whatever the app's volume (peak 0.9), and the lead is given back
        // at the level it came. (The mix was only turned down when louder, and the model's music then scaled by
        // the peak: with Spotify's volume low, peak 0.005, almost none of the music was taken away, the "lead
        // vocal" was the whole mix, and no syllable bars, and little melody, could be learned from it.)
        let peak = ch.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
        let want = ((stereo.len() / 2) as f64 / rate * crate::pitch::SR as f64).round() as usize;
        if peak < 1e-6 {
            return Ok(vec![0.0; want]);
        }
        let gain = 0.9 / peak;
        for v in ch.iter_mut().flatten() {
            *v *= gain;
        }
        // demix: pad, overlapping Hann-weighted chunks
        let gen = CHUNK - 2 * TRIM;
        let pad = gen + TRIM - len % gen;
        let total = TRIM + len + pad;
        let step = (0.75 * CHUNK as f64) as usize;
        let hann: Vec<f32> = (0..CHUNK).map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / (CHUNK - 1) as f32).cos()).collect();
        let padded: [Vec<f32>; 2] = [0, 1].map(|c| {
            let mut v = vec![0f32; TRIM];
            v.extend_from_slice(&ch[c]);
            v.resize(total, 0.0);
            v
        });
        let mut acc = [vec![0f32; total], vec![0f32; total]];
        let mut div = vec![0f32; total];
        let mut planner = RealFftPlanner::<f32>::new();
        let mut i = 0;
        while i < total {
            let end = (i + CHUNK).min(total);
            let actual = end - i;
            let w: Vec<f32> = if actual == CHUNK { hann.clone() } else {
                (0..actual).map(|n| if actual > 1 { 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / (actual - 1) as f32).cos() } else { 1.0 }).collect()
            };
            let parts: [Vec<f32>; 2] = [0, 1].map(|c| {
                let mut p = padded[c][i..end].to_vec();
                p.resize(CHUNK, 0.0);
                p
            });
            let music = self.run_chunk_or_cpu([&parts[0], &parts[1]], &mut planner)?;
            crate::progress::advance((i + step).min(total), total);     // (the window's progress bar)
            for n in 0..actual {
                acc[0][i + n] += music[0][n] * w[n];
                acc[1][i + n] += music[1][n] * w[n];
                div[i + n] += w[n];
            }
            i += step;
        }
        // lead vocal = mix - music (with backing vocals) × compensation, mono, then to 16 kHz
        let mut lead = vec![0f32; len];
        for k in 0..len {
            let j = k + TRIM;
            let d = if div[j] > 0.0 { div[j] } else { 1.0 };
            let m0 = acc[0][j] / d;
            let m1 = acc[1][j] / d;
            lead[k] = 0.5 * ((ch[0][k] - COMPENSATE * m0) + (ch[1][k] - COMPENSATE * m1)) / gain;
        }
        let mut out = Vec::new();
        let mut rs = Resampler::new(RATE, crate::pitch::SR as f64);
        rs.process(&lead, &mut out);
        rs.process(&vec![0.0; 2048], &mut out);
        out.resize(want, 0.0);
        Ok(out)
    }
}
