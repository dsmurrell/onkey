//! The music apps on Windows through the system media session (GlobalSystemMediaTransportControlsSessionManager):
//! what Spotify and Apple Music are playing and where, and their controls (`spotify` and `music` below). What
//! AppleScript does on macOS (platform/mac), with no permission to ask for. The browsers' sessions too
//! (`sessions`: browsers.rs).
//!
//! What it can't give that AppleScript does:
//! - Spotify's track id: the session has only title, artist and album, so the id is made from those
//!   ("spotify:media:<hash>"), stable for a song but not Spotify's own (so not the same song as on a Mac);
//! - an exact position: Spotify updates the session's timeline only now and then (on play, pause, seek), so the
//!   position is worked out from the last update and the time since (platform/timeline.rs, which also says when
//!   that can't be trusted: `exact_position`). Listening's own alignment (recognize.rs) has
//!   to make up the rest, and how far off it is needs measuring on a real machine;
//! - playing a given track, and Spotify's own volume.
use ::windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};

/// How long one ask of the media session may take (they take a few ms) before it's given up: `join` would wait
/// for ever, and a stalled app or media service would stop the engine with it.
const ASK: std::time::Duration = std::time::Duration::from_secs(2);
/// An app whose session didn't answer in time is left alone this long (as on macOS: source::OSA_REST).
const REST: std::time::Duration = std::time::Duration::from_secs(60);
/// Who's left alone (an app id, or "media sessions" for Windows' media service), until when.
static RESTING: std::sync::Mutex<Vec<(String, std::time::Instant)>> = std::sync::Mutex::new(Vec::new());

/// The answer to an ask of `who` (an app id, or "media sessions"), waiting at most `ASK`. None when it fails or
/// doesn't come in time (then `who` is left alone for `REST`, and that's logged), or while `who` is left alone.
fn within<T: windows_core::RuntimeType + 'static>(who: &str, op: windows_core::Result<windows_future::IAsyncOperation<T>>) -> Option<T> {
    let now = std::time::Instant::now();
    {
        let mut r = RESTING.lock().unwrap_or_else(|e| e.into_inner());
        r.retain(|(_, until)| *until > now);
        if r.iter().any(|(w, _)| w == who) {
            return None;
        }
    }
    let op = op.ok()?;
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    op.when(move |_| {
        let _ = tx.send(());
    }).ok()?;
    if rx.recv_timeout(ASK).is_err() {
        let _ = op.Cancel();
        RESTING.lock().unwrap_or_else(|e| e.into_inner()).push((who.to_string(), now + REST));
        crate::engine::log(&format!("media session: {who} didn't answer within {} s: left alone for {} s", ASK.as_secs(), REST.as_secs()));
        return None;
    }
    op.GetResults().ok()
}

/// The session of the app whose app id contains `name` (lower case: "spotify"), if it has one now.
fn session(name: &str) -> Option<Session> {
    all().into_iter().find(|s| s.SourceAppUserModelId().is_ok_and(|id| id.to_string().to_lowercase().contains(name)))
}

/// Every app's session now.
fn all() -> Vec<Session> {
    super::com();
    let Some(list) = within("media sessions", Manager::RequestAsync()).and_then(|m| m.GetSessions().ok()) else { return Vec::new() };
    (0..list.Size().unwrap_or(0)).filter_map(|i| list.GetAt(i).ok()).collect()
}

/// The session of the app with exactly this app id.
fn session_of(app: &str) -> Option<Session> {
    all().into_iter().find(|s| s.SourceAppUserModelId().is_ok_and(|id| id.to_string() == app))
}

/// One app's session as it says it, nothing worked out (the browsers: browsers.rs).
#[derive(Debug, Clone, Default)]
pub struct Said {
    /// its app id (AppUserModelID: "Chrome", "MSEdge"…)
    pub app: String,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// seconds; 0 when the session has no timeline
    pub duration: f64,
    /// seconds into it, as of `updated`
    pub position: f64,
    /// when the timeline was last updated (Windows DateTime, 100 ns since 1601; 0 when not said)
    pub updated: i64,
    /// the app takes a new position from the session (seek)
    pub can_seek: bool,
}

