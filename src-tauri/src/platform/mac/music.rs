//! Apple Music (the Music app) through its AppleScript interface: what's playing and where (by the track's
//! persistent ID), playback control, and playing a track from your library.
use super::applescript;
use crate::music::ID_PREFIX;
use crate::source::{Source, State, Track};

fn osa(script: &str) -> Option<String> {
    applescript::osa("Apple Music", script)
}

/// What Music is playing (fields separated by a character that can't appear in titles; its cover: music.rs).
/// Asking a running Music app never launches it; a stopped player (nothing loaded) is None.
pub fn state() -> Option<State> {
    let out = osa(r#"tell application "Music"
        set d to ASCII character 31
        set s to player state as string
        if s is "stopped" then return s
        return s & d & (persistent ID of current track) & d & (name of current track) & d & ¬
            (artist of current track) & d & (album of current track) & d & (duration of current track) & d & (player position)
    end tell"#)?;
    let f: Vec<&str> = out.split('\u{1f}').collect();
    if f.len() < 7 || f[1].is_empty() {
        return None;
    }
    Some(State {
        playing: f[0] == "playing",
        track: Track {
            id: format!("{ID_PREFIX}{}", f[1]),
            name: f[2].to_string(),
            artist: f[3].to_string(),
            album: f[4].to_string(),
            duration: f[5].replace(',', ".").parse().unwrap_or(0.0),
            art: None,
            link: None,
        },
        position: f[6].replace(',', ".").parse().unwrap_or(0.0),
        source: Source::Music,
    })
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
    osa(&format!(r#"tell application "Music" to {cmd}"#));
}

/// The position `state` gives is Music's own: always.
pub fn exact_position() -> bool {
    true
}

/// Jump to a position in the current track (seconds).
pub fn seek(seconds: f64) {
    osa(&format!("tell application \"Music\" to set player position to {:.2}", seconds.max(0.0)));
}

/// Play a track from your library by its id (tracks only streamed, not added to the library, can't be found);
/// false when Music didn't (not found, refused, or no answer). Music is started if it isn't running.
pub fn play_track(id: &str) -> bool {
    let Some(pid) = id.strip_prefix(ID_PREFIX) else { return false };
    if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let done = osa(&format!(r#"tell application "Music" to play (some track of library playlist 1 whose persistent ID is "{pid}")"#)).is_some();
    if !done {
        crate::engine::log(&format!("Apple Music didn't play {id} (not in your library, or Music didn't answer)"));
    }
    done
}
