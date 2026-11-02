//! The Swift library's C interface (native/*.swift), for Windows, in Rust: the same functions, arguments and
//! return codes, so the engine calls them unchanged. Real ones go to audio.rs, media.rs, procs.rs, mf.rs (takes'
//! audio), demo.rs and voice.rs; the rest are stand-ins that fail the way the Swift ones do when something isn't
//! there, so the engine carries on without that part (each says what's missing).
#![allow(clippy::missing_safety_doc)]
use std::ffi::{c_char, c_void};
use std::path::Path;

use super::{audio, copy_out, demo, from_c, mf, procs, share, sys, voice};

// ---------------------------------------------------------------- capture (native/capture.swift)

pub unsafe fn onkey_capture_start(bundle: *const c_char, cb: extern "C" fn(*const f32, i32, *mut c_void), ctx: *mut c_void,
                                  out_rate: *mut f64) -> i32 {
    let mut rate = 0.0;
    let code = audio::start(&from_c(bundle), cb, ctx, &mut rate);
    if !out_rate.is_null() {
        *out_rate = rate;
    }
    code
}

pub unsafe fn onkey_capture_stop() {
    audio::stop();
}

pub unsafe fn onkey_capture_mic(uid: *mut c_char, name: *mut c_char, len: i32) {
    let (u, n) = audio::capture_mic();
    copy_out(&u, uid, len);
    copy_out(&n, name, len);
}

pub unsafe fn onkey_mic_choice(uid: *mut c_char, name: *mut c_char, len: i32) -> i32 {
    match audio::chosen_mic() {
        Some((u, n)) => {
            copy_out(&u, uid, len);
            copy_out(&n, name, len);
            1
        }
        None => 0,
    }
}

pub unsafe fn onkey_mic_list(buf: *mut c_char, len: i32) {
    let list: Vec<_> = audio::mics().into_iter().map(|(uid, name)| serde_json::json!({"uid": uid, "name": name})).collect();
    copy_out(&serde_json::Value::Array(list).to_string(), buf, len);
}

pub unsafe fn onkey_set_mic_preference(uid: *const c_char) {
    audio::set_preference(&from_c(uid));
}

/// Which input of the mic device OnKey takes the voice from (mac: the loudest of a multi-input interface).
/// Windows hands over one mixed-down mic stream: input 0 of 1.
pub unsafe fn onkey_mic_channel(count: *mut i32) -> i32 {
    if !count.is_null() {
        *count = 1;
    }
    0
}

pub unsafe fn onkey_output_latency_ms() -> i32 {
    audio::output_latency_ms()
}

pub unsafe fn onkey_output_uid(uid: *mut c_char, len: i32) {
    copy_out(&audio::output().map(|o| o.0).unwrap_or_default(), uid, len);
}

pub unsafe fn onkey_output_name(name: *mut c_char, len: i32) {
    copy_out(&audio::output().map(|o| o.1).unwrap_or_default(), name, len);
}

pub unsafe fn onkey_app_sounding(bundle: *const c_char) -> i32 {
    audio::app_sounding(&from_c(bundle)) as i32
}

/// (macOS only: WebKit's shared GPU process. Browsers on Windows play in their own processes.)
pub unsafe fn onkey_webkit_separable() -> i32 {
    0
}

/// Wakes from sleep since launch, and default output changes while the whole output is heard (events.rs): the
/// engine restarts its capture after each.
pub unsafe fn onkey_wake_count() -> i32 {
    super::events::wakes()
}

// ---------------------------------------------------------------- permissions (native/permissions.swift)

/// Windows never asks a desktop app for the microphone: it's allowed unless a switch in Settings > Privacy &
/// security > Microphone is off. Denied (2) when "Microphone access" for the whole PC or "Let desktop apps access
/// your microphone" is (their CapabilityAccessManager keys say "Deny"); otherwise allowed (3). (The per-user
/// "Microphone access" key isn't read: which desktop apps it stops differs between versions of Windows.)
pub unsafe fn onkey_mic_status() -> i32 {
    use ::windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    let off = |hive, key: &str| sys::reg_str(hive, key, "Value").is_some_and(|v| v.eq_ignore_ascii_case("Deny"));
    if off(HKEY_LOCAL_MACHINE, KEY) || off(HKEY_CURRENT_USER, &format!(r"{KEY}\NonPackaged")) { 2 } else { 3 }
}

pub unsafe fn onkey_mic_request() {}

/// Loopback capture needs no permission on Windows.
pub unsafe fn onkey_audio_capture_status() -> i32 {
    3
}

pub unsafe fn onkey_audio_capture_request() -> i32 {
    0
}

/// "This system can capture another app's audio": loopback is there on every supported Windows.
pub unsafe fn onkey_macos_supported() -> i32 {
    1
}

pub unsafe fn onkey_mic_meter(on: i32) -> i32 {
    audio::meter(on != 0)
}

pub unsafe fn onkey_mic_level() -> f32 {
    audio::meter_level()
}

pub unsafe fn onkey_mic_blocks() -> i32 {
    audio::meter_blocks()
}

/// Controlling an app through its media session needs no permission: granted while it runs.
pub unsafe fn onkey_automation(bundle: *const c_char) -> i32 {
    if procs::running(&from_c(bundle)) { 3 } else { 4 }
}

/// 1 running, 4 not (whether it's installed isn't looked up: never 5).
pub unsafe fn onkey_app_running(bundle: *const c_char) -> i32 {
    if procs::running(&from_c(bundle)) { 1 } else { 4 }
}

