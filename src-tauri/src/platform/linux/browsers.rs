//! YouTube (and any page that says what it plays) in a web browser on Linux, through the browser's MPRIS player:
//! what youtube.rs does on macOS through AppleScript, and platform/windows/browsers.rs through the media session.
//! Chromium's browsers (Chrome, Chromium, Edge, Brave, Opera, Vivaldi) and Firefox each publish an MPRIS player
//! following the tab that plays, with what the page told it (navigator.mediaSession): YouTube's video title and
//! channel, YouTube Music's artist and title (and album). No address, so no video id: the song's id is made from its
//! names ("youtube:media-…", as on Windows).
//!
//! Chromium's browsers mostly all call their player "chromium.instance<pid>", so the browser is told by what the
//! player calls itself (Identity, DesktopEntry) and by the program that owns it (/proc/<pid>/comm).
//!
//! The position: MPRIS players give it when asked (Position). It's taken as exact once OnKey has seen it change, as
//! on Windows (`Clock`); with no length given, the position is OnKey's own clock from when the song began playing,
//! and listening's alignment (recognize.rs) places it once the song is learned.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use super::mpris::{self, Said};
use crate::source::{Source, State, Track};

/// The browsers, by a word in what their player says of itself (lower case): (that, bundle id OnKey names it by,
/// name). Checked in order: Edge, Brave, Vivaldi and Opera before Chrome, as their players may say "chromium" too.
const BROWSERS: [(&str, &str, &str); 9] = [
    ("msedge", "com.microsoft.edgemac", "Edge"),
    ("microsoft-edge", "com.microsoft.edgemac", "Edge"),
    ("brave", "com.brave.Browser", "Brave"),
    ("vivaldi", "com.vivaldi.Vivaldi", "Vivaldi"),
    ("opera", "com.operasoftware.Opera", "Opera"),
    ("firefox", "org.mozilla.firefox", "Firefox"),
    ("librewolf", "org.mozilla.firefox", "Firefox"),
    ("chrome", "com.google.Chrome", "Chrome"),
    ("chromium", "com.google.Chrome", "Chrome"),
];

/// The bundle ids of the browsers OnKey follows this way (listening hears their sound).
pub fn bundles() -> impl Iterator<Item = &'static str> {
    BROWSERS.iter().map(|b| b.1)
}

/// The browser a player belongs to, from what it says of itself (bus name, Identity, DesktopEntry) and the name of
/// the program that owns it: (bundle id, name). None: not a browser (Spotify, a video player…).
pub fn browser_of(about: &str) -> Option<(&'static str, &'static str)> {
    let a = about.to_lowercase();
    BROWSERS.iter().find(|(k, _, _)| a.contains(k)).map(|(_, b, n)| (*b, *n))
}

/// The session to follow, of `cands` (bus name, playing, when it began playing: s since some time): one playing, the
/// browser listened to first (`prefer`, listening mode), then the one that began playing last; else the one
/// followed so far (paused), else any.
pub fn pick(cands: &[(&str, bool, f64)], prefer: Option<&str>, followed: Option<&str>) -> Option<usize> {
    let playing = |c: &&(&str, bool, f64)| c.1;
    let best = |only: &dyn Fn(&str) -> bool| cands.iter().enumerate().filter(|(_, c)| playing(c) && only(c.0))
        .max_by(|a, b| a.1 .2.total_cmp(&b.1 .2)).map(|(i, _)| i);
    prefer.and_then(|p| best(&|a| a == p)).or_else(|| best(&|_| true))
        .or_else(|| cands.iter().position(|c| Some(c.0) == followed))
        .or((!cands.is_empty()).then_some(0))
}

/// Where a song is, as OnKey works it out from what the player says: its position, and whether that's the
/// browser's (exact) or OnKey's own clock. (Windows' browsers.rs's, but a pause or play that leaves the position
/// where it was carries on from where it had run to, rather than going back to it: Firefox, and some Spotify
/// clients, never move their Position.)
#[derive(Debug, Clone, Default)]
pub struct Clock {
    track: String,
    /// the player's last raw (position, playing), and when OnKey saw it so
    raw: Option<(f64, bool)>,
    at: Option<Instant>,
    /// where the song was at `at` (the raw position, or where it had run to when only playing changed)
    base: f64,
    /// OnKey saw the position change: it's current
    fresh: bool,
    /// playing time since the song began (no length), and when it was last read
    own: f64,
    last: Option<Instant>,
}

