pub mod analysis;
pub mod bleed;
pub mod capture;
pub mod ctc;
pub mod links;
pub mod beats;
pub mod dsp;
pub mod game;
pub mod gpu;
pub mod demo;
pub mod devtest;
#[cfg(any(windows, target_os = "linux", test))]
pub mod digest;
pub mod discard;
#[cfg(any(feature = "devtest", test))]
pub mod ear;
#[cfg(feature = "devtest")]
pub mod mirror;
pub mod engine;
pub mod learn;
pub mod listen;
pub mod lowmem;
pub mod landmark;
#[cfg(all(test, target_os = "macos"))]
mod landmark_bench;
pub mod lyrics;
pub mod models;
pub mod lyrics_align;
pub mod mdx;
pub mod music;
pub mod notmusic;
pub mod notes;
pub mod permissions;
pub mod platform;
pub mod pitch;
pub mod progress;
pub mod range;
pub mod recognize;
pub mod records;
pub mod report;
pub mod resample;
pub mod settings;
pub mod share;
// (drawn in Swift on macOS: checked there against it)
#[cfg(any(windows, test))]
pub mod share_draw;
pub mod source;
pub mod spotify;
pub mod store;
pub mod syllables;
pub mod take;
pub mod takeaudio;
pub mod timing;
pub mod updater;
pub mod tracker;
pub mod trackids;
pub mod viz;
pub mod voice;
pub mod youtube;

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::{Manager, State};

type Shared = Arc<Mutex<engine::Shared>>;

/// The app whose music OnKey follows: in listening mode, the one playing sound (whatever a song's id says).
fn following() -> source::Source {
    // the demo song, while it plays: OnKey's own player, in every mode
    if demo::on() {
        return source::Source::Demo;
    }
    if settings::listening() {
        // nothing heard yet (a first launch, before any music): the app OnKey follows otherwise, if it's open
        listen::want_bundle().map_or_else(|| Some(source::active()).filter(|a| source::app_running(a.bundle())).unwrap_or(source::Source::Other),
                                          listen::source_of)
    } else {
        source::active()
    }
}

/// In listening mode, a control needs your permission for that app: the answer the window shows instead of
/// doing it (which app, the setting that allows it, and the song, which can be opened there without control).
fn needs_control(app: source::Source, track: Option<&str>) -> Option<Value> {
    // (nothing heard and no music app open: nothing to control, and no app to ask about; the key does nothing)
    if source::controllable(app) || (app == source::Source::Other && listen::want_bundle().is_none()) {
        return None;
    }
    let (key, name) = source::control_key(app).unwrap_or(("", listen::want_bundle().map_or("your music app", listen::app_name)));
    Some(serde_json::json!({"needs": {"key": key, "app": name, "track": track}}))
}

/// Play, pause, skip or start over, in the music app being followed.
#[tauri::command(async)]
fn player_control(action: String) -> Value {
    let app = following();
    if let Some(n) = needs_control(app, None) {
        return n;
    }
    app.control(&action);
    serde_json::json!({"ok": true})
}

/// The app being listened to now (listening mode), if it's one OnKey knows.
fn running_app() -> Option<source::Source> {
    listen::want_bundle().map(listen::source_of).filter(|s| *s != source::Source::Other)
}

/// Start a song (the Library's Play, Sing it again): in the app its "in Apple Music ↗" link names, by its id there
/// (see `store::play_plan`). The answer: `ok` with the app's name and the id it was played by there; `needs` (listening mode, control not allowed:
/// the window asks); or `cant` with the app's name and why (`no_id`: OnKey knows no id for it there, `failed`:
/// the app didn't play it), and the window says so with a button to open it there.
#[tauri::command(async)]
fn play_track(id: String, window: tauri::WebviewWindow, store: State<store::Store>) -> Value {
    let open = |s: source::Source| match s {
        source::Source::Spotify => spotify::running(),
        source::Source::Music => music::running(),
        _ => false,
    };
    let (app, play) = match store.play_plan(&id, running_app(), open) {
        store::PlayPlan::Start { app, id } => (app, id),
        store::PlayPlan::NoId { app } => {
            engine::log(&format!("can't start {id}: no id for it in {}", app.name()));
            return serde_json::json!({"cant": {"app": app.name(), "why": "no_id", "track": id}});
        }
    };
    if let Some(n) = needs_control(app, Some(&id)) {
        return n;
    }
    platform::spotify::opening(&play, || store.open_pick(&id, None).1);
    let was_open = open(app);
    let t = std::time::Instant::now();
    let played = source::play_track(&play);
    // one line per start, so a song that "starts twice" shows whether OnKey asked twice or the app restarted it
    engine::log(&format!("asked {} to play {id} ({}, {} ms){}", app.name(),
        if was_open { "was open" } else { "wasn't open" }, t.elapsed().as_millis(), if played { "" } else { ": failed" }));
    if !played {
        return serde_json::json!({"cant": {"app": app.name(), "why": "failed", "track": id}});
    }
    keep_in_front(window, app);
    // (`id`: the id it's played by there, which the song shows by while it plays)
    serde_json::json!({"ok": true, "app": app.name(), "id": play})
}

/// Open a song in your music app without controlling it (listening mode, control not allowed; the Library's and
/// Sing page's "in Spotify ↗"): the app it was last played in, by its id there when known (see `store::pick_app`).
/// You press play there; OnKey hears it.
#[tauri::command]
fn open_in_app(id: String, store: State<store::Store>) {
    // the demo song has no app of its own: OnKey plays it
    if demo::is_demo(&id) {
        source::play_track(&id);
        return;
    }
    let (pick, track) = store.open_pick(&id, running_app());
    if pick.app == source::Source::YouTube {
        listen::youtube_used();         // you'll play it in your browser: its sound is listened to for a while
    }
    let url = open_url(&pick, track.as_ref(), platform::sys::region().as_deref());
    platform::spotify::opening(&url, || track.clone());
    platform::shell::open_song_link(&url);
}

/// The app a song opens in ("Spotify", "Apple Music", "YouTube"), for its "in Spotify ↗" link.
#[tauri::command]
fn open_app(id: String, store: State<store::Store>) -> &'static str {
    store.open_pick(&id, running_app()).0.app.name()
}

/// What `open` is given for a pick: Spotify's track (spotify:track:…) or a search there, the YouTube video or a
/// search, the song's Apple Music page or a search there. An id OnKey made up (Windows, trackids.rs) is searched for.
fn open_url(pick: &store::Pick, track: Option<&source::Track>, region: Option<&str>) -> String {
    let q = urlencoding::encode(&track.map(|t| format!("{} {}", t.artist, t.name)).unwrap_or_default()).into_owned();
    // (ids checked as play_track checks them: `open` is never handed anything else, an option or another kind of
    // address; nor an id OnKey made up)
    let spotify = |id: &&String| id.starts_with("spotify:track:") && !trackids::is_media_id(id)
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':');
    let video = |id: &&String| !trackids::is_media_id(id) && id.strip_prefix(youtube::ID_PREFIX)
        .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    match (pick.app, pick.id.as_ref()) {
        (source::Source::Spotify, Some(id)) if spotify(&id) => id.clone(),
        (source::Source::YouTube, Some(id)) if video(&id) => format!("https://www.youtube.com/watch?v={}", id.strip_prefix(youtube::ID_PREFIX).unwrap_or("")),
        (source::Source::YouTube, _) => format!("https://www.youtube.com/results?search_query={q}"),
        (source::Source::Music, _) => match track.and_then(|t| t.link.as_deref()).and_then(music::apple_music_link) {
            Some(page) => music::apple_music_opens(&page, region)[0].clone(),
            None if q.is_empty() => "music://".to_string(),
            None => format!("music://music.apple.com/search?term={q}"),
        },
        (source::Source::Spotify | source::Source::Other | source::Source::Demo, _) => format!("spotify:search:{q}"),
    }
}

/// The demo song bundled with this build ("Try it now": {id, name, artist, duration, credit, licence}), or null
/// when there's none (the window then doesn't offer it).
#[tauri::command]
fn demo_info() -> Value {
    demo::info()
}

