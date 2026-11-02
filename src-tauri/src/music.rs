//! Apple Music (the Music app): what's playing and where, and playback control; how it's asked is the platform's
//! (AppleScript on macOS: platform/mac/music.rs; the system media session on Windows: platform/windows/media.rs).
//! Album covers come from Apple's public iTunes Search (only the artist and title are sent).
use std::collections::HashMap;
use std::sync::Mutex;

use crate::source::State;

/// Track ids: "music:" and the track's persistent ID.
pub const ID_PREFIX: &str = "music:";

pub fn running() -> bool {
    crate::source::app_running("com.apple.Music")
}

/// What Music is playing, with its cover. Asking a running Music app never launches it.
pub fn state() -> Option<State> {
    if !running() {
        return None;
    }
    let mut s = crate::platform::music::state()?;
    let cover = cover_cached(&s.track.id, &s.track.artist, &s.track.name);
    s.track.art = cover.as_ref().map(|c| c.art.clone());
    s.track.link = cover.and_then(|c| c.link);
    Some(s)
}

/// "restart", "next", "previous", "playpause", "pause" or "play".
pub fn control(action: &str) {
    crate::platform::music::control(action);
}

/// Jump to a position in the current track (seconds).
pub fn seek(seconds: f64) {
    crate::platform::music::seek(seconds);
}

/// Play a track from your library by its id; false when Music didn't (on Windows never: the media session can't
/// start a given track).
pub fn play_track(id: &str) -> bool {
    crate::platform::music::play_track(id)
}

/// Covers found so far (None: being looked up, or none found), so each track is looked up once.
static COVERS: Mutex<Option<HashMap<String, Option<Cover>>>> = Mutex::new(None);

/// The cover for a playing track if already found; otherwise starts looking it up in the background (the
/// state is read every 0.4 s and must not wait on the network).
pub(crate) fn cover_cached(id: &str, artist: &str, name: &str) -> Option<Cover> {
    let mut covers = COVERS.lock().unwrap();
    let covers = covers.get_or_insert_with(HashMap::new);
    if let Some(c) = covers.get(id) {
        return c.clone();
    }
    covers.insert(id.to_string(), None);
    let (id, artist, name) = (id.to_string(), artist.to_string(), name.to_string());
    std::thread::spawn(move || {
        let art = cover_lookup(&artist, &name);
        if let Some(c) = COVERS.lock().unwrap().as_mut() {
            c.insert(id, art);
        }
    });
    None
}

/// An album cover, and the Apple Music page of the song it's for (shown linked to it, as Apple's terms ask).
#[derive(Debug, Clone, PartialEq)]
pub struct Cover {
    pub art: String,
    pub link: Option<String>,
}

/// An album cover from the iTunes Search API (no login): the best match for this artist and title.
pub fn cover_lookup(artist: &str, name: &str) -> Option<Cover> {
    let url = search_url(artist, name, 10, crate::platform::sys::region().as_deref());
    let v: serde_json::Value = ureq::get(&url).timeout(std::time::Duration::from_secs(6)).call().ok()?.into_json().ok()?;
    let hits = v["results"].as_array()?;
    let same = |a: &serde_json::Value, b: &str| a.as_str().is_some_and(|a| a.eq_ignore_ascii_case(b));
    let hit = hits.iter().find(|h| same(&h["trackName"], name) && same(&h["artistName"], artist))
        .or_else(|| hits.iter().find(|h| same(&h["artistName"], artist)))?;
    cover_of(hit)
}

/// An iTunes Search for a song, in the store of `region` (two letters, e.g. "jp": Mandarin or Japanese songs may be in
/// no other store, so get no cover or link from the US one); no region, or not a store code: the US store, as before.
fn search_url(artist: &str, title: &str, limit: u32, region: Option<&str>) -> String {
    let country = region.filter(|c| c.len() == 2 && c.chars().all(|c| c.is_ascii_alphabetic()))
        .map(|c| format!("&country={}", c.to_ascii_lowercase())).unwrap_or_default();
    format!("https://itunes.apple.com/search?media=music&entity=song&limit={limit}{country}&term={}",
            urlencoding::encode(&format!("{artist} {title}")))
}

