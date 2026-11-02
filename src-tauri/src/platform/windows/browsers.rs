//! YouTube (and any page that says what it plays) in a web browser on Windows, through the browser's system media
//! session: what youtube.rs does on macOS through AppleScript. Chromium's browsers (Chrome, Edge, Brave, Opera,
//! Vivaldi, Arc) and Firefox each publish one session for the whole browser, following its active tab, with what
//! the page told it (navigator.mediaSession): YouTube's video title and channel, YouTube Music's artist and title
//! (and album, from Chromium of mid 2026 on). No address, so no video id: the song's id is made from its names
//! ("youtube:media-…", as Spotify's are: media.rs).
//!
//! The position: both give a timeline (Chromium since about 110, Firefox since about 125) but never say when it was
//! last updated, and update it only on play, pause and seek. So it's run on from when OnKey saw it change, and
//! taken as exact only once OnKey has seen it change (a session already playing when first read may be stale).
//! With no timeline, or before that, the position is OnKey's own clock from when the song began playing, and
//! listening's alignment (recognize.rs) places it once the song is learned.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use super::media::{self, Said};
use crate::source::{Source, State, Track};

/// The browsers, by what their session's app id contains (lower case): (that, bundle id OnKey names it by, name).
const BROWSERS: [(&str, &str, &str); 9] = [
    ("msedge", "com.microsoft.edgemac", "Edge"),
    ("brave", "com.brave.Browser", "Brave"),
    ("vivaldi", "com.vivaldi.Vivaldi", "Vivaldi"),
    ("opera", "com.operasoftware.Opera", "Opera"),
    ("thebrowsercompany", "company.thebrowser.Browser", "Arc"),
    ("firefox", "org.mozilla.firefox", "Firefox"),
    ("chrome", "com.google.Chrome", "Chrome"),
    ("chromium", "com.google.Chrome", "Chrome"),
    ("arc", "company.thebrowser.Browser", "Arc"),
];

/// The bundle ids of the browsers OnKey follows this way (listening hears their sound).
pub fn bundles() -> impl Iterator<Item = &'static str> {
    BROWSERS.iter().map(|b| b.1)
}

/// The browser a session belongs to, by its app id: (bundle id, name). Firefox's app id is a hash of where it's
/// installed ("308046B0AF4A39CB" for the usual place): 16 hex digits is Firefox's when it's running.
pub fn browser_of(app: &str, firefox_running: impl Fn() -> bool) -> Option<(&'static str, &'static str)> {
    let id = app.to_lowercase();
    let first = id.split(['.', '!', '/', '\\', '_']).next().unwrap_or("");
    let found = BROWSERS.iter().find(|(k, _, _)| if *k == "arc" { first == "arc" } else { id.contains(k) });
    if let Some((_, b, n)) = found {
        return Some((b, n));
    }
    (app.len() == 16 && app.chars().all(|c| c.is_ascii_hexdigit()) && firefox_running()).then_some(("org.mozilla.firefox", "Firefox"))
}

/// The session to follow, of `cands` (app id, playing, when it began playing: s since some time): one playing, the
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

/// Where a song is, as OnKey works it out from what the session says: its position, and whether that's the
/// browser's (exact) or OnKey's own clock.
#[derive(Debug, Clone, Default)]
pub struct Clock {
    track: String,
    /// the session's last raw (position, playing), and when OnKey saw it so
    raw: Option<(f64, bool)>,
    at: Option<Instant>,
    /// OnKey saw the timeline change: it's current
    fresh: bool,
    /// playing time since the song began (no timeline), and when it was last read
    own: f64,
    last: Option<Instant>,
}

