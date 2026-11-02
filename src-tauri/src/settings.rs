//! Your choices that outlast a restart, in OnKey's settings file (settings.json in its data folder). Every
//! part of OnKey reads and writes it through here (one copy in memory, one lock), so no writer drops another's
//! key:
//!   listening        how songs are followed: by listening to them, asking the music apps as well where allowed
//!                    (on unless turned off). Off is the older way, by asking Spotify and Apple Music alone (no
//!                    switch for it in the window any more; the development builds' simulated player uses it).
//!                    It isn't "Listen only": that's the three control settings below all off (`listen_only`)
//!   control_spotify  let OnKey follow and control Spotify (exact positions; Sing it again, skip, pause)
//!   control_music    the same for Apple Music
//!   youtube          follow YouTube in your browser (youtube.rs)
//!   shazam           use Shazam to name songs not learned here, when no music app OnKey controls says what plays
//!                    (listen.rs `shazam_wanted`); on unless turned off. Off: nothing is sent to Shazam
//!   setup_done       first-run setup is done (permissions.rs)
//!   mic              the microphone chosen in Settings (a Core Audio device uid); none: automatic (capture.rs)
//!   show_lyrics      show lyrics (from LRCLIB) with the notes; on unless turned off. Off: LRCLIB is never asked
//!                    (lyrics.rs), and lyrics already kept on this Mac are hidden, not deleted (the window)
//!   fade_guide       "Fade the guide": on a song you've sung well before, your own pitch line on the Sing screen
//!                    fades (the window decides how far, from your past takes: ui/lib/guideFade.ts); off unless
//!                    turned on
//!   listening_paused "Pause listening": OnKey follows nothing (no capture, nothing named, recognised or learned,
//!                    no take) until you resume it (engine.rs); off unless turned on, and it stays paused across
//!                    restarts
//!   auto_update      check GitHub Releases for a new OnKey on launch and daily (updater.rs); on unless turned off
//!   scoring_level    how strictly takes are scored: "steady" or "pro" (dsp::LEVELS); none: Relaxed, the default
//!   language         the window's language: a language code ("en", "zh", "pt-BR"…) chosen in Settings; none:
//!                    "auto", following the computer's own language (the window reads navigator.language), then
//!                    English (ui/lib/i18n.ts)
//!   output_timing    per output device (Core Audio uid): your tap-along check of its delay, and whether its
//!                    suggestion to check was dismissed (timing.rs)
//! Control of Spotify and Apple Music is on by default (setup asks macOS for it; its "Listen only" turns both off),
//! YouTube's is off until you allow it; installs from before that keep what they had: off unless they allowed it.
//! Listen only (follow songs by their sound alone, touching no app) has no setting of its own: it's Spotify,
//! Apple Music and YouTube all off in Settings.
//!
//! The file is written atomically, and settings.json.bak keeps the last good copy: a file cut short (a crash
//! mid-write on an older OnKey, a disk problem) is read from it, never taken for a new install (that would turn
//! control off and show setup again).
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
static VALUES: Mutex<Option<Value>> = Mutex::new(None);

pub const CONTROL_KEYS: [&str; 2] = ["control_spotify", "control_music"];

/// Every setting as it is now (a problem report keeps only their on/off values: report.rs).
pub fn all() -> Value {
    VALUES.lock().unwrap().clone().unwrap_or_else(|| json!({}))
}

/// Read once at launch from OnKey's data folder. An install from before control was on by default (it has a
/// settings file, or songs learned) keeps control off unless it was allowed: written down now, once.
pub fn init(dir: &Path) {
    let file = dir.join("settings.json");
    let (v, why) = load(dir);
    if let Some(why) = &why {
        crate::engine::log(&format!("settings: {why}"));
    }
    // written back when it changed, and so that the last good copy (.bak) is there from now on
    if why.is_some() || read(&backup(&file)).as_ref() != Some(&v) {
        if let Err(e) = save(&file, &v) {
            crate::engine::log(&format!("settings: couldn't be saved: {e}"));
        }
    }
    *VALUES.lock().unwrap() = Some(v);
    *FILE.lock().unwrap() = Some(file);
}

fn backup(file: &Path) -> PathBuf {
    file.with_extension("json.bak")
}

/// A settings file's contents, if it's there and whole (a JSON object).
fn read(p: &Path) -> Option<Value> {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).filter(|v| v.is_object())
}