/// What every app's session says now (those naming something).
pub fn sessions() -> Vec<Said> {
    all().iter().filter_map(said).collect()
}

fn said(s: &Session) -> Option<Said> {
    let app = s.SourceAppUserModelId().ok()?.to_string();
    let p = within(&app, s.TryGetMediaPropertiesAsync())?;
    let (title, artist, album) = (p.Title().ok()?.to_string(), p.Artist().map(|a| a.to_string()).unwrap_or_default(),
                                  p.AlbumTitle().map(|a| a.to_string()).unwrap_or_default());
    if title.trim().is_empty() {
        return None;
    }
    let info = s.GetPlaybackInfo().ok()?;
    let playing = info.PlaybackStatus().ok()? == Status::Playing;
    let can_seek = info.Controls().and_then(|c| c.IsPlaybackPositionEnabled()).unwrap_or(false);
    let (mut duration, mut position, mut updated) = (0.0, 0.0, 0);
    if let Ok(t) = s.GetTimelineProperties() {
        let start = t.StartTime().map_or(0, |x| x.Duration);
        duration = (t.EndTime().map_or(0, |x| x.Duration) - start).max(0) as f64 / 1e7;
        position = (t.Position().map_or(0, |x| x.Duration) - start).max(0) as f64 / 1e7;
        updated = t.LastUpdatedTime().map_or(0, |x| x.UniversalTime);
    }
    Some(Said { app, playing, title, artist, album, duration, position, updated, can_seek })
}

/// "restart", "next", "previous", "playpause", "pause" or "play" to the session of the app with this app id.
pub fn control_app(app: &str, action: &str) {
    if let Some(s) = session_of(app) {
        act(&s, action);
    }
}

/// Jump to a position (seconds) in what the app with this app id plays.
pub fn seek_app(app: &str, seconds: f64) {
    if let Some(s) = session_of(app) {
        seek_in(&s, seconds);
    }
}

/// Now, as a Windows DateTime (100 ns since 1601).
pub fn now_100ns() -> i64 {
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64 / 100);
    unix + 116_444_736_000_000_000
}

/// A stable id for a song the session names (FNV-1a over artist, title and album).
pub fn media_id(prefix: &str, artist: &str, title: &str, album: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in [artist, title, album].join("\u{1f}").to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}{h:016x}")
}

/// Each app's timeline as last read (by the name `now_playing` is asked for), and whether its position was exact.
static TIMELINES: std::sync::Mutex<Vec<(String, crate::platform::timeline::Timeline, bool)>> = std::sync::Mutex::new(Vec::new());

/// Whether the position `now_playing` last gave for this app is its session's own (see platform/timeline.rs).
fn exact(name: &str) -> bool {
    TIMELINES.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(n, _, _)| n == name).is_none_or(|(_, _, x)| *x)
}

/// What an app's session is playing: (playing, title, artist, album, duration s, position s).
pub fn now_playing(name: &str) -> Option<(bool, String, String, String, f64, f64)> {
    let s = session(name)?;
    let p = within(&s.SourceAppUserModelId().ok()?.to_string(), s.TryGetMediaPropertiesAsync())?;
    let (title, artist, album) = (p.Title().ok()?.to_string(), p.Artist().ok()?.to_string(), p.AlbumTitle().ok()?.to_string());
    if title.is_empty() {
        return None;
    }
    let playing = s.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? == Status::Playing;
    let t = s.GetTimelineProperties().ok()?;
    let (start, end) = (t.StartTime().ok()?.Duration, t.EndTime().ok()?.Duration);
    let dur = (end - start).max(0) as f64 / 1e7;
    // the timeline is only updated now and then: where it would be now, unless its update time can't be trusted
    // (platform/timeline.rs; then not exact, and listening's alignment places the song)
    let updated = t.LastUpdatedTime().map_or(0, |x| x.UniversalTime);
    let pos = {
        let mut all = TIMELINES.lock().unwrap_or_else(|e| e.into_inner());
        let i = all.iter().position(|(n, _, _)| n == name).unwrap_or_else(|| {
            all.push((name.to_string(), Default::default(), true));
            all.len() - 1
        });
        let (pos, exact) = all[i].1.read(t.Position().ok()?.Duration - start, updated, now_100ns(), playing, dur);
        if all[i].2 && !exact {
            crate::engine::log(&format!("media session: {name}'s timeline can't be trusted (update time {}): its position is taken as said, not exact, and listening places the song",
                                        if updated == 0 { "never said".to_string() } else { format!("{:.1} s ago", (now_100ns() - updated) as f64 / 1e7) }));
        }
        all[i].2 = exact;
        pos
    };
    said_once(&s, name, &title, &artist, &album, dur, pos, playing);
    Some((playing, title, artist, album, dur, pos))
}