/// The cover and Apple Music page in an iTunes Search result.
fn cover_of(hit: &serde_json::Value) -> Option<Cover> {
    let art = hit["artworkUrl100"].as_str()?.replace("100x100bb", "600x600bb");
    Some(Cover { art, link: hit["trackViewUrl"].as_str().and_then(apple_music_link) })
}

/// An Apple Music page link, if `url` is one (iTunes' tracking parameter dropped).
pub fn apple_music_link(url: &str) -> Option<String> {
    url.starts_with("https://music.apple.com/").then(|| url.replace("&uo=4", "").replace("?uo=4", ""))
}

/// `page` (an Apple Music page) in the store of `region` (two letters, e.g. "gb"): iTunes Search answers with US-store
/// pages, and a song's id is the same in every store. A page with no store in its path, or no region, is as is.
pub fn in_store(page: &str, region: Option<&str>) -> String {
    let Some(cc) = region.filter(|c| c.len() == 2 && c.chars().all(|c| c.is_ascii_alphabetic())) else { return page.to_string() };
    let Some(rest) = page.strip_prefix("https://music.apple.com/") else { return page.to_string() };
    match rest.split_once('/') {
        Some((store, tail)) if store.len() == 2 && store.chars().all(|c| c.is_ascii_alphabetic()) =>
            format!("https://music.apple.com/{}/{tail}", cc.to_ascii_lowercase()),
        _ => page.to_string(),
    }
}

/// How an Apple Music page opens: the Music app first (music://, built into macOS), then the web page, both in the
/// Mac's own store.
pub fn apple_music_opens(page: &str, region: Option<&str>) -> [String; 2] {
    let page = in_store(page, region);
    [page.replacen("https://", "music://", 1), page]
}

/// The starter songs' covers (the empty states' "Try these" row): each song is looked up on iTunes Search once,
/// and its cover kept in the app's data folder (starter-covers.json: the Apple Music page and the image itself, a
/// small JPEG), so later shows need no request at all. A song with no exact match is asked again after a week; a
/// failed request (offline) next time. Look-ups are one at a time and spaced, as with LRCLIB.
const STARTER_FILE: &str = "starter-covers.json";
const STARTER_SPACING: std::time::Duration = std::time::Duration::from_millis(1500);
const STARTER_RETRY_SECS: u64 = 7 * 24 * 3600;
/// The image kept: 2x the card's 44 px cover (a few KB).
const STARTER_PX: u32 = 96;

/// Only one starter look-up at a time, and when the last one was (for the spacing).
static STARTER_LOCK: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// The cover for a starter song: from the cache, or looked up (exact artist and title, songs only) and kept.
/// `art` is the kept image as a data: URL (or Apple's image address if it couldn't be fetched).
pub fn starter_cover(data_dir: &std::path::Path, artist: &str, title: &str) -> Option<Cover> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut last = STARTER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    starter_cover_with(data_dir, artist, title, now, |a, t| {
        if let Some(wait) = last.and_then(|l| STARTER_SPACING.checked_sub(l.elapsed())) {
            std::thread::sleep(wait);
        }
        *last = Some(std::time::Instant::now());
        starter_lookup(a, t)
    }, fetch_image)
}

