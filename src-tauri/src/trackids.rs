//! Spotify's own track ids where the app doesn't say them. On Windows the system media session names only the song
//! (title, artist, album), so a song's id there is made from its names (platform/windows/media.rs: "spotify:media:…",
//! and "music:media-…" for Apple Music). Those are OnKey's own and never given to an app to open or play.
//!
//! The real id is learned without Spotify's Web API: when OnKey opens a song in Spotify by its real id (a song learned
//! on a Mac, say), that id is expected for a short while; when the media session then names a song with the same
//! artist and title (`store::same_title`), it's that id, and the made-up id is mapped to it from then on (kept in
//! spotify-ids.json in the app's data folder). Used only on Windows; the Mac asks Spotify for its id.
#![cfg_attr(not(windows), allow(dead_code))]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::source::Track;

/// Ids made from a song's names (media.rs), not an app's own.
pub const SPOTIFY_MEDIA: &str = "spotify:media:";

/// How long an opened track id is expected for: Spotify may have to start first.
const PENDING: Duration = Duration::from_secs(30);

/// The file the learned ids are kept in (made-up id → Spotify's).
const FILE: &str = "spotify-ids.json";

/// Spotify's own id for a track ("spotify:track:" and letters and digits).
pub fn is_spotify_track(id: &str) -> bool {
    id.strip_prefix("spotify:track:").is_some_and(|t| !t.is_empty() && t.len() <= 64 && t.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// An id made from a song's names (Windows' media session), which no app knows: Spotify's, Apple Music's, and a
/// browser's ("youtube:media-…", no video id: platform/windows/browsers.rs).
pub fn is_media_id(id: &str) -> bool {
    id.starts_with(SPOTIFY_MEDIA) || [crate::music::ID_PREFIX, crate::youtube::ID_PREFIX].iter()
        .any(|p| id.strip_prefix(p).is_some_and(|r| r.starts_with("media-")))
}

/// What opens a song in Spotify: its track by Spotify's own id, else a search for it there. A made-up id is never used.
pub fn spotify_uri(id: Option<&str>, artist: &str, title: &str) -> String {
    match id.filter(|i| is_spotify_track(i)) {
        Some(id) => id.to_string(),
        None => format!("spotify:search:{}", urlencoding::encode(&format!("{artist} {title}"))),
    }
}

/// The web page for an app link, for when the app isn't there to open it: Spotify's track or search, Apple Music's page.
pub fn web_link(uri: &str) -> Option<String> {
    if let Some(t) = uri.strip_prefix("spotify:track:") {
        return Some(format!("https://open.spotify.com/track/{t}"));
    }
    if let Some(q) = uri.strip_prefix("spotify:search:") {
        return Some(format!("https://open.spotify.com/search/{q}"));
    }
    if let Some(rest) = uri.strip_prefix("music://") {
        return Some(if rest.is_empty() { "https://music.apple.com/".to_string() } else { format!("https://{rest}") });
    }
    None
}

/// Track ids being learned: those just opened, and those learned (made-up id → Spotify's), kept in `path`.
#[derive(Default)]
pub struct Learner {
    pending: Vec<(String, Track, Instant)>,
    known: Option<HashMap<String, String>>,
    path: Option<PathBuf>,
}

impl Learner {
    pub fn new(dir: Option<&Path>) -> Learner {
        Learner { path: dir.map(|d| d.join(FILE)), ..Default::default() }
    }

    /// Spotify's track `id` (for `artist` and `title`) was just opened.
    pub fn expect(&mut self, id: &str, artist: &str, title: &str, now: Instant) {
        if !is_spotify_track(id) {
            return;
        }
        self.pending.retain(|(i, _, at)| i != id && now.duration_since(*at) < PENDING);
        let t = Track { id: id.to_string(), artist: artist.to_string(), name: title.to_string(), ..Default::default() };
        self.pending.push((id.to_string(), t, now));
    }

    /// Spotify's own id for the song the media session names (`made`: its made-up id), if known: one opened just now
    /// for the same artist and title (then kept), else one learned before.
    pub fn resolve(&mut self, made: &str, artist: &str, title: &str, now: Instant) -> Option<String> {
        self.pending.retain(|(_, _, at)| now.duration_since(*at) < PENDING);
        let heard = Track { id: made.to_string(), artist: artist.to_string(), name: title.to_string(), ..Default::default() };
        if let Some(k) = self.pending.iter().rposition(|(_, t, _)| crate::store::same_title(t, &heard)) {
            let id = self.pending[k].0.clone();
            self.pending.retain(|(_, t, _)| !crate::store::same_title(t, &heard));
            self.learn(made, &id);
            return Some(id);
        }
        self.known().get(made).cloned()
    }

    fn known(&mut self) -> &mut HashMap<String, String> {
        let path = self.path.clone();
        self.known.get_or_insert_with(|| path.and_then(|p| crate::store::read_kept(&p)).unwrap_or_default())
    }

    /// `made` is Spotify's `id` from now on (written to the file when it's new).
    fn learn(&mut self, made: &str, id: &str) {
        if self.known().get(made).map(String::as_str) == Some(id) {
            return;
        }
        self.known().insert(made.to_string(), id.to_string());
        let Some(path) = self.path.clone() else { return };
        // (read afresh: the file isn't written over when it couldn't be read)
        let Some(mut kept) = crate::store::read_kept::<HashMap<String, String>>(&path) else {
            crate::store::log_unreadable(&path);
            return;
        };
        kept.insert(made.to_string(), id.to_string());
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = crate::store::write_atomic(&path, serde_json::to_string(&kept).unwrap_or_default().as_bytes());
    }
}

static LEARNER: Mutex<Option<Learner>> = Mutex::new(None);

/// Where the learned ids are kept (the app's data folder), once at start-up.
pub fn set_data_dir(dir: &Path) {
    *LEARNER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Learner::new(Some(dir)));
}

fn with<R>(f: impl FnOnce(&mut Learner) -> R) -> R {
    let mut l = LEARNER.lock().unwrap_or_else(|e| e.into_inner());
    f(l.get_or_insert_with(|| Learner::new(None)))
}

/// Spotify's track `id` for this song is being opened (see `Learner::expect`).
pub fn expect(id: &str, artist: &str, title: &str) {
    with(|l| l.expect(id, artist, title, Instant::now()));
}

/// Spotify's own id for the song the media session names, if known (see `Learner::resolve`).
pub fn resolve(made: &str, artist: &str, title: &str) -> Option<String> {
    with(|l| l.resolve(made, artist, title, Instant::now()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = "spotify:track:48UPSzbZjgc449aqz8bxox";
    const MADE: &str = "spotify:media:0123456789abcdef";

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("onkey-trackids-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn only_spotifys_own_ids_are_opened() {
        assert!(is_spotify_track(REAL));
        assert!(!is_spotify_track(MADE));
        assert!(!is_spotify_track("spotify:track:"));
        assert!(!is_spotify_track("spotify:track:abc def"));
        assert!(is_media_id(MADE) && is_media_id("music:media-0123456789abcdef"));
        assert!(!is_media_id(REAL) && !is_media_id("music:ABC123"));
        assert_eq!(spotify_uri(Some(REAL), "RHCP", "Californication"), REAL);
        assert_eq!(spotify_uri(Some(MADE), "Red Hot Chili Peppers", "Californication"),
                   "spotify:search:Red%20Hot%20Chili%20Peppers%20Californication");
        assert_eq!(spotify_uri(None, "Oasis", "Wonderwall"), "spotify:search:Oasis%20Wonderwall");
    }

    #[test]
    fn app_links_have_web_pages() {
        assert_eq!(web_link(REAL).as_deref(), Some("https://open.spotify.com/track/48UPSzbZjgc449aqz8bxox"));
        assert_eq!(web_link("spotify:search:Oasis%20Wonderwall").as_deref(), Some("https://open.spotify.com/search/Oasis%20Wonderwall"));
        assert_eq!(web_link("music://music.apple.com/gb/album/x/1?i=2").as_deref(), Some("https://music.apple.com/gb/album/x/1?i=2"));
        assert_eq!(web_link("music://").as_deref(), Some("https://music.apple.com/"));
        assert_eq!(web_link("https://www.youtube.com/watch?v=abc"), None);
    }

    #[test]
    fn an_opened_id_is_learned_for_the_same_song_and_kept() {
        let d = dir("learn");
        let t0 = Instant::now();
        let mut l = Learner::new(Some(&d));
        assert_eq!(l.resolve(MADE, "Red Hot Chili Peppers", "Californication", t0), None, "nothing opened yet");
        l.expect(REAL, "Red Hot Chili Peppers", "Californication", t0);
        // another song first (the one before, still playing): not it
        assert_eq!(l.resolve("spotify:media:1111111111111111", "Oasis", "Wonderwall", t0 + Duration::from_secs(1)), None);
        // the same song, named a little differently
        assert_eq!(l.resolve(MADE, "red hot chili peppers", "Californication!", t0 + Duration::from_secs(2)).as_deref(), Some(REAL));
        // from then on, and after a restart (from the file)
        assert_eq!(l.resolve(MADE, "Red Hot Chili Peppers", "Californication", t0 + Duration::from_secs(600)).as_deref(), Some(REAL));
        let mut again = Learner::new(Some(&d));
        assert_eq!(again.resolve(MADE, "Red Hot Chili Peppers", "Californication", t0).as_deref(), Some(REAL));
        assert!(std::fs::read_dir(&d).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(".tmp-")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_opened_id_is_expected_only_for_a_while() {
        let t0 = Instant::now();
        let mut l = Learner::new(None);
        l.expect(REAL, "Red Hot Chili Peppers", "Californication", t0);
        assert_eq!(l.resolve(MADE, "Red Hot Chili Peppers", "Californication", t0 + PENDING + Duration::from_secs(1)), None);
        // a made-up id is never expected
        l.expect(MADE, "Red Hot Chili Peppers", "Californication", t0);
        assert_eq!(l.resolve("spotify:media:2222222222222222", "Red Hot Chili Peppers", "Californication", t0), None);
    }

    #[test]
    fn the_latest_id_opened_for_a_song_wins() {
        let t0 = Instant::now();
        let mut l = Learner::new(None);
        l.expect("spotify:track:old", "Oasis", "Wonderwall", t0);
        l.expect("spotify:track:new", "Oasis", "Wonderwall", t0 + Duration::from_secs(1));
        assert_eq!(l.resolve(MADE, "Oasis", "Wonderwall", t0 + Duration::from_secs(2)).as_deref(), Some("spotify:track:new"));
    }
}
