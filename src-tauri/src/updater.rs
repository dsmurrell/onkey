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
//! Nor do previews (a version like 0.2.0-preview.3: the Windows and Linux test builds CI makes before a release
//! ships for those platforms), which latest.json has no entry for; Settings tells testers to install the next one.
//!
//! How an update is installed differs by platform (the tauri-plugin-updater does it; `install` and `rehearse` here
//! allow for it):
//!   - macOS: the new OnKey.app (OnKey_X.Y.Z_aarch64.app.tar.gz) is unpacked over the running one; this process
//!     runs on and relaunches, or quits;
//!   - Windows: the new installer (OnKey_X.Y.Z_x64-setup.exe) is written to a temporary folder and started passively
//!     (a progress window, no questions: /P /UPDATE), and this process exits inside `install` at once; the installer
//!     ends OnKey if it still runs, installs over it, and relaunches it only when asked (/R: "Restart to update",
//!     never a quit);
//!   - Linux: the new OnKey_X.Y.Z_amd64.AppImage replaces the running AppImage's file in place (the one the APPIMAGE
//!     variable names; the old file is moved to a temporary folder on the same disk first, and put back if writing
//!     fails), keeping its permissions; the running copy runs on from memory.
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

/// An official build: made by scripts/release.sh, which sets ONKEY_OFFICIAL=1 for the build. Only an official
/// build installs updates. Any other release build (a fork's, or one built from source) checks too, but only says
/// when a new official version is out, with a link to download it: it never replaces itself with this project's
/// build (which would wipe out a fork's changes, and could read its data in a way it doesn't expect).
pub const OFFICIAL: bool = match option_env!("ONKEY_OFFICIAL") {
    Some(v) => v.len() == 1 && v.as_bytes()[0] == b'1',     // "1" (str can't be compared in a const yet)
    None => false,
};

/// This build checks for updates: release builds only (an official one installs them; others only say), on macOS,
/// Windows and Linux alike: a release carries all three in latest.json (`darwin-aarch64`, `windows-x86_64`,
/// `linux-x86_64`; docs/RELEASING.md), so a check on any of them finds its build. Not a preview, though (the
/// version CI gives a test build, 0.2.0-preview.3; scripts/ci-build.sh): until the release that follows it,
/// latest.json has no entry for its platform, so every check would end in "Couldn't check for updates", and the
/// preview after it is installed over it by hand (Settings says so). The platform doesn't come into it.
pub fn supported(app: &AppHandle) -> bool {
    release_build() && !preview(&app.package_info().version)
}

/// A release build: not a development one (the `devtest` feature, debug builds).
fn release_build() -> bool {
    !cfg!(feature = "devtest") && !cfg!(debug_assertions)
}

/// A preview's version: a pre-release whose pre-release part starts with "preview" (0.2.0-preview.3). A release
/// candidate (0.3.0-rc.1) isn't one: it is the release about to ship, and checks like one.
fn preview(version: &semver::Version) -> bool {
    version.pre.as_str().starts_with("preview")
}

/// The release's rehearsal (scripts/update-rehearsal.sh on the Mac; scripts/update-rehearsal-ci.sh and .ps1 on the
/// Linux and Windows CI runners): before a release is published, the exact app about to ship is pointed at a fake
/// next version served on the same machine, and must download, verify and install it. Only an address on this
/// machine (`http://127.0.0.1:<port>/…`) is taken.
pub const REHEARSAL_ENV: &str = "ONKEY_UPDATE_REHEARSAL";

pub fn rehearsal_url() -> Option<tauri::Url> {
    let u = tauri::Url::parse(&std::env::var(REHEARSAL_ENV).ok()?).ok()?;
    (u.scheme() == "http" && u.host_str() == Some("127.0.0.1")).then_some(u)
}

