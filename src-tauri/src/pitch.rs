//! Your pitch, every 10 ms: CREPE-tiny (MIT-licensed, ~2 MB) run with ONNX Runtime.
//! Same rules as the Python engine: frames of 1024 samples at 16 kHz, hop 160; weighted argmax over the
//! 9 bins around the peak; confidence ("periodicity") is the peak probability.
use ndarray::Array2;
use ort::session::{builder::{GraphOptimizationLevel, SessionBuilder}, Session};

pub const SR: usize = 16_000;
pub const HOP: usize = 160;
pub const WIN: usize = 1024;

pub struct Crepe {
    session: Session,
    /// models exported with a fixed batch (for Core ML) take exactly this many frames per run
    batch: Option<usize>,
    /// highest pitch bin considered (exclusive): voices need 1100 Hz; whistling goes to ~2 kHz
    hi_bin: usize,
    /// also look for a pure tone in each frame (live: whistling), see `pure_tone`
    tones: Option<Tones>,
    /// on DirectML (Windows): how to go to the CPU should a run fail there
    gpu: Option<crate::gpu::Fallback>,
}

/// Bin limits: 39 = 50 Hz; 308 = 1100 Hz (the singers being learned); 360 = 1975 Hz, the top of CREPE's range
/// (your voice live, so whistling and falsetto register too).
pub const BIN_1100_HZ: usize = 308;
pub const BIN_TOP: usize = 360;

/// CREPE-tiny, built into the app (your voice, live).
const TINY: &[u8] = include_bytes!("../models/crepe-tiny.onnx");

/// A session for inputs whose size changes from run to run (a whole song's singer): no memory arena and no memory
/// patterns, so what a run takes is given back after it. With them, ONNX Runtime keeps the most any run took, in
/// blocks that only grow: word alignment and GAME held 1.5-2 GB each after a few songs (live, OnKey reached 12 GB).
pub fn lean(b: ort::session::builder::SessionBuilder) -> ort::Result<ort::session::builder::SessionBuilder> {
    b.with_memory_pattern(false)?
        .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default().with_arena_allocator(false).build()])
}

/// A session on Core ML (GPU / Neural Engine) for the model file `model`, `configure` setting its own options
/// (threads, optimisation level). Core ML's ML Program model format: with ONNX Runtime's default (the older
/// NeuralNetwork format) Core ML couldn't take MDX-Net's MatMuls (4-D inputs), so the separator was cut into about
/// 30 Core ML pieces with CPU work between them, and every piece's 32-96 MB activations crossed to the CPU and
/// back: per set of models, ONNX Runtime's CPU arena held 2.3 GB and Core ML 1.4 GB more, 4.4 GB in all (two sets
/// live: 9-12 GB), and the CPU did 7x the work. As ML Programs the two models are 8 pieces and a set holds 1.3 GB.
///
/// The compiled models are kept in a cache folder (see `set_coreml_cache_dir`, under the model's name, size and
/// date, so a new version of a model is compiled afresh): a launch doesn't compile them again (seconds each, and
/// a minute or more on a small Mac), and they don't pile up in the temp folder as ONNX Runtime's own temporary
/// copies did (14 GB after a few days). One session loads at a time, so two learners starting together never
/// compile into the same cache entry.
///
/// On a small Mac (lowmem.rs) the session keeps no memory patterns and no CPU arena, so what a run took is given
/// back after it instead of held for the next (the `lean` sessions' reason, for the CPU side of a Core ML model).
pub fn load_coreml(model: &std::path::Path, configure: impl Fn(SessionBuilder) -> ort::Result<SessionBuilder>) -> ort::Result<Session> {
    use ort::execution_providers::{coreml::CoreMLModelFormat, CoreMLExecutionProvider, CPUExecutionProvider};
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _loading = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let build = |cache: Option<&std::path::Path>| -> ort::Result<Session> {
        let mut ep = CoreMLExecutionProvider::default().with_model_format(CoreMLModelFormat::MLProgram);
        if let Some(c) = cache {
            ep = ep.with_model_cache_dir(c.to_string_lossy());
        }
        let b = configure(Session::builder()?)?;
        if crate::lowmem::low_memory() {
            b.with_memory_pattern(false)?
                .with_execution_providers([ep.build(), CPUExecutionProvider::default().with_arena_allocator(false).build()])?
                .commit_from_file(model)
        } else {
            b.with_execution_providers([ep.build()])?.commit_from_file(model)
        }
    };
    let Some(cache) = coreml_cache(model) else { return build(None) };
    build(Some(&cache)).or_else(|e| {
        // a cache entry that can't be used (half written by a run that was stopped): made again, else done without
        crate::engine::log(&format!("Core ML cache for {}: {e}; compiling it again", model.display()));
        let _ = std::fs::remove_dir_all(&cache);
        build(Some(&cache)).or_else(|_| build(None))
    })
}