/// Once per song an app's session names: what it said, for a tester's log (the position's own accuracy is
/// measured per song against the sound: digest::AppGap, "app vs sound" in the log).
#[allow(clippy::too_many_arguments)]
fn said_once(s: &Session, name: &str, title: &str, artist: &str, album: &str, dur: f64, pos: f64, playing: bool) {
    // (each app's last song: Spotify's and Apple Music's are both asked about)
    static SAID: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
    let key = format!("{artist}\u{1f}{title}\u{1f}{album}");
    let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    match said.iter_mut().find(|(n, _)| n == name) {
        Some((_, k)) if *k == key => return,
        Some((_, k)) => *k = key,
        None => said.push((name.to_string(), key)),
    }
    drop(said);
    let app = s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default();
    let can_seek = s.GetPlaybackInfo().and_then(|i| i.Controls()).and_then(|c| c.IsPlaybackPositionEnabled()).unwrap_or(false);
    let updated = s.GetTimelineProperties().and_then(|t| t.LastUpdatedTime()).map_or(0, |t| t.UniversalTime);
    let timeline = if dur > 0.0 {
        let ago = if updated > 0 { format!(", last updated {:.1} s ago", (now_100ns() - updated) as f64 / 1e7) } else { ", never updated".into() };
        format!("timeline yes ({dur:.0} s long, at {pos:.1} s{ago})")
    } else {
        "timeline no".into()
    };
    crate::engine::log(&format!("media session: {name} (app id \"{app}\"): \"{title}\" by \"{artist}\" (album \"{album}\"), {}, {timeline}, seek {}",
                                if playing { "playing" } else { "paused" }, if can_seek { "allowed" } else { "not allowed" }));
}

/// "restart", "next", "previous", "playpause", "pause" or "play" to an app's session.
pub fn control(name: &str, action: &str) {
    if let Some(s) = session(name) {
        act(&s, action);
    }
}

fn act(s: &Session, action: &str) {
    let r = match action {
        "restart" => s.TryChangePlaybackPositionAsync(0),
        "next" => s.TrySkipNextAsync(),
        "previous" => s.TrySkipPreviousAsync(),
        "playpause" => s.TryTogglePlayPauseAsync(),
        "pause" => s.TryPauseAsync(),
        "play" => s.TryPlayAsync(),
        _ => return,
    };
    if let Ok(app) = s.SourceAppUserModelId() {
        within(&app.to_string(), r);
    }
}

/// Jump to a position (seconds) in what an app's session plays.
pub fn seek(name: &str, seconds: f64) {
    if let Some(s) = session(name) {
        seek_in(&s, seconds);
    }
}

fn seek_in(s: &Session, seconds: f64) {
    let start = s.GetTimelineProperties().and_then(|t| t.StartTime()).map_or(0, |t| t.Duration);
    if let Ok(app) = s.SourceAppUserModelId() {
        within(&app.to_string(), s.TryChangePlaybackPositionAsync(start + (seconds.max(0.0) * 1e7) as i64));
    }
}

/// Spotify through its media session: its id Spotify's own when OnKey has learned it (trackids.rs), else made from
/// the song's names; no volume; a given track opened by its spotify:track: link.
pub mod spotify {
    use crate::source::{Source, State, Track};

    /// What Spotify's media session says it plays, with a cover from Apple's iTunes Search (the session's own
    /// thumbnail isn't a link the window can show), as Apple Music's and a browser's songs get theirs.
    pub fn state() -> Option<State> {
        let (playing, name, artist, album, duration, position) = super::now_playing("spotify")?;
        let made = super::media_id(crate::trackids::SPOTIFY_MEDIA, &artist, &name, &album);
        let id = crate::trackids::resolve(&made, &artist, &name).unwrap_or(made);
        let cover = crate::music::cover_cached(&id, &artist, &name);
        let (art, link) = cover.map_or((None, None), |c| (Some(c.art), c.link));
        Some(State {
            playing,
            track: Track { id, name, artist, album, duration, art, link },
            position,
            source: Source::Spotify,
        })
    }