impl Clock {
    /// Read `s` (its song `track`) at `now`: (position s, exact).
    pub fn read(&mut self, track: &str, s: &Said, now: Instant) -> (f64, bool) {
        let since = self.last.map_or(0.0, |a| now.duration_since(a).as_secs_f64());
        self.last = Some(now);
        if self.track != track {
            // a new song: its timeline is the browser's new word (fresh), OnKey's clock starts at 0
            let first = self.track.is_empty();
            *self = Clock { track: track.to_string(), raw: Some((s.position, s.playing)), at: Some(now), fresh: !first, own: 0.0, last: Some(now) };
            return (s.position, s.duration > 0.0 && !first);
        }
        if self.raw.is_some_and(|(_, p)| p) {
            self.own += since;
        }
        if self.raw != Some((s.position, s.playing)) {
            self.fresh |= self.raw.is_some();
            self.raw = Some((s.position, s.playing));
            self.at = Some(now);
        }
        if s.duration <= 0.0 {
            return (self.own, false);
        }
        let run = if s.playing { self.at.map_or(0.0, |a| now.duration_since(a).as_secs_f64()) } else { 0.0 };
        ((s.position + run).min(s.duration), self.fresh)
    }
}

/// What's followed: the session's app id, the clock, and what's been logged.
struct Followed {
    app: Option<String>,
    bundle: &'static str,
    exact: bool,
    can_seek: bool,
    /// app id → (playing when last read, when it began playing)
    began: HashMap<String, (bool, Instant)>,
    clock: Clock,
    /// songs and sessions already logged (once each)
    said: Vec<String>,
}

