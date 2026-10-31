//! Which processor runs each model on Windows: DirectML on the GPU with the most memory of its own (gpu.rs `pick`),
//! the CPU when there's none or DirectML fails (gpu.rs says why, once).
use crate::gpu::{note, pick, Adapter};
use ort::execution_providers::DirectMLExecutionProvider;
use ort::session::{builder::SessionBuilder, Session};
use std::sync::OnceLock;

/// The display adapters, as DXGI lists them (empty if it can't).
pub fn adapters() -> Vec<Adapter> {
    use ::windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else { return Vec::new() };
    let mut out = Vec::new();
    for index in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else { break };
        let Ok(d) = (unsafe { adapter.GetDesc1() }) else { continue };
        let len = d.Description.iter().position(|c| *c == 0).unwrap_or(d.Description.len());
        out.push(Adapter { index, name: String::from_utf16_lossy(&d.Description[..len]).trim().to_string(), vendor: d.VendorId,
                           dedicated: d.DedicatedVideoMemory as u64, shared: d.SharedSystemMemory as u64,
                           software: d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 });
    }
    out
}

/// The GPU DirectML runs on, found once; Err: why the models run on the CPU instead.
pub fn directml() -> Result<&'static Adapter, &'static str> {
    static PICKED: OnceLock<Result<Adapter, String>> = OnceLock::new();
    PICKED.get_or_init(|| {
        if crate::devtest::knob!("ONKEY_CPU").is_some() {
            return Err("ONKEY_CPU is set".into());
        }
        pick(&adapters()).cloned()
    }).as_ref().map_err(|e| e.as_str())
}

/// Once a launch: the GPUs Windows has, the one DirectML runs on, and which DirectML.dll is loaded (the one
/// beside onkey.exe, or Windows' own older copy).
pub fn log_gpus() {
    let all = adapters();
    for a in &all {
        crate::engine::log(&format!("gpu {}: {}", a.index, a.describe()));
    }
    match directml() {
        Ok(a) => crate::engine::log(&format!("gpu: DirectML runs on gpu {} ({})", a.index, a.name)),
        Err(why) => crate::engine::log(&format!("gpu: the models run on the CPU: {why}")),
    }
    crate::engine::log(&format!("gpu: DirectML.dll {}", directml_dll().unwrap_or_else(|| "not loaded yet".into())));
}

/// Where the DirectML.dll this process has loaded is.
fn directml_dll() -> Option<String> {
    use ::windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
    let h = unsafe { GetModuleHandleW(::windows::core::w!("DirectML.dll")) }.ok()?;
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(Some(h), &mut buf) } as usize;
    (n > 0).then(|| String::from_utf16_lossy(&buf[..n]))
}

/// A session for `model` on DirectML when there's a GPU for it, else on the CPU; with DirectML, how to make it
/// again on the CPU should a run fail there. `options` sets the session's own (threads, optimisation, the CPU's
/// allocator), `commit` loads the model. On DirectML the session has no memory patterns and runs its nodes in
/// order, as DirectML requires, and DirectML comes first, the CPU taking what it can't (ONNX Runtime adds the
/// CPU's itself, unless `options` did). If DirectML can't load the model, the CPU does.
pub fn session(model: &'static str, options: impl Fn(SessionBuilder) -> ort::Result<SessionBuilder> + Send + 'static,
               commit: impl Fn(SessionBuilder) -> ort::Result<Session> + Send + 'static) -> ort::Result<(Session, Option<Fallback>)> {
    match directml() {
        Ok(gpu) => {
            let dml = Session::builder()
                .and_then(|b| b.with_execution_providers([
                    DirectMLExecutionProvider::default().with_device_id(gpu.index as i32).build().error_on_failure()]))
                .and_then(|b| b.with_memory_pattern(false))
                .and_then(|b| b.with_parallel_execution(false))
                .and_then(&options)
                .and_then(&commit);
            match dml {
                Ok(s) => {
                    note(model, &format!("DirectML ({})", gpu.name), None);
                    let cpu = Box::new(move || Session::builder().and_then(&options).and_then(&commit));
                    return Ok((s, Some(Fallback { model, cpu })));
                }
                Err(e) => note(model, "the CPU", Some(&format!("DirectML couldn't load it: {e}"))),
            }
        }
        Err(why) => note(model, "the CPU", Some(why)),
    }
    Ok((Session::builder().and_then(options).and_then(commit)?, None))
}

/// A model on DirectML: how to make its session on the CPU instead.
pub struct Fallback {
    model: &'static str,
    cpu: Box<dyn Fn() -> ort::Result<Session> + Send>,
}

impl Fallback {
    /// A run on DirectML failed (`e`): the model's session on the CPU, used from now on.
    pub fn to_cpu(self, e: &dyn std::fmt::Display) -> ort::Result<Session> {
        note(self.model, "the CPU", Some(&format!("DirectML failed running it: {e}")));
        (self.cpu)()
    }
}

/// `session` for a model in a file (`model`: its name in the log).
pub fn load(model: &'static str, path: &std::path::Path, options: impl Fn(SessionBuilder) -> ort::Result<SessionBuilder> + Send + 'static)
    -> ort::Result<(Session, Option<Fallback>)> {
    let file = path.to_path_buf();
    session(model, options, move |b| b.commit_from_file(&file))
}

/// The live pitch (CREPE-tiny) on DirectML when there's a GPU for it (else the CPU); its first run prepares it, or
/// finds DirectML can't run it (then the CPU: pitch.rs `frames`).
pub fn live_pitch() -> ort::Result<Option<crate::pitch::Crepe>> {
    use ort::session::builder::GraphOptimizationLevel;
    use crate::pitch::{Crepe, HOP, WIN};
    let (session, gpu) = session("live pitch",
        |b| b.with_optimization_level(GraphOptimizationLevel::Level3)?.with_intra_threads(2), |b| b.commit_from_memory(Crepe::tiny_model()))?;
    let mut c = Crepe::tiny_on(session, gpu);
    c.frames(&[0.0; WIN + 2 * HOP], 3)?;
    Ok(Some(c))
}