// ---------------------------------------------------------------- Shazam's catalog (native/shazam.swift)
// ShazamKit is Apple's. Learned songs' signatures and their catalog are OnKey's own here (landmark.rs, through
// recognize.rs); Shazam's online catalog has no counterpart: listening on Windows follows the media session's
// title and artist for songs not learned.

/// No ticket: nothing is sent to Shazam.
pub unsafe fn onkey_shz_shazam_begin(_sig: *const u8, _len: i32) -> i32 {
    0
}

pub unsafe fn onkey_shz_shazam_poll(_ticket: i32, out: *mut c_char, cap: i32) -> i32 {
    copy_out("", out, cap);
    -1
}

pub unsafe fn onkey_shz_shazam_forget(_ticket: i32) {}

// ---------------------------------------------------------------- takes' audio (native/audiofile.swift)
// Media Foundation (mf.rs): its AAC encoder through a sink writer, its source reader for decoding.

pub unsafe fn onkey_aac_write(samples: *const i16, frames: i64, rate: f64, bit_rate: u32, path: *const c_char) -> i32 {
    let pcm = if samples.is_null() || frames <= 0 { &[][..] } else { std::slice::from_raw_parts(samples, frames as usize) };
    mf::aac_write(pcm, rate, bit_rate, Path::new(&from_c(path))).err().unwrap_or(0)
}

pub unsafe fn onkey_aac_convert(src: *const c_char, dst: *const c_char) -> i32 {
    mf::aac_convert(Path::new(&from_c(src)), Path::new(&from_c(dst))).err().unwrap_or(0)
}

/// Decodes the whole file (proving it decodes): its length in frames, rate and channels, and with `out` (room for
/// `cap` frames) the audio itself, mixed to mono. 0, or the error.
pub unsafe fn onkey_audio_decode(path: *const c_char, out: *mut f32, cap: i64, frames: *mut i64, rate: *mut f64, channels: *mut i32) -> i32 {
    if !frames.is_null() {
        *frames = 0;
    }
    match mf::decode(Path::new(&from_c(path)), true) {
        Ok(d) => {
            if !out.is_null() {
                let n = d.frames.min(cap.max(0) as usize);
                std::ptr::copy_nonoverlapping(d.samples.as_ptr(), out, n);
            }
            if !frames.is_null() {
                *frames = d.frames as i64;
            }
            if !rate.is_null() {
                *rate = d.rate;
            }
            if !channels.is_null() {
                *channels = d.channels as i32;
            }
            0
        }
        Err(e) => e,
    }
}

// ---------------------------------------------------------------- the demo song's player (native/demo.swift)

pub unsafe fn onkey_demo_open(path: *const c_char) -> f64 {
    demo::open(Path::new(&from_c(path)))
}

pub unsafe fn onkey_demo_play() {
    demo::play();
}

pub unsafe fn onkey_demo_pause() {
    demo::pause();
}

pub unsafe fn onkey_demo_seek(seconds: f64) {
    demo::seek(seconds);
}

pub unsafe fn onkey_demo_close() {
    demo::close();
}

pub unsafe fn onkey_demo_state(position: *mut f64) -> i32 {
    let mut p = 0.0;
    let st = demo::state(&mut p);
    if !position.is_null() {
        *position = p;
    }
    st
}

// ---------------------------------------------------------------- a take played with the song (native/voice.swift)

pub unsafe fn onkey_voice_open(path: *const c_char) -> f64 {
    voice::open(Path::new(&from_c(path)))
}

pub unsafe fn onkey_voice_gain(db: f64) {
    voice::gain(db);
}

pub unsafe fn onkey_voice_play(seconds: f64) {
    voice::play(seconds);
}

pub unsafe fn onkey_voice_stop() {
    voice::stop();
}

pub unsafe fn onkey_voice_close() {
    voice::close();
}

pub unsafe fn onkey_voice_position() -> f64 {
    voice::position()
}

// ---------------------------------------------------------------- the ear (native/ear.swift; development builds only)
// The ear's taps (Core Audio process taps on one aggregate device) aren't on Windows: it never starts, and says
// why (once: the ear asks again every half second for 30 s).

/// What the ear is told when it asks: it isn't here.
#[cfg(feature = "devtest")]
pub const EAR_NOT_HERE: i32 = 9;

#[cfg(feature = "devtest")]
pub unsafe fn onkey_ear_start(_music: *const c_char, _cb: extern "C" fn(*const f32, i32, *mut c_void), _ctx: *mut c_void,
                              out_rate: *mut f64) -> i32 {
    static SAID: std::sync::Once = std::sync::Once::new();
    SAID.call_once(|| crate::engine::log("ear: not on Windows"));
    if !out_rate.is_null() {
        *out_rate = 0.0;
    }
    EAR_NOT_HERE
}

#[cfg(feature = "devtest")]
pub unsafe fn onkey_ear_stop() {}

#[cfg(feature = "devtest")]
pub unsafe fn onkey_ear_outputs(buf: *mut c_char, len: i32) {
    copy_out("[]", buf, len);
}

// ---------------------------------------------------------------- share videos (native/share.swift)
// Drawn by share_draw.rs and encoded with Media Foundation (share.rs here), as AVFoundation does on macOS.

pub unsafe fn onkey_share_render(json: *const c_char, audio: *const c_char, out: *const c_char, icon: *const u8, icon_len: i64) -> i32 {
    let icon = if icon.is_null() || icon_len <= 0 { &[][..] } else { std::slice::from_raw_parts(icon, icon_len as usize) };
    share::render(&from_c(json), std::path::Path::new(&from_c(audio)), std::path::Path::new(&from_c(out)), icon)
}

pub unsafe fn onkey_media_tracks(path: *const c_char) -> i32 {
    share::tracks(std::path::Path::new(&from_c(path)))
}
