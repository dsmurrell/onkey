//! Your recording, played with the song (native/voice.swift): "Play with the song" puts it on the song the way a
//! take's bars are put on it, and needs to know exactly where the voice is, which the window's Web Audio can't say.

use crate::platform::native::*;

/// Open a recording: its length (s).
pub fn open(path: &std::path::Path) -> Option<f64> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let d = unsafe { onkey_voice_open(c.as_ptr()) };
    (d > 0.0).then_some(d)
}
pub fn gain(db: f64) { unsafe { onkey_voice_gain(db) } }
pub fn play(seconds: f64) { unsafe { onkey_voice_play(seconds) } }
pub fn stop() { unsafe { onkey_voice_stop() } }
pub fn close() { unsafe { onkey_voice_close() } }
/// Where in the recording the voice is now (s), if it's playing.
pub fn position() -> Option<f64> {
    let p = unsafe { onkey_voice_position() };
    (p >= 0.0).then_some(p)
}