/// The cache's layout and the session options that shape a compiled model (ONNX Runtime keys an entry by the
/// model's path alone): bumped whenever those change, so old entries aren't reused.
const COREML_CACHE_VERSION: u32 = 2;

/// Where the compiled models are kept: the app sets its data folder at launch, so each model is compiled once,
/// ever (the temp folder, the default for tools and tests, is cleared by macOS of files untouched for three days,
/// and a first learn after that compiled them all again: a minute or more on a small Mac).
static CACHE_BASE: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

pub fn set_coreml_cache_dir(dir: &std::path::Path) {
    *CACHE_BASE.lock().unwrap() = Some(dir.to_path_buf());
}

/// Where `model`'s compiled Core ML pieces are kept (None: no cache folder could be made).
fn coreml_cache(model: &std::path::Path) -> Option<std::path::PathBuf> {
    let meta = std::fs::metadata(model).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let base = CACHE_BASE.lock().unwrap().clone().unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("onkey-coreml-{COREML_CACHE_VERSION}"))
        .join(format!("{}-{}-{modified}", model.file_name()?.to_string_lossy(), meta.len()));
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// MIDI note number (69 = A4), NaN when not voiced
    pub midi: f32,
    pub confidence: f32,
    pub rms: f32,
    /// a pure tone's pitch (MIDI), NaN when the frame isn't one (only looked for live)
    pub tone: f32,
}

impl Crepe {
    /// CREPE-tiny, built into the app: your voice, live.
    ///
    /// Run by Core ML on the CPU (Apple's own kernels, through ONNX Runtime's Core ML provider as an ML Program): the
    /// very same model and frames, the same pitch and confidence (within 0.0001 semitone on 4417 voiced frames, no
    /// frame's voicing changed: examples/crepebench), for about a tenth of the CPU. ONNX Runtime's own CPU kernels
    /// spent ~45-55% of a core on it (each frame's first convolution unrolled into memory, two threads spinning
    /// between runs), so the engine fell behind your voice on a busy Mac (the dot 0.1-0.5 s late). Not on the Neural
    /// Engine: it needs fixed input sizes, and runs only part of it. If Core ML can't take it, ONNX Runtime's CPU
    /// kernels as before (ONKEY_LIVE_PITCH_CPU=1 forces that in development builds, for comparison). Core ML is
    /// Apple's: on Windows DirectML is tried instead (gpu.rs; its first run there checks it). Which is the platform's
    /// (platform/*/gpu.rs `live_pitch`).
    pub fn new() -> ort::Result<Crepe> {
        if crate::devtest::knob!("ONKEY_LIVE_PITCH_CPU").is_none() {
            if let Some(c) = crate::platform::gpu::live_pitch()? {
                return Ok(c);
            }
        }
        Crepe::tiny_with(|b| b.with_intra_threads(2))
    }

    /// CREPE-tiny on a session made for it elsewhere (platform/*/gpu.rs), with how to go to the CPU should a run
    /// fail there.
    pub fn tiny_on(session: Session, gpu: Option<crate::gpu::Fallback>) -> Crepe {
        Crepe { session, batch: None, hi_bin: BIN_TOP, tones: Some(Tones::new()), gpu }
    }