/// The settings in `dir`, and why they need writing back (None: as they are on disk).
fn load(dir: &Path) -> (Value, Option<String>) {
    let file = dir.join("settings.json");
    if let Some(mut v) = read(&file) {
        let migrated = migrate(&mut v, true);
        return (v, migrated.then(|| "control kept as it was before it was on by default".to_string()));
    }
    let bak = backup(&file);
    if file.exists() || bak.exists() {
        // there, but cut short or garbled: the last good copy, as it was
        return match read(&bak) {
            Some(v) => (v, Some("settings.json was damaged: restored from settings.json.bak".into())),
            None => (json!({}), Some("settings.json was damaged and there's no good copy: starting afresh".into())),
        };
    }
    let mut v = json!({});
    let existed = std::fs::read_dir(dir.join("songs")).is_ok_and(|mut d| d.next().is_some());
    let migrated = migrate(&mut v, existed);
    (v, migrated.then(|| "songs from before control was on by default: kept off".to_string()))
}

/// Write the settings, then the same as the last good copy.
fn save(file: &Path, v: &Value) -> std::io::Result<()> {
    let b = v.to_string();
    crate::store::write_atomic(file, b.as_bytes())?;
    let _ = crate::store::write_atomic(&backup(file), b.as_bytes());
    Ok(())
}

/// An existing install's control settings, as they were: off where never allowed. A new install gets none
/// written (so control is on by default once setup is done: see `control`). True if `v` changed.
fn migrate(v: &mut Value, existed: bool) -> bool {
    if !existed {
        return false;
    }
    let mut changed = false;
    for k in CONTROL_KEYS {
        if v[k].is_null() {
            v[k] = json!(false);
            changed = true;
        }
    }
    changed
}

pub fn get(key: &str) -> bool {
    VALUES.lock().unwrap().as_ref().and_then(|v| v[key].as_bool()).unwrap_or(false)
}

/// A setting that's on or off, if it's been chosen.
pub fn get_opt(key: &str) -> Option<bool> {
    VALUES.lock().unwrap().as_ref().and_then(|v| v[key].as_bool())
}

/// May OnKey control this app ("control_spotify", "control_music")? As chosen; with no choice yet (a new install)
/// on, once first-run setup is done (so macOS's prompt comes from setup's own step, not out of the blue).
pub fn control(key: &str) -> bool {
    control_in(VALUES.lock().unwrap().as_ref().unwrap_or(&Value::Null), key)
}

fn control_in(v: &Value, key: &str) -> bool {
    v[key].as_bool().unwrap_or_else(|| v["setup_done"].as_bool().unwrap_or(false))
}

/// Listen only: follow songs by their sound alone, controlling no app: listening mode with Spotify, Apple Music and
/// YouTube in your browser all off in Settings. Derived, never kept: there's no setting of its own.
pub fn listen_only() -> bool {
    listen_only_in(VALUES.lock().unwrap().as_ref().unwrap_or(&Value::Null))
}

fn listen_only_in(v: &Value) -> bool {
    v["listening"].as_bool().unwrap_or(true) && CONTROL_KEYS.iter().all(|k| !control_in(v, k)) && !v["youtube"].as_bool().unwrap_or(false)
}

/// Change one setting and save it. Every writer comes through here, under one lock, so none drops another's key.
pub fn set(key: &str, on: bool) {
    set_value(key, json!(on));
}

/// A setting that's text (the microphone chosen: "mic", a device uid), if set.
pub fn get_str(key: &str) -> Option<String> {
    VALUES.lock().unwrap().as_ref().and_then(|v| v[key].as_str().map(String::from))
}

/// A setting as it's kept (Null if not set).
pub fn get_value(key: &str) -> Value {
    VALUES.lock().unwrap().as_ref().map_or(Value::Null, |v| v[key].clone())
}

/// A setting that's a number, if set.
pub fn get_f64(key: &str) -> Option<f64> {
    VALUES.lock().unwrap().as_ref().and_then(|v| v[key].as_f64())
}

/// Change one setting to any value (null removes it) and save it.
pub fn set_value(key: &str, value: Value) {
    let _ = try_set_value(key, value);
}

/// `set`, saying whether it was saved (the Settings page says so when it wasn't: a full disk, say).
pub fn try_set(key: &str, on: bool) -> Result<(), String> {
    try_set_value(key, json!(on))
}

/// `set_value`, saying whether it was saved: Err (and logged) if the file couldn't be written. The setting holds
/// until OnKey quits either way.
pub fn try_set_value(key: &str, value: Value) -> Result<(), String> {
    let mut values = VALUES.lock().unwrap();
    let v = values.get_or_insert_with(|| json!({}));
    if value.is_null() {
        if let Some(o) = v.as_object_mut() {
            o.remove(key);
        }
    } else {
        v[key] = value;
    }
    if let Some(f) = FILE.lock().unwrap().as_ref() {
        if let Err(e) = save(f, v) {
            crate::engine::log(&format!("settings: {key} couldn't be saved: {e}"));
            return Err(format!("Couldn’t save the setting ({e}). It holds until OnKey quits."));
        }
    }
    Ok(())
}

