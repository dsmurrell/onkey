//! Which processor runs each model (`load`: platform/mac/gpu.rs, platform/windows/gpu.rs). On a Mac, Core ML for
//! separation and pitch (pitch.rs load_coreml), the CPU for the others, as before. On Windows, DirectML:
//! the GPU Windows reports with the most memory of its own (a software one, such as the Basic Render Driver of a
//! PC or VM with no GPU, doesn't count), with ONNX Runtime's CPU kernels for any part DirectML can't take. A model
//! DirectML can't load, or can't run, goes to the CPU instead, and the log says why, once.
//!
//! What a tester's log should answer without asking them: the GPUs Windows has (once a launch, `log_gpus`),
//! where each model ended up (`note`, once per model and place) and, per song learned, how long each stage took
//! and on what (progress.rs, with `places`).
use std::sync::Mutex;

/// Where each model runs now: (model, place), e.g. ("separation", "DirectML (NVIDIA GeForce RTX 3060)").
static PLACES: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());

/// `model` runs on `on`, `why` when it isn't where it was meant to: logged the first time, and when it moves.
pub fn note(model: &'static str, on: &str, why: Option<&str>) {
    let mut p = PLACES.lock().unwrap_or_else(|e| e.into_inner());
    let moved = match p.iter_mut().find(|(m, _)| *m == model) {
        Some((_, was)) if was == on => false,
        Some((_, was)) => {
            *was = on.to_string();
            true
        }
        None => {
            p.push((model, on.to_string()));
            true
        }
    };
    drop(p);
    if moved {
        crate::engine::log(&format!("models: {model} on {on}{}", why.map(|w| format!(" ({w})")).unwrap_or_default()));
    }
}

/// "separation on DirectML (…), pitch on the CPU, …": where the models noted so far run.
pub fn places() -> String {
    PLACES.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(m, on)| format!("{m} on {on}")).collect::<Vec<_>>().join(", ")
}

/// A display adapter as Windows lists it (DXGI).
#[derive(Debug, Clone)]
pub struct Adapter {
    /// its place in DXGI's list: DirectML's device id
    pub index: u32,
    pub name: String,
    pub vendor: u32,
    /// memory of its own, and system memory it may share (bytes)
    pub dedicated: u64,
    pub shared: u64,
    /// drawn by the CPU (the Basic Render Driver, or a remote display's)
    pub software: bool,
}

/// The one Microsoft's software renderer (WARP, "Microsoft Basic Render Driver") has, whatever its flags say.
const MICROSOFT_VENDOR: u32 = 0x1414;

impl Adapter {
    fn vendor_name(&self) -> String {
        match self.vendor {
            0x10de => "NVIDIA".into(),
            0x1002 | 0x1022 => "AMD".into(),
            0x8086 => "Intel".into(),
            0x5143 => "Qualcomm".into(),
            MICROSOFT_VENDOR => "Microsoft".into(),
            v => format!("vendor {v:04x}"),
        }
    }

    /// "NVIDIA GeForce RTX 3060 (NVIDIA, 12.0 GB of its own, 15.9 GB shared)"
    pub fn describe(&self) -> String {
        let gb = |b: u64| b as f64 / (1u64 << 30) as f64;
        format!("{} ({}, {:.1} GB of its own, {:.1} GB shared{})", self.name, self.vendor_name(), gb(self.dedicated), gb(self.shared),
                if self.software { ", software" } else { "" })
    }
}

/// The adapter DirectML should run on: the hardware one with the most memory of its own (a laptop's own GPU over
/// the one built into its processor). Err: why there's none.
pub fn pick(adapters: &[Adapter]) -> Result<&Adapter, String> {
    let hardware = adapters.iter().filter(|a| !a.software && a.vendor != MICROSOFT_VENDOR);
    hardware.max_by_key(|a| (a.dedicated, std::cmp::Reverse(a.index))).ok_or_else(|| match adapters.len() {
        0 => "Windows lists no display adapter".to_string(),
        _ => format!("no GPU DirectML can use: only {}", adapters.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")),
    })
}

/// A model loaded where this OS runs it (platform/*/gpu.rs), and how to move it to the CPU should a run fail there.
pub use crate::platform::gpu::{load, Fallback};

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(index: u32, name: &str, vendor: u32, dedicated_gb: u64, software: bool) -> Adapter {
        Adapter { index, name: name.into(), vendor, dedicated: dedicated_gb << 30, shared: 8 << 30, software }
    }

    #[test]
    fn directml_takes_the_gpu_with_the_most_memory_of_its_own_and_never_a_software_one() {
        let laptop = [adapter(0, "Intel(R) UHD Graphics", 0x8086, 0, false), adapter(1, "NVIDIA GeForce RTX 4060 Laptop GPU", 0x10de, 8, false),
                      adapter(2, "Microsoft Basic Render Driver", 0x1414, 0, true)];
        assert_eq!(pick(&laptop).unwrap().index, 1);
        // a PC (or CI's VM) with no GPU: the CPU, saying why
        let none = [adapter(0, "Microsoft Basic Render Driver", 0x1414, 0, true)];
        assert_eq!(pick(&none).unwrap_err(), "no GPU DirectML can use: only Microsoft Basic Render Driver");
        assert!(pick(&[]).is_err());
        // two alike: the first Windows lists
        let two = [adapter(0, "A", 0x1002, 4, false), adapter(1, "B", 0x1002, 4, false)];
        assert_eq!(pick(&two).unwrap().index, 0);
        assert_eq!(laptop[1].describe(), "NVIDIA GeForce RTX 4060 Laptop GPU (NVIDIA, 8.0 GB of its own, 8.0 GB shared)");
    }
}