    /// CREPE-tiny itself (the model's bytes).
    pub fn tiny_model() -> &'static [u8] {
        TINY
    }

    /// CREPE-tiny with the session set up by `configure` (threads, execution providers): for measuring how to run
    /// the very same model (examples/crepebench).
    pub fn tiny_with(configure: impl FnOnce(SessionBuilder) -> ort::Result<SessionBuilder>) -> ort::Result<Crepe> {
        let session = configure(Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3)?)?
            .commit_from_memory(TINY)?;
        Ok(Crepe { session, batch: None, hi_bin: BIN_TOP, tones: Some(Tones::new()), gpu: None })
    }

    /// A CREPE model from a file (e.g. CREPE-full, for the singer's line when learning a song). With
    /// `coreml_batch`, the model has that fixed batch size and runs on Core ML (GPU / Neural Engine), or on Windows
    /// DirectML (gpu.rs).
    pub fn from_file(path: &std::path::Path, threads: usize, coreml_batch: Option<usize>) -> ort::Result<Crepe> {
        let configure = move |b: SessionBuilder| b.with_optimization_level(GraphOptimizationLevel::Level3)?.with_intra_threads(threads);
        let (session, gpu) = match coreml_batch {
            Some(_) => crate::gpu::load("pitch", path, configure)?,
            None => (configure(Session::builder()?)?.commit_from_file(path)?, None),
        };
        Ok(Crepe { session, batch: coreml_batch, hi_bin: BIN_1100_HZ, tones: None, gpu })
    }

    /// Run `batch` frames at a time (the last run padded): a fixed input shape, which Core ML compiles once.
    pub fn set_batch(&mut self, batch: Option<usize>) {
        self.batch = batch;
    }

    pub fn set_ceiling(&mut self, hi_bin: usize) {
        self.hi_bin = hi_bin.clamp(40, BIN_TOP);
    }

    /// Pitch for `k` frames, the last of which is centred 512 samples before the end of `x` (16 kHz mono).
    pub fn frames(&mut self, x: &[f32], k: usize) -> ort::Result<Vec<Frame>> {
        let r = self.frames_here(x, k);
        // (on the CPU from now on, if it failed on DirectML)
        if let Err(e) = &r {
            if let Some(f) = self.gpu.take() {
                self.session = f.to_cpu(e)?;
                return self.frames_here(x, k);
            }
        }
        r
    }

    fn frames_here(&mut self, x: &[f32], k: usize) -> ort::Result<Vec<Frame>> {
        let need = (k.max(1) - 1) * HOP + WIN;
        if k == 0 || x.len() < need {
            return Ok(vec![Frame { midi: f32::NAN, confidence: 0.0, rms: 0.0, tone: f32::NAN }; k]);
        }
        let seg = &x[x.len() - need..];
        let rows = self.batch.map_or(k, |b| k.div_ceil(b) * b);
        let mut input = Array2::<f32>::zeros((rows, WIN));
        let mut rms = vec![0f32; k];
        let mut tone = vec![f32::NAN; k];
        for j in 0..k {
            let w = &seg[j * HOP..j * HOP + WIN];
            if let Some(t) = self.tones.as_mut() {
                tone[j] = t.pure_tone(w);
            }
            let mean = w.iter().sum::<f32>() / WIN as f32;
            let var = w.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / WIN as f32;
            let std = var.sqrt().max(1e-10);
            rms[j] = (w.iter().map(|v| v * v).sum::<f32>() / WIN as f32).sqrt();
            for (i, v) in w.iter().enumerate() {
                input[[j, i]] = (v - mean) / std;
            }
        }
        let mut probs: Vec<f32> = Vec::with_capacity(rows * 360);
        let step = self.batch.unwrap_or(rows);
        for r in (0..rows).step_by(step) {
            let part = input.slice(ndarray::s![r..r + step, ..]).to_owned();
            let outputs = self.session.run(ort::inputs!["frames" => ort::value::Tensor::from_array(part)?])?;
            probs.extend_from_slice(outputs["probs"].try_extract_tensor::<f32>()?.1);
        }
        Ok((0..k).map(|j| {
            let (midi, confidence) = decode(&probs[j * 360..(j + 1) * 360], self.hi_bin);
            Frame { midi, confidence, rms: rms[j], tone: tone[j] }
        }).collect())
    }
}