/// Let OnKey control an app from now on (listening mode): "control_spotify", "control_music" or "youtube".
#[tauri::command(async)]
fn allow_control(key: String) {
    match key.as_str() {
        "youtube" => youtube::set_enabled(true),
        "control_spotify" | "control_music" => settings::set(&key, true),
        _ => {}
    }
}

/// Spotify brings itself to the front when told to play a track. For the next few seconds (longer if it
/// had to start), whenever the track's app does, hide it again (it keeps playing) and give OnKey back the focus.
fn keep_in_front(window: tauri::WebviewWindow, app: source::Source) {
    let Some(bundle) = app.app_bundle() else { return };   // YouTube is opened in the background
    std::thread::spawn(move || {
        // checked every 15 ms for the first 2 s (so it's hidden before it has fully appeared), then every 100 ms
        for i in 0..150 {
            std::thread::sleep(std::time::Duration::from_millis(if i < 133 { 15 } else { 100 }));
            platform::app::hide_if_front(&window, bundle);
        }
    });
}

/// Your own artist and title for a YouTube video whose names were read wrong: shown, and used to find its lyrics.
#[tauri::command]
fn rename_track(id: String, artist: String, name: String) -> Result<(), String> {
    youtube::rename(&id, &artist, &name)
}

#[tauri::command]
fn set_blind(on: bool, shared: State<Shared>) {
    shared.lock().unwrap().blind = on;
}

/// Listening back to a take of a song kept under `id`: the app it plays in, its id there (None: none known, so
/// OnKey can't ask the app to play it), and the id the engine knows it by while it plays (`seen`, what
/// `play_along` is compared with). A song kept by its ISRC (named by Shazam) is played in the app it opens in, by
/// its id there (see `store::pick_app`), as "Sing it again" plays it. `kept`: the id the song is kept under, when
/// it's learned. In listening mode the engine names a learned song by that id whichever app plays it (the app's
/// own id is swapped for it); otherwise it follows the app's own id.
#[derive(Debug, PartialEq)]
struct Along {
    app: source::Source,
    play: Option<String>,
    seen: String,
}

/// Is `playing` (the app's id for its current song) the song being listened back to (`track`, as `along_of` took it)?
fn is_along_song(playing: &str, track: &str, along: &Along) -> bool {
    playing == track || playing == along.seen || along.play.as_deref() == Some(playing)
}

fn along_ids(id: &str, pick: &store::Pick, kept: Option<&str>, listening: bool) -> Along {
    let own = source::Source::of_id(id);
    let (app, play) = if own == source::Source::Other { (pick.app, pick.id.clone()) } else { (own, Some(id.to_string())) };
    let seen = if listening { kept.unwrap_or(id).to_string() } else { play.clone().unwrap_or_else(|| id.to_string()) };
    Along { app, play, seen }
}

fn along_of(id: &str, store: &store::Store) -> Along {
    let (pick, song) = store.open_pick(id, running_app());
    along_ids(id, &pick, song.as_ref().map(|t| t.id.as_str()), settings::listening())
}

/// Listening back to a take of the song the engine knows as `seen`: no new take while it plays. `told`: the app
/// has been told where to play already (else `told_app` says when it has): the song's place is found afresh from then.
fn listen_back(shared: &Shared, seen: String, told: bool) {
    let mut sh = shared.lock().unwrap();
    sh.play_along = Some(seen);
    sh.along_stopping = false;
    sh.along_from = told.then(std::time::Instant::now);
    sh.along_place = None;
    // heard once the music app plays it: until then the song that was playing (the app takes a moment to switch)
    // mustn't end the listening back (live: it did, and the requested song became a new take)
    sh.along_heard = false;
}

/// Listen back to a take with the song: play the track in its app from this position (s) without recording a
/// new take while it plays. `track`: the take's id or the app's own id for it (`along_mode`'s play_id).
/// OnKey only tells the app what to do: where your recording plays is where the engine places the song, as it
/// places a take (`player_now`, engine::Along), from what the app says once it has been told.
#[tauri::command(async)]
fn play_along(track: String, position: f64, shared: State<Shared>, store: State<store::Store>, window: tauri::WebviewWindow) -> Value {
    let along = along_of(&track, &store);
    if let Some(n) = needs_control(along.app, Some(&track)) {
        return n;
    }
    // no id in its app to play it by (along_mode said so: the window joins in instead)
    let Some(play) = along.play else { return serde_json::json!({"ok": false}) };
    listen_back(&shared, along.seen, false);
    let shared = shared.inner().clone();
    let store = store.inner().clone();
    std::thread::spawn(move || {
        let app = along.app;
        // (the app itself says its own id, not the one the song is kept under)
        // the song is started once: the old pause, seek and play again after starting it made Spotify start it two
        // or three times. The song's place is found afresh once the app has been told (told_app)
        let now = app.state();
        if now.as_ref().map_or(true, |s| s.track.id != play) {
            engine::log(&format!("play with the song: starting {play} in {}, then to {position:.1} s", app.name()));
            platform::spotify::opening(&play, || store.open_pick(&track, None).1);
            let played = source::play_track(&play);
            keep_in_front(window, app);
            // (until the app plays it: a seek before then would move the song before)
            let t = std::time::Instant::now();
            while played && app.state().map_or(true, |s| s.track.id != play) && t.elapsed() < std::time::Duration::from_secs(3) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // from the start: it's there already, no seek (a seek right after starting can make it start again);
            // not played: no seek either (it would move whatever the app has instead)
            if played && position > 1.0 {
                app.seek(position);
            }
        } else {
            let playing = now.is_some_and(|s| s.playing);
            engine::log(&format!("play with the song: {play} already in {}, to {position:.1} s{}", app.name(), if playing { "" } else { " and play" }));
            app.seek(position);
            if !playing {
                app.control("play");
            }
        }
        told_app(&shared);
    });
    serde_json::json!({"ok": true})
}

/// How "Play with the song" can work for this track: with control of its app (one click: OnKey starts it and
/// goes to the place), or, in listening mode without control, by joining in (you play the song in its app;
/// OnKey hears where it is and plays your recording in step). The app's name, for the window to say which, and
/// `play_id`: the id the app plays it by (the window starts it with that, and knows the player's track by it).
#[tauri::command(async)]
fn along_mode(track: String, store: State<store::Store>) -> Value {
    let along = along_of(&track, &store);
    let name = match along.app {
        source::Source::Other => listen::want_bundle().map_or("your music app", listen::app_name),
        source::Source::YouTube => "your browser",
        a => a.name(),
    };
    // without the song's id in its app, OnKey can't ask the app to play it: allowing control wouldn't help
    let key = along.play.as_ref().and(source::control_key(along.app)).map(|k| k.0);
    serde_json::json!({"control": source::controllable(along.app) && along.play.is_some(), "app": name, "key": key, "play_id": along.play})
}

/// Listening back by joining in (listening mode, no control): no new take while this track plays, and the
/// recording follows the song wherever you play it in its app, placed as a take places it (the same rule as
/// `play_along`, which only also tells the app to play).
#[tauri::command]
fn join_along(track: String, shared: State<Shared>, store: State<store::Store>) {
    let along = along_of(&track, &store);
    if along.app == source::Source::YouTube {
        listen::youtube_used();
    }
    listen_back(&shared, along.seen, true);
}

/// Listening back: the app has been told where to play (play_along's thread), so the song's place is found from
/// what it says from now (engine::Along). Only for the listening back still under way.
fn told_app(shared: &Shared) {
    let mut sh = shared.lock().unwrap();
    if sh.play_along.is_some() && sh.along_from.is_none() {
        sh.along_from = Some(std::time::Instant::now());
    }
}