impl Clock {
    /// Read `s` (its song `track`) at `now`: (position s, exact).
    pub fn read(&mut self, track: &str, s: &Said, now: Instant) -> (f64, bool) {
        let since = self.last.map_or(0.0, |a| now.duration_since(a).as_secs_f64());
        self.last = Some(now);
        if self.track != track {
            let first = self.track.is_empty();
            *self = Clock { track: track.to_string(), raw: Some((s.position, s.playing)), at: Some(now), base: s.position, fresh: !first, own: 0.0,
                            last: Some(now) };
            return (s.position, s.duration > 0.0 && !first);
        }
        if self.raw.is_some_and(|(_, p)| p) {
            self.own += since;
        }
        if self.raw != Some((s.position, s.playing)) {
            let moved = self.raw.is_some_and(|(p, _)| p != s.position);
            self.base = match self.raw {
                // only playing changed: carry on from where it had run to
                Some((p, was_playing)) if p == s.position => self.base + if was_playing { self.at.map_or(0.0, |a| now.duration_since(a).as_secs_f64()) } else { 0.0 },
                _ => s.position,
            };
            self.fresh |= moved;
            self.raw = Some((s.position, s.playing));
            self.at = Some(now);
        }
        if s.duration <= 0.0 {
            return (self.own, false);
        }
        let run = if s.playing { self.at.map_or(0.0, |a| now.duration_since(a).as_secs_f64()) } else { 0.0 };
        ((self.base + run).min(s.duration), self.fresh)
    }
}

/// What's followed: the player's bus name, the clock, and what's been logged.
struct Followed {
    app: Option<String>,
    bundle: &'static str,
    exact: bool,
    can_seek: bool,
    /// bus name → (playing when last read, when it began playing)
    began: HashMap<String, (bool, Instant)>,
    /// bus name → the program that owns it (looked up once)
    owner: HashMap<String, String>,
    clock: Clock,
    /// songs and players already logged (once each)
    said: Vec<String>,
    /// the song followed, by name: which of the browser's sound streams to hear (a tab's stream is named after its
    /// title, which names the song)
    song: String,
}

static FOLLOWED: Mutex<Option<Followed>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Followed) -> R) -> R {
    let mut g = FOLLOWED.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| Followed { app: None, bundle: "com.google.Chrome", exact: false, can_seek: false, began: HashMap::new(),
                                         owner: HashMap::new(), clock: Clock::default(), said: Vec::new(), song: String::new() }))
}

/// Log `msg` once under `key`.
fn once(f: &mut Followed, key: String, msg: impl FnOnce() -> String) {
    if !f.said.contains(&key) {
        if f.said.len() > 500 {
            f.said.clear();
        }
        f.said.push(key);
        crate::engine::log(&msg());
    }
}

/// The browser followed (its bundle id, for capturing its sound).
pub fn audio_bundle() -> &'static str {
    with(|f| f.bundle)
}

/// The name of the song followed in a browser ("" when none yet).
pub fn song() -> String {
    with(|f| f.song.clone())
}

/// The position followed comes from the browser's own player, seen current.
pub fn exact_position() -> bool {
    with(|f| f.exact)
}

/// Every browser that publishes an MPRIS player can be followed (by the bundle id its sound is captured by).
pub fn followable(bundle: &str) -> bool {
    bundles().any(|b| b == bundle)
}

/// (Nothing to allow in a browser here: its MPRIS player needs no setting.)
pub fn js_blocked() -> Option<(&'static str, &'static str, &'static str)> {
    None
}

/// A browser's player as a song to follow: what it said, with the song's names filled in (`with_names`), the browser,
/// and what named the song (`from`, for the log) with the tab's title when the names aren't the page's own.
struct Player {
    said: Said,
    bundle: &'static str,
    name: &'static str,
    from: &'static str,
    tab: Option<String>,
}

/// What named a song: the page itself (through the browser's player), KDE's browser integration's player (which
/// reads the page's own names when the browser's player gives the tab's title), or the tab's title.
const FROM_PAGE: &str = "the page";
const FROM_KDE: &str = "KDE's browser integration";
const FROM_TAB: &str = "the tab's title";

