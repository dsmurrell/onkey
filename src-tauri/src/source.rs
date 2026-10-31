//! Where the music comes from: an app on this Mac that OnKey follows (what's playing and where) and listens to
//! (its audio, through a process tap). Spotify, Apple Music, and YouTube in a browser, all through AppleScript:
//! no login, no web API. Whichever is playing is followed; a track id says which it belongs to ("music:…" for
//! Apple Music, "youtube:…" for YouTube, "spotify:track:…" for Spotify).
use std::sync::Mutex;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    /// seconds
    pub duration: f64,
    /// album cover image URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art: Option<String>,
    /// the song's Apple Music page, when its cover came from Apple (iTunes Search or Shazam): a cover from Apple
    /// is shown linked to it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub playing: bool,
    pub track: Track,
    /// seconds into the track, when the state was read
    pub position: f64,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Spotify,
    Music,
    YouTube,
    /// listening mode: a song recognised by its sound, in an app OnKey only listens to (or doesn't control)
    Other,
    /// the demo song, which OnKey plays itself (demo.rs): followed by its player's clock in every mode
    Demo,
}

impl Source {
    /// The apps OnKey can follow by asking them (listening mode needs none of these).
    pub const ALL: [Source; 3] = [Source::Spotify, Source::Music, Source::YouTube];

    /// The app a track id belongs to.
    pub fn of_id(id: &str) -> Source {
        if id.starts_with(crate::music::ID_PREFIX) {
            Source::Music
        } else if crate::demo::is_demo(id) {
            Source::Demo
        } else if id.starts_with(crate::youtube::ID_PREFIX) {
            Source::YouTube
        } else if id.starts_with("isrc:") || id.starts_with("shazam:") {
            Source::Other
        } else {
            Source::Spotify
        }
    }