/// Stop listening back (the music pauses; plays after this are takes again). Listening back ends once the music
/// is paused or moves on (engine::along_follows): the song playing on meanwhile isn't a new take.
#[tauri::command(async)]
fn stop_play_along(shared: State<Shared>, store: State<store::Store>) {
    let was = {
        let mut sh = shared.lock().unwrap();
        sh.along_stopping = sh.play_along.is_some();
        sh.play_along.clone()
    };
    if let Some(track) = was {
        // (it may be the id the song is kept under: its app is the one it was played in)
        let along = along_of(&track, &store);
        let app = along.app;
        // only while that song is still the one playing: leaving the take's page for another song's Sing would
        // otherwise pause the new song a moment after it started (live, 29 Oct: Spotify's answer took seconds)
        if source::controllable(app) && app.state().is_some_and(|s| s.playing && is_along_song(&s.track.id, &track, &along)) {
            app.control("playpause");
        }
    }
}

/// The window is playing one of your takes back (or stopped): while it plays, listening mode doesn't listen to
/// WebKit's sound if OnKey's own can't be told apart from Safari's.
#[tauri::command]
fn take_playing(on: bool) {
    listen::set_take_playing(on);
}

/// Development tools in the window (marked in purple): only in a development build started by `./run`
/// (ONKEY_DEV_TOOLS=1); a release never has them.
#[tauri::command]
fn dev_tools() -> bool {
    dev_on()
}

/// Whether this is a development build started with the development tools (`dev_tools`).
pub(crate) fn dev_on() -> bool {
    cfg!(feature = "devtest") && std::env::var("ONKEY_DEV_TOOLS").is_ok_and(|v| v == "1")
}

/// Development tools of OnKey's private development checkout (its dev/ folder, which isn't published): `name` and
/// its `args`, answered by devtest::dev_hook. Null in every build of the published source, and outside a development
/// build started by `./run`.
#[tauri::command(async)]
fn dev_hook(name: String, args: Value) -> Value {
    if !dev_on() {
        return Value::Null;
    }
    devtest::dev_hook(&name, &args)
}

/// Development tools: the ear (ear.rs) listens to "Play with the song" while it plays this take (`on`), measuring
/// the gap you hear between the song and your voice from what the music app and the window actually output, and
/// says it in the log every 2 s; off, its summary. Nothing outside a development build started by `./run`.
#[tauri::command(async)]
#[allow(unused_variables)]
fn dev_ear(take: String, on: bool, shared: State<Shared>, store: State<store::Store>) {
    #[cfg(feature = "devtest")]
    {
        if !dev_on() {
            return;
        }
        if !on {
            return ear::stop();
        }
        let Some(r) = store.take_result(&take) else { return engine::log(&format!("ear: no take {take}")) };
        let Some(dir) = store.take_dir(&take) else { return };
        let Some(mic) = takeaudio::recording(&dir) else { return engine::log(&format!("ear: take {take} has no recording kept")) };
        let Ok(track) = serde_json::from_value::<source::Track>(r["track"].clone()) else { return };
        let song = store.song_key(&track);
        let along = along_of(&track.id, &store);
        let app_name = match along.app {
            source::Source::Spotify => "Spotify",
            source::Source::Music => "Music",
            source::Source::YouTube => "YouTube",
            source::Source::Demo => "demo",
            source::Source::Other => "app",
        };
        let mut ids = vec![track.id.clone(), song.clone(), along.seen.clone()];
        ids.extend(along.play.clone());
        ear::start(ear::Take {
            id: take,
            track_id: track.id.clone(),
            ids,
            frame: ear::Frame { song_offset: r["song_offset"].as_f64().unwrap_or(0.0), latency: r["latency_ms"].as_f64().unwrap_or(0.0) / 1000.0 },
            mic,
            onsets: store.load_song(&song).map(|l| l.onset).unwrap_or_default(),
            bundle: along.app.bundle().to_string(),
            app_name: app_name.to_string(),
            shared: shared.inner().clone(),
        });
    }
}

/// Development tools: put `text` on the clipboard (a take's link, say).
#[tauri::command]
fn dev_copy(text: String) -> bool {
    dev_tools() && platform::shell::copy(&text)
}

/// "Challenge a friend": a challenge link (ui/lib/challenge.ts) on the clipboard. Only a singonkey.app/c/ link, and
/// a short one, so this can't be used to put anything else there. Whether it was copied.
#[tauri::command]
fn copy_challenge(link: String) -> bool {
    challenge_link_ok(&link) && platform::shell::copy(&link)
}

fn challenge_link_ok(link: &str) -> bool {
    link.starts_with("https://singonkey.app/c/") && link.len() <= 1000 && !link.chars().any(char::is_control)
}

/// Where the music app is right now: track, playing, position (s, estimated to this moment). And, listening back,
/// where the song is (`heard`, s, null while it isn't playing or placed yet): placed as a take places it (engine::Along:
/// from the app's position or listening's clock, then the music's own fingerprint), running on between audio
/// deliveries (up to a quarter second). "Play with the song" plays your recording there, as a take's bars are shown
/// there.
#[tauri::command]
fn player_now(shared: State<Shared>) -> Option<serde_json::Value> {
    let (id, playing, pos, at) = shared.lock().unwrap().player_now.clone()?;
    let p = if playing { pos + at.elapsed().as_secs_f64() } else { pos };
    // the demo song: OnKey's own player plays the very audio the song was learned from, so its position is the
    // song's (no other clock to be off), and listening steps aside while it plays (demo.rs): it's what's heard
    // (`feed`: listening's timing, for the log only: [age of its last feed, that block's length, how far the audio
    // fed has fallen behind the wall clock], s; see listen::timing)
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    let feed = listen::timing().map(|(age, block, behind)| [r3(age), r3(block), r3(behind)]);
    // (`voice`: where your recording is, read with the song's place, so the two are compared at one moment)
    let voice = voice::position();
    if crate::demo::is_demo(&id) {
        return Some(serde_json::json!({"track": id, "playing": playing, "position": p, "heard": playing.then_some(p), "demo": true, "feed": feed, "voice": voice}));
    }
    let (along, take) = { let sh = shared.lock().unwrap(); (sh.along_place, sh.take_place) };
    let heard = along.filter(|_| playing).map(|(s, at)| s + at.elapsed().as_secs_f64().min(0.25));
    // (`take`: the take under way's place, the same way, for development tools)
    let take = take.filter(|_| playing).map(|(s, at)| s + at.elapsed().as_secs_f64().min(0.25));
    Some(serde_json::json!({"track": id, "playing": playing, "position": p, "heard": heard, "take": take, "feed": feed, "voice": voice}))
}

/// "Play with the song": your recording, played natively (voice.rs), so where it is can be known exactly. Opens the
/// take's recording: its length (s).
#[tauri::command(async)]
fn voice_open(id: String, store: State<store::Store>) -> Option<f64> {
    voice::open(&takeaudio::recording(&store.take_dir(&id)?)?)
}
#[tauri::command(async)]
fn voice_play(from: f64) { voice::play(from) }
#[tauri::command(async)]
fn voice_stop() { voice::stop() }
#[tauri::command(async)]
fn voice_close() { voice::close() }
/// Louder or quieter (dB): the listening level the window works out, plus "Your voice".
#[tauri::command(async)]
fn voice_gain(db: f64) { voice::gain(db) }

/// What's happening now, for a window that has just loaded (engine::catch_up).
#[tauri::command]
fn catch_up() -> Vec<serde_json::Value> {
    engine::catch_up()
}

/// Score the play now (when the lyrics are over), without waiting for the song to end.
#[tauri::command]
fn finish_take(shared: State<Shared>) {
    shared.lock().unwrap().finish = true;
}

/// Remove the music's sound from the microphone (off: to test with the song playing from speakers).
/// Experimental YouTube support on/off (off by default: it asks to control your browser).
#[tauri::command]
fn set_youtube(on: bool) {
    youtube::set_enabled(on);
}

/// A setting saved across restarts (see settings.rs for the keys).
#[tauri::command]
fn set_setting(key: String, on: bool) -> Result<(), String> {
    if matches!(key.as_str(), "listening" | "control_spotify" | "control_music" | "hide_starters" | "shazam" | "show_lyrics" | "fade_guide") {
        settings::try_set(&key, on)?;
    }
    if key == "battery_30fps" || key == "voice_breaks" {
        settings::try_set(&key, on)?;
    }
    // keep the learning models ready between songs (lowmem::keep_learning_models); off: let go of them now
    if key == "keep_models" {
        let saved = settings::try_set(&key, on);
        if !on {
            std::thread::spawn(|| engine::release_learning_models(true));
        }
        saved?;
    }
    Ok(())
}