    /// The position `state` gave is the session's own, run on (not when its update time can't be trusted).
    pub fn exact_position() -> bool {
        super::exact("spotify")
    }

    /// (The media session doesn't say Spotify's volume.)
    pub fn volume() -> Option<i32> {
        None
    }

    pub fn control(action: &str) {
        if crate::source::may_control("Spotify") {
            super::control("spotify", action);
        }
    }

    pub fn seek(seconds: f64) {
        if crate::source::may_control("Spotify") {
            super::seek("spotify", seconds);
        }
    }

    /// Open the track by its spotify:track: link (the media session can't start a given track), then wait until
    /// Spotify's session names it (up to 10 s: Spotify may have to start first), playing it if it's only shown.
    /// False if it never does, so the window offers to open it there rather than saying it plays. A made-up id
    /// (trackids.rs) is never opened: false. (The session knows the song by this id through `opening`.)
    pub fn play_track(id: &str) -> bool {
        if !(crate::trackids::is_spotify_track(id) && crate::platform::shell::open_app_link(id)) {
            crate::engine::log(&format!("Spotify didn't open {id} (not Spotify's own id, or Spotify isn't installed)"));
            return false;
        }
        let t = std::time::Instant::now();
        while t.elapsed() < WAIT {
            if let Some(s) = state().filter(|s| s.track.id == id) {
                if !s.playing {
                    control("play");
                }
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        crate::engine::log(&format!("Spotify was asked to open {id}, but its media session didn't name it within {} s", WAIT.as_secs()));
        false
    }

    /// How long `play_track` waits for Spotify to name the song (it may have to start first: 5-10 s).
    const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

    /// Spotify's track `id` is about to be opened, so the song the media session names next is known by it
    /// (trackids.rs). Anything else (a search, a made-up id) is nothing to learn. `track`: the song.
    pub fn opening(id: &str, track: impl FnOnce() -> Option<Track>) {
        if crate::trackids::is_spotify_track(id) {
            if let Some(t) = track() {
                crate::trackids::expect(id, &t.artist, &t.name);
            }
        }
    }

    /// Where the ids learned are kept (OnKey's data folder).
    pub fn set_data_dir(dir: &std::path::Path) {
        crate::trackids::set_data_dir(dir);
    }
}

/// The Apple Music app through its media session: ids made from the song's names, as Spotify's; no playing a
/// given track.
pub mod music {
    use crate::source::{Source, State, Track};

    /// What the Apple Music app's media session says it plays (its cover: music.rs).
    pub fn state() -> Option<State> {
        let (playing, name, artist, album, duration, position) = super::now_playing("applemusic")?;
        let id = super::media_id(&format!("{}media-", crate::music::ID_PREFIX), &artist, &name, &album);
        Some(State {
            playing,
            track: Track { id, name, artist, album, duration, art: None, link: None },
            position,
            source: Source::Music,
        })
    }

    pub fn control(action: &str) {
        if crate::source::may_control("Apple Music") {
            super::control("applemusic", action);
        }
    }

    pub fn seek(seconds: f64) {
        if crate::source::may_control("Apple Music") {
            super::seek("applemusic", seconds);
        }
    }

    /// The position `state` gave is the session's own, run on (not when its update time can't be trusted).
    pub fn exact_position() -> bool {
        super::exact("applemusic")
    }

    /// (The media session can't start a given track.)
    pub fn play_track(_id: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_songs_id_is_stable_and_its_own() {
        let a = super::media_id("spotify:media:", "Red Hot Chili Peppers", "Californication", "Californication");
        assert_eq!(a, super::media_id("spotify:media:", "red hot chili peppers", "californication", "Californication"));
        assert_ne!(a, super::media_id("spotify:media:", "Red Hot Chili Peppers", "Otherside", "Californication"));
        assert!(a.starts_with("spotify:media:"));
    }
}