/// Run the rehearsal and exit: 0 when the update was downloaded, its signature checked against the key in
/// tauri.conf.json, and installed over this app; 1 otherwise. Prints one line the rehearsal script reads (and
/// writes it to the log too: a Windows app has no console), saying too whether this is an official build (one
/// that isn't would never update anyone).
///
/// On Windows the install hands over to the new installer and this process exits inside it (`install` above), so
/// there the line before that, `installing`, with exit status 0 and the installer's result on disk, is the proof.
pub fn rehearse(app: &AppHandle, url: tauri::Url) -> ! {
    use tauri::Manager;
    for w in app.webview_windows().values() {
        let _ = w.hide();
    }
    let say = |line: String| {
        println!("{line}");
        engine::log(&line);
    };
    let app = app.clone();
    let done = std::thread::spawn(move || tauri::async_runtime::block_on(async move {
        let update = app.updater_builder().endpoints(vec![url]).map_err(|e| e.to_string())?
            .build().map_err(|e| e.to_string())?
            .check().await.map_err(|e| format!("check: {e}"))?
            .ok_or("check: no update offered")?;
        let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| format!("download or signature: {e}"))?;
        if cfg!(target_os = "windows") {
            let line = format!("update-rehearsal: installing {} (official {OFFICIAL})", update.version);
            println!("{line}");
            engine::log(&line);
        }
        let version = update.version.clone();
        // the rehearsal plays "Restart to update": on Windows the installer relaunches the installed app, with this
        // process's environment, and that app must start as a person's would, not rehearse again (Windows only:
        // on the Mac the rehearsal stays exactly as 0.2.0's)
        #[cfg(windows)]
        std::env::remove_var(REHEARSAL_ENV);
        update.restart_after_install(true).install(&bytes).map_err(|e| format!("install: {e}"))?;
        Ok::<String, String>(version)
    })).join().unwrap_or_else(|_| Err("the rehearsal panicked".into()));
    match done {
        Ok(v) => {
            say(format!("update-rehearsal: ok, installed {v} (official {OFFICIAL})"));
            std::process::exit(0)
        }
        Err(e) => {
            say(format!("update-rehearsal: failed: {e} (official {OFFICIAL})"));
            std::process::exit(1)
        }
    }
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
    v["supported"] = json!(supported(app));
    v["preview"] = json!(preview(&app.package_info().version));
    v["official"] = json!(OFFICIAL);
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
    if !supported(app) {
        return Err("this build doesn't update itself".into());
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
            set_state(app, json!({"state": "error", "code": "check_failed", "error": e}));
            return Err(e);
        }
    };
    let version = update.version.clone();
    // not an official build (a fork's, or built from source): say so, and never download or install it
    if !OFFICIAL {
        engine::log(&format!("update: OnKey {version} is out from the original project (this is a build from source: not installed)"));
        set_state(app, json!({"state": "official", "available": version}));
        return Ok(Some(version));
    }
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
            set_state(app, json!({"state": "error", "code": "download_failed", "error": e, "available": version}));
            Err(e)
        }
    }
}

/// The daily check, from launch.
pub fn start(app: AppHandle) {
    if !supported(&app) {
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
        return json!({"ok": false, "code": "not_ready", "error": "no update is ready"});
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
        if cfg!(target_os = "windows") {
            // the installer takes over and this process ends inside the install (see the top of this file), so what
            // has been learned is saved first; the installer relaunches OnKey when done
            engine::shutdown(quit_wait);
            engine::log("update: handing over to the installer, which restarts OnKey");
            install_then(true);
        } else if install_then(false) {
            // installed first (the app on disk is swapped; this one runs on), so a failed install leaves OnKey running
            engine::shutdown(quit_wait);
            engine::log("update: restarting into the new version");
            app.restart();
        }
        RESTART_WANTED.store(false, Ordering::SeqCst);
        set_state(&app, json!({"state": "error", "code": "install_failed", "error": "the update couldn't be installed", "available": available}));
    });
    now
}

/// Install the downloaded update, if there is one, on quit: the next launch is the new version. True if installed.
/// (On Windows the installer runs now and this process ends inside; it doesn't relaunch OnKey, which was quit.)
pub fn install() -> bool {
    install_then(false)
}