/// "Pause listening" (the Sing page's Live chip, the header's "Resume", ⌘⇧P from the menu): kept across restarts.
/// The engine, if napping, is woken, so it stops or starts within about half a second and the window hears so.
#[tauri::command]
fn set_listening_paused(on: bool) -> Result<(), String> {
    let saved = settings::try_set("listening_paused", on);     // (held until OnKey quits even if not saved)
    engine::wake();
    listening_menu();
    saved
}

/// The app menu's "Pause Listening" / "Resume Listening" (⌘⇧P), kept in step with the setting.
static LISTENING_ITEM: std::sync::OnceLock<tauri::menu::MenuItem<tauri::Wry>> = std::sync::OnceLock::new();

fn listening_menu() {
    if let Some(item) = LISTENING_ITEM.get() {
        let _ = item.set_text(if settings::listening_paused() { "Resume Listening" } else { "Pause Listening" });
    }
}

/// How strictly takes are scored from the next one on: "relaxed" (the default), "steady" or "pro" (dsp::LEVELS).
#[tauri::command]
fn set_scoring_level(level: String) {
    let l = dsp::Level::parse(Some(&level));
    settings::set_value("scoring_level", if l == dsp::Level::Relaxed { Value::Null } else { Value::String(l.name().into()) });
}

/// The microphones connected, the one OnKey listens to now (setup's mic check and the takes alike), and the one
/// chosen in Settings (null: automatic). Async: listing Core Audio's devices is kept off the window's thread.
#[tauri::command(async)]
fn mics() -> Value {
    // the chosen mic as listed, so it says whether it's virtual (chosen_mic doesn't look that up)
    let devices = capture::mics();
    let chosen = capture::chosen_mic().map(|c| devices.iter().find(|d| d.uid == c.uid).cloned().unwrap_or(c));
    serde_json::json!({"devices": devices, "chosen": chosen, "preferred": settings::get_str("mic")})
}

/// Listen to this microphone (a uid from `mics`), or automatically (null). The engine moves to it within 3 s.
#[tauri::command(async)]
fn set_mic(uid: Option<String>) {
    let uid = uid.filter(|u| !u.is_empty() && u.len() <= 512);
    capture::set_mic_preference(uid.as_deref().unwrap_or(""));
    settings::set_value("mic", uid.map_or(Value::Null, Value::String));
}

/// The output device now and how its delay is allowed for (timing.rs), and whether the music is playing (the
/// timing check asks for it paused first).
#[tauri::command]
fn output_timing(shared: State<Shared>) -> Value {
    let mut v = timing::status();
    v["music_playing"] = serde_json::json!(shared.lock().unwrap().player_now.as_ref().is_some_and(|p| p.1));
    v
}

/// A timing check of output `uid` done: its clicks, taps and the clocks they're on (timing::Run). Kept and
/// allowed for from now on when believable and confirmed; the result (or what to say) comes back.
#[tauri::command]
fn output_timing_check(uid: String, mut run: timing::Run) -> Value {
    run.clicks.truncate(64);
    run.taps.truncate(256);
    run.kinds.truncate(256);
    run.stamps.truncate(2000);
    run.now.truncate(2000);
    timing::check(&uid, &run)
}

/// Forget output `uid`'s timing check (Core Audio's figure again).
#[tauri::command]
fn output_timing_reset(uid: String) {
    timing::reset(&uid);
}

/// Don't suggest a timing check for output `uid` again.
#[tauri::command]
fn output_timing_dismiss(uid: String) {
    timing::dismiss(&uid);
}

#[tauri::command]
fn settings_now() -> Value {
    serde_json::json!({"listening": settings::listening(), "control_spotify": settings::control("control_spotify"),
        "control_music": settings::control("control_music"), "youtube": youtube::enabled(), "listen_only": settings::listen_only(),
        "hide_starters": settings::get("hide_starters"), "shazam": settings::shazam(), "show_lyrics": settings::show_lyrics(),
        "fade_guide": settings::fade_guide(),
        "keep_models": lowmem::keep_learning_models(), "keep_models_default": lowmem::keep_by_default(), "small_mac": lowmem::low_memory(),
        "battery_30fps": settings::get_opt("battery_30fps").unwrap_or(true),
        "voice_breaks": settings::get_opt("voice_breaks").unwrap_or(true), "scoring_level": dsp::Level::current().name()})
}

#[tauri::command]
fn set_echo_cancel(on: bool, shared: State<Shared>) {
    shared.lock().unwrap().echo_cancel = on;
}

/// Jump ahead to just before the singing resumes (song seconds).
#[tauri::command(async)]
fn skip_to(seconds: f64, shared: State<Shared>) -> Value {
    if let Some(n) = needs_control(following(), None) {
        return n;
    }
    shared.lock().unwrap().skip_to = Some(seconds);
    serde_json::json!({"ok": true})
}

/// Forget a learned song (its melody, fingerprint and lyrics). Your takes of it stay in History.
#[tauri::command]
fn delete_take(id: String, store: State<store::Store>) -> Result<(), String> {
    store.delete_take(&id)
}

#[tauri::command]
fn forget_song(id: String, store: State<store::Store>) -> Result<(), String> {
    let r = store.forget(&id);
    engine::forgotten(&id);         // (a play of it still being scored doesn't make it again)
    listen::songs_changed();        // (listening mode stops recognising it)
    r
}

/// Learn a song again on its next play (`on`), or take that back (store::learning_again). Also: whether it's
/// asked for, and where its learned copy looks off (`relearn`, from jumps seen on two plays; null if not).
#[tauri::command]
fn learn_again(id: String, on: Option<bool>, store: State<store::Store>) -> Result<serde_json::Value, String> {
    if let Some(on) = on {
        let r = store.set_learn_again(&id, on);
        engine::log(&format!("learn again {}: {id}{}", if on { "asked for" } else { "taken back" }, r.as_ref().err().map_or(String::new(), |e| format!(": couldn't ({e})"))));
        r?;
        listen::songs_changed();        // (its signature is set aside, or back)
    }
    let m = store.song_state(&id);
    Ok(m)
}

/// Stop learning a song and delete it (a learning card's "Stop learning and delete": a video that isn't a song,
/// say). `name` and `artist` find it when it's being kept under another id (its ISRC). See engine::stop_learning.
#[tauri::command]
fn stop_learning(id: String, name: Option<String>, artist: Option<String>, app: tauri::AppHandle, shared: State<Shared>, store: State<store::Store>) -> Result<(), String> {
    let ids = store.stop_learning_ids(&source::Track { id, name: name.unwrap_or_default(), artist: artist.unwrap_or_default(), ..Default::default() });
    let r = engine::stop_learning(&app, &shared, &store, &ids);
    listen::songs_changed();
    r
}

/// A video OnKey took for talk is a song after all: it's learned the next time it plays.
#[tauri::command]
fn learn_anyway(id: String) {
    discard::undo(&id);
}

/// Heart a song (or take its heart away). `name` and `artist` find the learned song when it's kept under another
/// id (by its ISRC, played from Spotify); a song still being learned gets its heart once it's saved.
#[tauri::command]
fn set_favorite(id: String, favorite: bool, name: Option<String>, artist: Option<String>, store: State<store::Store>) -> bool {
    store.set_favorite(&source::Track { id, name: name.unwrap_or_default(), artist: artist.unwrap_or_default(), ..Default::default() }, favorite)
}

#[tauri::command]
fn is_favorite(id: String, name: Option<String>, artist: Option<String>, store: State<store::Store>) -> bool {
    store.is_favorite(&source::Track { id, name: name.unwrap_or_default(), artist: artist.unwrap_or_default(), ..Default::default() })
}

#[tauri::command]
fn takes(store: State<store::Store>) -> Result<Vec<Value>, String> {
    store.readable(false)?;
    Ok(store.takes())
}