/// How much of the energy a pure tone's peak holds (ONKEY_PURE overrides it, for tests).
static PURE_SHARE: std::sync::LazyLock<f32> =
    std::sync::LazyLock::new(|| crate::devtest::knob!("ONKEY_PURE").and_then(|v| v.parse().ok()).unwrap_or(0.5));

/// Pure tones (a whistle): CREPE-tiny is often unsure of them (a whistle's frames had a median confidence of 0.48)
/// though a spectrum shows them plainly: one peak holding most of the energy.
struct Tones {
    fft: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    window: Vec<f32>,
    buf: Vec<f32>,
    spec: Vec<realfft::num_complex::Complex32>,
}

impl Tones {
    fn new() -> Tones {
        let fft = realfft::RealFftPlanner::<f32>::new().plan_fft_forward(WIN);
        let window = (0..WIN).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (WIN - 1) as f32).cos()).collect();
        let spec = fft.make_output_vec();
        Tones { fft, window, buf: vec![0.0; WIN], spec }
    }

    /// The pitch (MIDI) of a pure tone at 900-2090 Hz: the spectrum's peak there (+-3 bins) holding at least half of
    /// the energy at 100-4000 Hz. NaN otherwise (a voice's harmonics, noise, music spread their energy). Not lower:
    /// music bleeding into the mic (no echo cancelling) had pure moments, 90% of them under 900 Hz, where a
    /// whistle is rare (Under Pressure's whistle: all of the frames this added were over 900 Hz); CREPE still
    /// tracks a whistle down there when it's sure.
    fn pure_tone(&mut self, w: &[f32]) -> f32 {
        for (b, (x, h)) in self.buf.iter_mut().zip(w.iter().zip(&self.window)) {
            *b = x * h;
        }
        if self.fft.process(&mut self.buf, &mut self.spec).is_err() {
            return f32::NAN;
        }
        let bin = |hz: f32| (hz * WIN as f32 / SR as f32).round() as usize;
        let p: Vec<f32> = self.spec[..=bin(4000.0)].iter().map(|c| c.norm_sqr()).collect();
        let (lo, hi) = (bin(900.0), bin(2090.0));
        let j = (lo..=hi).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap();
        let total: f32 = p[bin(100.0)..].iter().sum();
        let peak: f32 = p[j - 3..=j + 3].iter().sum();
        if total <= 0.0 || peak < *PURE_SHARE * total {
            return f32::NAN;
        }
        // the peak's centre from the log magnitudes either side (parabolic)
        let (a, b, c) = ((p[j - 1] + 1e-30).ln(), (p[j] + 1e-30).ln(), (p[j + 1] + 1e-30).ln());
        let d = a - 2.0 * b + c;
        let off = if d.abs() > 1e-9 { (0.5 * (a - c) / d).clamp(-0.5, 0.5) } else { 0.0 };
        let hz = (j as f32 + off) * SR as f32 / WIN as f32;
        69.0 + 12.0 * (hz / 440.0).log2()
    }
}

/// Decoding exactly as torchcrepe does (the Python engine's scores were tuned on it): only bins for
/// 50-1100 Hz count; pitch is the average of the 9 bins around the peak weighted by sigmoid(probability)
/// (torchcrepe applies the sigmoid twice); confidence ("periodicity") is the peak probability. torchcrepe
/// also adds random dither to the bin centres; that is left out so results are repeatable.
fn decode(p: &[f32], hi: usize) -> (f32, f32) {
    const LO: usize = 39;   // 50 Hz
    let (center, &peak) = p[LO..hi].iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, v)| (i + LO, v)).unwrap();
    let lo = center.saturating_sub(4);
    let hi = (center + 5).min(hi);
    let (mut wsum, mut csum) = (0f32, 0f32);
    for b in lo..hi {
        let w = 1.0 / (1.0 + (-p[b]).exp());
        wsum += w;
        csum += w * (20.0 * b as f32 + 1997.3794);
    }
    let cents = csum / wsum;
    let hz = 10.0 * 2f32.powf(cents / 1200.0);
    (69.0 + 12.0 * (hz / 440.0).log2(), peak)
}

