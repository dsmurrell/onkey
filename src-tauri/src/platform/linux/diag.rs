//! What a tester's report needs that nothing else logs (report.rs, digest.rs): a panic's message and place,
//! whether OnKey closed normally the last time, the browser engine's version (WebKitGTK), how this copy was started
//! (an AppImage or not), the sound server, the session bus and the media players on it, and the audio devices. A
//! panic in an AppImage started from the desktop would otherwise go nowhere.
use std::io::Write;

/// The live log (engine.rs's) and today's file in the records, written at once without the log's lock: a panic
/// inside `engine::log` itself mustn't wait on it.
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

/// A panic's message and place into the log before the thread (or OnKey) goes.
fn install_crash_logging() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let p = info.payload();
        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "(no message)".into());
        let at = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "?".into());
        write_now(&format!("panic in thread '{}' at {at}: {msg}", std::thread::current().name().unwrap_or("unnamed")));
        before(info);
    }));
}

/// The browser engine the window runs in (WebKitGTK's version), None if it can't be read.
pub fn webview() -> Option<String> {
    tauri::webview_version().ok()
}

/// Before the first log line: how the last session ended if it wasn't a quit; and a panic's cause into the log from
/// now on.
pub fn starting() -> Option<String> {
    let previous = crate::digest::unclean(&previous_log());
    install_crash_logging();
    nvidia_workaround();
    previous
}

/// WebKitGTK draws a blank window in two common cases, each with its own switch, set here unless the person has set
/// it either way: NVIDIA's own driver (its DMA-BUF renderer: WEBKIT_DISABLE_DMABUF_RENDERER), and no GPU device at
/// all, as in most virtual machines (its compositing: WEBKIT_DISABLE_COMPOSITING_MODE). (Before the window or any
/// thread exists, so the environment is safe to change.)
fn nvidia_workaround() {
    if std::path::Path::new("/proc/driver/nvidia/version").exists() && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if !std::path::Path::new("/dev/dri").exists() && std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
}

/// The window is on a Wayland display (GDK's own, not XWayland), as found once it's up (`started`).
static WAYLAND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn wayland() -> bool {
    WAYLAND.load(std::sync::atomic::Ordering::Relaxed)
}

/// GDK's display: "GdkWaylandDisplay" or "GdkX11Display" (XWayland included). On GTK's main thread only.
fn gdk_display() -> String {
    gdk::Display::default().map(|d| glib::prelude::ObjectExt::type_(&d).name().to_string()).unwrap_or_else(|| "none".into())
}

/// Launched (after the log's header): the last session's end if it wasn't a quit, this copy, the window's engine, the
/// sound server and its devices, and the media players on the session bus.
pub fn started(previous: Option<String>) {
    if let Some(p) = previous {
        crate::engine::log(&p);
    }
    let appimage = std::env::var("APPIMAGE").ok().map(|p| format!("the AppImage {}", p.rsplit('/').next().unwrap_or(&p)))
        .unwrap_or_else(|| "not an AppImage".into());
    crate::engine::log(&format!("linux: {} | {appimage} | WebKitGTK {} | session {} on {}", super::sys::machine("kern.osproductversion").unwrap_or_default(),
                                webview().unwrap_or_else(|| "version not found".into()),
                                std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".into()),
                                std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "?".into())));
    let display = gdk_display();
    WAYLAND.store(display.contains("Wayland"), std::sync::atomic::Ordering::Relaxed);
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| "unset".into());
    crate::engine::log(&format!("linux: display {display} (GDK_BACKEND {}, WAYLAND_DISPLAY {}, DISPLAY {}); WEBKIT_DISABLE_DMABUF_RENDERER {}, \
                                 WEBKIT_DISABLE_COMPOSITING_MODE {}; NVIDIA driver {}, /dev/dri {}", var("GDK_BACKEND"), var("WAYLAND_DISPLAY"),
                                var("DISPLAY"), var("WEBKIT_DISABLE_DMABUF_RENDERER"), var("WEBKIT_DISABLE_COMPOSITING_MODE"),
                                if std::path::Path::new("/proc/driver/nvidia/version").exists() { "loaded" } else { "no" },
                                if std::path::Path::new("/dev/dri").exists() { "yes" } else { "no" }));
    // (the sound server and the bus asked off the main thread: the window comes first)
    std::thread::spawn(|| {
        crate::engine::log(&format!("audio server: {}", super::pulse::describe_server()));
        crate::engine::log(&format!("audio devices: {}", super::pulse::describe_devices()));
        let players = super::mpris::players();
        crate::engine::log(&format!("MPRIS: players on the session bus now: {}",
                                    if players.is_empty() { "none (start Spotify, or play a video in Chrome or Firefox)".to_string() }
                                    else { players.iter().map(|p| format!("{p} ({})", super::mpris::install_kind(p))).collect::<Vec<_>>().join(", ") }));
    });
}

/// Files a report adds from the logs it gathered (`logs`: that text): summary.txt, the logs' answers to what we'd
/// otherwise ask a tester (digest.rs). (name, contents, what it is)
pub fn report_files(logs: &str) -> Vec<(String, String, String)> {
    let facts = serde_json::json!({"pc": crate::records::mac(), "webview2": webview().map(|v| format!("(WebKitGTK) {v}")), "build": crate::records::build()});
    vec![("summary.txt".into(), crate::digest::summary(logs, &facts), "the logs summed up: this computer, audio, music apps, learning, takes, errors".into())]
}

/// What a report's about.json says of Linux: the window's engine, the distribution, whether it's an AppImage.
pub fn report_about(about: &mut serde_json::Value) {
    about["linux"] = serde_json::json!({"webkitgtk": webview(), "os": super::sys::machine("kern.osproductversion"),
                                        "appimage": std::env::var_os("APPIMAGE").is_some(),
                                        "session": std::env::var("XDG_SESSION_TYPE").ok(), "desktop": std::env::var("XDG_CURRENT_DESKTOP").ok()});
}

/// Each song learned: how long each stage took and on what processor (progress.rs), for a tester's log.
pub const LEARN_TIMES: bool = true;
