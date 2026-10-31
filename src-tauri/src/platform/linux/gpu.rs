//! Which processor runs each model on Linux: the CPU, for now. (ONNX Runtime could run them on a GPU here through
//! CUDA or TensorRT (NVIDIA), ROCm or MIGraphX (AMD), OpenVINO (Intel) or WebGPU; none is built in yet: each needs its
//! own ONNX Runtime build and libraries shipped with the app. README.md says more.) Nothing to move to the CPU after
//! a failed run: `Fallback` has no values here.
use ort::session::{builder::SessionBuilder, Session};

/// (None on Linux: see the module's note.)
pub enum Fallback {}

impl Fallback {
    pub fn to_cpu(self, _e: &dyn std::fmt::Display) -> ort::Result<Session> {
        match self {}
    }
}

/// A model in a file (`model`: its name, as gpu.rs logs it), on the CPU.
pub fn load(model: &'static str, path: &std::path::Path, options: impl Fn(SessionBuilder) -> ort::Result<SessionBuilder> + Send + 'static)
    -> ort::Result<(Session, Option<Fallback>)> {
    crate::gpu::note(model, "the CPU", Some("no GPU support on Linux yet"));
    Ok((Session::builder().and_then(options).and_then(|b| b.commit_from_file(path))?, None))
}

/// The live pitch: ONNX Runtime's CPU kernels (None: pitch.rs makes it itself).
pub fn live_pitch() -> ort::Result<Option<crate::pitch::Crepe>> {
    Ok(None)
}