    /// The app's name, as people know it.
    pub fn name(self) -> &'static str {
        match self {
            Source::Spotify => "Spotify",
            Source::Music => "Apple Music",
            Source::YouTube => "YouTube",
            Source::Other => "your music app",
            Source::Demo => "OnKey",
        }
    }

    /// Its audio is captured from processes whose bundle id starts with this (for YouTube, those of the browser
    /// playing it).
    pub fn bundle(self) -> &'static str {
        match self {
            Source::Spotify => "com.spotify.client",
            Source::Music => "com.apple.Music",
            Source::YouTube => crate::youtube::browser().map_or("com.google.Chrome", |b| b.audio_bundle()),
            Source::Other => crate::listen::bundle().unwrap_or("com.spotify.client"),
            // OnKey's own process: the demo's player is the only sound OnKey's process makes (the window's sounds
            // come from WebKit's process)
            Source::Demo => crate::demo::bundle(),
        }
    }

    /// The app that comes to the front when told to play something, to be put back behind OnKey. None for
    /// YouTube: it's opened in the background, and a browser is yours to keep in front.
    pub fn app_bundle(self) -> Option<&'static str> {
        match self {
            Source::YouTube | Source::Other | Source::Demo => None,
            s => Some(s.bundle()),
        }
    }

    /// What the app is playing and where (None: not running, or nothing to play).
    pub fn state(self) -> Option<State> {
        match self {
            Source::Spotify => crate::spotify::state(),
            Source::Music => crate::music::state(),
            Source::YouTube => crate::youtube::state(),
            Source::Other => crate::listen::state(),
            Source::Demo => crate::demo::state(),
        }
    }

    /// "restart", "next", "previous", "playpause", "pause" or "play".
    pub fn control(self, action: &str) {
        // logged: a song that pauses or restarts by itself then shows whether OnKey asked for it
        crate::engine::log(&format!("asked {} to {action}", self.name()));
        // listening knows OnKey moved the song itself (it may make no gap to hear)
        match action {
            "restart" => crate::listen::moved(Some(0.0)),
            "next" | "previous" => crate::listen::moved(None),
            _ => {}
        }
        match self {
            Source::Spotify => crate::spotify::control(action),
            Source::Music => crate::music::control(action),
            Source::YouTube => crate::youtube::control(action),
            Source::Demo => crate::demo::control(action),
            Source::Other => {}
        }
    }

    /// Jump to a position in the current track (seconds).
    pub fn seek(self, seconds: f64) {
        crate::listen::moved(Some(seconds));
        match self {
            Source::Spotify => crate::spotify::seek(seconds),
            Source::Music => crate::music::seek(seconds),
            Source::YouTube => crate::youtube::seek(seconds),
            Source::Demo => crate::demo::seek(seconds),
            Source::Other => {}
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The app being followed: controls go to it.
static ACTIVE: Mutex<Source> = Mutex::new(Source::Spotify);

pub fn active() -> Source {
    *ACTIVE.lock().unwrap()
}

/// What's playing now. The app followed so far stays followed while it plays; when it stops or pauses and
/// another app is playing, that one is followed instead. With nothing playing, the followed app's state
/// (paused), or else another app's.
pub fn poll() -> Option<State> {
    // development builds: a simulated player stands in for the apps (ONKEY_SIM_PLAYER), which are never asked
    if let Some(sim) = crate::devtest::sim_player() {
        return sim;
    }
    let cur = active();
    let s = cur.state();
    if s.as_ref().is_some_and(|s| s.playing) {
        return s;
    }
    for other in Source::ALL.into_iter().filter(|o| *o != cur) {
        if let Some(o) = other.state() {
            if o.playing || s.is_none() {
                *ACTIVE.lock().unwrap() = other;
                return Some(o);
            }
        }
    }
    s
}

/// Is the app with this bundle id running? Asked of macOS directly (NSRunningApplication): no osascript process and
/// no Apple event (each AppleScript "is running" was a process launched, several times a second while following).
pub fn app_running(bundle: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2::rc::autoreleasepool(|_| {
            let id = objc2_foundation::NSString::from_str(bundle);
            objc2_app_kit::NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).count() > 0
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = bundle;
        false
    }
}

/// Is what the music app plays (its id) a song? Not a podcast episode, an audiobook chapter or an ad (QA: a podcast
/// got a learn take, a 20 MB library entry and a score).
pub fn is_song(id: &str) -> bool {
    !["spotify:episode:", "spotify:show:", "spotify:ad:", "spotify:audiobook:", "spotify:chapter:"].iter().any(|p| id.starts_with(p))
}

/// May OnKey control this app (play, pause, seek, start a song)? Always, outside listening mode. In listening
/// mode Spotify and Apple Music unless you turned them off (on by default: see settings.rs), and YouTube in
/// your browser once you've allowed it (the bigger permission: control of the browser). Apps OnKey only listens
/// to can't be controlled; one macOS won't let it control is listened to (the window says so).
pub fn controllable(s: Source) -> bool {
    if !crate::settings::listening() {
        return true;
    }
    match s {
        Source::Spotify => crate::settings::control("control_spotify") && !refused(s),
        Source::Music => crate::settings::control("control_music") && !refused(s),
        Source::YouTube => crate::youtube::enabled(),
        Source::Other => false,
        Source::Demo => true,
    }
}

/// Spotify's and Apple Music's Automation permission as macOS last said it was refused (checked at most every 5 s,
/// without asking): a refused app is followed by listening, quietly.
static REFUSED: Mutex<[Option<(std::time::Instant, bool)>; 2]> = Mutex::new([None, None]);

fn refused(s: Source) -> bool {
    if crate::devtest::sim_sounding().is_some() {
        return false;       // the simulated player stands in for Spotify: macOS isn't asked
    }
    let (i, bundle) = match s {
        Source::Spotify => (0, "com.spotify.client"),
        Source::Music => (1, "com.apple.Music"),
        _ => return false,
    };
    if let Some((_, no)) = REFUSED.lock().unwrap()[i].filter(|(at, _)| at.elapsed().as_secs() < 5) {
        return no;
    }
    // asked without the lock: macOS can take up to 1.5 s, and the engine's poll and the window's controls
    // mustn't wait behind it (a check already under way gives its last answer at once)
    let no = crate::permissions::automation(bundle) == "denied";
    REFUSED.lock().unwrap()[i] = Some((std::time::Instant::now(), no));
    no
}

/// The app macOS last refused to let OnKey control (Automation is off for it), and when: the window says so,
/// rather than the app just seeming not to be running.
static DENIED: Mutex<Option<(&'static str, std::time::Instant)>> = Mutex::new(None);

/// AppleScript's errors when macOS won't let OnKey send an app Apple events (refused, or not yet asked and
/// asking isn't allowed).
pub fn is_denial(stderr: &str) -> bool {
    stderr.contains("-1743") || stderr.contains("-1744")
}

/// Run AppleScript that talks to `app` (its name as people know it). A refusal by macOS is remembered for the
/// window; a script that got through to the app forgets it.
///
/// Spotify and Apple Music are never sent anything while you've chosen not to let OnKey control them (listening
/// mode with their control off in Settings), whoever asks: an Apple event to them is what makes macOS ask
/// for control, which listening alone must never do.
pub fn osa(app: &'static str, script: &str) -> Option<String> {
    // refusals in a row: (how many, since when). One alone can be macOS hiccupping (live: a single -1743 two
    // minutes into a song, Automation allowed all along, split the take in three); refused 3 times over 5 s is real
    static STREAK: Mutex<(u32, Option<std::time::Instant>)> = Mutex::new((0, None));
    if !may_send(app, crate::settings::listening(), crate::settings::control) {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::engine::log(&format!("not talking to {app}: its control is off (listen only)"));
        }
        return None;
    }
    // an app that didn't answer in time is left alone for a while: one browser stuck (live, 20 Oct: Chrome waiting on
    // its Automation prompt for 4 minutes) mustn't stop OnKey asking Spotify or Music
    if resting(app) {
        return None;
    }
    let sent = std::time::Instant::now();
    let out = match run_timed(script, OSA_TIMEOUT) {
        Some(o) => Ok::<_, std::io::Error>(o),
        None => {
            rest(app);
            crate::engine::log(&format!("{app} didn't answer in {:.0} s (a permission prompt may be waiting): not asked again for {:.0} s",
                                        OSA_TIMEOUT.as_secs_f64(), OSA_REST.as_secs_f64()));
            crate::records::apple_event(app, sent.elapsed().as_secs_f64() * 1000.0, "timeout");
            return None;
        }
    };
    crate::records::apple_event(app, sent.elapsed().as_secs_f64() * 1000.0, match &out {
        Ok(o) if o.status.success() => "ok",
        Ok(o) if is_denial(&String::from_utf8_lossy(&o.stderr)) => "denied",
        _ => "error",
    });
    let out = out.ok()?;
    let mut denied = DENIED.lock().unwrap();
    let mut streak = STREAK.lock().unwrap();
    if out.status.success() {
        if script.contains("tell application") && denied.is_some_and(|(a, _)| a == app) {
            *denied = None;
        }
        *streak = (0, None);
    } else if is_denial(&String::from_utf8_lossy(&out.stderr)) {
        let since = *streak.1.get_or_insert_with(std::time::Instant::now);
        if really_refused(&mut streak.0, since.elapsed().as_secs_f64()) {
            if denied.is_none() {
                crate::engine::log(&format!("macOS doesn't let OnKey control {app} (Automation): refused {} times in {:.0} s",
                                            streak.0, since.elapsed().as_secs_f64()));
            }
            *denied = Some((app, std::time::Instant::now()));
        } else if streak.0 == 1 {
            crate::engine::log(&format!("{app} refused one request ({}): not taken as Automation off yet",
                                        String::from_utf8_lossy(&out.stderr).trim()));
        }
    }
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// How long an app has to answer one Apple event (they take 50-300 ms; starting Spotify a few seconds).
const OSA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// How long an app that didn't answer in time is left alone.
const OSA_REST: std::time::Duration = std::time::Duration::from_secs(60);

static RESTING: Mutex<Vec<(&'static str, std::time::Instant)>> = Mutex::new(Vec::new());

/// An app OnKey has stopped asking because it didn't answer (for the window to say), if any.
pub fn not_answering() -> Option<&'static str> {
    let mut r = RESTING.lock().unwrap();
    r.retain(|(_, until)| *until > std::time::Instant::now());
    r.first().map(|(a, _)| *a)
}

fn resting(app: &str) -> bool {
    let mut r = RESTING.lock().unwrap();
    r.retain(|(_, until)| *until > std::time::Instant::now());
    r.iter().any(|(a, _)| *a == app)
}

fn rest(app: &'static str) {
    RESTING.lock().unwrap().push((app, std::time::Instant::now() + OSA_REST));
}

/// osascript running `script`, given `limit` to finish: its output, or None when it didn't (it's stopped).
fn run_timed(script: &str, limit: std::time::Duration) -> Option<std::process::Output> {
    use std::process::Stdio;
    let child = std::process::Command::new("osascript").args(["-e", script])
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let id = child.id();
    // the output is read on its own thread (a full pipe would otherwise stall the script); here, only the clock
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(child.wait_with_output()); });
    match rx.recv_timeout(limit) {
        Ok(out) => out.ok(),
        Err(_) => {
            let _ = std::process::Command::new("kill").arg(id.to_string()).status();
            None
        }
    }
}

/// May OnKey send `app` (osa's name for it) Apple events? Spotify and Apple Music only with their control allowed
/// (`control`, by settings key) in listening mode; anything outside it (where the apps are followed by asking
/// them), and the browsers (YouTube's own setting gates those).
fn may_send(app: &str, listening: bool, control: impl Fn(&str) -> bool) -> bool {
    let key = match app {
        "Spotify" => "control_spotify",
        "Apple Music" => "control_music",
        _ => return true,
    };
    !listening || control(key)
}

/// One more refusal in a row (`count` of them, the first `secs` ago): is Automation really off? 3 over 5 s.
fn really_refused(count: &mut u32, secs: f64) -> bool {
    *count += 1;
    *count >= 3 && secs >= 5.0
}

/// The app OnKey wasn't allowed to control in the last minute, if any; in listening mode, also an app you let
/// OnKey control that macOS refuses (it's listened to instead).
pub fn automation_denied() -> Option<&'static str> {
    let recent = DENIED.lock().unwrap().filter(|(_, at)| at.elapsed().as_secs() < 60).map(|(a, _)| a);
    recent.or_else(|| {
        if !crate::settings::listening() {
            return None;
        }
        [(Source::Spotify, "control_spotify"), (Source::Music, "control_music")].into_iter()
            .find(|(s, k)| crate::settings::control(k) && refused(*s)).map(|(s, _)| s.name())
    })
}

