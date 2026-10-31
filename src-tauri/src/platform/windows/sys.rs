//! What the engine asks of Windows about the PC and this process: memory, CPU time, the model and version (the
//! registry), the user's names. (What macOS answers through sysctl, rusage and NSLocale: platform/mac/sys.rs.)
use ::windows::Win32::Foundation::FILETIME;
use ::windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use ::windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use ::windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// The machine's physical memory in bytes.
pub fn physical_memory() -> Option<u64> {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut m) }.ok().map(|_| m.ullTotalPhys)
}

/// CPU time this process has used (user + kernel), seconds.
pub fn cpu_seconds() -> f64 {
    let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) }.is_err() {
        return 0.0;
    }
    let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 / 1e7;
    t(k) + t(u)
}

/// This process's private memory (what Task Manager shows as its memory), MB.
pub fn footprint_mb() -> f64 {
    let mut m = PROCESS_MEMORY_COUNTERS_EX::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut m as *mut _ as *mut PROCESS_MEMORY_COUNTERS, size) }.is_ok();
    if ok { m.PrivateUsage as f64 / 1e6 } else { 0.0 }
}

/// A text value in the registry (`hive`: HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER), trimmed; None when it's missing
/// or empty.
pub fn reg_str(hive: ::windows::Win32::System::Registry::HKEY, key: &str, value: &str) -> Option<String> {
    use ::windows::core::HSTRING;
    use ::windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let (k, v) = (HSTRING::from(key), HSTRING::from(value));
    let mut buf = [0u16; 256];
    let mut len = std::mem::size_of_val(&buf) as u32;
    let r = unsafe { RegGetValueW(hive, &k, &v, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) };
    if r.is_err() {
        return None;
    }
    let n = (len as usize / 2).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').trim().to_string()).filter(|s| !s.is_empty())
}

/// What macOS's sysctl names say, from the registry: the BIOS's maker and model ("hw.model"), the CPU's name
/// ("machdep.cpu.brand_string") and the Windows version with its build ("kern.osproductversion": Windows 11 still
/// calls itself "Windows 10" in ProductName, so 11 is told by build).
pub fn machine(name: &str) -> Option<String> {
    use ::windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    const BIOS: &str = r"HARDWARE\DESCRIPTION\System\BIOS";
    const NT: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let reg = |key: &str, value: &str| reg_str(HKEY_LOCAL_MACHINE, key, value);
    match name {
        "hw.model" => {
            let model = reg(BIOS, "SystemProductName")?;
            Some(match reg(BIOS, "SystemManufacturer") {
                Some(maker) if !model.starts_with(&maker) => format!("{maker} {model}"),
                _ => model,
            })
        }
        "machdep.cpu.brand_string" => reg(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString"),
        "kern.osproductversion" => {
            let build = reg(NT, "CurrentBuild")?;
            let major = if build.parse::<u32>().unwrap_or(0) >= 22_000 { "11" } else { "10" };
            let release = reg(NT, "DisplayVersion").map(|v| format!(" {v}")).unwrap_or_default();
            Some(format!("Windows {major}{release} (build {build})"))
        }
        _ => None,
    }
}

/// How the OS is named before its version in the log's first line: nothing ("Windows 11 24H2 (build …)" says it).
pub const OS_NAME: &str = "";

/// (Not read on Windows yet: GetUserDefaultGeoName would give it. Without it Apple Music links have no store code.)
pub fn region() -> Option<String> {
    None
}

/// (Not read on Windows: the window always draws at full rate.)
pub fn low_power() -> bool {
    false
}

/// (Not read on Windows.)
pub fn on_battery() -> bool {
    false
}

/// (Threads' priority isn't lowered on Windows.)
pub fn background_thread() {}

/// (Threads' priority isn't lowered on Windows.)
pub fn utility_thread() {}

/// The user's names a report mustn't carry, besides the profile folder's and $USER: the account name (USERNAME)
/// and the display name (GetUserNameExW's NameDisplay: a Microsoft account's full name), if it has one.
pub fn user_names() -> Vec<String> {
    use ::windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    let mut names: Vec<String> = std::env::var("USERNAME").into_iter().collect();
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    let ok = unsafe { GetUserNameExW(NameDisplay, Some(::windows::core::PWSTR(buf.as_mut_ptr())), &mut len) };
    if !ok {
        return names;
    }
    names.extend(Some(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]).trim().to_string()).filter(|n| !n.is_empty()));
    names
}

/// Is the program a macOS bundle id stands for running (procs.rs)?
pub fn app_running(bundle: &str) -> bool {
    super::procs::running(bundle)
}

/// The UI font's faces (path, face index, weight) and its fallbacks (path, face index).
pub type UiFonts = (Vec<(std::path::PathBuf, u32, u16)>, Vec<(std::path::PathBuf, u32)>);

/// The OS's own UI font (path, face index, weight) and its fallbacks (path, face index), for drawing text: Segoe UI
/// Variable (Windows 11), else Segoe UI's own weights (Windows 10: no medium, semibold for it).
pub fn ui_fonts() -> UiFonts {
    let dir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
    let f = |n: &str| dir.join(n);
    (vec![(f("SegUIVar.ttf"), 0, 400), (f("segoeui.ttf"), 0, 400), (f("seguisb.ttf"), 0, 600), (f("segoeuib.ttf"), 0, 700)],
     vec![(f("segoeui.ttf"), 0), (f("seguisym.ttf"), 0), (f("seguiemj.ttf"), 0), (f("YuGothM.ttc"), 0), (f("msyh.ttc"), 0),
          (f("msjh.ttc"), 0), (f("malgun.ttf"), 0), (f("Nirmala.ttc"), 0), (f("Nirmala.ttf"), 0), (f("ebrima.ttf"), 0),
          (f("gadugi.ttf"), 0), (f("tahoma.ttf"), 0)])
}
