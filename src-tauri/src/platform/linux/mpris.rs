//! The music apps on Linux through MPRIS, the media player interface every Linux player publishes on the session
//! D-Bus: what Spotify plays and where, and its controls (`spotify` below); the browsers' players too (`players`,
//! `read`: browsers.rs). What AppleScript does on macOS (platform/mac) and the system media session on Windows,
//! with no permission to ask for.
//!
//! What Spotify's MPRIS player gives: its own track id (mpris:trackid, or the track's open.spotify.com address), the
//! title, artists, album, length and cover, the position (Position, in microseconds), and play, pause, next,
//! previous, seek and "open this spotify: link" (OpenUri). There's no Apple Music app on Linux: `music::state` is
//! always None.
//!
//! Not yet checked against a real Spotify on Linux: whether its Position is current (some older clients always said
//! 0; the log says what it gave, once per song), and whether OpenUri plays the track or only shows it.
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const ROOT: &str = "org.mpris.MediaPlayer2";

/// The session bus, connected once (None: there's none, logged once). A player that doesn't answer within a second
/// isn't waited for.
fn bus() -> Option<&'static Connection> {
    static BUS: OnceLock<Option<Connection>> = OnceLock::new();
    BUS.get_or_init(|| {
        match zbus::blocking::connection::Builder::session().and_then(|b| b.method_timeout(Duration::from_secs(1)).build()) {
            Ok(c) => Some(c),
            Err(e) => {
                crate::engine::log(&format!("MPRIS: no session D-Bus ({e}): the music apps and browsers can't be followed"));
                None
            }
        }
    }).as_ref()
}

fn proxy(dest: &str, iface: &'static str) -> Option<Proxy<'static>> {
    zbus::blocking::proxy::Builder::new(bus()?).destination(dest.to_string()).ok()?.path(PATH).ok()?.interface(iface).ok()?
        .cache_properties(zbus::proxy::CacheProperties::No).build().ok()
}

/// Every MPRIS player on the bus now, by its bus name ("org.mpris.MediaPlayer2.spotify").
pub fn players() -> Vec<String> {
    let Some(c) = bus() else { return Vec::new() };
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(c) else { return Vec::new() };
    let mut names: Vec<String> = dbus.list_names().map(|n| n.into_iter().map(|n| n.to_string()).collect()).unwrap_or_default();
    names.retain(|n| n.starts_with(PREFIX));
    names.sort();
    names
}

/// The process that owns a bus name (its pid), if the bus says.
pub fn pid_of(name: &str) -> Option<u32> {
    let dbus = zbus::blocking::fdo::DBusProxy::new(bus()?).ok()?;
    dbus.get_connection_unix_process_id(zbus::names::BusName::try_from(name).ok()?).ok()
}

/// How the program behind a bus name is installed, for a tester's log: "snap", "flatpak" (its bus goes through
/// xdg-dbus-proxy) or "native", by its pid's program.
pub fn install_kind(name: &str) -> &'static str {
    let Some(pid) = pid_of(name) else { return "unknown" };
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    if exe.starts_with("/snap/") {
        "snap"
    } else if exe.ends_with("xdg-dbus-proxy") || std::path::Path::new(&format!("/proc/{pid}/root/.flatpak-info")).exists() {
        "flatpak"
    } else if exe.is_empty() {
        "unknown"
    } else {
        "native"
    }
}

/// One player as it says it, nothing worked out.
#[derive(Debug, Clone, Default)]
pub struct Said {
    /// its bus name ("org.mpris.MediaPlayer2.chromium.instance1234")
    pub app: String,
    /// what it calls itself ("Google Chrome", "Mozilla Firefox", "Spotify") and its .desktop file's name
    pub identity: String,
    pub desktop: String,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// seconds; 0 when it doesn't say
    pub duration: f64,
    /// seconds into it
    pub position: f64,
    /// it takes a new position (SetPosition)
    pub can_seek: bool,
    /// mpris:trackid (an object path), xesam:url and mpris:artUrl, as given ("" when not)
    pub trackid: String,
    pub url: String,
    pub art: String,
}

fn text(md: &HashMap<String, OwnedValue>, key: &str) -> String {
    md.get(key).and_then(|v| String::try_from(v.try_clone().ok()?).ok()).unwrap_or_default()
}

fn texts(md: &HashMap<String, OwnedValue>, key: &str) -> String {
    let Some(v) = md.get(key) else { return String::new() };
    if let Ok(list) = <Vec<String>>::try_from(v.try_clone().unwrap_or_else(|_| OwnedValue::from(0u8))) {
        return list.join(", ");
    }
    text(md, key)
}