/// The settings key that allows control of an app (listening mode), and the app's name for asking.
pub fn control_key(s: Source) -> Option<(&'static str, &'static str)> {
    match s {
        Source::Spotify => Some(("control_spotify", "Spotify")),
        Source::Music => Some(("control_music", "Apple Music")),
        Source::YouTube => Some(("youtube", "your browser")),
        Source::Other | Source::Demo => None,
    }
}

/// Start a track in the app it belongs to; false when the app didn't (or there's no app to ask: a song only ever
/// heard). YouTube's is taken as done (it opens a tab).
pub fn play_track(id: &str) -> bool {
    let app = Source::of_id(id);
    // the demo plays only until another song is started from OnKey
    if app != Source::Demo {
        crate::demo::stop();
    }
    if app == Source::Other {
        return false;                               // a song only ever heard: nothing to tell to play it
    }
    // listening: the song before ends now, and this one starts once the app really plays it
    if matches!(app, Source::Spotify | Source::Music) {
        crate::listen::onkey_plays(id);
    }
    let done = match app {
        Source::Spotify => crate::spotify::play_track(id),
        Source::Music => crate::music::play_track(id),
        Source::YouTube => {
            crate::youtube::set_enabled(true);      // you chose a YouTube song in OnKey: follow YouTube from now on
            crate::youtube::play_track(id);
            true
        }
        Source::Demo => {
            crate::demo::start();                   // OnKey's own player (demo.rs)
            true
        }
        Source::Other => false,
    };
    if done {
        *ACTIVE.lock().unwrap() = app;
        pause_others(app);
    }
    done
}