/// Pitch for a whole recording, one frame per 10 ms centred on sample i*160 (zero-padded at the ends), with
/// the same median filters the Python engine applied (periodicity over 3 frames, pitch over 5).
/// Returns (midi, periodicity, rms) per frame.
pub fn track(crepe: &mut Crepe, x: &[f32]) -> ort::Result<(Vec<f32>, Vec<f32>, Vec<f32>)> {
    let n = 1 + x.len() / HOP;
    let mut padded = vec![0f32; WIN / 2];
    padded.extend_from_slice(x);
    padded.extend(std::iter::repeat(0.0).take(WIN / 2 + HOP * 512));
    let (mut midi, mut per, mut rms) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let mut f = 0;
    while f < n {
        let k = (n - f).min(512);
        let end = f * HOP + (k - 1) * HOP + WIN;
        for fr in crepe.frames(&padded[..end], k)? {
            midi.push(fr.midi);
            per.push(fr.confidence);
            rms.push(fr.rms);
        }
        f += k;
    }
    let per = crate::dsp::nan_median_filter(&per, 3);
    // the pitch filter runs on Hz in torchcrepe; the median of Hz and of MIDI pick the same frame
    let midi = crate::dsp::nan_median_filter(&midi, 5);
    Ok((midi, per, rms))
}

/// The singer's pitch from a separated vocal, for learning: the same tracking and smoothing as `track`.
impl crate::learn::VocalPitch for Crepe {
    fn track(&mut self, vocal16: &[f32]) -> Result<(Vec<f32>, Vec<f32>), String> {
        let (m, p, _) = track(self, vocal16).map_err(|e| e.to_string())?;
        Ok((m, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, amp: f32) -> Vec<f32> {
        (0..WIN).map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / SR as f32).sin()).collect()
    }

    #[test]
    fn a_whistle_is_a_pure_tone_and_a_voice_is_not() {
        let mut t = Tones::new();
        for hz in [950.0f32, 1000.0, 1530.0, 1950.0] {
            let m = t.pure_tone(&sine(hz, 0.01));
            assert!((m - (69.0 + 12.0 * (hz / 440.0).log2())).abs() < 0.1, "{hz} Hz -> {m}");
        }
        // a sung note: 220 Hz with its harmonics
        let voice: Vec<f32> = (0..WIN).map(|i| (1..12).map(|h| {
            (2.0 * std::f32::consts::PI * 220.0 * h as f32 * i as f32 / SR as f32).sin() / h as f32
        }).sum()).collect();
        assert!(t.pure_tone(&voice).is_nan());
        // noise
        let mut s = 1u32;
        let noise: Vec<f32> = (0..WIN).map(|_| { s = s.wrapping_mul(1664525).wrapping_add(1013904223); (s >> 8) as f32 / 16777216.0 - 0.5 }).collect();
        assert!(t.pure_tone(&noise).is_nan());
    }

    #[test]
    fn a_quiet_steady_whistle_is_shown_when_crepe_is_unsure() {
        let mut live = crate::take::LivePitch::default();
        let frame = |tone: f32, rms: f32| Frame { midi: 50.0, confidence: 0.2, rms, tone };
        // 30 s of room noise sets the gate at 3x its level (0.015)
        live.push(&vec![Frame { midi: f32::NAN, confidence: 0.1, rms: 0.005, tone: f32::NAN }; 3000]);
        let shown = live.push(&[frame(84.0, 0.008), frame(84.1, 0.008), frame(84.2, 0.008), frame(86.0, 0.008), frame(84.0, 0.001)]);
        assert!(shown[0].is_nan());                     // one frame isn't steady yet
        assert_eq!(&shown[1..3], &[84.1, 84.2]);
        assert!(shown[3].is_nan());                     // a jump: not steady
        assert!(shown[4].is_nan());                     // too quiet to be anything
    }
}