/// Learned songs (an error, not an empty list, if their folder or the takes' can't be read: see Store::readable).
#[tauri::command]
fn library(store: State<store::Store>) -> Result<Vec<Value>, String> {
    store.readable(true)?;
    let mut rows = store.library();
    range::annotate(&mut rows, &store, &store.root);
    Ok(rows)
}

/// Your vocal range from your takes (range.rs): null until there's enough singing.
#[tauri::command]
fn voice_range(store: State<store::Store>) -> Value {
    range::summary(&store.root)
}

#[tauri::command]
fn take(id: String, store: State<store::Store>) -> Option<Value> {
    let mut r = store.take_result(&id)?;
    // at the level chosen in Settings, as every score shown is (store::take_summary)
    analysis::at_level(&mut r, dsp::Level::current());
    // the song's stretches marked "Not the melody" (song seconds), for the take page to show and put back
    if let Ok(t) = serde_json::from_value::<source::Track>(r["track"].clone()) {
        let song = store.song_key(&t);
        r["song_id"] = json!(song);
        r["not_melody"] = json!(store.not_melody(&song).iter().map(|&(a, b)| [a, b]).collect::<Vec<_>>());
    }
    Some(r)
}

/// The take page's "Not the melody": a learned song's stretches (song seconds) whose learned notes follow a backing
/// vocal or a harmony, kept with the song (all of them, as they are now: none puts every one back), and every take
/// of it scored again with them left out. Returns the stretches as kept and how many takes' scores changed; None
/// when the song isn't learned.
#[tauri::command]
fn set_not_melody(id: String, stretches: Vec<(f64, f64)>, store: State<store::Store>) -> Option<Value> {
    let kept = store.set_not_melody(&id, &stretches)?;
    let takes = store.takes_of(&id);
    let changed = takes.iter().filter(|t| store.rescore_take(t, &kept).is_some()).count();
    engine::log(&format!("{id}: {} stretch{} marked not the melody; {changed} of {} take{} rescored", kept.len(),
                         if kept.len() == 1 { "" } else { "es" }, takes.len(), if takes.len() == 1 { "" } else { "s" }));
    Some(json!({"not_melody": kept.iter().map(|&(a, b)| [a, b]).collect::<Vec<_>>(), "rescored": changed}))
}

/// "Share this take": the clip to make, from the playhead `t` (s into the take, snapped down to a whole second of
/// the song), or with no `t` the take's best bit (the 20 s with the most notes hit). Song seconds: start, end.
#[tauri::command]
fn share_window(id: String, t: Option<f64>, store: State<store::Store>) -> Option<Value> {
    let take = share::Take::from_result(&store.take_result(&id)?)?;
    let start = match t {
        Some(t) => take.snap(t),
        None => take.best()?,
    };
    // (where it is on the take's graph, to show it there; and how it went, so the choice can be seen)
    Some(serde_json::json!({"start": start, "end": start + share::CLIP_S as i64, "at": share::clock(start),
        "to": share::clock(start + share::CLIP_S as i64), "graph": start as f64 - take.song_offset,
        "hits": take.hits(start), "notes": take.notes_in(start)}))
}

/// The take page's "Share this take" ask, at a proud moment (share::prompt): {"kind", "reason"}, or null.
#[tauri::command]
fn share_prompt(id: String, store: State<store::Store>) -> Option<Value> {
    share::prompt(&store, &id)
}

/// The ask for take `id` has been shown: it isn't shown again.
#[tauri::command]
fn share_prompt_seen(id: String, store: State<store::Store>) {
    share::prompt_seen(&store, &id)
}

/// Makes the share video of take `id` from song second `start` into Downloads, shows it in Finder and puts the
/// song's start time ("0:42") on the clipboard, for Instagram's song picker. The video's path.
#[tauri::command(async)]
fn share_take(id: String, start: i64, store: State<store::Store>) -> Result<Value, String> {
    let r = store.take_result(&id).ok_or("this take couldn't be found")?;
    let dir = store.take_dir(&id).ok_or("this take couldn't be found")?;
    let downloads = platform::shell::downloads();
    let t = std::time::Instant::now();
    let path = share::render(&r, &dir, start, &downloads)?;
    engine::log(&format!("share video of {} from {}: {} in {:.1} s", r["track"]["name"].as_str().unwrap_or("?"),
                         share::clock(start), path.display(), t.elapsed().as_secs_f64()));
    platform::shell::reveal(&path);
    platform::shell::copy(&share::clock(start));
    Ok(serde_json::json!({"path": path.to_string_lossy(), "at": share::clock(start)}))
}

/// Where a take's recording is, for the window to play it (through the asset protocol).
#[tauri::command]
fn take_audio(id: String, store: State<store::Store>) -> Option<String> {
    // mic.m4a, or an older take's mic.wav not converted yet (takeaudio.rs); none past the cap (takeaudio::retain)
    takeaudio::recording(&store.take_dir(&id)?).map(|p| p.to_string_lossy().into_owned())
}

/// Open a song's Apple Music page (an Apple cover or a Shazam match links to it) in the Music app, or Spotify's
/// search for it in Spotify.
/// Only those, and Settings' own links (`link_allowed`).
#[tauri::command]
fn open_link(url: String) {
    if !link_allowed(&url) {
        return;
    }
    // a Spotify link opens the Spotify app, which OnKey hears (open.spotify.com opened the web player in the
    // browser, which it doesn't); an Apple Music page opens in the Music app (it opened the website), in this
    // computer's own store (iTunes Search gives US-store pages). The web page (in that store too) only when the
    // app doesn't open it
    let (app, web) = match music::spotify_app_link(&url) {
        Some(app) => (Some(app), url),
        None if music::apple_music_link(&url).is_some() => {
            let [app, web] = music::apple_music_opens(&url, platform::sys::region().as_deref());
            (Some(app), web)
        }
        None => (None, url),
    };
    if !app.is_some_and(|a| platform::shell::open_app_link(&a)) {
        platform::shell::open(&web);
    }
}

/// Settings' own links, exactly these: the "Made by" site, OnKey's site, its guides and its Discord.
const FIXED_LINKS: [&str; 5] = ["https://dsmurrell.com", "https://singonkey.app", "https://singonkey.app/docs/",
    "https://singonkey.app/changelog/", "https://discord.gg/afHvqTNS6r"];

