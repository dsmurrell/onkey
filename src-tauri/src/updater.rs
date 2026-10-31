//! Updates from GitHub Releases. On launch (a minute in) and once a day after, if "Check for updates
//! automatically" is on (Settings; the `auto_update` setting), OnKey asks for the latest release's latest.json
//! (tauri.conf.json's `plugins.updater` endpoint). A newer version is downloaded quietly and checked against the
//! public key there (the tauri-plugin-updater does both), then the window shows "Update available — Restart to
//! update". Nothing interrupts a song:
//!   - "Restart to update" mid-song waits for the song to end (the music stops or another song starts) and for
//!     what's being learned to be saved, then installs and restarts;
//!   - otherwise it's installed when you quit, and the next launch is the new version.
//!
//! Development builds (the `devtest` feature, debug builds) never check: they'd replace themselves with a release.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::engine;

/// The setting for "Check for updates automatically" (on unless turned off).
pub const SETTING: &str = "auto_update";
/// The first check, after launch (the window and the engine are settled by then).
const FIRST_CHECK: Duration = Duration::from_secs(60);
/// How often after that.
const EVERY: Duration = Duration::from_secs(24 * 3600);
/// A restart waiting for the song to end waits at most this long for takes still being learned.
const LEARN_WAIT: Duration = Duration::from_secs(180);

/// An update downloaded and verified, waiting to be installed (on restart, or on quit).
struct Ready {
    update: Update,
    bytes: Vec<u8>,
}

static READY: Mutex<Option<Ready>> = Mutex::new(None);
/// What the window shows: {"state": "idle" | "checking" | "current" | "downloading" | "ready" | "error", ...}.
static STATE: Mutex<Option<Value>> = Mutex::new(None);
static LAST_CHECK: Mutex<Option<SystemTime>> = Mutex::new(None);
/// "Restart to update" was pressed: restarting once the song playing is over.
static RESTART_WANTED: AtomicBool = AtomicBool::new(false);
/// One check at a time (the daily one and "Check now").
static CHECKING: Mutex<()> = Mutex::new(());

/// This build updates itself: release builds only.
pub fn supported() -> bool {
    !cfg!(feature = "devtest") && !cfg!(debug_assertions)
}

pub fn auto() -> bool {
    crate::settings::get_opt(SETTING).unwrap_or(true)
}

fn set_state(app: &AppHandle, v: Value) {
    *STATE.lock().unwrap() = Some(v);
    let _ = app.emit("onkey-update", status(app));
}

/// For Settings and the window's update pill.
pub fn status(app: &AppHandle) -> Value {
    let mut v = STATE.lock().unwrap().clone().unwrap_or(json!({"state": "idle"}));
    v["version"] = json!(app.package_info().version.to_string());
    v["supported"] = json!(supported());
    v["auto"] = json!(auto());
    v["waiting"] = json!(RESTART_WANTED.load(Ordering::SeqCst));
    v["checked"] = json!(LAST_CHECK.lock().unwrap().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs()));
    v
}

/// Is a check due? Wall-clock time, so a Mac asleep overnight checks when it wakes.
fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.map_or(true, |t| now.duration_since(t).map_or(true, |d| d >= EVERY))
}

/// Check now, and download what's new (quietly). The version found, if any.
pub fn check(app: &AppHandle) -> Result<Option<String>, String> {
    if !supported() {
        return Err("development builds don't update".into());
    }
    let _one = CHECKING.lock().unwrap();
    *LAST_CHECK.lock().unwrap() = Some(SystemTime::now());
    if let Some(r) = READY.lock().unwrap().as_ref() {
        return Ok(Some(r.update.version.clone()));       // downloaded already: installed on restart or quit
    }
    set_state(app, json!({"state": "checking"}));
    let found = tauri::async_runtime::block_on(async {
        app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
    });
    let update = match found {
        Ok(Some(u)) => u,
        Ok(None) => {
            set_state(app, json!({"state": "current"}));
            return Ok(None);
        }
        Err(e) => {
            engine::log(&format!("update: couldn't check: {e}"));
            set_state(app, json!({"state": "error", "error": e}));
            return Err(e);
        }
    };
    let version = update.version.clone();
    engine::log(&format!("update: OnKey {version} is out (this is {}): downloading", update.current_version));
    set_state(app, json!({"state": "downloading", "available": version}));
    match tauri::async_runtime::block_on(update.download(|_, _| {}, || {})) {
        Ok(bytes) => {
            engine::log(&format!("update: {version} downloaded and verified ({:.1} MB)", bytes.len() as f64 / 1e6));
            *READY.lock().unwrap() = Some(Ready { update, bytes });
            set_state(app, json!({"state": "ready", "available": version}));
            Ok(Some(version))
        }
        Err(e) => {
            let e = e.to_string();
            engine::log(&format!("update: download of {version} failed: {e}"));
            set_state(app, json!({"state": "error", "error": e, "available": version}));
            Err(e)
        }
    }
}