/// `starter_cover` with the network passed in (for the tests): `lookup` is Err when the request failed (not
/// remembered), Ok(None) when there's no exact match (remembered for a week).
fn starter_cover_with(dir: &std::path::Path, artist: &str, title: &str, now: u64,
                      lookup: impl FnOnce(&str, &str) -> Result<Option<Cover>, ()>,
                      image: impl FnOnce(&str) -> Option<Vec<u8>>) -> Option<Cover> {
    let path = dir.join(STARTER_FILE);
    let mut cache: serde_json::Map<String, serde_json::Value> = std::fs::read(&path).ok()
        .and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let key = format!("{}\n{}", artist.to_lowercase(), title.to_lowercase());
    if let Some(e) = cache.get(&key) {
        if let Some(art) = e["image"].as_str().or(e["art"].as_str()) {
            return Some(Cover { art: art.to_string(), link: e["link"].as_str().map(String::from) });
        }
        if e["missing"].as_u64().is_some_and(|t| now.saturating_sub(t) < STARTER_RETRY_SECS) {
            return None;
        }
    }
    let entry = match lookup(artist, title) {
        Err(()) => return None,
        Ok(None) => serde_json::json!({"missing": now}),
        Ok(Some(c)) => {
            let img = image(&sized_art(&c.art, STARTER_PX)).map(|b| format!("data:image/jpeg;base64,{}", base64(&b)));
            serde_json::json!({"art": c.art, "link": c.link, "image": img})
        }
    };
    cache.insert(key, entry.clone());
    let _ = std::fs::create_dir_all(dir);
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec(&cache).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
    let art = entry["image"].as_str().or(entry["art"].as_str())?;
    Some(Cover { art: art.to_string(), link: entry["link"].as_str().map(String::from) })
}

/// iTunes Search for exactly this song: Err if the request failed, Ok(None) if no result matches.
fn starter_lookup(artist: &str, title: &str) -> Result<Option<Cover>, ()> {
    let url = search_url(artist, title, 25, crate::platform::sys::region().as_deref());
    let v: serde_json::Value = ureq::get(&url).timeout(std::time::Duration::from_secs(8)).call().map_err(|_| ())?
        .into_json().map_err(|_| ())?;
    Ok(starter_match(v["results"].as_array().map(Vec::as_slice).unwrap_or_default(), artist, title).and_then(cover_of))
}

/// The first result that is a song with exactly this artist and title (ignoring case): no live takes, remixes,
/// covers by others or karaoke versions.
fn starter_match<'a>(hits: &'a [serde_json::Value], artist: &str, title: &str) -> Option<&'a serde_json::Value> {
    let same = |a: &serde_json::Value, b: &str| a.as_str().is_some_and(|a| a.trim().eq_ignore_ascii_case(b.trim()));
    hits.iter().find(|h| h["kind"] == "song" && same(&h["trackName"], title) && same(&h["artistName"], artist))
}

/// Apple's image address at another size (".../600x600bb.jpg" names its size).
fn sized_art(art: &str, px: u32) -> String {
    match art.rsplit_once('/') {
        Some((base, file)) if file.ends_with("bb.jpg") && file.split('x').count() == 2 => format!("{base}/{px}x{px}bb.jpg"),
        _ => art.to_string(),
    }
}

/// A cover image from Apple's image server (a JPEG, at most 256 KB).
fn fetch_image(url: &str) -> Option<Vec<u8>> {
    if !url.strip_prefix("https://").and_then(|r| r.split('/').next()).is_some_and(|h| h.ends_with(".mzstatic.com")) {
        return None;
    }
    let r = ureq::get(url).timeout(std::time::Duration::from_secs(8)).call().ok()?;
    if r.content_type() != "image/jpeg" {
        return None;
    }
    let mut b = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(r.into_reader(), 256 * 1024 + 1), &mut b).ok()?;
    (b.len() <= 256 * 1024 && b.starts_with(&[0xFF, 0xD8])).then_some(b)
}

/// Standard base64 (for the kept image's data: URL).
fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

/// Spotify's search for a song (the window's "Open in Spotify": a plain link, no API).
pub fn is_spotify_search(url: &str) -> bool {
    url.strip_prefix("https://open.spotify.com/search/").is_some_and(|q| !q.is_empty() && !q.contains(['/', '?', '#']))
}