/// May `open_link` open this? A song's Apple Music page, a Spotify search or playlist, or one of Settings' own
/// links; nothing with spaces or control characters.
fn link_allowed(url: &str) -> bool {
    let allowed = FIXED_LINKS.contains(&url) || music::apple_music_link(url).is_some() || music::is_spotify_search(url)
        || music::is_spotify_playlist(url);
    allowed && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// A starter song's cover (the "Try these" row): kept in the app's data folder after the first look-up.
#[tauri::command(async)]
fn starter_cover(artist: String, title: String, app: tauri::AppHandle) -> Option<serde_json::Value> {
    let c = music::starter_cover(&app.path().app_data_dir().ok()?, &artist, &title)?;
    Some(serde_json::json!({"art": c.art, "link": c.link}))
}

/// The open-source licences and notices for everything OnKey ships (THIRD_PARTY_NOTICES.md, a bundle resource).
#[tauri::command]
fn third_party_notices(app: tauri::AppHandle) -> Result<String, String> {
    let path = app.path().resource_dir().map_err(|e| e.to_string())?.join("THIRD_PARTY_NOTICES.md");
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The window's drawing while a song plays (frame intervals, frames dropped, how late updates reach it), every few
/// seconds: into the records (records.rs).
#[tauri::command]
fn perf_frames(report: Value) {
    records::window_frames(&report);
}

/// "Report a problem": put OnKey's records together in a folder of their own (report.rs), about a take when given:
/// what's in it, for the window to show before anything is saved.
#[tauri::command(async)]
fn report_prepare(take: Option<String>, app: tauri::AppHandle) -> Result<Value, String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let take = take.filter(|t| store::valid_take_id(t));
    report::prepare(&data, take.as_deref(), 3)
}

/// Save the report prepared, with your description: macOS's save panel (~/Desktop/OnKey-report-<date>.zip by
/// default). Where it was saved (with ~ for the home folder), or None if cancelled.
#[tauri::command(async)]
fn report_save(description: String, app: tauri::AppHandle) -> Result<Option<String>, String> {
    let Some(to) = platform::app::report_destination(&app) else { return Ok(None) };
    let to = if to.extension().is_some_and(|x| x == "zip") { to } else { to.with_extension("zip") };
    report::save(&description, &to)?;
    engine::log("report: a problem report was saved");
    *SAVED_REPORT.lock().unwrap_or_else(|e| e.into_inner()) = Some(to.clone());
    platform::app::report_saved(&to);
    let home = platform::shell::home();
    let shown = to.to_string_lossy().into_owned();
    Ok(Some(if !home.is_empty() && shown.starts_with(&home) { shown.replacen(&home, "~", 1) } else { shown }))
}

/// The report's files in Finder, to look through before saving.
#[tauri::command(async)]
fn report_open_folder() {
    if let Some(d) = report::staged() {
        platform::shell::open(&d);
    }
}

/// The saved report, selected in Finder (`path` as report_save gave it).
#[tauri::command(async)]
fn report_show(path: String) {
    // the report this OnKey saved, not the path the window sends (which could name any zip on the computer)
    let _ = path;
    let saved = SAVED_REPORT.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(p) = saved.filter(|p| p.exists()) {
        platform::shell::reveal(&p);
    }
}

/// The last report saved (report_save), for report_show.
static SAVED_REPORT: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// (Windows) ONKEY_REPORT_TO=<path.zip>: a report made as "Report a problem" makes it, saved there 30 s after launch,
/// with no window: the CI's launch check keeps what a tester would send. Local only, like any report. (Nothing
/// without that variable.)
fn report_for_launch_check(data: std::path::PathBuf) {
    let Some(to) = std::env::var_os("ONKEY_REPORT_TO").map(std::path::PathBuf::from) else { return };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let made = report::prepare(&data, None, 3).and_then(|_| report::save("(made by the launch check)", &to));
        engine::log(&match made {
            Ok(()) => "report: made for the launch check".to_string(),
            Err(e) => format!("report: couldn't make one for the launch check: {e}"),
        });
    });
}

/// A new GitHub issue for OnKey in the browser: the bug report form (.github/ISSUE_TEMPLATE/bug.yml), titled, with
/// this version filled in; the form asks for the zip to be attached (nothing is uploaded).
#[tauri::command(async)]
fn report_issue(take: Option<String>) {
    let b = records::build();
    let title = if take.is_some() { "Problem with a take" } else { "Problem report" };
    let version = format!("{} ({})", b["version"].as_str().unwrap_or("?"), b["sha"].as_str().unwrap_or("?"));
    let url = format!("{}?template=bug.yml&title={}&version={}", report::NEW_ISSUE_URL,
                      urlencoding::encode(title), urlencoding::encode(&version));
    platform::shell::open(&url);
}

/// Errors from the window, into the engine's log (so problems in the interface are visible too).
#[tauri::command]
fn ui_log(msg: String) {
    engine::log(&format!("ui: {msg}"));
}

/// The window shows a song playing and can be seen (true), or not (paused, finished, hidden): while you sing a
/// learned song and watch it, learning waits (engine::WINDOW_MOVING).
#[tauri::command]
fn ui_moving(moving: bool) {
    engine::WINDOW_MOVING.store(moving, std::sync::atomic::Ordering::Relaxed);
}

/// Mini mode: a small lyrics bar that floats above every app (and on every desktop), so you can use your Mac
/// while you sing. Off: back to the full window.
#[tauri::command]
fn set_mini(on: bool, window: tauri::WebviewWindow) -> Result<(), String> {
    use tauri::{LogicalPosition, LogicalSize};
    let e = |r: tauri::Result<()>| r.map_err(|e| e.to_string());
    // (Linux on Wayland: no app may place its own window or keep it above others there, so the bar stays where it is
    // and is moved by hand; logged once rather than asked for and silently ignored)
    #[cfg(target_os = "linux")]
    let floats = !platform::linux::diag::wayland();
    #[cfg(not(target_os = "linux"))]
    let floats = true;
    #[cfg(target_os = "linux")]
    if on && !floats {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| engine::log("mini mode: on Wayland the bar can't place itself or stay on top; drag it where you like"));
    }
    if on {
        e(window.unmaximize())?;
        e(window.set_min_size(Some(LogicalSize::new(320.0, 80.0))))?;
        e(window.set_size(LogicalSize::new(540.0, 92.0)))?;
        if let Some(m) = window.current_monitor().map_err(|e| e.to_string())?.filter(|_| floats) {
            let scale = m.scale_factor();
            let size = m.size().to_logical::<f64>(scale);
            let pos = m.position().to_logical::<f64>(scale);
            e(window.set_position(LogicalPosition::new(pos.x + size.width - 540.0 - 24.0, pos.y + 44.0)))?;
        }
        window_buttons(&window, false)?;        // no window buttons over the lyrics
        e(window.set_shadow(false))?;           // the card draws its own (the window around it is transparent)
        if floats {
            e(window.set_always_on_top(true))?;
            e(window.set_visible_on_all_workspaces(true))?;
        }
    } else {
        window_buttons(&window, true)?;
        e(window.set_shadow(true))?;
        e(window.set_always_on_top(false))?;
        e(window.set_visible_on_all_workspaces(false))?;
        e(window.set_min_size(Some(LogicalSize::new(900.0, 600.0))))?;
        e(window.maximize())?;
    }
    Ok(())
}

/// Show or hide the close/minimise/zoom buttons. Not `set_decorations`: on macOS turning decorations back on
/// resets the window to a standard title bar, dropping the overlay style, and on this transparent window that
/// title bar is see-through (whatever is behind the app shows above it).
fn window_buttons(window: &tauri::WebviewWindow, visible: bool) -> Result<(), String> {
    platform::app::window_buttons(window, visible)
}

#[tauri::command(async)]
fn open_privacy_settings() {
    // (system audio recording has no Settings page on Windows: loopback needs no permission)
    platform::shell::open_settings("audio");
}

/// First-run setup: the microphone's and system audio recording's permission, and whether setup was done.
#[tauri::command(async)]
fn setup_status() -> Value {
    permissions::status()
}

#[tauri::command(async)]
fn setup_request(what: String) -> bool {
    if what == "mic" {
        permissions::request_mic();
        true
    } else {
        permissions::request_audio()
    }
}

#[tauri::command(async)]
fn setup_open_settings(what: String) {
    permissions::open_settings(&what);
}

#[tauri::command(async)]
fn setup_meter(on: bool) -> bool {
    permissions::meter(on)
}

#[tauri::command(async)]
fn setup_level() -> f32 {
    permissions::level()
}

/// Setup's music-app step (and Settings' way out of the older way, following the apps by asking them): control of
/// Spotify and Apple Music on (macOS asks for each running one now) or off. Returns setup's status, with each app's permission, and what asking each
/// app answered ("asked": by app, see `permissions::ask_automation`; empty for listen only).
#[tauri::command(async)]
fn setup_control(allow: bool) -> Value {
    let asked = permissions::choose_control(allow);
    let mut st = permissions::status();
    st["asked"] = asked;
    st
}

#[tauri::command(async)]
fn setup_done(done: bool) {
    permissions::set_done(done);
}

/// Updates (updater.rs): this version, and whether a newer one is downloaded and waiting.
#[tauri::command]
fn update_status(app: tauri::AppHandle) -> Value {
    updater::status(&app)
}

/// Settings' "Check now".
#[tauri::command(async)]
fn update_check(app: tauri::AppHandle) -> Value {
    let _ = updater::check(&app);
    updater::status(&app)
}

/// Settings' "Check for updates automatically".
#[tauri::command]
fn update_set_auto(on: bool, app: tauri::AppHandle) -> Value {
    settings::set(updater::SETTING, on);
    updater::status(&app)
}

