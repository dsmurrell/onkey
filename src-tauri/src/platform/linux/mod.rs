//! Linux: the sound server through the PulseAudio protocol (pulse.rs: PipeWire's pipewire-pulse speaks it too) for
//! capture, the microphones and what OnKey plays itself (render.rs: demo.rs, voice.rs), audio files decoded in Rust
//! (audiofile.rs), the music apps and browsers through MPRIS on D-Bus (mpris.rs for Spotify and Apple Music,
//! browsers.rs for YouTube), the process list (procs.rs), the shell and the clipboard (shell.rs), facts about the
//! computer (sys.rs), the window (app.rs), what a tester's report needs (diag.rs), the CPU for the models (gpu.rs),
//! landmark signatures (signature.rs) and the Swift library's C interface in Rust (native.rs).
pub mod app;
pub mod audiofile;
pub mod browsers;
pub mod demo;
pub mod diag;
pub mod gpu;
pub mod mpris;
pub mod native;
pub mod procs;
pub mod pulse;
pub mod render;
pub mod shell;
pub mod signature;
pub mod sys;
pub mod voice;

// one MPRIS reader (mpris.rs) for Spotify and Apple Music
pub use mpris::{music, spotify};
// YouTube in a browser: the browsers' MPRIS players
pub use browsers as youtube;

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
