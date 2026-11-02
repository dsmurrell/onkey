//! The platform boundary: what OnKey needs from the OS, one folder per OS (mac/, windows/, linux/), each providing the same
//! modules with the same functions. The one for this OS is re-exported here, so shared code calls
//! `platform::shell::open(…)` with no `cfg`, and an OS missing a module or a function doesn't compile.
//!
//! The contract (each module's functions are documented in both folders; README.md says how to add an OS):
//! - `spotify`, `music`, `youtube`: the music apps and YouTube in a browser: `state`, `control`, `seek`,
//!   `play_track`, `exact_position`, plus each one's extras (Spotify's volume, YouTube's browser and position). What OnKey does with
//!   them whatever the OS stays in spotify.rs, music.rs and youtube.rs (ids, covers, names, song rules).
//! - `shell`: open a link or file, an app's own link, a song's link, show a file in Finder / File Explorer, the
//!   clipboard, the home folder, the privacy settings.
//! - `sys`: this computer and process: memory, CPU time, model and OS version, region, preferred languages, power, thread priority, the
//!   user's names, whether an app is running, UI fonts.
//! - `app`: the menu bar, the window's buttons, the Dock, events only one OS sends, a report's save location.
//! - `diag`: what a tester's report and log need beyond the shared ones.
//! - `native`: the Swift library's C interface (macOS) and the same functions in Rust (Windows): audio capture,
//!   permissions, takes' audio files, the share video, signatures and Shazam, the voice and demo players.
//! - `signature`: songs' audio signatures and their catalog (ShazamKit's, or landmark.rs's).
//! - `gpu`: where each model runs (Core ML, DirectML or the CPU), and how to fall back to the CPU.
// MPEG-4 edit lists (AAC's priming), for Windows' encoder and decoder (and Linux's decoder); tested on macOS too
#[cfg(any(windows, target_os = "linux", test))]
pub mod mp4;
// which Windows program a bundle id stands for; tested on macOS too
#[cfg(any(windows, test))]
pub mod exes;
// Windows' music and mic into the engine's frames; tested on macOS too
#[cfg(any(windows, test))]
pub mod interleave;
// where a song is from a Windows media session's timeline; tested on macOS too
#[cfg(any(windows, test))]
pub mod timeline;
#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(target_os = "macos")]
use self::mac as os;
#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
use self::windows as os;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
use self::linux as os;

pub use os::{app, diag, gpu, music, native, shell, signature, spotify, sys, youtube};