/// A number of microseconds (players give it as a signed or unsigned 64-bit integer, some as a double) in seconds.
fn micros(v: Option<&OwnedValue>) -> f64 {
    let Some(v) = v else { return 0.0 };
    let us = i64::try_from(v).map(|x| x as f64).or_else(|_| u64::try_from(v).map(|x| x as f64))
        .or_else(|_| f64::try_from(v)).or_else(|_| i32::try_from(v).map(|x| x as f64)).unwrap_or(0.0);
    (us / 1e6).max(0.0)
}

/// What the player with this bus name says now (None: it isn't there, or names nothing).
pub fn read(app: &str) -> Option<Said> {
    let p = proxy(app, PLAYER)?;
    let md: HashMap<String, OwnedValue> = p.get_property("Metadata").ok()?;
    let title = text(&md, "xesam:title");
    if title.trim().is_empty() {
        return None;
    }
    let status: String = p.get_property("PlaybackStatus").unwrap_or_default();
    let position = p.get_property::<i64>("Position").map(|us| (us as f64 / 1e6).max(0.0)).unwrap_or(0.0);
    let can_seek = p.get_property::<bool>("CanSeek").unwrap_or(false);
    let trackid = md.get("mpris:trackid").and_then(|v| OwnedObjectPath::try_from(v.try_clone().ok()?).ok()).map(|p| p.to_string())
        .unwrap_or_else(|| text(&md, "mpris:trackid"));
    let root = proxy(app, ROOT);
    let identity = root.as_ref().and_then(|r| r.get_property::<String>("Identity").ok()).unwrap_or_default();
    let desktop = root.as_ref().and_then(|r| r.get_property::<String>("DesktopEntry").ok()).unwrap_or_default();
    Some(Said {
        app: app.to_string(), identity, desktop, playing: status == "Playing", title, artist: texts(&md, "xesam:artist"),
        album: text(&md, "xesam:album"), duration: micros(md.get("mpris:length")), position, can_seek, trackid,
        url: text(&md, "xesam:url"), art: text(&md, "mpris:artUrl"),
    })
}

/// Every player that names something now.
pub fn sessions() -> Vec<Said> {
    players().iter().filter_map(|p| read(p)).collect()
}

/// "restart", "next", "previous", "playpause", "pause" or "play" to the player with this bus name.
pub fn control_app(app: &str, action: &str) {
    let Some(p) = proxy(app, PLAYER) else { return };
    let method = match action {
        "restart" => return seek_app(app, 0.0),
        "next" => "Next",
        "previous" => "Previous",
        "playpause" => "PlayPause",
        "pause" => "Pause",
        "play" => "Play",
        _ => return,
    };
    if let Err(e) = p.call::<_, _, ()>(method, &()) {
        crate::engine::log(&format!("MPRIS: {app} didn't take {method}: {e}"));
    }
}

/// Jump to a position (seconds) in what the player with this bus name plays: SetPosition on its current track, else
/// Seek by the difference.
pub fn seek_app(app: &str, seconds: f64) {
    let Some(p) = proxy(app, PLAYER) else { return };
    let us = (seconds.max(0.0) * 1e6) as i64;
    let md: HashMap<String, OwnedValue> = p.get_property("Metadata").unwrap_or_default();
    let track = md.get("mpris:trackid").and_then(|v| OwnedObjectPath::try_from(v.try_clone().ok()?).ok());
    if let Some(t) = track {
        if p.call::<_, _, ()>("SetPosition", &(t, us)).is_ok() {
            return;
        }
    }
    let now = p.get_property::<i64>("Position").unwrap_or(0);
    if let Err(e) = p.call::<_, _, ()>("Seek", &(us - now,)) {
        crate::engine::log(&format!("MPRIS: {app} didn't take a seek: {e}"));
    }
}

/// Spotify's player's bus name, if Spotify is running ("org.mpris.MediaPlayer2.spotify"; a second copy adds
/// ".instance<pid>").
fn spotify_bus() -> Option<String> {
    players().into_iter().find(|n| n[PREFIX.len()..].to_lowercase().starts_with("spotify"))
}

/// Spotify's own id for what it plays, as macOS's AppleScript gives it ("spotify:track:<id>", "spotify:ad:<id>",
/// "spotify:episode:<id>"…), from its trackid ("/com/spotify/<kind>/<id>", or the id itself in older clients) or its
/// open.spotify.com address. Ads and episodes keep their kind, so source.rs `is_song` leaves them out as on the Mac.
/// None when it gives neither.
pub fn spotify_id(trackid: &str, url: &str) -> Option<String> {
    let ok = |kind: &str, id: &str| !kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase())
        && !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '_' || c == '-' || c == '%' || c == '+' || c == '.');
    let split = |rest: &str, sep: char| rest.split_once(sep).map(|(k, i)| (k.to_string(), i.to_string()));
    let from_path = trackid.strip_prefix("/com/spotify/").and_then(|r| split(r, '/'))
        .or_else(|| trackid.strip_prefix("spotify:").and_then(|r| split(r, ':')));
    let from_url = url.split('?').next().unwrap_or("").strip_prefix("https://open.spotify.com/").and_then(|r| split(r, '/'));
    from_path.filter(|(k, i)| ok(k, i)).or(from_url.filter(|(k, i)| ok(k, i))).map(|(k, i)| format!("spotify:{k}:{i}"))
}

