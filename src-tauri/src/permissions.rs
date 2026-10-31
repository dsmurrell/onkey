//! First-run setup: what OnKey needs from macOS (the microphone, and system audio recording to hear the music
//! app), a mic level meter for "say something", and whether setup has been done (setup_done, in OnKey's settings
//! through settings.rs).
use serde_json::{json, Value};

use crate::platform::native::*;

fn name(code: i32) -> &'static str {
    match code {
        0 => "ask",          // not asked yet
        1 => "restricted",   // blocked by a profile
        2 => "denied",
        3 => "granted",
        _ => "unknown",
    }
}

/// The microphone's and system audio recording's permission, whether this macOS is new enough, and whether
/// setup has been done.
pub fn status() -> Value {
    let (mic, audio, supported) = crate::records::timed("perm", "mic, audio and macOS status",
        || unsafe { (onkey_mic_status(), onkey_audio_capture_status(), onkey_macos_supported()) });
    json!({"mic": name(mic), "audio": name(audio), "macos_ok": supported == 1,
           "done": crate::settings::get("setup_done"),
           "control": {"spotify": automation("com.spotify.client"), "music": automation("com.apple.Music")},
           "listen_only": crate::settings::listen_only()})
}

/// Whether OnKey may control an app (macOS's Automation permission), by bundle id, never asking: "granted",
/// "denied", "ask" (not asked yet), "not_running" (macOS asks once OnKey first talks to it), "not_installed" or
/// "unknown". Asking is `ask_automation`.
///
/// It never waits long: macOS can take minutes to answer even a question that asks nothing (seen on a fresh
/// install, macOS 15.5: the window hung at "Starting OnKey" for 9 minutes). The answer is waited for up to 1.5 s
/// on a thread of its own; past that, the last answer known (or "unknown") is given and the check finishes in the
/// background, one at a time per app.
pub fn automation(bundle: &str) -> &'static str {
    static KNOWN: std::sync::Mutex<Vec<(String, &'static str, bool)>> = std::sync::Mutex::new(Vec::new());
    let last = |k: &[(String, &'static str, bool)]| k.iter().find(|e| e.0 == bundle).map(|e| (e.1, e.2));
    {
        let mut k = KNOWN.lock().unwrap();
        match last(&k) {
            Some((was, true)) => return was,         // a check still under way: its last answer
            Some(_) => k.iter_mut().filter(|e| e.0 == bundle).for_each(|e| e.2 = true),
            None => k.push((bundle.to_string(), "unknown", true)),
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let b = bundle.to_string();
    std::thread::spawn(move || {
        let now = automation_now(&b);
        if let Some(e) = KNOWN.lock().unwrap().iter_mut().find(|e| e.0 == b) {
            e.1 = now;
            e.2 = false;
        }
        let _ = tx.send(now);
    });
    rx.recv_timeout(std::time::Duration::from_millis(1500))
        .unwrap_or_else(|_| last(&KNOWN.lock().unwrap()).map_or("unknown", |e| e.0))
}

fn automation_now(bundle: &str) -> &'static str {
    let Ok(b) = std::ffi::CString::new(bundle) else { return "unknown" };
    match crate::records::timed("perm", &format!("automation status {bundle}"), || unsafe { onkey_automation(b.as_ptr()) }) {
        0 => "ask",
        2 => "denied",
        3 => "granted",
        4 => "not_running",
        5 => "not_installed",
        _ => "unknown",
    }
}

/// How long setup waits for your answer to macOS's question about one app.
const ASK_WAIT: std::time::Duration = std::time::Duration::from_secs(40);

/// Have macOS ask whether OnKey may control an app (Automation), the way that's proven to bring its question up:
/// one real, harmless Apple event (what's the player state?) sent to the app through osascript, as OnKey's own
/// controls are sent. AEDeterminePermissionToAutomateTarget with askUserIfNeeded was used before: on a fresh M2
/// Air (macOS 15.5) it never returned and no question was shown (setup said "Asking…" forever), and it asked
/// closed apps too, which Apple says it mustn't be given.
///
/// Only a running app is asked (an Apple event to a closed one would open it): "not_running" means macOS asks the
/// first time OnKey talks to it. Waits for the answer up to `ASK_WAIT`, never on the main thread: "granted",
/// "denied", "waiting" (no answer yet: the question may still be showing; the answer is logged when it comes),
/// "ask" (macOS wouldn't ask), "not_installed" or "unknown".
pub fn ask_automation(bundle: &'static str) -> &'static str {
    let Ok(b) = std::ffi::CString::new(bundle) else { return "unknown" };
    match unsafe { onkey_app_running(b.as_ptr()) } {
        1 => {}
        5 => return "not_installed",
        _ => return "not_running",
    }
    let script = ask_script(bundle);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = crate::records::timed("perm", &format!("ask automation {bundle}"),
                                        || std::process::Command::new("osascript").args(["-e", &script]).output());
        let answer = out.as_ref().map_or("unknown", |o| {
            asked(o.status.success(), &String::from_utf8_lossy(&o.stdout), &String::from_utf8_lossy(&o.stderr))
        });
        if tx.send(answer).is_err() {
            crate::engine::log(&format!("setup: control of {bundle}: answered after setup stopped waiting: {answer}"));
        }
    });
    rx.recv_timeout(ASK_WAIT).unwrap_or("waiting")
}

const NOT_RUNNING: &str = "ONKEY-NOT-RUNNING";

/// The AppleScript that asks: the app's player state (Spotify and Music both have it; it changes nothing). The app
/// is checked to be running inside the script too (AppleScript's own `running`, which sends the app nothing), so
/// one that quit a moment ago isn't opened.
fn ask_script(bundle: &str) -> String {
    format!("if application id \"{bundle}\" is running then\n\
             tell application id \"{bundle}\" to return (player state as string)\n\
             else\nreturn \"{NOT_RUNNING}\"\nend if")
}

/// macOS's answer, from osascript's result: the event got through (allowed), -1743 refused, -1744 it would have
/// to ask and didn't, -600/-609 the app quit meanwhile.
fn asked(ok: bool, stdout: &str, stderr: &str) -> &'static str {
    if ok {
        return if stdout.trim() == NOT_RUNNING { "not_running" } else { "granted" };
    }
    if stderr.contains("-1743") {
        "denied"
    } else if stderr.contains("-1744") {
        "ask"
    } else if stderr.contains("-600") || stderr.contains("-609") {
        "not_running"
    } else {
        "unknown"
    }
}

/// Setup's "Let OnKey follow Spotify and Apple Music" step: allowing turns control
/// of both on and has macOS ask for each installed app that's running (the others are asked the first time OnKey
/// talks to them); listen only turns both off. The answers by app ("spotify", "music"), as `ask_automation`'s.
///
/// An app whose control is off by the time it'd be asked ("Listen only" chosen while the one before was being
/// asked) isn't asked.
pub fn choose_control(allow: bool) -> Value {
    crate::settings::set("listening", true);
    for k in crate::settings::CONTROL_KEYS {
        crate::settings::set(k, allow);
    }
    let mut answers = json!({});
    if allow {
        for (app, key, bundle) in [("spotify", "control_spotify", "com.spotify.client"), ("music", "control_music", "com.apple.Music")] {
            let st = if crate::settings::control(key) { ask_automation(bundle) } else { "not_asked" };
            crate::engine::log(&format!("setup: control of {bundle}: {st}"));
            answers[app] = json!(st);
        }
    }
    answers
}

/// The microphone's and system audio recording's permission alone (never prompts), for the window to say when
/// one is refused.
pub fn granted_now() -> (&'static str, &'static str) {
    unsafe { (name(onkey_mic_status()), name(onkey_audio_capture_status())) }
}

pub fn request_mic() {
    crate::records::timed("perm", "request microphone", || unsafe { onkey_mic_request() })
}

/// Ask for system audio recording (macOS prompts the first time). True if it's allowed now.
pub fn request_audio() -> bool {
    crate::records::timed("perm", "request audio recording", || unsafe { onkey_audio_capture_request() == 0 })
}

/// Open the System Settings pane for "mic", "audio" (system audio recording) or "automation" (control of apps).
pub fn open_settings(which: &str) {
    crate::platform::shell::open_settings(which);
}

/// Start or stop the mic level meter; true if it's running afterwards. Says in the log why it couldn't start,
/// and how much audio reached it a moment later (so an empty meter can be told apart from a quiet room).
pub fn meter(on: bool) -> bool {
    let r = unsafe { onkey_mic_meter(on as i32) };
    if !on {
        return false;
    }
    if r != 0 {
        let why = match r { 1 => "no input device", 2 => "couldn't open the input device", _ => "couldn't start the input device" };
        crate::engine::log(&format!("setup mic meter: {why} (code {r})"));
        return false;
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let (blocks, level) = unsafe { (onkey_mic_blocks(), onkey_mic_level()) };
        if level > -120.0 || blocks > 0 {
            crate::engine::log(&format!("setup mic meter: {blocks} audio blocks in 1.5 s, level {level:.0} dBFS"));
        }
    });
    true
}