/// A song was just started in `app`: Spotify or Apple Music, if another is playing (one OnKey may control), is paused,
/// so the two don't play over each other (listening hears both; the window followed the one still playing).
fn pause_others(app: Source) {
    for other in [Source::Spotify, Source::Music].into_iter().filter(|o| *o != app) {
        let running = match other {
            Source::Spotify => crate::spotify::running(),
            _ => crate::music::running(),
        };
        // (asked only when running: an Apple event to an app that isn't would start it)
        if running && controllable(other) && other.state().is_some_and(|s| s.playing) {
            other.control("pause");
        }
    }
}

/// An album cover for a learned song that doesn't have one (`track` as stored: id, name, artist): Apple's
/// public iTunes Search by artist and title (Spotify's songs too: no Spotify endpoint is used), with the song's
/// Apple Music page, or YouTube's thumbnail.
pub fn cover_for(track: &serde_json::Value) -> Option<crate::music::Cover> {
    let id = track["id"].as_str()?;
    match Source::of_id(id) {
        Source::YouTube => Some(crate::music::Cover { art: crate::youtube::thumbnail(id.strip_prefix(crate::youtube::ID_PREFIX)?), link: None }),
        Source::Demo => None,                       // (never looked up: the demo isn't in anyone's catalogue)
        Source::Spotify | Source::Music | Source::Other =>
            crate::music::cover_lookup(track["artist"].as_str()?, track["name"].as_str()?),
    }
}

