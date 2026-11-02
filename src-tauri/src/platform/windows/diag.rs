//! What a tester's report needs that nothing else logs (report.rs, digest.rs): a panic's message and place, a
//! crash's exception and the DLL it was in, whether OnKey closed normally the last time, WebView2's version, the
//! microphone switches in Windows' privacy settings and the audio devices there are. A window app on Windows has no
//! console, so a panic's message would otherwise go nowhere.
use std::io::Write;

/// The live log (engine.rs's) and today's file in the records, written at once without the log's lock: a panic
/// inside `engine::log` itself, or a crash, mustn't wait on it.
fn write_now(msg: &str) {
    let line = format!("{} {msg}", crate::store::now_iso());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("onkey.log")) {
        let _ = writeln!(f, "{line}");
    }
    crate::records::log_line(&line);
}

/// The end of the log before this launch (the rotated file's end, then the live one's), at most 2 MB of each.
fn previous_log() -> String {
    let tail = |p: std::path::PathBuf| {
        let b = std::fs::read(p).unwrap_or_default();
        String::from_utf8_lossy(&b[b.len().saturating_sub(2 << 20)..]).into_owned()
    };
    let dir = std::env::temp_dir();
    tail(dir.join("onkey.log.1")) + &tail(dir.join("onkey.log"))
}

/// Read before this launch's first log line: whether the last session ended without a quit (a crash, a hang ended
/// in Task Manager, the power going), to log once the log has its header (`log_launch`).
pub fn previous_session() -> Option<String> {
    crate::digest::unclean(&previous_log())
}

/// A panic's message and place, and a crash's exception, into the log before OnKey goes (or the thread does).
pub fn install_crash_logging() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let p = info.payload();
        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "(no message)".into());
        let at = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "?".into());
        write_now(&format!("panic in thread '{}' at {at}: {msg}", std::thread::current().name().unwrap_or("unnamed")));
        before(info);
    }));
    unsafe {
        ::windows::Win32::System::Diagnostics::Debug::SetUnhandledExceptionFilter(Some(crashed));
    }
}

/// An exception nothing handled (an access violation in a DLL, say): logged, then Windows ends OnKey as it would.
unsafe extern "system" fn crashed(p: *const ::windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS) -> i32 {
    if let Some(r) = p.as_ref().and_then(|p| p.ExceptionRecord.as_ref()) {
        let code = r.ExceptionCode.0 as u32;
        let what = match code {
            0xC000_0005 => " (access violation)",
            0xC000_001D => " (illegal instruction)",
            0xC000_0094 => " (integer divide by zero)",
            0xC000_00FD => " (stack overflow)",
            0xC000_0409 => " (stack buffer overrun)",
            0xC000_0374 => " (heap corruption)",
            0xE06D_7363 => " (a C++ exception)",
            _ => "",
        };
        write_now(&format!("crash: exception {code:#010x}{what} at {}", module_of(r.ExceptionAddress as usize)));
    }
    0       // EXCEPTION_CONTINUE_SEARCH: Windows ends the process as it would have (and Windows Error Reporting sees it)
}

/// "onnxruntime.dll+0x1a2b3c": the DLL (or exe) an address is in, and how far into it.
fn module_of(addr: usize) -> String {
    use ::windows::Win32::Foundation::HMODULE;
    use ::windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                                                    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};
    let mut h = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    if unsafe { GetModuleHandleExW(flags, ::windows::core::PCWSTR(addr as *const u16), &mut h) }.is_err() {
        return format!("{addr:#x}");
    }
    let mut buf = [0u16; 512];
    let n = unsafe { GetModuleFileNameW(Some(h), &mut buf) } as usize;
    let path = String::from_utf16_lossy(&buf[..n]);
    let name = path.rsplit(['\\', '/']).next().unwrap_or("?");
    format!("{name}+{:#x}", addr.wrapping_sub(h.0 as usize))
}

/// WebView2's version (the Evergreen runtime the window runs in), None if it can't be read.
pub fn webview2() -> Option<String> {
    tauri::webview_version().ok()
}

/// The microphone switches in Settings > Privacy & security > Microphone: (PC-wide, desktop apps), each "Allow",
/// "Deny" or "not set".
fn mic_switches() -> (String, String) {
    use ::windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    let get = |hive, key: &str| super::sys::reg_str(hive, key, "Value").unwrap_or_else(|| "not set".into());
    (get(HKEY_LOCAL_MACHINE, KEY), get(HKEY_CURRENT_USER, &format!(r"{KEY}\NonPackaged")))
}

/// Once a launch, after its header: the last session's end if it wasn't a quit, WebView2, the mic switches and the
/// audio devices.
pub fn log_launch(previous: Option<String>) {
    if let Some(p) = previous {
        crate::engine::log(&p);
    }
    crate::engine::log(&format!("webview2: {}", webview2().unwrap_or_else(|| "version not found".into())));
    let (pc, apps) = mic_switches();
    let allowed = unsafe { super::native::onkey_mic_status() } == 3;
    crate::engine::log(&format!("mic access (Windows privacy): {} (microphone access for this PC: {pc}; for desktop apps: {apps})",
                                if allowed { "allowed" } else { "turned off in Settings > Privacy & security > Microphone" }));
    crate::engine::log(&format!("audio devices: {}", super::audio::describe_devices()));
}

/// Before the first log line: how the last session ended if it wasn't a quit; and a panic's or crash's cause into
/// the log from now on (a window app here has no console).
pub fn starting() -> Option<String> {
    let previous = previous_session();
    install_crash_logging();
    previous
}

/// Launched (after the log's header): the GPUs Windows has and the one learning runs on (gpu.rs), then `log_launch`.
pub fn started(previous: Option<String>) {
    super::events::watch();         // sleep, sign-out and output changes
    super::gpu::log_gpus();
    log_launch(previous);
}

/// Files a report adds from the logs it gathered (`logs`: that text): summary.txt, the logs' answers to what we'd
/// otherwise ask a tester (digest.rs). (name, contents, what it is)
pub fn report_files(logs: &str) -> Vec<(String, String, String)> {
    let facts = serde_json::json!({"pc": crate::records::mac(), "webview2": webview2(), "build": crate::records::build()});
    vec![("summary.txt".into(), crate::digest::summary(logs, &facts), "the logs summed up: this PC, audio, music apps, learning, takes, errors".into())]
}

/// What a report's about.json says of Windows ("mac" holds this PC, its "macos" the Windows version): WebView2.
pub fn report_about(about: &mut serde_json::Value) {
    about["windows"] = serde_json::json!({"webview2": webview2()});
}

/// Each song learned: how long each stage took and on what processor (progress.rs), for a tester's log.
pub const LEARN_TIMES: bool = true;