/// The daily check, from launch.
pub fn start(app: AppHandle) {
    if !supported() {
        return;
    }
    std::thread::Builder::new().name("onkey-update".into()).spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if auto() && !engine::quitting() && due(*LAST_CHECK.lock().unwrap(), SystemTime::now()) {
                let _ = check(&app);
            }
            std::thread::sleep(Duration::from_secs(600));
        }
    }).ok();
}

/// Is the song that was playing when "Restart to update" was pressed (`at_click`) over? Nothing playing now, or
/// another song: yes.
fn song_over(at_click: Option<&str>, now: Option<(&str, bool)>) -> bool {
    match now {
        None | Some((_, false)) => true,
        Some((id, true)) => at_click.is_some_and(|c| c != id),
    }
}

fn playing(shared: &Arc<Mutex<engine::Shared>>) -> Option<(String, bool)> {
    shared.lock().unwrap().player_now.as_ref().map(|(id, playing, _, _)| (id.clone(), *playing))
}

/// "Restart to update": now if no song is playing, else as soon as it's over (and what's being learned is saved).
pub fn restart_when_ready(app: AppHandle, shared: Arc<Mutex<engine::Shared>>, quit_wait: Duration) -> Value {
    if READY.lock().unwrap().is_none() {
        return json!({"ok": false, "error": "no update is ready"});
    }
    if RESTART_WANTED.swap(true, Ordering::SeqCst) {
        return status(&app);
    }
    let at_click = playing(&shared).filter(|p| p.1).map(|p| p.0);
    let now = status(&app);
    let _ = app.emit("onkey-update", now.clone());
    let available = now["available"].clone();
    std::thread::spawn(move || {
        while !song_over(at_click.as_deref(), playing(&shared).as_ref().map(|(id, p)| (id.as_str(), *p))) {
            std::thread::sleep(Duration::from_millis(500));
        }
        let t = std::time::Instant::now();
        while engine::learning() && t.elapsed() < LEARN_WAIT {
            std::thread::sleep(Duration::from_millis(500));
        }
        // installed first (the app on disk is swapped; this one runs on), so a failed install leaves OnKey running
        if install() {
            engine::shutdown(quit_wait);
            engine::log("update: restarting into the new version");
            app.restart();
        }
        RESTART_WANTED.store(false, Ordering::SeqCst);
        set_state(&app, json!({"state": "error", "error": "the update couldn't be installed", "available": available}));
    });
    now
}

/// Install the downloaded update, if there is one (on quit, and before restarting). True if installed.
pub fn install() -> bool {
    let Some(r) = READY.lock().unwrap().take() else { return false };
    match r.update.install(&r.bytes) {
        Ok(()) => {
            engine::log(&format!("update: {} installed", r.update.version));
            true
        }
        Err(e) => {
            engine::log(&format!("update: {} couldn't be installed: {e}", r.update.version));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restart_to_update_waits_for_the_song_to_end() {
        // nothing was playing: restart now
        assert!(song_over(None, None));
        assert!(song_over(None, Some(("spotify:track:a", false))));
        // the song is still playing: wait
        assert!(!song_over(Some("spotify:track:a"), Some(("spotify:track:a", true))));
        // it was paused or stopped, or the next song started: between songs, restart
        assert!(song_over(Some("spotify:track:a"), Some(("spotify:track:a", false))));
        assert!(song_over(Some("spotify:track:a"), Some(("spotify:track:b", true))));
        assert!(song_over(Some("spotify:track:a"), None));
    }

    #[test]
    fn checks_are_daily_by_the_clock() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(due(None, t), "the first check");
        assert!(!due(Some(t), t + Duration::from_secs(3600)));
        assert!(due(Some(t), t + EVERY));
        assert!(due(Some(t + Duration::from_secs(60)), t), "a clock set back checks rather than never again");
    }

    #[test]
    fn release_builds_update_and_test_builds_do_not() {
        assert_eq!(supported(), !cfg!(feature = "devtest") && !cfg!(debug_assertions));
    }

    /// The public key the app trusts is the one that signs releases (scripts/release.sh, ~/.tauri/onkey-updater.key)
    /// and latest.json is looked for on the latest GitHub release.
    #[test]
    fn the_updater_is_configured() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let u = &conf["plugins"]["updater"];
        assert!(u["pubkey"].as_str().is_some_and(|k| k.len() > 100), "a minisign public key, base64");
        assert_eq!(u["endpoints"][0], "https://github.com/dsmurrell/onkey/releases/latest/download/latest.json");
        assert_ne!(conf["bundle"]["createUpdaterArtifacts"], json!(true), "release.sh makes the updater bundle from the notarised app");
    }
}