/// A stable id for a song a player names but doesn't give an id for (FNV-1a over artist, title and album).
pub fn media_id(prefix: &str, artist: &str, title: &str, album: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in [artist, title, album].join("\u{1f}").to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}{h:016x}")
}

/// Once per song a player names: what it said, for a tester's log.
pub fn said_once(who: &str, s: &Said) {
    static SAID: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
    let key = format!("{}\u{1f}{}\u{1f}{}", s.artist, s.title, s.album);
    let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    match said.iter_mut().find(|(n, _)| n == who) {
        Some((_, k)) if *k == key => return,
        Some((_, k)) => *k = key,
        None => said.push((who.to_string(), key)),
    }
    drop(said);
    crate::engine::log(&format!("MPRIS: {who} ({} \"{}\", desktop \"{}\"): \"{}\" by \"{}\" (album \"{}\"), {}, length {:.1} s, at {:.1} s, seek {}, trackid \"{}\", url \"{}\"",
                                s.app, s.identity, s.desktop, s.title, s.artist, s.album, if s.playing { "playing" } else { "paused" },
                                s.duration, s.position, if s.can_seek { "allowed" } else { "not allowed" }, s.trackid, s.url));
}

/// Spotify through its MPRIS player: Spotify's own ids, its cover, its volume; a track played by its spotify: link.
pub mod spotify {
    use crate::source::{Source, State, Track};

    /// The position worked out (browsers.rs `Clock`: run on from when Spotify's Position last changed, as some
    /// clients only update it on play, pause and seek), and for the log: the song, when it was first seen and the
    /// Position then, and whether the 10 s check has been logged.
    struct Followed {
        clock: crate::platform::linux::browsers::Clock,
        song: String,
        first: Option<(std::time::Instant, f64)>,
        said: bool,
    }

    static FOLLOWED: std::sync::Mutex<Option<Followed>> = std::sync::Mutex::new(None);

    /// The position `state` gives is followed as Spotify's: always, as before (the clock above runs on a Position
    /// that some clients only update on play, pause and seek, so it stays right between them).
    pub fn exact_position() -> bool {
        true
    }

    /// What Spotify's player says it plays.
    pub fn state() -> Option<State> {
        let s = super::read(&super::spotify_bus()?)?;
        super::said_once("Spotify", &s);
        let id = super::spotify_id(&s.trackid, &s.url)
            .unwrap_or_else(|| super::media_id(crate::trackids::SPOTIFY_MEDIA, &s.artist, &s.title, &s.album));
        let now = std::time::Instant::now();
        let mut g = FOLLOWED.lock().unwrap_or_else(|e| e.into_inner());
        let f = g.get_or_insert_with(|| Followed { clock: Default::default(), song: String::new(), first: None, said: false });
        let (pos, _exact) = f.clock.read(&id, &s, now);
        if f.song != id {
            *f = Followed { clock: std::mem::take(&mut f.clock), song: id.clone(), first: s.playing.then_some((now, s.position)), said: false };
        } else if f.first.is_none() && s.playing {
            f.first = Some((now, s.position));
        }
        // ten seconds of playing later, once per song: whether Spotify's own Position moved (a frozen one shows here)
        if let Some((t0, p0)) = f.first.filter(|_| !f.said && s.playing) {
            if now.duration_since(t0).as_secs_f64() >= 10.0 {
                f.said = true;
                crate::engine::log(&format!("MPRIS: Spotify position after {:.0} s playing: Spotify says {:.1} s (was {p0:.1} s), OnKey works out {pos:.1} s{}",
                                            now.duration_since(t0).as_secs_f64(), s.position,
                                            if (s.position - p0).abs() < 1.0 { " (Spotify's Position isn't moving: OnKey runs it on itself)" } else { "" }));
            }
        }
        drop(g);
        let pos = if s.duration > 0.0 { pos.min(s.duration) } else { pos };
        Some(State {
            playing: s.playing,
            track: Track { id, name: s.title, artist: s.artist, album: s.album, duration: s.duration,
                           art: Some(s.art.trim().to_string()).filter(|a| a.starts_with("https://")), link: None },
            position: pos,
            source: Source::Spotify,
        })
    }