/// Install the downloaded update, if there is one. `relaunch`: on Windows, the installer starts the new OnKey when
/// it's done ("Restart to update"); elsewhere the caller restarts or quits. True if installed.
fn install_then(relaunch: bool) -> bool {
    let Some(Ready { update, bytes }) = READY.lock().unwrap().take() else { return false };
    let version = update.version.clone();
    match update.restart_after_install(relaunch).install(&bytes) {
        Ok(()) => {
            engine::log(&format!("update: {version} installed"));
            true
        }
        Err(e) => {
            engine::log(&format!("update: {version} couldn't be installed: {e}"));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every installed OnKey checks this address and trusts only this key: change either by accident and no copy
    /// out there can update again. To change them on purpose (a new repo, a rotated key: docs/RELEASING.md), change
    /// these pins in the same commit, and say why in it.
    const PINNED_ENDPOINT: &str = "https://github.com/dsmurrell/onkey/releases/latest/download/latest.json";
    const PINNED_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDU5RjUzMDNDNkU1OEQ4REQKUldUZDJGaHVQREQxV2V6RTBQZjZZY2N5eU9xV1dkeVN2VGFkOTl5T0JZK2g5dXlLV0JDVFpCN0EK";

    #[test]
    fn the_update_address_and_key_are_the_ones_every_installed_copy_trusts() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let u = &conf["plugins"]["updater"];
        assert_eq!(u["endpoints"], json!([PINNED_ENDPOINT]), "the update address changed");
        assert_eq!(u["pubkey"], json!(PINNED_PUBKEY), "the update key changed");
        // plain http only for the release script's rehearsal, switched on in code (lib.rs context), never here
        assert_ne!(u["dangerousInsecureTransportProtocol"], json!(true));
        assert_eq!(conf["identifier"], json!("audio.onkey.app"), "a new app id is a new app: installed copies wouldn't update into it");
    }

    #[test]
    fn the_rehearsal_takes_only_an_address_on_this_mac() {
        for (url, ok) in [("http://127.0.0.1:8765/latest.json", true), ("http://localhost:8765/latest.json", false),
                          ("https://127.0.0.1/latest.json", false), ("http://127.0.0.1.evil.example/latest.json", false),
                          ("http://10.0.0.2/latest.json", false), ("file:///tmp/latest.json", false)] {
            std::env::set_var(REHEARSAL_ENV, url);
            assert_eq!(rehearsal_url().is_some(), ok, "{url}");
        }
        std::env::remove_var(REHEARSAL_ENV);
        assert!(rehearsal_url().is_none());
    }

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
        // on every platform OnKey is released for: latest.json carries all three (release-publish.sh refuses to
        // publish one without)
        assert_eq!(release_build(), !cfg!(feature = "devtest") && !cfg!(debug_assertions));
        assert!(cfg!(any(target_os = "macos", target_os = "windows", target_os = "linux")));
        let v = |s: &str| semver::Version::parse(s).unwrap();
        // a release checks; a preview (CI's Windows and Linux test builds) doesn't: latest.json has nothing for it
        assert!(!preview(&v("0.3.0")));
        assert!(!preview(&v("0.2.0")));
        assert!(preview(&v("0.2.0-preview.3")));
        assert!(preview(&v("0.2.0-preview")));
        assert!(preview(&v("1.0.0-preview.12")));
        // a release candidate is the release about to ship: it checks, and updates into the release
        assert!(!preview(&v("0.3.0-rc.1")));
        // the version in tauri.conf.json (what a release says) is never a preview
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(!preview(&v(conf["version"].as_str().unwrap())));
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
        // Windows and Linux: CI turns createUpdaterArtifacts on for a release build (--config), with the signing key
        // from its secrets; a plain `tauri build` there has no key and makes no updater files, so it isn't on here
        for (f, text) in [("tauri.windows.conf.json", include_str!("../tauri.windows.conf.json")), ("tauri.linux.conf.json", include_str!("../tauri.linux.conf.json"))] {
            let c: Value = serde_json::from_str(text).unwrap();
            assert_ne!(c["bundle"]["createUpdaterArtifacts"], json!(true), "{f}: CI turns it on, with the key");
            assert!(c["plugins"]["updater"]["pubkey"].is_null() && c["plugins"]["updater"]["endpoints"].is_null(), "{f}: the key and address are tauri.conf.json's only");
        }
        // the Windows installer runs passively (a progress window, no questions) when the updater starts it
        let w: Value = serde_json::from_str(include_str!("../tauri.windows.conf.json")).unwrap();
        assert_eq!(w["plugins"]["updater"]["windows"]["installMode"], json!("passive"));
    }
}
