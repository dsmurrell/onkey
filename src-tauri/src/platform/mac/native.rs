//! The Swift library's C interface (native/*.swift, built by build.rs), declared once for the engine. Windows has
//! the same functions in Rust (platform/windows/native.rs).
#![allow(clippy::missing_safety_doc)]
use std::ffi::{c_char, c_void};

// ---------------------------------------------------------------- capture (native/capture.swift)

extern "C" {
    pub fn onkey_capture_start(bundle: *const c_char,
                               cb: extern "C" fn(*const f32, i32, *mut c_void), ctx: *mut c_void,
                               out_rate: *mut f64) -> i32;
    pub fn onkey_capture_stop();
    pub fn onkey_capture_mic(uid: *mut c_char, name: *mut c_char, len: i32);
    pub fn onkey_mic_choice(uid: *mut c_char, name: *mut c_char, len: i32) -> i32;
    pub fn onkey_mic_list(buf: *mut c_char, len: i32);
    pub fn onkey_set_mic_preference(uid: *const c_char);
    pub fn onkey_output_latency_ms() -> i32;
    pub fn onkey_mic_channel(count: *mut i32) -> i32;
    pub fn onkey_output_uid(uid: *mut c_char, len: i32);
    pub fn onkey_output_name(name: *mut c_char, len: i32);
    pub fn onkey_app_sounding(bundle: *const c_char) -> i32;
    pub fn onkey_webkit_separable() -> i32;
    pub fn onkey_wake_count() -> i32;
}

// ---------------------------------------------------------------- permissions (native/permissions.swift)

extern "C" {
    pub fn onkey_mic_status() -> i32;
    pub fn onkey_mic_request();
    pub fn onkey_audio_capture_status() -> i32;
    pub fn onkey_audio_capture_request() -> i32;
    pub fn onkey_macos_supported() -> i32;
    pub fn onkey_mic_meter(on: i32) -> i32;
    pub fn onkey_mic_level() -> f32;
    pub fn onkey_mic_blocks() -> i32;
    pub fn onkey_automation(bundle: *const c_char) -> i32;
    pub fn onkey_app_running(bundle: *const c_char) -> i32;
}

// ---------------------------------------------------------------- takes' audio files (native/audiofile.swift)

extern "C" {
    pub fn onkey_aac_write(samples: *const i16, frames: i64, rate: f64, bit_rate: u32, path: *const c_char) -> i32;
    pub fn onkey_aac_convert(src: *const c_char, dst: *const c_char) -> i32;
    pub fn onkey_audio_decode(path: *const c_char, out: *mut f32, cap: i64, frames: *mut i64, rate: *mut f64, channels: *mut i32) -> i32;
}

// ---------------------------------------------------------------- the share video (native/share.swift)

extern "C" {
    pub fn onkey_share_render(json: *const c_char, audio: *const c_char, out: *const c_char,
                              icon: *const u8, icon_len: i64) -> i32;
    pub fn onkey_media_tracks(path: *const c_char) -> i32;
}

// ---------------------------------------------------------------- signatures and Shazam (native/shazam.swift)

extern "C" {
    pub fn onkey_shz_signature(x: *const f32, n: i32, rate: f64, out_len: *mut i32) -> *mut u8;
    pub fn onkey_shz_free(p: *mut u8);
    pub fn onkey_shz_catalog_new() -> i32;
    pub fn onkey_shz_catalog_free(h: i32);
    pub fn onkey_shz_catalog_add(h: i32, sig: *const u8, len: i32, id: *const c_char) -> i32;
    pub fn onkey_shz_match(h: i32, sig: *const u8, len: i32, out_id: *mut c_char, cap: i32, out_offset: *mut f64) -> i32;
    pub fn onkey_shz_shazam_begin(sig: *const u8, len: i32) -> i32;
    pub fn onkey_shz_shazam_poll(ticket: i32, out: *mut c_char, cap: i32) -> i32;
    pub fn onkey_shz_shazam_forget(ticket: i32);
}

// ---------------------------------------------------------------- your voice played back (native/voice.swift)

extern "C" {
    pub fn onkey_voice_open(path: *const c_char) -> f64;
    pub fn onkey_voice_gain(db: f64);
    pub fn onkey_voice_play(seconds: f64);
    pub fn onkey_voice_stop();
    pub fn onkey_voice_close();
    pub fn onkey_voice_position() -> f64;
}

// ---------------------------------------------------------------- the demo song's player (native/demo.swift)

extern "C" {
    pub fn onkey_demo_open(path: *const c_char) -> f64;
    pub fn onkey_demo_play();
    pub fn onkey_demo_pause();
    pub fn onkey_demo_seek(seconds: f64);
    pub fn onkey_demo_close();
    pub fn onkey_demo_state(position: *mut f64) -> i32;
}

// ---------------------------------------------------------------- the development ear (native/ear.swift)

extern "C" {
    pub fn onkey_ear_start(music: *const c_char, cb: extern "C" fn(*const f32, i32, *mut c_void), ctx: *mut c_void,
                           out_rate: *mut f64) -> i32;
    pub fn onkey_ear_stop();
    pub fn onkey_ear_outputs(buf: *mut c_char, len: i32);
}
