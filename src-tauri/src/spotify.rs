//! Spotify, through its AppleScript interface: what's playing and where, and playback control. No login,
//! no web API, and it works with free accounts.

use std::sync::Mutex;
pub use crate::source::{State, Track};
use crate::source::Source;

fn osa(script: &str) -> Option<String> {
    crate::source::osa("Spotify", script)
}

pub fn running() -> bool {
    if crate::devtest::sim_app() {
        return true;
    }
    crate::source::app_running("com.spotify.client")
}

/// Spotify's own volume (0-100) as last read, when OnKey may control Spotify; read again in the background at
/// most every 20 s (an AppleScript call: never on the engine's thread). None until read, or when not allowed.
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
            let v = running().then(|| osa(r#"tell application "Spotify" to sound volume"#)).flatten()
                .and_then(|v| v.trim().parse::<i32>().ok());
            VOLUME.store(v.unwrap_or(-1), Ordering::Relaxed);
            READING.store(false, Ordering::SeqCst);
        });
    }
    u32::try_from(VOLUME.load(Ordering::Relaxed)).ok()
}

/// What Spotify is playing (fields separated by a character that can't appear in titles).
pub fn state() -> Option<State> {
    if !running() {
        return None;
    }
    // development builds: the simulated player answers as Spotify (ONKEY_SIM_LISTEN)
    if let Some(sim) = crate::devtest::sim_app_state() {
        crate::devtest::sim_osa_cost();
        return sim;
    }
    let out = osa(r#"tell application "Spotify"
        set d to ASCII character 31
        return (player state as string) & d & (id of current track) & d & (name of current track) & d & ¬
            (artist of current track) & d & (album of current track) & d & (duration of current track) & d & (player position) & d & (artwork url of current track)
    end tell"#)?;
    let f: Vec<&str> = out.split('\u{1f}').collect();
    if f.len() < 7 {
        return None;
    }
    let duration = f[5].replace(',', ".").parse::<f64>().unwrap_or(0.0) / 1000.0;
    // as one song gives way to the next, Spotify can answer with the old song's id and the new one's name and
    // length (live, 20 Oct: Californication learned under Get On Top's id, so its Library card played Get On
    // Top). Such an answer isn't believed for a few polls, until the id catches up
    {
        static LAST: Mutex<(Option<(String, String, f64)>, u32)> = Mutex::new((None, 0));
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if stale_id(last.0.as_ref(), f[1], f[2], duration) && last.1 < STALE_POLLS {
            last.1 += 1;
            return None;
        }
        *last = (Some((f[1].to_string(), f[2].to_string(), duration)), 0);
    }
    Some(State {
        playing: f[0] == "playing",
        track: Track {
            id: f[1].to_string(),
            name: f[2].to_string(),
            artist: f[3].to_string(),
            album: f[4].to_string(),
            duration,
            art: f.get(7).map(|a| a.trim().to_string()).filter(|a| a.starts_with("https://")),
            link: None,
        },
        position: f[6].replace(',', ".").parse().unwrap_or(0.0),
        source: Source::Spotify,
    })
}

/// Polls an answer whose id doesn't match its song is skipped for (about 3 s), before it's taken as it is.
const STALE_POLLS: u32 = 8;

/// Spotify's answer has the same id as the song before but another song's name or length: its id is stale.
fn stale_id(last: Option<&(String, String, f64)>, id: &str, name: &str, duration: f64) -> bool {
    last.is_some_and(|(lid, lname, ldur)| lid == id && (lname != name || (ldur - duration).abs() > 1.0))
}

pub fn control(action: &str) {
    let cmd = match action {
        "restart" => "set player position to 0",
        "next" => "next track",
        "previous" => "previous track",
        "playpause" => "playpause",
        "pause" => "pause",
        "play" => "play",
        _ => return,
    };
    osa(&format!(r#"tell application "Spotify" to {cmd}"#));
}

/// Jump to a position in the current track (seconds).
pub fn seek(seconds: f64) {
    osa(&format!("tell application \"Spotify\" to set player position to {:.2}", seconds.max(0.0)));
}

/// Play a track by its id; false when Spotify didn't (not installed, refused, or no answer).
pub fn play_track(id: &str) -> bool {
    if !(id.starts_with("spotify:track:") && id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':')) {
        return false;
    }
    // The song already in Spotify (paused, say) is started over from the top instead: asked to play it again,
    // Spotify resumes it for a split second and then starts it over (live, 29 Oct), which sounds like a double start.
    let done = osa(&play_script(id)).is_some();
    if !done {
        crate::engine::log(&format!("Spotify didn't play {id} (not installed, or it didn't answer)"));
    }
    done
}

fn play_script(id: &str) -> String {
    format!(r#"tell application "Spotify"
set same to false
try
set same to ((id of current track) is "{id}")
end try
if same then
set player position to 0
play
else
play track "{id}"
end if
end tell"#)
}

#[cfg(test)]
mod play_tests {
    #[test]
    fn the_song_already_there_is_started_over_not_played_again() {
        let s = super::play_script("spotify:track:3JvKfv6T31zO0ini8iNItO");
        assert!(s.contains(r#"(id of current track) is "spotify:track:3JvKfv6T31zO0ini8iNItO""#));
        assert!(s.contains("set player position to 0\nplay\nelse\nplay track"));
    }
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
