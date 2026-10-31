//! Windows: WASAPI for audio (audio.rs; render.rs for what OnKey plays itself: demo.rs, voice.rs), Media Foundation
//! for takes' audio files (mf.rs) and the share video (share.rs), one system media session reader for Spotify, Apple
//! Music (media.rs) and YouTube in a browser (browsers.rs), the process list (procs.rs), the shell and the clipboard
//! (shell.rs), facts about the PC (sys.rs), the window (app.rs), sleep, sign-out and output changes (events.rs), what
//! a tester's report needs logged (diag.rs), DirectML (gpu.rs), landmark signatures (signature.rs) and the Swift library's C interface in Rust (native.rs).
pub mod app;
pub mod audio;
pub mod browsers;
pub mod demo;
pub mod diag;
pub mod events;
pub mod gpu;
pub mod media;
pub mod mf;
pub mod native;
pub mod procs;
pub mod render;
pub mod share;
pub mod shell;
pub mod signature;
pub mod sys;
pub mod voice;

// one media session reader (media.rs) for Spotify and Apple Music
pub use media::{music, spotify};
// YouTube in a browser: the browsers' media sessions
pub use browsers as youtube;

/// COM on this thread (multithreaded apartment). Harmless when it's already set up, in either apartment: the
/// error saying so is ignored, and calls made then work in the apartment the thread already has.
pub fn com() {
    use ::windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

/// Copy `s` into a C buffer of `len` bytes, NUL-terminated and cut to fit (what the Swift side's copyOut does).
pub(crate) unsafe fn copy_out(s: &str, buf: *mut std::ffi::c_char, len: i32) {
    if buf.is_null() || len <= 0 {
        return;
    }
    let mut n = s.len().min(len as usize - 1);
    while n > 0 && !s.is_char_boundary(n) {
        n -= 1;
    }
    std::ptr::copy_nonoverlapping(s.as_ptr() as *const std::ffi::c_char, buf, n);
    *buf.add(n) = 0;
}

/// A C string from the engine, as Rust text ("" for a null pointer).
pub(crate) unsafe fn from_c(p: *const std::ffi::c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
}
