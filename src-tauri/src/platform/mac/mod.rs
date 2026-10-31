//! macOS: the Swift library's C interface (native.rs), the music apps and browsers through AppleScript
//! (applescript.rs; spotify.rs, music.rs, youtube.rs), Finder, the pasteboard and System Settings (shell.rs), facts
//! about the Mac (sys.rs), the menu bar, the window and the Dock (app.rs), a tester's report (diag.rs), Core ML
//! (gpu.rs) and ShazamKit's signatures (signature.rs).
pub mod app;
pub mod applescript;
pub mod diag;
pub mod gpu;
pub mod music;
pub mod native;
pub mod shell;
pub mod signature;
pub mod spotify;
pub mod sys;
pub mod youtube;
