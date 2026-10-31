//! Which processor runs each model on macOS: Core ML for separation and pitch (pitch.rs load_coreml: the GPU and
//! Neural Engine, the CPU taking what Core ML can't), the CPU for the others (words, notes), as before. Nothing to
//! move to the CPU after a failed run: `Fallback` has no values here.
use ort::session::{builder::SessionBuilder, Session};

use crate::pitch::{Crepe, HOP, WIN};

/// (None on macOS: see the module's note.)
pub enum Fallback {}

impl Fallback {
    pub fn to_cpu(self, _e: &dyn std::fmt::Display) -> ort::Result<Session> {
        match self {}
    }
}

/// A model in a file (`model`: its name, as gpu.rs logs it), where it runs on a Mac.
pub fn load(model: &'static str, path: &std::path::Path, options: impl Fn(SessionBuilder) -> ort::Result<SessionBuilder> + Send + 'static)
    -> ort::Result<(Session, Option<Fallback>)> {
    let session = match model {
        "separation" | "pitch" => crate::pitch::load_coreml(path, options)?,
        _ => Session::builder().and_then(options).and_then(|b| b.commit_from_file(path))?,
    };
    Ok((session, None))
}

/// The live pitch (CREPE-tiny) on Core ML, on the CPU through Core ML's own kernels (about a tenth of ONNX Runtime's
/// CPU: pitch.rs `Crepe::new`). None: Core ML couldn't take it (logged), so ONNX Runtime's CPU kernels instead.
pub fn live_pitch() -> ort::Result<Option<Crepe>> {
    use ort::execution_providers::{coreml::{CoreMLComputeUnits, CoreMLModelFormat}, CoreMLExecutionProvider, CPUExecutionProvider};
    // (Core ML's compiler prints, on stdout, that the model's sizes don't suit the Neural Engine; it isn't
    // asked to use it. Kept off stdout: tools print their results there)
    let coreml = quiet_stdout(|| -> ort::Result<Crepe> {
        let mut c = Crepe::tiny_with(|b| b.with_intra_threads(1)?.with_execution_providers([
            CoreMLExecutionProvider::default().with_model_format(CoreMLModelFormat::MLProgram)
                .with_compute_units(CoreMLComputeUnits::CPUOnly).build().error_on_failure(),
            CPUExecutionProvider::default().build()]))?;
        c.frames(&[0.0; WIN + 2 * HOP], 3)?;       // (its first run prepares it)
        Ok(c)
    });
    match coreml {
        Ok(c) => Ok(Some(c)),
        Err(e) => {
            crate::engine::log(&format!("live pitch: Core ML couldn't take CREPE-tiny ({e}); ONNX Runtime's CPU kernels instead"));
            Ok(None)
        }
    }
}

/// Run `f` with this process's stdout going nowhere (what native code prints to it meanwhile is dropped).
fn quiet_stdout<R>(f: impl FnOnce() -> R) -> R {
    extern "C" {
        fn fflush(stream: *mut libc::FILE) -> libc::c_int;
    }
    unsafe {
        fflush(std::ptr::null_mut());
        let saved = libc::dup(1);
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
        if saved < 0 || null < 0 {
            if saved >= 0 { libc::close(saved); }
            if null >= 0 { libc::close(null); }
            return f();
        }
        libc::dup2(null, 1);
        libc::close(null);
        let r = f();
        fflush(std::ptr::null_mut());
        libc::dup2(saved, 1);
        libc::close(saved);
        r
    }
}