/// The mic's latest level in dBFS (-120 when the meter isn't running).
pub fn level() -> f32 {
    unsafe { onkey_mic_level() }
}

/// Record that setup has been done (or not, to show it again), keeping the file's other settings.
pub fn set_done(done: bool) {
    // through the settings (they keep the file's other keys), so control's default follows it at once
    crate::settings::set("setup_done", done);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_reads_macos_answer_from_osascript() {
        assert_eq!(asked(true, "playing\n", ""), "granted");
        assert_eq!(asked(true, "paused", ""), "granted");
        assert_eq!(asked(true, &format!("{NOT_RUNNING}\n"), ""), "not_running");
        assert_eq!(asked(false, "", "execution error: Not authorized to send Apple events to Spotify. (-1743)"), "denied");
        assert_eq!(asked(false, "", "execution error: (-1744)"), "ask");
        assert_eq!(asked(false, "", "execution error: Spotify got an error: Application isn’t running. (-600)"), "not_running");
        assert_eq!(asked(false, "", "syntax error"), "unknown");
    }

    #[test]
    fn asking_never_opens_a_closed_app() {
        let s = ask_script("com.spotify.client");
        let check = s.find(r#"if application id "com.spotify.client" is running then"#).expect("checked first");
        assert!(s.find("tell application").unwrap() > check, "the event is sent only inside the check");
        assert!(s.contains("player state") && !s.contains(" set ") && !s.contains("playpause"), "reads only");
    }
}
