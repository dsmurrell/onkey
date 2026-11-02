//! Spotify: what's playing and where, and playback control. No login, no web API, and it works with free
//! accounts. How it's asked is the platform's: AppleScript on macOS (platform/mac/spotify.rs), the system media
//! session on Windows (platform/windows/media.rs).
use std::sync::Mutex;
pub use crate::source::{State, Track};

pub fn running() -> bool {
    if crate::devtest::sim_app() {
        return true;
    }
    crate::source::app_running("com.spotify.client")
}

/// Spotify's own volume (0-100) as last read, when OnKey may control Spotify; read again in the background at
/// most every 20 s (an AppleScript call: never on the engine's thread). None until read, or when not allowed
/// (or not known: Windows' media session doesn't say it).
pub fn volume_now() -> Option<u32> {
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
    static VOLUME: AtomicI32 = AtomicI32::new(-1);
    static READ_AT: AtomicU64 = AtomicU64::new(0);
    static READING: AtomicBool = AtomicBool::new(false);
    if !crate::settings::control("control_spotify") {
        return None;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if now >= READ_AT.load(Ordering::Relaxed) + 20 && !READING.swap(true, Ordering::SeqCst) {
        READ_AT.store(now, Ordering::Relaxed);
        std::thread::spawn(|| {
            // (only if it's running: asking a closed Spotify would open it)
            let v = running().then(crate::platform::spotify::volume).flatten();
            VOLUME.store(v.unwrap_or(-1), Ordering::Relaxed);
            READING.store(false, Ordering::SeqCst);
        });
    }
    u32::try_from(VOLUME.load(Ordering::Relaxed)).ok()
}

/// What Spotify is playing.
pub fn state() -> Option<State> {
    if !running() {
        return None;
    }
    // development builds: the simulated player answers as Spotify (ONKEY_SIM_LISTEN)
    if let Some(sim) = crate::devtest::sim_app_state() {
        crate::devtest::sim_osa_cost();
        return sim;
    }
    crate::platform::spotify::state()
}

/// Is Spotify's answer (its track's id, name and length) to be believed? As one song gives way to the next,
/// Spotify can answer with the old song's id and the new one's name and length (live, 20 Oct: Californication
/// learned under Get On Top's id, so its Library card played Get On Top). Such an answer isn't believed for a few
/// polls, until the id catches up. (macOS: AppleScript's answers; Windows' ids are made from the names.)
pub fn fresh(id: &str, name: &str, duration: f64) -> bool {
    static LAST: Mutex<(Option<(String, String, f64)>, u32)> = Mutex::new((None, 0));
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if stale_id(last.0.as_ref(), id, name, duration) && last.1 < STALE_POLLS {
        last.1 += 1;
        return false;
    }
    *last = (Some((id.to_string(), name.to_string(), duration)), 0);
    true
}

/// Polls an answer whose id doesn't match its song is skipped for (about 3 s), before it's taken as it is.
const STALE_POLLS: u32 = 8;

/// Spotify's answer has the same id as the song before but another song's name or length: its id is stale.
fn stale_id(last: Option<&(String, String, f64)>, id: &str, name: &str, duration: f64) -> bool {
    last.is_some_and(|(lid, lname, ldur)| lid == id && (lname != name || (ldur - duration).abs() > 1.0))
}

/// "restart", "next", "previous", "playpause", "pause" or "play".
pub fn control(action: &str) {
    crate::platform::spotify::control(action);
}

/// Jump to a position in the current track (seconds).
pub fn seek(seconds: f64) {
    crate::platform::spotify::seek(seconds);
}

/// Play a track by its id; false when Spotify didn't (not installed, refused, or no answer). On Windows the track
/// is opened by its spotify:track: link (the media session can't start a given track), and it's done once Spotify's
/// media session names it (platform/windows/media.rs). A made-up id (trackids.rs) is never opened: false.
pub fn play_track(id: &str) -> bool {
    crate::platform::spotify::play_track(id)
}

#[cfg(test)]
mod stale_tests {
    use super::stale_id;

    #[test]
    fn an_old_id_with_a_new_songs_name_is_stale() {
        let get_on_top = ("spotify:track:0GcWf1".to_string(), "Get on Top".to_string(), 198.1);
        assert!(stale_id(Some(&get_on_top), "spotify:track:0GcWf1", "Californication", 329.7));
        assert!(!stale_id(Some(&get_on_top), "spotify:track:48UPSz", "Californication", 329.7));
        assert!(!stale_id(Some(&get_on_top), "spotify:track:0GcWf1", "Get on Top", 198.1));
        assert!(!stale_id(None, "spotify:track:0GcWf1", "Californication", 329.7));
    }
}