/// Listening mode: songs are followed by listening (see listen.rs), and by asking the apps OnKey may control.
/// Off: the music apps are only asked. On unless you turned it off.
pub fn listening() -> bool {
    let set = VALUES.lock().unwrap().as_ref().and_then(|v| v["listening"].as_bool());
    set.unwrap_or(true) || crate::devtest::knob!("ONKEY_LISTENING").is_some()
}

/// "Use Shazam to name new songs": on unless you turned it off.
pub fn shazam() -> bool {
    get_opt("shazam").unwrap_or(true)
}

/// "Show lyrics": on unless you turned it off. Off: no request goes to LRCLIB, and no lyrics are shown.
pub fn show_lyrics() -> bool {
    get_opt("show_lyrics").unwrap_or(true)
}

/// "Pause listening": off unless you paused it; kept until you resume, also across restarts.
pub fn listening_paused() -> bool {
    listening_paused_in(VALUES.lock().unwrap().as_ref().unwrap_or(&Value::Null))
}

fn listening_paused_in(v: &Value) -> bool {
    v["listening_paused"].as_bool().unwrap_or(false)
}

/// "Fade the guide": off unless you turned it on (the window does the fading).
pub fn fade_guide() -> bool {
    get_opt("fade_guide").unwrap_or(false)
}

/// "Language": "auto" (follow the computer's language) unless a language code was chosen in Settings.
pub fn language() -> String {
    language_in(VALUES.lock().unwrap().as_ref().unwrap_or(&Value::Null))
}

fn language_in(v: &Value) -> String {
    v["language"].as_str().and_then(language_code).unwrap_or_else(|| "auto".into())
}

/// A language code as kept ("en", "zh-Hans", "pt-BR": letters, digits and dashes, a 2–3 letter language first),
/// or None for "auto" and anything that isn't a code.
pub fn language_code(code: &str) -> Option<String> {
    let code = code.trim();
    let mut parts = code.split(['-', '_']);
    let lang = parts.next()?;
    let ok = code.len() <= 35
        && (2..=3).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_alphabetic())
        && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()));
    (ok && !code.eq_ignore_ascii_case("auto")).then(|| code.replace('_', "-"))
}