/// "Restart to update": now, or once the song playing is over.
#[tauri::command]
fn update_restart(app: tauri::AppHandle, shared: State<Shared>) -> Value {
    updater::restart_when_ready(app, shared.inner().clone(), QUIT_WAIT)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // whether the last session closed normally, read before this one's first log line (Windows: and a panic's or
    // crash's cause into the log from now on: a window app there has no console)
    let previous = platform::diag::starting();
    devtest::init();
    let shared: Shared = Arc::new(Mutex::new(engine::Shared { blind: false, echo_cancel: true, skip_to: None, finish: false, play_along: None, along_heard: false, along_stopping: false, player_now: None, along_from: None, along_place: None, take_place: None, takes: Vec::new() }));
    let builder = tauri::Builder::default()
        .manage(shared.clone())
        .plugin(tauri_plugin_updater::Builder::new().build());
    // the menu bar: the Mac's standard menus with "Report a Problem…" in Help; none on Windows (it would sit inside
    // the window: Report a problem is in Settings)
    platform::app::menu(builder)
        .invoke_handler(tauri::generate_handler![catch_up, player_control, play_track, set_blind, takes, library, voice_range, take, set_not_melody, take_audio, share_window, share_take, share_prompt, share_prompt_seen, open_privacy_settings, ui_log, ui_moving, skip_to, set_mini, forget_song, learn_again, dev_hook, stop_learning, learn_anyway, delete_take, set_echo_cancel, set_youtube, set_setting, set_listening_paused, set_scoring_level, settings_now, allow_control, open_in_app, along_mode, join_along, finish_take, play_along, stop_play_along, player_now, voice_open, voice_play, voice_stop, voice_close, voice_gain, links::link_take, dev_tools, dev_ear, dev_copy, copy_challenge, rename_track, take_playing, demo_info,
            setup_status, setup_request, setup_open_settings, setup_meter, setup_level, setup_done, setup_control, third_party_notices, perf_frames, report_prepare, report_save, report_open_folder, report_show, report_issue, open_link, starter_cover, set_favorite, is_favorite, open_app, mics, set_mic, output_timing, output_timing_check, output_timing_reset, output_timing_dismiss,
            update_status, update_check, update_set_auto, update_restart])
        .setup(move |app| {
            // the records first: this log session starts with the build and the Mac (records.rs)
            records::init(&app.path().app_data_dir()?);
            // (Windows) the GPUs and the one learning runs on, the last session's end if it wasn't a quit, WebView2,
            // the mic switches, the audio devices
            platform::diag::started(previous);
            report_for_launch_check(app.path().app_data_dir()?);
            // the takes' recordings saved as WAV before AAC: converted once, in the background, while idle
            takeaudio::start(app.path().app_data_dir()?);
            // older takes measured once for your range (range.rs), also in the background while idle
            range::start(app.path().app_data_dir()?);
            let store = store::Store::new(app.path().app_data_dir()?);
            app.manage(store.clone());
            // the settings first: YouTube's switch is one of them
            settings::init(&app.path().app_data_dir()?);
            listening_menu();
            capture::set_mic_preference(&settings::get_str("mic").unwrap_or_default());
            youtube::set_data_dir(&app.path().app_data_dir()?);
            platform::spotify::set_data_dir(&app.path().app_data_dir()?);
            discard::set_data_dir(&app.path().app_data_dir()?);
            lyrics::set_cache_dir(&app.path().app_data_dir()?);
            // the models compiled for Core ML, kept for good (compiled once, not again after a temp-folder clean)
            pitch::set_coreml_cache_dir(&app.path().app_data_dir()?);
            // songs named before ids with punctuation had file names of their own: renamed to theirs, once
            let renamed = store.rename_to_safe_names();
            if renamed > 0 {
                engine::log(&format!("library: {renamed} song{} renamed to a file name of its own", if renamed == 1 { "" } else { "s" }));
            }
            // songs kept twice (learned under two ids before songs were kept under one) become one, once
            let merged = store.merge_duplicates();
            if merged > 0 {
                engine::log(&format!("library: {merged} song{} kept twice merged", if merged == 1 { "" } else { "s" }));
            }
            // a podcast episode (or an ad) learned as if it were a song, before they were told apart: to the Trash
            for t in store.learned_tracks().into_iter().filter(|t| !source::is_song(&t.id)) {
                match store.forget(&t.id) {
                    Ok(()) => engine::log(&format!("library: {} ({}) isn't a song: removed (to the Trash)", t.name, t.id)),
                    Err(e) => engine::log(&format!("library: {} ({}) isn't a song, but stays: {e}", t.name, t.id)),
                }
            }
            // the apps' ids and the app played in, for songs learned before they were kept (once each)
            let filled = store.backfill_sources();
            if filled > 0 {
                engine::log(&format!("library: where {filled} song{} played found from History", if filled == 1 { "" } else { "s" }));
            }
            listen::init(store.clone());
            // the demo song ("Try it now"), if this build has one: installed into the store when first played
            demo::init(app.path().resource_dir().ok().as_deref(), store.clone(), &app.config().identifier);
            {
                let store = store.clone();
                std::thread::spawn(move || store.find_missing_covers());
            }
            // learning needs the separator, CREPE-full and the word timer, which ship inside the app (and only
            // there); if the set isn't whole (a damaged install), learned songs can be sung but new ones aren't
            // learned, and the window says so (models::state, in the engine's status)
            let bundled = app.path().resource_dir()?.join("models");
            let model_dir = match models::check(&bundled) {
                Ok(()) => Some(bundled),
                Err(e) => {
                    engine::log(&format!("learning models: {e}"));
                    models::set_state(serde_json::json!({"state": "missing", "error": e}));
                    None
                }
            };
            *engine::WORD_MODEL.lock().unwrap() = model_dir.as_ref().map(|d| d.join("wav2vec2-base-960h-fp16.onnx"));
            // the models load on the learner's own thread (seconds): a new take starts at once
            let learners: engine::LearnerFactory = Arc::new(move |rate| {
                let models = model_dir.clone()?;
                Some(learn::Learner::start_with(Box::new(move || {
                    let sep = mdx::Mdx::load(&models.join("UVR_MDXNET_KARA_2.onnx"), 4, crate::devtest::knob!("ONKEY_CPU").is_none())
                        .inspect_err(|e| engine::log(&format!("learner: the separation model couldn't be loaded: {e}")))?;
                    let pitch = pitch::Crepe::from_file(&models.join("crepe-full-b32.onnx"), 2, Some(32))
                        .map_err(|e| e.to_string())
                        .inspect_err(|e| engine::log(&format!("learner: the pitch model couldn't be loaded: {e}")))?;
                    Ok((Box::new(sep) as Box<dyn learn::Separator>, Box::new(pitch) as Box<dyn learn::VocalPitch>))
                }), rate))
            });
            #[cfg(feature = "devtest")]
            mirror::start(app.handle().clone(), store.clone());
            engine::keep_learned_lyrics(store.clone());
            engine::spawn(app.handle().clone(), shared.clone(), store, learners);
            updater::start(app.handle().clone());
            on_terminate();
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running OnKey")
        .run(|app, event| match event {
            // quitting (the last window closed, or an exit asked for): out of sight at once (quit_out_of_sight)
            tauri::RunEvent::ExitRequested { api, .. } if !engine::quitting() => {
                api.prevent_exit();
                quit_out_of_sight(app);
            }
            // ⌘Q again (or the last window closed) while it finishes out of sight: the quit going on exits when done.
            // (A requested exit, `code` Some, is the update's restart, which has waited with the quit itself.)
            tauri::RunEvent::ExitRequested { api, code: None, .. } => {
                api.prevent_exit();
            }
            // quit some other way (the Dock's Quit, logging out): the app can't hide while it waits here, so it
            // waits only a moment; a song still being learned is learned on its next play
            tauri::RunEvent::Exit => {
                engine::shutdown(QUIT_BRIEF);
                updater::install();
            }
            // macOS's own: opened again while quitting (a fresh copy starts as this one exits), an onkey:// link
            _ => platform::app::on_event(app, &event),
        });
}

/// How long a quit waits for the song playing (and any still finishing) to be learned and saved, when it has to be
/// quick: told to stop (logging out, `./run` restarting the development build), or restarting for an update.
const QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(25);

/// How long a quit the app can't hide during (RunEvent::Exit: the Dock's Quit, logging out) waits: the window
/// stays up and frozen meanwhile, so only a moment.
const QUIT_BRIEF: std::time::Duration = std::time::Duration::from_secs(2);

/// Quit (⌘Q, or the last window closed): OnKey looks gone at once (windows and Dock icon), and finishes learning
/// the song playing and any still waiting in the background before it exits. Songs learn one at a time, so
/// waiting 25 s lost a song queued behind another (Galway Girl, 19 Oct).
fn quit_out_of_sight(app: &tauri::AppHandle) {
    use tauri::Manager;
    for w in app.webview_windows().values() {
        let _ = w.hide();
    }
    platform::app::hide_dock(app);
    std::thread::spawn(|| {
        engine::shutdown(QUIT_LEARN_WAIT);
        updater::install();     // an update downloaded and not yet installed: the next launch is the new version
        relaunch_if_asked();
        std::process::exit(0);
    });
}

static RELAUNCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// OnKey was opened again while quitting: open a new copy of the app (a second after this one has gone).
fn relaunch_if_asked() {
    if !RELAUNCH.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    platform::app::relaunch();
}

/// How long a quit from the menu waits, out of sight, for every song being learned or waiting to be (each takes
/// up to a minute or so, and they all finish together once quitting).
const QUIT_LEARN_WAIT: std::time::Duration = std::time::Duration::from_secs(150);

/// Told to stop (SIGTERM: `kill`, `./run` restarting it, logging out): quit as from the menu, keeping what's been
/// learned, rather than at once.
fn on_terminate() {
    static TERM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    extern "C" fn handler(_: libc::c_int) {
        TERM.store(true, std::sync::atomic::Ordering::SeqCst);      // (all a signal handler may safely do)
    }
    unsafe {
        libc::signal(libc::SIGTERM, handler as *const () as libc::sighandler_t);
    }
    std::thread::spawn(|| {
        while !TERM.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        engine::log("told to stop (SIGTERM)");
        engine::shutdown(QUIT_WAIT);
        updater::install();
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn stopping_a_listen_back_leaves_another_song_playing() {
        let along = super::Along { app: crate::source::Source::Spotify, play: Some("spotify:track:dream".into()), seen: "take-1".into() };
        assert!(super::is_along_song("spotify:track:dream", "take-1", &along));
        assert!(super::is_along_song("take-1", "take-1", &along));
        assert!(!super::is_along_song("spotify:track:anotherlove", "take-1", &along), "a song started since is left alone");
    }

    #[test]
    fn settings_links_open_only_exactly_as_given() {
        assert!(super::link_allowed("https://discord.gg/afHvqTNS6r"));
        assert!(super::link_allowed("https://singonkey.app"));
        assert!(super::link_allowed("https://dsmurrell.com"));
        assert!(super::link_allowed("https://singonkey.app/docs/"));
        assert!(super::link_allowed("https://singonkey.app/changelog/"));
        assert!(!super::link_allowed("https://singonkey.app/docs/../c/"));
        assert!(!super::link_allowed("https://discord.gg/someoneelse"));
        assert!(!super::link_allowed("https://singonkey.app.evil.example"));
        assert!(!super::link_allowed("https://discord.gg/afHvqTNS6r --args"));
    }

    #[test]
    fn only_a_challenge_link_is_copied() {
        assert!(super::challenge_link_ok("https://singonkey.app/c/?ref=challenge#song=Yellow&score=91"));
        assert!(!super::challenge_link_ok("https://singonkey.app.evil.example/c/"));
        assert!(!super::challenge_link_ok("rm -rf ~"));
        assert!(!super::challenge_link_ok(&format!("https://singonkey.app/c/#{}", "x".repeat(2000))));
        assert!(!super::challenge_link_ok("https://singonkey.app/c/#a\nb"));
    }

    use super::*;
    use source::{Source, Track};
    use store::Pick;

    #[test]
    fn a_song_opens_by_its_id_or_a_search_in_its_app() {
        let t = Track { id: "isrc:USSM10603618".into(), name: "How to Save a Life".into(), artist: "The Fray".into(), ..Default::default() };
        let pick = |app, id: Option<&str>| Pick { app, id: id.map(String::from) };
        assert_eq!(open_url(&pick(Source::Spotify, Some("spotify:track:5fVZC9GiM4e8vu99W0Xf6J")), Some(&t), None), "spotify:track:5fVZC9GiM4e8vu99W0Xf6J");
        assert_eq!(open_url(&pick(Source::Spotify, None), Some(&t), None), "spotify:search:The%20Fray%20How%20to%20Save%20a%20Life");
        // an id OnKey made from the song's names (Windows) is no Spotify id: a search
        assert_eq!(open_url(&pick(Source::Spotify, Some("spotify:media:0123456789abcdef")), Some(&t), None), "spotify:search:The%20Fray%20How%20to%20Save%20a%20Life");
        assert_eq!(open_url(&pick(Source::YouTube, Some("youtube:abc")), Some(&t), None), "https://www.youtube.com/watch?v=abc");
        // an id that isn't one: a search, never handed to `open` as it is
        for bad in ["-aTerminal", "file:///Applications/Calculator.app", "spotify:track:x/../y"] {
            assert_eq!(open_url(&pick(Source::Spotify, Some(bad)), Some(&t), None), "spotify:search:The%20Fray%20How%20to%20Save%20a%20Life");
        }
        assert_eq!(open_url(&pick(Source::YouTube, Some("youtube:a&b")), Some(&t), None), "https://www.youtube.com/results?search_query=The%20Fray%20How%20to%20Save%20a%20Life");
        assert_eq!(open_url(&pick(Source::Music, None), Some(&t), Some("gb")), "music://music.apple.com/search?term=The%20Fray%20How%20to%20Save%20a%20Life");
        let linked = Track { link: Some("https://music.apple.com/us/album/x/1?i=2".into()), ..t.clone() };
        assert_eq!(open_url(&pick(Source::Music, Some("music:ABC")), Some(&linked), Some("gb")), "music://music.apple.com/gb/album/x/1?i=2",
                   "in the Music app, in the Mac's own store");
        assert_eq!(open_url(&pick(Source::Music, Some("music:ABC")), Some(&linked), None), "music://music.apple.com/us/album/x/1?i=2");
    }

    #[test]
    fn a_take_kept_by_its_isrc_plays_along_by_its_id_in_its_app() {
        let pick = |app, id: Option<&str>| Pick { app, id: id.map(String::from) };
        let isrc = "isrc:USWB19900123";
        let sp = "spotify:track:1TfqLAPs4K3s2rJMoCokcS";
        // kept by its ISRC, last played in Spotify: Spotify plays it by its own id; listening mode follows the song
        // by the id it's kept under, outside listening mode by Spotify's
        let a = along_ids(isrc, &pick(Source::Spotify, Some(sp)), Some(isrc), true);
        assert_eq!(a, Along { app: Source::Spotify, play: Some(sp.into()), seen: isrc.into() });
        assert_eq!(along_ids(isrc, &pick(Source::Spotify, Some(sp)), Some(isrc), false).seen, sp);
        // the window passes the app's id back (play_along): the same answer
        assert_eq!(along_ids(sp, &pick(Source::Spotify, Some(sp)), Some(isrc), true), a);
        // no id in its app: nothing to ask the app to play
        let none = along_ids(isrc, &pick(Source::Spotify, None), Some(isrc), true);
        assert_eq!(none, Along { app: Source::Spotify, play: None, seen: isrc.into() });
        // a song kept under the app's own id is played by it, in its own app (whatever the pick says)
        assert_eq!(along_ids("music:42", &pick(Source::Spotify, Some(sp)), Some("music:42"), true),
                   Along { app: Source::Music, play: Some("music:42".into()), seen: "music:42".into() });
        // not learned yet: listening mode doesn't rename it, the engine follows the app's id
        assert_eq!(along_ids(sp, &pick(Source::Spotify, Some(sp)), None, true).seen, sp);
    }
}