/// `s` with its song's names filled in when the page gave no artist (the browser then gives the tab's title as the
/// title): from KDE's browser integration's player for the same browser (`mirror`), else from the tab's title when
/// that names a song (youtube::tab_title_names). → (the player as named, what named it, the tab's title when the
/// names aren't the page's own). Names from the tab's title are the song's already (parsed); the others are a
/// page's, for `session_names`.
fn with_names(s: &Said, mirror: Option<&Said>) -> (Said, &'static str, Option<String>) {
    let mut s = s.clone();
    if !s.artist.trim().is_empty() {
        return (s, FROM_PAGE, None);
    }
    let tab = s.title.clone();
    if let Some(m) = mirror {
        s.title = m.title.clone();
        s.artist = m.artist.clone();
        s.album = m.album.clone();
        return (s, FROM_KDE, Some(tab));
    }
    match crate::youtube::tab_title_names(&tab) {
        Some((artist, title)) => {
            s.artist = artist;
            s.title = title;
            (s, FROM_TAB, Some(tab))
        }
        None => (s, FROM_PAGE, None),
    }
}

/// Does KDE's browser integration's player `m` (of browser `mb`, when it says) repeat the tab browser `b`'s own
/// player `s` plays, with the names the page gave? Only with an artist and a title; the same browser when `m` says
/// which; else when the tab's title holds `m`'s title ("(3) Katy Perry - Dark Horse - YouTube" holds "Dark Horse").
fn mirror_names(s: &Said, b: &str, m: &Said, mb: Option<&str>) -> bool {
    let (title, artist) = (m.title.trim(), m.artist.trim());
    if title.is_empty() || artist.is_empty() {
        return false;
    }
    match mb {
        Some(mb) => mb == b,
        None => s.title.to_lowercase().contains(&title.to_lowercase()),
    }
}