static FOLLOWED: Mutex<Option<Followed>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Followed) -> R) -> R {
    let mut g = FOLLOWED.lock().unwrap();
    f(g.get_or_insert_with(|| Followed { app: None, bundle: "com.google.Chrome", exact: false, can_seek: false, began: HashMap::new(),
                                         clock: Clock::default(), said: Vec::new() }))
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

/// The position followed comes from the browser's own timeline, seen current.
pub fn exact_position() -> bool {
    with(|f| f.exact)
}

/// Every browser that publishes a media session can be followed (by the bundle id its sound is captured by).
pub fn followable(bundle: &str) -> bool {
    bundles().any(|b| b == bundle)
}

/// (Nothing to allow in a browser here: its media session needs no setting.)
pub fn js_blocked() -> Option<(&'static str, &'static str, &'static str)> {
    None
}

/// A browser's session as a song to follow: what it said, with the song's names filled in (`with_names`), the
/// browser, and whether the names were read from the tab's title (that title, for the log) rather than the page's own.
struct Player {
    said: Said,
    bundle: &'static str,
    name: &'static str,
    tab: Option<String>,
}

/// `s` with its song's names filled in when the page gave no artist (the browser then gives the tab's title as the
/// title) and the tab's title names a song (youtube::tab_title_names, parsed already: not for `session_names`).
/// → (the session as named, the tab's title when the names were read from it).
fn with_names(mut s: Said) -> (Said, Option<String>) {
    if !s.artist.trim().is_empty() {
        return (s, None);
    }
    match crate::youtube::tab_title_names(&s.title) {
        Some((artist, title)) => {
            let tab = std::mem::replace(&mut s.title, title);
            s.artist = artist;
            (s, Some(tab))
        }
        None => (s, None),
    }
}

/// What a browser plays and where, as a song.
pub fn state() -> Option<State> {
    let firefox = std::cell::OnceCell::new();
    let firefox_running = || *firefox.get_or_init(|| crate::source::app_running("org.mozilla.firefox"));
    // (a page that doesn't say what it plays gives no artist, and the browser shows the tab's title instead: the song
    // is then named from the tab's title, as on Linux: romulka, issue #1)
    let all: Vec<Player> = media::sessions().into_iter()
        .filter_map(|s| browser_of(&s.app, firefox_running).map(|(bundle, name)| {
            let (said, tab) = with_names(s);
            Player { said, bundle, name, tab }
        })).collect();
    let now = Instant::now();
    with(|f| {
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
        let (artist, title) = if p.tab.is_some() { (s.artist.clone(), s.title.clone()) } else { crate::youtube::session_names(&s.title, &s.artist, &s.album) };
        let key = media::media_id("media-", &s.artist, &s.title, &s.album);
        let id = format!("{}{key}", crate::youtube::ID_PREFIX);
        let (artist, title) = crate::youtube::named(&key).unwrap_or((artist, title));
        crate::youtube::note_kind(&key, !s.album.trim().is_empty(), "");
        let (position, exact) = f.clock.read(&id, s, now);
        if f.app.as_deref() != Some(s.app.as_str()) {
            crate::engine::log(&format!("YouTube on Windows: following {name}'s media session (app id \"{}\")", s.app));
        }
        f.app = Some(s.app.clone());
        f.bundle = bundle;
        f.can_seek = s.can_seek;
        f.exact = exact;
        once(f, format!("song:{id}"), || {
            let timeline = if s.duration > 0.0 { format!("a timeline ({:.0} s long, at {:.1} s)", s.duration, s.position) } else { "no timeline".into() };
            let how = match (s.duration > 0.0, exact) {
                (true, true) => "from the browser's timeline, run on from its last change",
                (true, false) => "from the browser's timeline once it changes (it may be stale when first read); until then OnKey's alignment once learned",
                _ => "OnKey's own clock from when it began playing, then OnKey's alignment once learned",
            };
            let from = match &p.tab { Some(t) => format!(" (its tab's title \"{t}\")"), None => String::new() };
            format!("YouTube on Windows: {name} gave title \"{}\", artist \"{}\", album \"{}\"{from}, with {timeline}, seek {}; named {artist} - {title} ({id}) from {}; position {how}",
                    s.title, s.artist, s.album, if s.can_seek { "allowed" } else { "not allowed" }, if p.tab.is_some() { "the tab's title" } else { "the page" })
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
        "restart" if can_seek => media::seek_app(&app, 0.0),
        "restart" => {}
        a => media::control_app(&app, a),
    }
}

/// Jump to a position (seconds), where the browser takes one (the page handles seeking).
pub fn seek(seconds: f64) {
    if let Some(app) = with(|f| f.app.clone().filter(|_| f.can_seek)) {
        media::seek_app(&app, seconds);
    }
}

/// Play a video ("youtube:<id>"): a real video id (a song learned on a Mac) opens its page in the default browser;
/// an id made from a song's names has no page (never given here: store.rs).
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
    fn browsers_are_told_by_their_app_id() {
        let no_ff = || false;
        assert_eq!(browser_of("Chrome", no_ff), Some(("com.google.Chrome", "Chrome")));
        assert_eq!(browser_of("MSEdge", no_ff), Some(("com.microsoft.edgemac", "Edge")));
        assert_eq!(browser_of("Brave", no_ff).map(|b| b.1), Some("Brave"));
        assert_eq!(browser_of("Opera Software.OperaGX", no_ff).map(|b| b.1), Some("Opera"));
        assert_eq!(browser_of("Vivaldi.ABCDEF", no_ff).map(|b| b.1), Some("Vivaldi"));
        assert_eq!(browser_of("TheBrowserCompany.Arc_ttt1ap7aakyb4!Arc", no_ff).map(|b| b.1), Some("Arc"));
        assert_eq!(browser_of("308046B0AF4A39CB", || true).map(|b| b.1), Some("Firefox"));
        assert_eq!(browser_of("308046B0AF4A39CB", no_ff), None, "a hex app id is Firefox's only while it runs");
        assert_eq!(browser_of("Spotify.exe", || true), None);
        assert_eq!(browser_of("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic", || true), None, "'arc' isn't in every app id");
        assert_eq!(browser_of("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App", || true), None);
        for b in bundles() {
            assert!(!super::super::procs::exes_for(b).is_empty(), "{b}: its sound is captured by its program");
        }
    }

    /// A session with no artist gives the tab's title (Firefox without the page's names): the song is read from that.
    #[test]
    fn a_session_that_gives_no_artist_is_named_from_the_tabs_title() {
        let tab = "(3460) Katy Perry - Dark Horse ft. Juicy J - YouTube";
        let (s, raw) = with_names(Said { app: "308046B0AF4A39CB".into(), title: tab.into(), ..Default::default() });
        assert_eq!((s.artist.as_str(), s.title.as_str(), raw.as_deref()), ("Katy Perry", "Dark Horse", Some(tab)));
        let (s, raw) = with_names(Said { title: "(3461) YouTube".into(), ..Default::default() });
        assert_eq!((s.artist.as_str(), s.title.as_str(), raw), ("", "(3461) YouTube", None));
        let (s, raw) = with_names(Said { title: "Dark Horse".into(), artist: "Katy Perry".into(), album: "PRISM".into(), ..Default::default() });
        assert_eq!((s.title.as_str(), s.artist.as_str(), s.album.as_str(), raw), ("Dark Horse", "Katy Perry", "PRISM", None), "the page's own names stay");
        // the same id whatever the count of notifications in the tab's title
        let (a, _) = with_names(Said { title: "(7) Katy Perry - Dark Horse ft. Juicy J - YouTube".into(), ..Default::default() });
        assert_eq!(media::media_id("media-", &a.artist, &a.title, &a.album), media::media_id("media-", "Katy Perry", "Dark Horse", ""));
    }

    #[test]
    fn the_session_that_began_playing_last_is_followed() {
        let c = [("Chrome", true, 1.0), ("MSEdge", true, 5.0), ("Brave", false, 9.0)];
        assert_eq!(pick(&c, None, None), Some(1));
        assert_eq!(pick(&c, Some("Chrome"), None), Some(0), "the browser listening hears first");
        assert_eq!(pick(&c, Some("Brave"), None), Some(1), "(not when it's paused)");
        let paused = [("Chrome", false, 1.0), ("MSEdge", false, 5.0)];
        assert_eq!(pick(&paused, None, Some("MSEdge")), Some(1), "paused: the one followed so far");
        assert_eq!(pick(&paused, None, None), Some(0));
        assert_eq!(pick(&[], None, None), None);
    }

    #[test]
    fn the_position_runs_on_from_when_it_changed_and_is_exact_only_once_seen_changing() {
        let t = Instant::now();
        let at = |s: f64| t + std::time::Duration::from_secs_f64(s);
        let said = |pos: f64, playing: bool| Said { app: "Chrome".into(), playing, title: "x".into(), duration: 200.0, position: pos, ..Default::default() };
        let mut c = Clock::default();
        // first read, mid-song: the timeline may be stale
        assert_eq!(c.read("a", &said(30.0, true), at(0.0)), (30.0, false));
        let (p, exact) = c.read("a", &said(30.0, true), at(2.0));
        assert!((p - 32.0).abs() < 1e-6 && !exact);
        // paused there: the browser updates its timeline, now current
        assert_eq!(c.read("a", &said(33.0, false), at(3.0)), (33.0, true));
        assert_eq!(c.read("a", &said(33.0, false), at(10.0)), (33.0, true));
        let (p, exact) = c.read("a", &said(33.0, true), at(11.0));
        assert!((p - 33.0).abs() < 1e-6 && exact);
        let (p, _) = c.read("a", &said(33.0, true), at(15.5));
        assert!((p - 37.5).abs() < 1e-6);
        // the next song starts: the browser's new word
        assert_eq!(c.read("b", &said(0.0, true), at(16.0)), (0.0, true));
    }

    #[test]
    fn without_a_timeline_the_position_is_playing_time_since_the_song_began() {
        let t = Instant::now();
        let at = |s: f64| t + std::time::Duration::from_secs_f64(s);
        let said = |playing: bool| Said { app: "Chrome".into(), playing, title: "x".into(), ..Default::default() };
        let mut c = Clock::default();
        assert_eq!(c.read("a", &said(true), at(0.0)), (0.0, false));
        assert_eq!(c.read("a", &said(true), at(5.0)), (5.0, false));
        assert_eq!(c.read("a", &said(false), at(6.0)), (6.0, false));
        assert_eq!(c.read("a", &said(true), at(20.0)), (6.0, false), "paused time isn't counted");
        assert_eq!(c.read("a", &said(true), at(21.0)), (7.0, false));
        assert_eq!(c.read("b", &said(true), at(22.0)), (0.0, false));
    }
}