/// Choose the window's language: a code, or "auto" (kept as no setting at all). Something that isn't a code is "auto".
pub fn set_language(code: &str) -> Result<(), String> {
    try_set_value("language", language_code(code).map_or(Value::Null, Value::String))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_is_auto_unless_a_code_was_chosen_and_a_choice_outlasts_a_restart() {
        assert_eq!(language_in(&json!({})), "auto", "a new install follows the computer");
        assert_eq!(language_in(&json!({"setup_done": true})), "auto", "an existing one too");
        assert_eq!(language_in(&json!({"language": "zh"})), "zh");
        assert_eq!(language_in(&json!({"language": "pt_BR"})), "pt-BR");
        assert_eq!(language_in(&json!({"language": "../../etc"})), "auto", "not a code: auto");
        assert_eq!(language_in(&json!({"language": 7})), "auto");
        for bad in ["", "auto", "AUTO", "e", "english", "en--", "zh-", "1a", "en-<b>"] {
            assert_eq!(language_code(bad), None, "{bad}");
        }
        for (good, kept) in [("en", "en"), ("zh-Hans", "zh-Hans"), ("es-419", "es-419"), ("pt_BR", "pt-BR"), (" ja ", "ja")] {
            assert_eq!(language_code(good).as_deref(), Some(kept));
        }
        let dir = std::env::temp_dir().join(format!("onkey-settings-language-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        for (chosen, back) in [(Some("es"), "es"), (None, "auto")] {
            let mut v = json!({"setup_done": true});
            if let Some(c) = chosen {
                v["language"] = json!(c);
            }
            save(&file, &v).unwrap();
            let (read_back, _) = load(&dir);
            assert_eq!(language_in(&read_back), back, "read back as saved");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fade_the_guide_is_off_unless_turned_on() {
        assert!(!fade_guide());
    }

    #[test]
    fn control_is_on_by_default_for_new_installs_only() {
        // a new install: nothing written; off until setup is done (its step asks macOS), then on
        let mut v = json!({});
        assert!(!migrate(&mut v, false));
        assert!(!control_in(&v, "control_spotify") && !control_in(&v, "control_music"));
        v["setup_done"] = json!(true);
        assert!(control_in(&v, "control_spotify") && control_in(&v, "control_music"));
        // "Listen only" in setup: both off
        v["control_spotify"] = json!(false);
        v["control_music"] = json!(false);
        assert!(!control_in(&v, "control_spotify") && !control_in(&v, "control_music"));

        // an existing install keeps its choice: off where never allowed, on where allowed, and it's written once
        let mut old = json!({"setup_done": true, "listening": true, "control_spotify": true});
        assert!(migrate(&mut old, true));
        assert!(control_in(&old, "control_spotify") && !control_in(&old, "control_music"));
        assert!(!migrate(&mut old, true), "once");
        let mut never = json!({"setup_done": true});
        migrate(&mut never, true);
        assert!(!control_in(&never, "control_spotify") && !control_in(&never, "control_music"));
        // songs learned but no settings file (before setup existed): existing too
        let mut bare = json!({});
        assert!(migrate(&mut bare, true));
        bare["setup_done"] = json!(true);
        assert!(!control_in(&bare, "control_spotify"));
    }

    #[test]
    fn listen_only_is_every_app_off_and_has_no_setting_of_its_own() {
        // a new install after setup: Spotify and Apple Music on, YouTube off (the defaults)
        let mut v = json!({"setup_done": true});
        assert!(!listen_only_in(&v));
        // setup's "Listen only" (or both switched off in Settings): every app off, YouTube never allowed
        v["control_spotify"] = json!(false);
        v["control_music"] = json!(false);
        assert!(listen_only_in(&v));
        // any one app on is not listen only: Spotify alone off changes nothing for Apple Music, and the reverse
        assert!(!listen_only_in(&json!({"setup_done": true, "control_spotify": false})));
        assert!(!listen_only_in(&json!({"setup_done": true, "control_music": false})));
        assert!(!listen_only_in(&json!({"setup_done": true, "control_spotify": false, "control_music": false, "youtube": true})));
        // a file from when Settings had its own Listen only switch: it wrote "listening": true and both apps off.
        // Read as it is (no migration needed: listen only was always the apps being off), it's still listen only,
        // and it's not written back
        let old = json!({"setup_done": true, "listening": true, "control_spotify": false, "control_music": false, "youtube": false});
        let mut migrated = old.clone();
        assert!(!migrate(&mut migrated, true));
        assert_eq!(migrated, old);
        assert!(listen_only_in(&old));
        // "listening": true alone is listening mode, the default, not listen only: control stays as chosen
        assert!(!listen_only_in(&json!({"setup_done": true, "listening": true})));
        // the older way (asking the apps alone) controls them: never listen only
        assert!(!listen_only_in(&json!({"setup_done": true, "listening": false, "control_spotify": false, "control_music": false})));
    }

    #[test]
    fn listening_is_not_paused_unless_you_paused_it_and_a_pause_outlasts_a_restart() {
        assert!(!listening_paused_in(&json!({})), "a new install listens");
        assert!(!listening_paused_in(&json!({"setup_done": true, "listening": true})), "an existing one too");
        let dir = std::env::temp_dir().join(format!("onkey-settings-pause-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        for on in [true, false] {
            let mut v = json!({"setup_done": true});
            v["listening_paused"] = json!(on);
            save(&file, &v).unwrap();
            let (back, _) = load(&dir);
            assert_eq!(listening_paused_in(&back), on, "read back as saved");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_settings_file_is_read_from_its_last_good_copy() {
        let dir = std::env::temp_dir().join(format!("onkey-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        std::fs::write(dir.join("songs").join("x.json"), "{}").unwrap();
        let file = dir.join("settings.json");
        let good = json!({"setup_done": true, "listening": true, "control_spotify": true, "control_music": true, "youtube": true});
        save(&file, &good).unwrap();
        assert_eq!(read(&backup(&file)), Some(good.clone()), "the last good copy is kept beside it");
        // a write that fails says so (here: the folder it goes in isn't there)
        assert!(save(&dir.join("gone").join("settings.json"), &good).is_err());
        assert_eq!(load(&dir), (good.clone(), None), "a whole file is read as it is");

        // cut short mid-write: the copy, with setup still done and control still on (not migrated as if new)
        let whole = good.to_string();
        std::fs::write(&file, &whole[..whole.len() / 2]).unwrap();
        let (v, why) = load(&dir);
        assert_eq!(v, good);
        assert!(why.is_some_and(|w| w.contains("restored")));
        assert!(control_in(&v, "control_spotify") && v["setup_done"] == json!(true));

        // empty, and no good copy either: started afresh, but control isn't turned off as for an old install
        std::fs::write(&file, "").unwrap();
        std::fs::remove_file(backup(&file)).unwrap();
        let (v, why) = load(&dir);
        assert_eq!(v, json!({}));
        assert!(why.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