    /// Spotify's volume (0-100), as its player says it.
    pub fn volume() -> Option<i32> {
        let p = super::proxy(&super::spotify_bus()?, super::PLAYER)?;
        p.get_property::<f64>("Volume").ok().map(|v| (v * 100.0).round().clamp(0.0, 100.0) as i32)
    }

    pub fn control(action: &str) {
        if crate::source::may_control("Spotify") {
            if let Some(b) = super::spotify_bus() {
                super::control_app(&b, action);
            }
        }
    }

    pub fn seek(seconds: f64) {
        if crate::source::may_control("Spotify") {
            if let Some(b) = super::spotify_bus() {
                super::seek_app(&b, seconds);
            }
        }
    }

    /// Play a track by its id. The song already in Spotify is started over from the top (as on macOS); another is
    /// opened by its link (OpenUri while Spotify runs, else the link opened, which starts Spotify). False when
    /// neither worked.
    pub fn play_track(id: &str) -> bool {
        if !(id.starts_with("spotify:track:") && id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':')) {
            return false;
        }
        if let Some(b) = super::spotify_bus() {
            let same = super::read(&b).and_then(|s| super::spotify_id(&s.trackid, &s.url)).is_some_and(|cur| cur == id);
            if same {
                super::seek_app(&b, 0.0);
                super::control_app(&b, "play");
                return true;
            }
            if let Some(p) = super::proxy(&b, super::PLAYER) {
                if p.call::<_, _, ()>("OpenUri", &(id,)).is_ok() {
                    return true;
                }
            }
        }
        let done = crate::platform::shell::open_app_link(id);
        if !done {
            crate::engine::log(&format!("Spotify didn't open {id} (not running and its link isn't registered, or it didn't answer)"));
        }
        done
    }

    /// (Spotify names its own ids here: nothing to learn when one is about to be opened.)
    pub fn opening(_id: &str, _track: impl FnOnce() -> Option<Track>) {}

    /// (Nothing kept: see `opening`.)
    pub fn set_data_dir(_dir: &std::path::Path) {}
}

/// Apple Music: there's no Music app on Linux.
pub mod music {
    use crate::source::State;

    /// (No Apple Music app on Linux.)
    pub fn state() -> Option<State> {
        None
    }

    pub fn control(_action: &str) {}

    /// (Nothing is followed from it: the question never comes up.)
    pub fn exact_position() -> bool {
        true
    }

    pub fn seek(_seconds: f64) {}

    pub fn play_track(_id: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn spotifys_own_id_is_read_from_its_trackid_or_address() {
        let id = "3JvKfv6T31zO0ini8iNItO";
        assert_eq!(super::spotify_id(&format!("/com/spotify/track/{id}"), ""), Some(format!("spotify:track:{id}")));
        assert_eq!(super::spotify_id(&format!("spotify:track:{id}"), ""), Some(format!("spotify:track:{id}")));
        assert_eq!(super::spotify_id("/org/mpris/MediaPlayer2/TrackList/NoTrack", &format!("https://open.spotify.com/track/{id}?si=x")),
                   Some(format!("spotify:track:{id}")));
        // ads and episodes keep their kind, so the engine's song filter (source.rs is_song) leaves them out
        let ad = super::spotify_id("/com/spotify/ad/4bn2LgcFNq5c2B7DIQXzVd", "").unwrap();
        assert_eq!(ad, "spotify:ad:4bn2LgcFNq5c2B7DIQXzVd");
        assert!(!crate::source::is_song(&ad));
        let ep = super::spotify_id("/com/spotify/episode/512ojhOuo1ktJprKbVcKyQ", "").unwrap();
        assert!(!crate::source::is_song(&ep), "{ep}");
        assert!(!crate::source::is_song(&super::spotify_id("", "https://open.spotify.com/episode/512ojhOuo1ktJprKbVcKyQ").unwrap()));
        assert!(crate::source::is_song(&format!("spotify:track:{id}")));
        assert_eq!(super::spotify_id("spotify:local:a:b:c:200", "").as_deref(), Some("spotify:local:a:b:c:200"));
        assert_eq!(super::spotify_id("/org/mpris/MediaPlayer2/TrackList/NoTrack", ""), None);
    }

    #[test]
    fn a_songs_made_id_is_stable_and_its_own() {
        let a = super::media_id("spotify:media:", "Red Hot Chili Peppers", "Californication", "Californication");
        assert_eq!(a, super::media_id("spotify:media:", "red hot chili peppers", "californication", "Californication"));
        assert_ne!(a, super::media_id("spotify:media:", "Red Hot Chili Peppers", "Otherside", "Californication"));
    }
}