/// What a browser plays and where, as a song.
pub fn state() -> Option<State> {
    let players = mpris::sessions();
    let now = Instant::now();
    with(|f| {
        // which browser each player is (the owning program looked up once per player)
        f.owner.retain(|app, _| players.iter().any(|s| s.app == *app));
        let mut all: Vec<(&Said, &'static str, &'static str)> = Vec::new();
        // KDE's plasma-browser-integration and playerctld repeat another player's song: not followed while the
        // browser's own player is there (but see `with_names`: the integration's names may fill in the browser's)
        let mirror = |app: &str| ["plasma-browser-integration", "playerctld"].iter().any(|m| app.contains(m));
        let natives = players.iter().filter(|s| !mirror(&s.app)).count();
        let mut mirrors: Vec<(&Said, Option<&'static str>)> = Vec::new();
        for s in &players {
            let owner = f.owner.entry(s.app.clone()).or_insert_with(|| {
                mpris::pid_of(&s.app).and_then(|p| std::fs::read_to_string(format!("/proc/{p}/comm")).ok()).unwrap_or_default().trim().to_string()
            }).clone();
            let about = format!("{} {} {} {owner}", s.app.strip_prefix("org.mpris.MediaPlayer2.").unwrap_or(&s.app), s.identity, s.desktop);
            if mirror(&s.app) && natives > 0 {
                if s.app.contains("plasma-browser-integration") {
                    mpris::said_once("KDE's browser integration", s);
                    mirrors.push((s, browser_of(&about).map(|(b, _)| b)));
                }
                continue;
            }
            if let Some((b, n)) = browser_of(&about) {
                mpris::said_once(n, s);     // what it says is playing (each change), as Spotify's is
                all.push((s, b, n));
            }
        }
        // every player there and which were taken as browsers, each time that changes: with YouTube on and a browser
        // not followed, the log says whether its player was missing or not recognised (romulka, issue #1, 1 Nov 2026)
        let seen: Vec<String> = players.iter().map(|s| {
            let owner = f.owner.get(&s.app).cloned().unwrap_or_default();
            let taken = all.iter().find(|(x, _, _)| x.app == s.app).map_or(String::new(), |(_, _, n)| format!(": {n}"));
            format!("{} (\"{}\", desktop \"{}\", program \"{owner}\"){taken}", s.app, s.identity, s.desktop)
        }).collect();
        let key = format!("players:{}", seen.join("|"));
        once(f, key, || if seen.is_empty() { "YouTube on Linux: no MPRIS players (no browser or music app says what it plays)".to_string() }
                        else { format!("YouTube on Linux: MPRIS players {}", seen.join("; ")) });
        // a page that doesn't say what it plays gives no artist, and the browser shows the tab's title instead: the
        // song is then named from KDE's browser integration's player, which read the page itself, or from the tab's
        // title (romulka, issue #1: Firefox on KDE gave every YouTube video so, and none was followed)
        let all: Vec<Player> = all.into_iter().map(|(s, bundle, name)| {
            let mirror = mirrors.iter().find(|(m, mb)| mirror_names(s, bundle, m, *mb)).map(|(m, _)| *m);
            let (said, from, tab) = with_names(s, mirror);
            Player { said, bundle, name, from, tab }
        }).collect();
        // when each began playing
        f.began.retain(|app, _| all.iter().any(|p| p.said.app == *app));
        for p in &all {
            let s = &p.said;
            let e = f.began.entry(s.app.clone()).or_insert((false, now));
            if s.playing && !e.0 {
                e.1 = now;
            }
            e.0 = s.playing;
        }
        // only what looks like a song
        let songs: Vec<&Player> = all.iter().filter(|p| match crate::youtube::not_a_song(&p.said.title, &p.said.artist, &p.said.album, p.said.duration) {
            Some(why) => {
                let s = &p.said;
                let key = format!("no:{}:{}:{}", s.app, s.title, s.artist);
                let tab = p.tab.as_ref().map_or(String::new(), |t| format!(", tab title \"{t}\""));
                once(f, key, || format!("{} plays \"{}\" by \"{}\" (album \"{}\"{tab}): not followed as a song, {why}", p.name, s.title, s.artist, s.album));
                false
            }
            None => true,
        }).collect();
        let t0 = f.began.values().map(|b| b.1).min().unwrap_or(now);
        let cands: Vec<(&str, bool, f64)> = songs.iter()
            .map(|p| (p.said.app.as_str(), p.said.playing, f.began.get(&p.said.app).map_or(0.0, |b| b.1.duration_since(t0).as_secs_f64()))).collect();
        let prefer = crate::settings::listening().then(crate::listen::want_bundle).flatten()
            .and_then(|b| songs.iter().find(|p| p.bundle == b).map(|p| p.said.app.as_str()));
        let p = *songs.get(pick(&cands, prefer, f.app.as_deref())?)?;
        let (s, bundle, name) = (&p.said, p.bundle, p.name);
        // (names read from the tab's title are the song's already: parse_title has had them)
        let (artist, title) = if p.from == FROM_TAB { (s.artist.clone(), s.title.clone()) } else { crate::youtube::session_names(&s.title, &s.artist, &s.album) };
        let key = mpris::media_id("media-", &s.artist, &s.title, &s.album);
        let id = format!("{}{key}", crate::youtube::ID_PREFIX);
        let (artist, title) = crate::youtube::named(&key).unwrap_or((artist, title));
        crate::youtube::note_kind(&key, !s.album.trim().is_empty(), "");
        let (position, exact) = f.clock.read(&id, s, now);
        if f.app.as_deref() != Some(s.app.as_str()) {
            crate::engine::log(&format!("YouTube on Linux: following {name}'s MPRIS player ({}, \"{}\")", s.app, s.identity));
        }
        f.app = Some(s.app.clone());
        f.bundle = bundle;
        f.song = title.clone();
        f.can_seek = s.can_seek;
        f.exact = exact;
        once(f, format!("song:{id}"), || {
            let length = if s.duration > 0.0 { format!("a length ({:.0} s, at {:.1} s)", s.duration, s.position) } else { "no length".into() };
            let tab = p.tab.as_ref().map_or(String::new(), |t| format!(" (its tab's title \"{t}\")"));
            format!("YouTube on Linux: {name} gave title \"{}\", artist \"{}\", album \"{}\"{tab}, with {length}, seek {}; named {artist} - {title} ({id}) from {}; position {}",
                    s.title, s.artist, s.album, if s.can_seek { "allowed" } else { "not allowed" }, p.from,
                    if s.duration > 0.0 { "from the browser's player" } else { "OnKey's own clock, then OnKey's alignment once learned" })
        });
        Some(State {
            playing: s.playing,
            track: Track { art: crate::music::cover_cached(&id, &artist, &title).map(|c| c.art), id, name: title, artist,
                           album: s.album.clone(), duration: s.duration, link: None },
            position,
            source: Source::YouTube,
        })
    })
}

/// "restart", "next", "previous", "playpause", "pause" or "play" to the browser followed.
pub fn control(action: &str) {
    let Some((app, can_seek)) = with(|f| f.app.clone().map(|a| (a, f.can_seek))) else { return };
    match action {
        "restart" if can_seek => mpris::seek_app(&app, 0.0),
        // (Firefox's player takes no position: Previous starts the video over, as its own button does)
        "restart" => mpris::control_app(&app, "previous"),
        a => mpris::control_app(&app, a),
    }
}

/// Jump to a position (seconds), where the browser takes one (the page handles seeking).
pub fn seek(seconds: f64) {
    if let Some(app) = with(|f| f.app.clone().filter(|_| f.can_seek)) {
        mpris::seek_app(&app, seconds);
    }
}

/// Play a video ("youtube:<id>"): a real video id (a song learned on a Mac) opens its page in the default browser;
/// an id made from a song's names has no page.
pub fn play_track(id: &str) {
    let video = id.strip_prefix(crate::youtube::ID_PREFIX).unwrap_or("");
    if video.len() == 11 && video.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        super::shell::open(format!("https://www.youtube.com/watch?v={video}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_that_never_moves_runs_on_across_a_pause() {
        let t = Instant::now();
        let at = |s: f64| t + std::time::Duration::from_secs_f64(s);
        let said = |playing: bool| Said { app: "x".into(), playing, title: "x".into(), duration: 200.0, position: 0.0, ..Default::default() };
        let mut c = Clock::default();
        assert_eq!(c.read("a", &said(true), at(0.0)), (0.0, false));
        assert_eq!(c.read("a", &said(true), at(10.0)), (10.0, false));
        assert_eq!(c.read("a", &said(false), at(12.0)), (12.0, false), "paused at 12 s, not back to 0");
        assert_eq!(c.read("a", &said(false), at(20.0)), (12.0, false));
        let (p, exact) = c.read("a", &said(true), at(21.0));
        assert!((p - 12.0).abs() < 1e-9 && !exact, "{p}");
        let (p, _) = c.read("a", &said(true), at(24.0));
        assert!((p - 15.0).abs() < 1e-9, "{p}");
    }

    #[test]
    fn browsers_are_told_by_what_their_player_says() {
        assert_eq!(browser_of("chromium.instance123 Google Chrome google-chrome chrome"), Some(("com.google.Chrome", "Chrome")));
        assert_eq!(browser_of("chromium.instance9 Chromium chromium chromium"), Some(("com.google.Chrome", "Chrome")));
        assert_eq!(browser_of("chromium.instance9 Brave brave-browser brave").map(|b| b.1), Some("Brave"));
        assert_eq!(browser_of("chromium.instance9 Microsoft Edge microsoft-edge msedge").map(|b| b.1), Some("Edge"));
        assert_eq!(browser_of("chromium.instance9 Vivaldi vivaldi-stable vivaldi-bin").map(|b| b.1), Some("Vivaldi"));
        assert_eq!(browser_of("firefox.instance_1_45 Mozilla Firefox firefox firefox").map(|b| b.1), Some("Firefox"));
        assert_eq!(browser_of("firefox.instance_1_45 Firefox org.mozilla.firefox firefox-bin").map(|b| b.1), Some("Firefox"));
        assert_eq!(browser_of("spotify Spotify spotify spotify"), None);
        assert_eq!(browser_of("vlc VLC media player vlc vlc"), None);
        for b in bundles() {
            assert!(!super::super::procs::exes_for(b).is_empty(), "{b}: its sound is captured by its program");
        }
    }

    /// Firefox on KDE gives the tab's title and no artist (romulka, issue #1): the song is named from KDE's browser
    /// integration's player when it read the page, else from the tab's title; a page that names an artist is left as is.
    #[test]
    fn a_player_that_gives_no_artist_is_named_from_kdes_integration_or_the_tabs_title() {
        let tab = "(3460) Katy Perry - Dark Horse ft. Juicy J - YouTube";
        let firefox = Said { app: "org.mpris.MediaPlayer2.firefox.instance_1_39".into(), title: tab.into(), ..Default::default() };
        let kde = Said { app: "org.mpris.MediaPlayer2.plasma-browser-integration".into(), title: "Katy Perry - Dark Horse ft. Juicy J".into(),
                         artist: "KatyPerryVEVO".into(), ..Default::default() };
        // from the tab's title (parsed: the song's names)
        let (s, from, raw) = with_names(&firefox, None);
        assert_eq!((s.artist.as_str(), s.title.as_str(), from, raw.as_deref()), ("Katy Perry", "Dark Horse", FROM_TAB, Some(tab)));
        assert_eq!(s.app, firefox.app, "the browser's own player is still the one followed");
        // from KDE's integration (the page's names, for session_names)
        let (s, from, raw) = with_names(&firefox, Some(&kde));
        assert_eq!((s.artist.as_str(), s.title.as_str(), from, raw.as_deref()), ("KatyPerryVEVO", "Katy Perry - Dark Horse ft. Juicy J", FROM_KDE, Some(tab)));
        assert_eq!(crate::youtube::session_names(&s.title, &s.artist, &s.album), ("Katy Perry".to_string(), "Dark Horse".to_string()));
        // a tab whose title names no song stays as it was (not_a_song turns it away)
        let home = Said { title: "(3461) YouTube".into(), ..firefox.clone() };
        let (s, from, raw) = with_names(&home, None);
        assert_eq!((s.artist.as_str(), s.title.as_str(), from, raw), ("", "(3461) YouTube", FROM_PAGE, None));
        // a page that names its artist (Chromium's player) is the page's own
        let chrome = Said { title: "Dark Horse".into(), artist: "Katy Perry".into(), album: "PRISM".into(), ..Default::default() };
        let (s, from, raw) = with_names(&chrome, Some(&kde));
        assert_eq!((s.title.as_str(), s.artist.as_str(), s.album.as_str(), from, raw), ("Dark Horse", "Katy Perry", "PRISM", FROM_PAGE, None));
        // which integration player is the browser's: the same browser when it says, else the tab's title holds its title
        assert!(mirror_names(&firefox, "org.mozilla.firefox", &kde, Some("org.mozilla.firefox")));
        assert!(!mirror_names(&firefox, "org.mozilla.firefox", &kde, Some("com.google.Chrome")));
        assert!(mirror_names(&firefox, "org.mozilla.firefox", &kde, None), "the tab's title holds the song's");
        let other = Said { title: "Lateralus".into(), artist: "TOOL".into(), ..Default::default() };
        assert!(!mirror_names(&firefox, "org.mozilla.firefox", &other, None));
        let unnamed = Said { title: tab.into(), ..Default::default() };
        assert!(!mirror_names(&firefox, "org.mozilla.firefox", &unnamed, Some("org.mozilla.firefox")), "no artist: nothing to take");
        // the song's id comes from the parsed names, so the count in the tab's title doesn't make a new song each time
        let (a, _, _) = with_names(&Said { title: "(3462) Katy Perry - Dark Horse ft. Juicy J - YouTube".into(), ..firefox.clone() }, None);
        let (b, _, _) = with_names(&firefox, None);
        assert_eq!(mpris::media_id("media-", &a.artist, &a.title, &a.album), mpris::media_id("media-", &b.artist, &b.title, &b.album));
    }

    #[test]
    fn the_player_that_began_playing_last_is_followed() {
        let c = [("a", true, 1.0), ("b", true, 5.0), ("c", false, 9.0)];
        assert_eq!(pick(&c, None, None), Some(1));
        assert_eq!(pick(&c, Some("a"), None), Some(0), "the browser listening hears first");
        assert_eq!(pick(&c, Some("c"), None), Some(1), "(not when it's paused)");
        let paused = [("a", false, 1.0), ("b", false, 5.0)];
        assert_eq!(pick(&paused, None, Some("b")), Some(1), "paused: the one followed so far");
        assert_eq!(pick(&[], None, None), None);
    }

    #[test]
    fn the_position_runs_on_from_when_it_changed_and_is_exact_only_once_seen_changing() {
        let t = Instant::now();
        let at = |s: f64| t + std::time::Duration::from_secs_f64(s);
        let said = |pos: f64, playing: bool| Said { app: "x".into(), playing, title: "x".into(), duration: 200.0, position: pos, ..Default::default() };
        let mut c = Clock::default();
        assert_eq!(c.read("a", &said(30.0, true), at(0.0)), (30.0, false));
        let (p, exact) = c.read("a", &said(30.0, true), at(2.0));
        assert!((p - 32.0).abs() < 1e-6 && !exact);
        assert_eq!(c.read("a", &said(33.0, false), at(3.0)), (33.0, true));
        let (p, exact) = c.read("a", &said(33.0, true), at(11.0));
        assert!((p - 33.0).abs() < 1e-6 && exact);
        assert_eq!(c.read("b", &said(0.0, true), at(16.0)), (0.0, true));
    }
}
