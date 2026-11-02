//! Spotify through its AppleScript interface: what's playing (with Spotify's own track id and cover), its volume,
//! playback control, and playing a track by its id.
use super::applescript;
use crate::source::{Source, State, Track};

fn osa(script: &str) -> Option<String> {
    applescript::osa("Spotify", script)
}

/// What Spotify is playing (fields separated by a character that can't appear in titles). None: no answer, or an
/// answer whose id is stale (spotify.rs `fresh`).
pub fn state() -> Option<State> {
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
    if !crate::spotify::fresh(f[1], f[2], duration) {
        return None;
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

/// Spotify's own volume (0-100). Asks Spotify (an Apple event): only when it's running, never on the engine's thread.
pub fn volume() -> Option<i32> {
    osa(r#"tell application "Spotify" to sound volume"#).and_then(|v| v.trim().parse::<i32>().ok())
}

/// "restart", "next", "previous", "playpause", "pause" or "play".
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

/// The position `state` gives is Spotify's own: always.
pub fn exact_position() -> bool {
    true
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
    // Another song is paused first, a moment before the new one is asked for: a cleaner switch (Daniel's request,
    // 1 Nov, against the double start some starts have; not proven the cause, harmless if it isn't).
    let done = osa(&play_script(id)).is_some();
    if !done {
        crate::engine::log(&format!("Spotify didn't play {id} (not installed, or it didn't answer)"));
    }
    done
}

/// (Spotify names its own ids here: nothing to learn when one is about to be opened.)
pub fn opening(_id: &str, _track: impl FnOnce() -> Option<Track>) {}

/// (Nothing kept: see `opening`.)
pub fn set_data_dir(_dir: &std::path::Path) {}

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
if player state is playing then pause
delay 0.15
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
        assert!(s.contains("set player position to 0\nplay\nelse\n"));
    }

    #[test]
    fn another_song_is_paused_a_moment_before_the_new_one_is_asked_for() {
        let s = super::play_script("spotify:track:3JvKfv6T31zO0ini8iNItO");
        assert!(s.contains("else\nif player state is playing then pause\ndelay 0.15\nplay track \"spotify:track:3JvKfv6T31zO0ini8iNItO\""));
    }
}