/// A public Spotify playlist (the empty states' "OnKey starter playlist"): open.spotify.com/playlist/<id>.
pub fn is_spotify_playlist(url: &str) -> bool {
    url.strip_prefix("https://open.spotify.com/playlist/")
        .map(|q| q.split('?').next().unwrap_or(""))
        .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// The Spotify app's own link for an open.spotify.com search or playlist link (spotify:search:…, spotify:playlist:…).
pub fn spotify_app_link(url: &str) -> Option<String> {
    if is_spotify_search(url) {
        return url.strip_prefix("https://open.spotify.com/search/").map(|q| format!("spotify:search:{q}"));
    }
    if is_spotify_playlist(url) {
        let id = url.strip_prefix("https://open.spotify.com/playlist/")?.split('?').next()?;
        return Some(format!("spotify:playlist:{id}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_music_pages_open_in_the_app_in_your_own_store() {
        let us = "https://music.apple.com/us/album/wonderwall/1?i=2";
        assert_eq!(in_store(us, Some("gb")), "https://music.apple.com/gb/album/wonderwall/1?i=2");
        assert_eq!(in_store(us, Some("GB")), "https://music.apple.com/gb/album/wonderwall/1?i=2");
        assert_eq!(in_store(us, None), us, "no region: as it is");
        assert_eq!(in_store(us, Some("gbr")), us, "not a store code");
        assert_eq!(in_store("https://music.apple.com/album/x/1", Some("gb")), "https://music.apple.com/album/x/1", "no store in the path");
        assert_eq!(apple_music_opens(us, Some("de")),
                   ["music://music.apple.com/de/album/wonderwall/1?i=2".to_string(), "https://music.apple.com/de/album/wonderwall/1?i=2".to_string()]);
    }

    #[test]
    fn itunes_is_searched_in_your_own_store() {
        assert_eq!(search_url("周杰伦", "晴天", 10, Some("CN")),
                   "https://itunes.apple.com/search?media=music&entity=song&limit=10&country=cn&term=%E5%91%A8%E6%9D%B0%E4%BC%A6%20%E6%99%B4%E5%A4%A9");
        assert_eq!(search_url("Coldplay", "Yellow", 25, None),
                   "https://itunes.apple.com/search?media=music&entity=song&limit=25&term=Coldplay%20Yellow", "no region: the US store");
        for bad in ["001", "g", "g&b", ""] {
            assert!(!search_url("a", "b", 10, Some(bad)).contains("country"), "{bad}");
        }
    }

    #[test]
    fn spotify_links_open_the_app() {
        assert_eq!(spotify_app_link("https://open.spotify.com/search/Oasis%20Wonderwall").as_deref(), Some("spotify:search:Oasis%20Wonderwall"));
        assert_eq!(spotify_app_link("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc").as_deref(), Some("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"));
        assert_eq!(spotify_app_link("https://music.apple.com/us/album/x/1"), None);
    }

    #[test]
    fn itunes_result_gives_cover_and_apple_music_page() {
        let hit = serde_json::json!({"artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb/a/100x100bb.jpg",
                                     "trackViewUrl": "https://music.apple.com/us/album/x/123?i=456&uo=4"});
        assert_eq!(cover_of(&hit), Some(Cover { art: "https://is1-ssl.mzstatic.com/image/thumb/a/600x600bb.jpg".into(),
                                                link: Some("https://music.apple.com/us/album/x/123?i=456".into()) }));
        assert_eq!(apple_music_link("https://example.com/music.apple.com/"), None);
        assert_eq!(apple_music_link("https://music.apple.com/us/song/x/1?uo=4").as_deref(), Some("https://music.apple.com/us/song/x/1"));
    }

    #[test]
    fn starter_cover_is_the_exact_song() {
        let hits: Vec<serde_json::Value> = serde_json::from_value(serde_json::json!([
            {"kind": "song", "artistName": "Oasis", "trackName": "Wonderwall (Remastered)", "artworkUrl100": "https://a.mzstatic.com/x/100x100bb.jpg"},
            {"kind": "music-video", "artistName": "Oasis", "trackName": "Wonderwall", "artworkUrl100": "https://a.mzstatic.com/v/100x100bb.jpg"},
            {"kind": "song", "artistName": "Ryan Adams", "trackName": "Wonderwall", "artworkUrl100": "https://a.mzstatic.com/r/100x100bb.jpg"},
            {"kind": "song", "artistName": "Oasis", "trackName": "wonderwall", "artworkUrl100": "https://a.mzstatic.com/o/100x100bb.jpg",
             "trackViewUrl": "https://music.apple.com/us/album/wonderwall/1?i=2&uo=4"}])).unwrap();
        let hit = starter_match(&hits, "Oasis", "Wonderwall").unwrap();
        assert_eq!(hit["artworkUrl100"], "https://a.mzstatic.com/o/100x100bb.jpg");
        assert!(starter_match(&hits, "Oasis", "Champagne Supernova").is_none());
        assert!(starter_match(&hits[..3], "Oasis", "Wonderwall").is_none(), "no remaster, video or other artist");
        assert_eq!(sized_art("https://a.mzstatic.com/o/600x600bb.jpg", 96), "https://a.mzstatic.com/o/96x96bb.jpg");
        assert_eq!(sized_art("https://a.mzstatic.com/o/cover.png", 96), "https://a.mzstatic.com/o/cover.png");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(&[0xFF, 0xD8, 0xFF, 0xE0]), "/9j/4A==");
    }

    #[test]
    fn starter_covers_are_looked_up_once_and_kept() {
        let dir = std::env::temp_dir().join(format!("onkey-starters-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let found = || Ok(Some(Cover { art: "https://a.mzstatic.com/o/600x600bb.jpg".into(),
                                       link: Some("https://music.apple.com/us/album/x/1?i=2".into()) }));
        // offline: nothing kept, so it's asked again next time
        assert_eq!(starter_cover_with(&dir, "Oasis", "Wonderwall", 100, |_, _| Err(()), |_| None), None);
        // found: the image kept as a data: URL (asked for at the card's size)
        let c = starter_cover_with(&dir, "Oasis", "Wonderwall", 100, |_, _| found(), |u| {
            assert_eq!(u, "https://a.mzstatic.com/o/96x96bb.jpg");
            Some(vec![0xFF, 0xD8, 0xFF])
        }).unwrap();
        assert_eq!(c.art, "data:image/jpeg;base64,/9j/");
        assert_eq!(c.link.as_deref(), Some("https://music.apple.com/us/album/x/1?i=2"));
        // later: from the folder, no request
        let again = starter_cover_with(&dir, "OASIS", "wonderwall", 200, |_, _| panic!("asked again"), |_| panic!("fetched again"));
        assert_eq!(again, Some(c));
        // no image fetched: Apple's address instead
        let c = starter_cover_with(&dir, "Ben E. King", "Stand By Me", 100, |_, _| found(), |_| None).unwrap();
        assert_eq!(c.art, "https://a.mzstatic.com/o/600x600bb.jpg");
        // not found: remembered for a week, then asked again
        assert_eq!(starter_cover_with(&dir, "Nobody", "Nothing", 100, |_, _| Ok(None), |_| None), None);
        assert_eq!(starter_cover_with(&dir, "Nobody", "Nothing", 100 + 3600, |_, _| panic!("asked again"), |_| None), None);
        let mut asked = false;
        starter_cover_with(&dir, "Nobody", "Nothing", 100 + STARTER_RETRY_SECS, |_, _| { asked = true; Ok(None) }, |_| None);
        assert!(asked);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_spotify_searches_open() {
        assert!(is_spotify_search("https://open.spotify.com/search/Coldplay%20Yellow"));
        assert!(!is_spotify_search("https://open.spotify.com/search/"));
        assert!(!is_spotify_search("https://open.spotify.com/track/abc"));
        assert!(is_spotify_playlist("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc"));
        assert!(!is_spotify_playlist("https://open.spotify.com/playlist/"));
        assert!(!is_spotify_playlist("https://open.spotify.com/playlist/a/../track"));
        assert!(!is_spotify_playlist("https://example.com/open.spotify.com/playlist/abc"));
        assert!(!is_spotify_search("https://open.spotify.com/search/x/../../account"));
        assert!(!is_spotify_search("https://evil.example/https://open.spotify.com/search/x"));
    }
}