/// Per-app values learned on this Mac (e.g. how late an app's jump lands), as atomics indexed by source.
pub struct PerSource(pub [std::sync::atomic::AtomicI64; 5]);

impl PerSource {
    pub const fn new(v: i64) -> PerSource {
        use std::sync::atomic::AtomicI64;
        PerSource([AtomicI64::new(v), AtomicI64::new(v), AtomicI64::new(v), AtomicI64::new(v), AtomicI64::new(v)])
    }
    pub fn get(&self, s: Source) -> i64 {
        self.0[s.index()].load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn set(&self, s: Source, v: i64) {
        self.0[s.index()].store(v, std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apple_event_that_takes_too_long_is_stopped() {
        let t = std::time::Instant::now();
        assert!(run_timed("delay 5", std::time::Duration::from_millis(300)).is_none());
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "waited for the slow script");
        let out = run_timed("return \"hi\"", std::time::Duration::from_secs(5)).expect("a quick script finishes");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
    }

    #[test]
    fn an_app_that_did_not_answer_is_left_alone_for_a_while() {
        assert!(!resting("TestApp"));
        rest("TestApp");
        assert!(resting("TestApp") && !resting("OtherApp"));
    }

    #[test]
    fn the_demo_song_is_onkeys_own_and_always_controlled() {
        assert_eq!(Source::of_id("demo:wellerman"), Source::Demo);
        assert_ne!(Source::of_id("spotify:track:demo"), Source::Demo);
        assert!(is_song("demo:wellerman"));
        assert!(controllable(Source::Demo), "OnKey plays it: no permission to ask, in any mode");
        assert_eq!(Source::Demo.app_bundle(), None, "nothing to bring to the front or hide");
        assert_eq!(Source::Demo.bundle(), crate::demo::bundle(), "its sound is captured from OnKey's own process");
        assert!(cover_for(&serde_json::json!({"id": "demo:wellerman", "name": "Wellerman", "artist": "Traditional"})).is_none(),
                "no cover lookup over the network for it");
    }

    #[test]
    fn podcasts_and_ads_are_not_songs() {
        assert!(!is_song("spotify:episode:0ZLhKjfvOYlosTelLqBaqZ"));
        assert!(!is_song("spotify:ad:abc"));
        assert!(!is_song("spotify:chapter:abc"));
        assert!(is_song("spotify:track:6zk4lFEYIWs1UZZ03NVetT"));
        assert!(is_song("spotify:local:Artist:Album:Title:200"));
        assert!(is_song("isrc:SEAYD7601010") && is_song("music:99FF1F96B2427A01") && is_song("youtube:byxFUKxhT3s"));
    }

    #[test]
    fn automation_refusals_are_told_from_other_errors() {
        assert!(is_denial("execution error: Not authorized to send Apple events to Spotify. (-1743)"));
        assert!(is_denial("execution error: Music got an error: (-1744)"));
        // one refusal (or a few at once) is a hiccup; refused again and again over 5 s, it's off
        let mut n = 0;
        assert!(!really_refused(&mut n, 0.0));
        assert!(!really_refused(&mut n, 0.4));
        assert!(!really_refused(&mut n, 0.8), "3 at once");
        assert!(really_refused(&mut n, 5.2));
        assert!(!is_denial("execution error: Spotify got an error: Can't get current track. (-1728)"));
        assert!(!is_denial(""));
    }

    #[test]
    fn listen_only_never_talks_to_spotify_or_music() {
        let off = |_: &str| false;
        let on = |_: &str| true;
        let only_spotify = |k: &str| k == "control_spotify";
        // listening, control off ("Listen only"): neither app is sent anything, the browsers still are
        assert!(!may_send("Spotify", true, off) && !may_send("Apple Music", true, off));
        assert!(may_send("Safari", true, off) && may_send("Google Chrome", true, off));
        assert!(may_send("Spotify", true, on) && may_send("Apple Music", true, on));
        assert!(may_send("Spotify", true, only_spotify) && !may_send("Apple Music", true, only_spotify));
        // outside listening mode the apps are followed by asking them
        assert!(may_send("Spotify", false, off));
        // the names the apps' own modules use
        assert!(include_str!("spotify.rs").contains(r#"crate::source::osa("Spotify", script)"#));
        assert!(include_str!("music.rs").contains(r#"crate::source::osa("Apple Music", script)"#));
    }
}
