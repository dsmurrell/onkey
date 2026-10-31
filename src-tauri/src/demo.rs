//! The demo song: one song OnKey has the rights to, shipped inside the app with its notes already learned, so a new
//! user can sing and be scored within seconds ("Try it now") instead of playing a whole song and waiting for it to
//! be learned first.
//!
//! It's the only music OnKey plays itself (native/demo.swift: an AVAudioPlayer in OnKey's own process). Everything
//! else is the path every song takes: OnKey is just another music app (`Source::Demo`), followed by asking it
//! where it is, like Spotify outside listening mode. Its sound is captured through a process tap of OnKey's own
//! process, sample-aligned with the microphone, so placement by fingerprint, echo cancellation on speakers, live
//! scoring, History and "Play with the song" all work unchanged.
//!
//! What ships, in the app's Resources/demo/ (made by `cargo run --release --example demosong`, see that file):
//!   demo.json        the manifest: id ("demo:…"), name, artist, length, audio file, version, credit and licence
//!   demo.m4a         the recording (any file Core Audio plays; the manifest names it)
//!   song.bin/.json   the learned song as store.rs keeps it (melody, fingerprint, which frames are learned), with
//!   song.syl.json    its syllable bars and lyrics (the words kept in the .json: the demo's lyrics are bundled, so
//!   song.words       LRCLIB is never asked), and the words as heard when it was learned
//! The learned files are copied into the store (songs/<id>.*) the first time it's played, and again when a new
//! version ships, so the demo is in the Library and its takes in History like any song.
//!
//! Development builds can point at another folder of the same shape with ONKEY_DEMO_DIR (a placeholder recording
//! in the lab that must never ship); release builds read only the bundled one.
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::source::{Source, State, Track};
use crate::store::Store;

/// A demo song's id starts with this.
pub const ID_PREFIX: &str = "demo:";
/// The manifest in a demo folder.
pub const MANIFEST: &str = "demo.json";
/// The learned song's files in a demo folder: store.rs's extensions, each as song.<ext>. The first two are needed.
pub const SONG_FILES: [&str; 4] = ["bin", "json", "syl.json", "words"];
/// Paused this long, the demo is put away (the engine ends a take paused for a minute) and OnKey listens for your
/// music app again.
const PAUSED_PUT_AWAY: Duration = Duration::from_secs(65);

pub fn is_demo(id: &str) -> bool {
    id.starts_with(ID_PREFIX)
}

/// demo.json.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub artist: String,
    /// seconds
    pub duration: f64,
    /// the recording's file name, beside the manifest
    pub audio: String,
    /// changes whenever the learned files do: a new version replaces the one installed
    pub version: String,
    /// who made the recording, shown with it ("Recording: …")
    #[serde(default)]
    pub credit: String,
    #[serde(default)]
    pub licence: String,
}

impl Manifest {
    pub fn track(&self) -> Track {
        Track { id: self.id.clone(), name: self.name.clone(), artist: self.artist.clone(), album: String::new(),
                duration: self.duration, art: None, link: None }
    }
}

/// The demo in `dir`, if it's whole: a manifest with a demo id, its recording and the learned song.
pub fn read_manifest(dir: &Path) -> Option<Manifest> {
    let m: Manifest = serde_json::from_slice(&std::fs::read(dir.join(MANIFEST)).ok()?).ok()?;
    let plain = |f: &str| !f.is_empty() && !f.contains('/') && !f.starts_with('.');
    let whole = is_demo(&m.id) && m.duration > 0.0 && plain(&m.audio) && dir.join(&m.audio).is_file()
        && SONG_FILES[..2].iter().all(|e| dir.join(format!("song.{e}")).is_file());
    whole.then_some(m)
}

/// Put the demo's learned song into the store (songs/<id>.*), unless this version is there already. A heart
/// given to it, and when it was last played, are kept. Returns whether anything was written.
pub fn install(dir: &Path, m: &Manifest, store: &Store) -> std::io::Result<bool> {
    let old: Option<Value> = std::fs::read(store.song_file(&m.id, "json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if old.as_ref().is_some_and(|o| o["demo_version"].as_str() == Some(m.version.as_str())) && store.song_file(&m.id, "bin").is_file() {
        return Ok(false);
    }
    let _writing = crate::store::writing();
    for ext in ["bin", "syl.json", "words"] {
        let src = dir.join(format!("song.{ext}"));
        let dst = store.song_file(&m.id, ext);
        if src.is_file() {
            crate::store::write_atomic(&dst, &std::fs::read(&src)?)?;
        } else if dst.exists() {
            std::fs::remove_file(&dst)?;      // (an older version's, made for other lyrics)
        }
    }
    let mut meta: Value = serde_json::from_slice(&std::fs::read(dir.join("song.json"))?)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    meta["track"] = json!(m.track());
    meta["demo_version"] = json!(m.version);
    for k in ["favorite", "favorited_at", "played"] {
        if let Some(v) = old.as_ref().map(|o| &o[k]).filter(|v| !v.is_null()) {
            meta[k] = v.clone();
        }
    }
    crate::store::write_atomic(&store.song_file(&m.id, "json"), meta.to_string().as_bytes())?;
    Ok(true)
}

/// What the player says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Playing {
    Playing,
    Paused,
    /// it came to the end by itself
    Ended,
}

/// The demo's player (native/demo.swift in the app; a stand-in in tests).
pub trait Player: Send {
    /// Open a recording, ready to play from its start: its length (s).
    fn open(&mut self, path: &Path) -> Option<f64>;
    fn play(&mut self);
    fn pause(&mut self);
    fn seek(&mut self, seconds: f64);
    fn close(&mut self);
    /// Where it is (s) and whether it plays; None with nothing open.
    fn state(&self) -> Option<(f64, Playing)>;
}

use crate::platform::native::*;

/// The AVAudioPlayer in native/demo.swift (on Windows, a WASAPI stream: platform/windows/demo.rs).
struct Native;

impl Player for Native {
    fn open(&mut self, path: &Path) -> Option<f64> {
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
        let d = unsafe { onkey_demo_open(c.as_ptr()) };
        (d > 0.0).then_some(d)
    }
    fn play(&mut self) { unsafe { onkey_demo_play() } }
    fn pause(&mut self) { unsafe { onkey_demo_pause() } }
    fn seek(&mut self, seconds: f64) { unsafe { onkey_demo_seek(seconds) } }
    fn close(&mut self) { unsafe { onkey_demo_close() } }
    fn state(&self) -> Option<(f64, Playing)> {
        let mut pos = 0.0;
        match unsafe { onkey_demo_state(&mut pos) } {
            1 => Some((pos, Playing::Playing)),
            0 => Some((pos, Playing::Paused)),
            2 => Some((pos, Playing::Ended)),
            _ => None,
        }
    }
}

/// A play of the demo, from "Try it now" (or Play in the Library) until it ends, is put away, or another song starts.
struct Session {
    track: Track,
    paused_at: Option<Instant>,
    /// it came to its end: the engine finishes its take (`ended_take`), then the session is over
    ended: bool,
}

/// The demo: where it is, and the play under way.
pub struct Demo {
    dir: Option<PathBuf>,
    player: Box<dyn Player>,
    session: Option<Session>,
}

impl Demo {
    pub fn new(dir: Option<PathBuf>, player: Box<dyn Player>) -> Demo {
        Demo { dir, player, session: None }
    }

    /// The demo bundled (or pointed at), if there's a whole one.
    pub fn manifest(&self) -> Option<Manifest> {
        read_manifest(self.dir.as_ref()?)
    }

    /// Play it from the start, installed into `store` first. The track it plays as.
    pub fn start(&mut self, store: &Store) -> Result<Track, String> {
        let dir = self.dir.clone().ok_or("no demo song in this build")?;
        let m = read_manifest(&dir).ok_or("the demo song's files aren't all there")?;
        install(&dir, &m, store).map_err(|e| format!("installing the demo song: {e}"))?;
        self.player.close();
        self.player.open(&dir.join(&m.audio)).ok_or("the demo song's recording couldn't be opened")?;
        self.player.play();
        let track = m.track();
        self.session = Some(Session { track: track.clone(), paused_at: None, ended: false });
        Ok(track)
    }

    /// Whether a play of the demo is under way (then OnKey follows it, not your music app).
    pub fn on(&self) -> bool {
        self.session.is_some()
    }

    /// What the demo is doing, as a music app's state; None when it isn't playing or paused.
    pub fn state(&mut self, now: Instant) -> Option<State> {
        let s = self.session.as_mut()?;
        let Some((pos, playing)) = self.player.state() else {
            self.session = None;
            return None;
        };
        if s.ended || playing == Playing::Ended {
            s.ended = true;
            return Some(State { playing: false, track: s.track.clone(), position: s.track.duration, source: Source::Demo });
        }
        if playing == Playing::Playing {
            s.paused_at = None;
        } else if now.duration_since(*s.paused_at.get_or_insert(now)) >= PAUSED_PUT_AWAY {
            self.stop();
            return None;
        }
        Some(State { playing: playing == Playing::Playing, track: s.track.clone(), position: pos, source: Source::Demo })
    }

    /// The demo came to its end: its track, once (the engine finishes the take), and the play is over.
    pub fn ended_take(&mut self) -> Option<Track> {
        let ended = self.session.as_ref().is_some_and(|s| s.ended)
            || (self.session.is_some() && self.player.state().is_some_and(|(_, p)| p == Playing::Ended));
        if !ended {
            return None;
        }
        let track = self.session.take().map(|s| s.track);
        self.player.close();
        track
    }

    /// "restart", "playpause", "pause", "play" (as `Source::control`); "next" ends it now.
    pub fn control(&mut self, action: &str) {
        let Some(s) = self.session.as_mut().filter(|s| !s.ended) else { return };
        let playing = self.player.state().is_some_and(|(_, p)| p == Playing::Playing);
        match action {
            "restart" | "previous" => {
                self.player.seek(0.0);
                self.player.play();
            }
            "next" => s.ended = true,
            "pause" => self.player.pause(),
            "play" => self.player.play(),
            "playpause" if playing => self.player.pause(),
            "playpause" => self.player.play(),
            _ => {}
        }
    }

    pub fn seek(&mut self, seconds: f64) {
        if self.session.as_ref().is_some_and(|s| !s.ended) {
            self.player.seek(seconds);
        }
    }

    /// Put it away (another song started from OnKey, or it was paused too long).
    pub fn stop(&mut self) {
        if self.session.take().is_some() {
            self.player.close();
        }
    }
}

static DEMO: Mutex<Option<Demo>> = Mutex::new(None);
static STORE: Mutex<Option<Store>> = Mutex::new(None);
/// OnKey's own bundle id: the demo's sound is captured from OnKey's process (see `bundle`).
static BUNDLE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// At start-up: the demo bundled in the app's resources (`resources`/demo), or in development builds the folder
/// ONKEY_DEMO_DIR names; the store it's installed into; and OnKey's bundle id.
pub fn init(resources: Option<&Path>, store: Store, bundle: &str) {
    let dir = crate::devtest::knob!("ONKEY_DEMO_DIR").map(PathBuf::from).or_else(|| resources.map(|r| r.join("demo")));
    let demo = Demo::new(dir, Box::new(Native));
    match demo.manifest() {
        Some(m) => crate::engine::log(&format!("demo song: {} - {} ({:.0} s, version {})", m.artist, m.name, m.duration, m.version)),
        None => crate::engine::log("demo song: none in this build"),
    }
    *DEMO.lock().unwrap() = Some(demo);
    *STORE.lock().unwrap() = Some(store);
    let _ = BUNDLE.set(bundle.to_string());
}

fn with<T>(f: impl FnOnce(&mut Demo) -> T) -> Option<T> {
    DEMO.lock().unwrap().as_mut().map(f)
}

/// The demo for the window ("Try it now"), or null when this build has none.
pub fn info() -> Value {
    with(|d| d.manifest()).flatten().map_or(Value::Null, |m| json!({"id": m.id, "name": m.name, "artist": m.artist,
        "duration": m.duration, "credit": m.credit, "licence": m.licence}))
}

/// Play the demo from the start (Source::play_track with a demo id).
pub fn start() {
    let Some(store) = STORE.lock().unwrap().clone() else { return };
    match with(|d| d.start(&store)) {
        Some(Ok(t)) => crate::engine::log(&format!("demo song: playing {} - {}", t.artist, t.name)),
        Some(Err(e)) => crate::engine::log(&format!("demo song: couldn't play it: {e}")),
        None => {}
    }
}

pub fn on() -> bool {
    with(|d| d.on()).unwrap_or(false)
}

pub fn state() -> Option<State> {
    with(|d| d.state(Instant::now())).flatten()
}

pub fn ended_take() -> Option<Track> {
    with(|d| d.ended_take()).flatten()
}

pub fn control(action: &str) {
    with(|d| d.control(action));
}

pub fn seek(seconds: f64) {
    with(|d| d.seek(seconds));
}

pub fn stop() {
    if let Some(true) = with(|d| { let was = d.on(); d.stop(); was }) {
        crate::engine::log("demo song: put away (another song started)");
    }
}

/// The processes the demo's sound comes from: OnKey's own.
pub fn bundle() -> &'static str {
    BUNDLE.get().map_or("audio.onkey.app", |b| b.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as M};

    /// A player whose clock the test moves.
    #[derive(Clone, Default)]
    struct Fake(Arc<M<FakeState>>);
    #[derive(Default)]
    struct FakeState { open: bool, pos: f64, len: f64, playing: bool, ended: bool, opened: Vec<PathBuf> }

    impl Fake {
        fn advance(&self, s: f64) {
            let mut f = self.0.lock().unwrap();
            if f.playing {
                f.pos += s;
                if f.pos >= f.len {
                    f.pos = 0.0;            // (as AVAudioPlayer does)
                    f.playing = false;
                    f.ended = true;
                }
            }
        }
    }

    impl Player for Fake {
        fn open(&mut self, path: &Path) -> Option<f64> {
            let mut f = self.0.lock().unwrap();
            let mut opened = std::mem::take(&mut f.opened);
            opened.push(path.to_path_buf());
            *f = FakeState { open: true, len: 200.0, opened, ..Default::default() };
            Some(200.0)
        }
        fn play(&mut self) { let mut f = self.0.lock().unwrap(); f.playing = f.open; f.ended = false; }
        fn pause(&mut self) { self.0.lock().unwrap().playing = false; }
        fn seek(&mut self, s: f64) { self.0.lock().unwrap().pos = s; }
        fn close(&mut self) {
            let mut f = self.0.lock().unwrap();
            let opened = std::mem::take(&mut f.opened);
            *f = FakeState { opened, ..Default::default() };
        }
        fn state(&self) -> Option<(f64, Playing)> {
            let f = self.0.lock().unwrap();
            f.open.then(|| (if f.ended { f.len } else { f.pos }, if f.ended { Playing::Ended } else if f.playing { Playing::Playing } else { Playing::Paused }))
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("onkey-demo-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A demo folder as the demosong example writes it.
    fn demo_dir(name: &str, version: &str) -> PathBuf {
        let d = tmp(name);
        let m = Manifest { id: "demo:test-song".into(), name: "Test Song".into(), artist: "OnKey".into(), duration: 200.0,
                           audio: "demo.m4a".into(), version: version.into(), credit: "Made for OnKey".into(), licence: "CC0".into() };
        std::fs::write(d.join(MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
        std::fs::write(d.join("demo.m4a"), b"audio").unwrap();
        std::fs::write(d.join("song.bin"), format!("bin {version}")).unwrap();
        std::fs::write(d.join("song.json"), json!({"track": {"id": "demo:old", "name": "x", "artist": "y", "album": "", "duration": 1.0},
            "lyrics": [[1.0, "Some words"]], "fraction": 0.9}).to_string()).unwrap();
        std::fs::write(d.join("song.syl.json"), format!("syl {version}")).unwrap();
        d
    }

    fn demo(dir: &Path) -> (Demo, Fake) {
        let f = Fake::default();
        (Demo::new(Some(dir.to_path_buf()), Box::new(f.clone())), f)
    }

    #[test]
    fn a_demo_folder_is_whole_or_there_is_no_demo() {
        let d = demo_dir("whole", "1");
        assert_eq!(read_manifest(&d).map(|m| m.id), Some("demo:test-song".into()));
        // no recording (a checkout where it isn't bundled): no demo, so the window shows no "Try it now"
        std::fs::remove_file(d.join("demo.m4a")).unwrap();
        assert!(read_manifest(&d).is_none());
        let d = demo_dir("nobin", "1");
        std::fs::remove_file(d.join("song.bin")).unwrap();
        assert!(read_manifest(&d).is_none());
        // an id that isn't a demo's, or a recording outside the folder, is refused
        let d = demo_dir("badid", "1");
        let mut m: Value = serde_json::from_slice(&std::fs::read(d.join(MANIFEST)).unwrap()).unwrap();
        m["id"] = json!("spotify:track:abc");
        std::fs::write(d.join(MANIFEST), m.to_string()).unwrap();
        assert!(read_manifest(&d).is_none());
        m["id"] = json!("demo:x");
        m["audio"] = json!("../demo.m4a");
        std::fs::write(d.join(MANIFEST), m.to_string()).unwrap();
        assert!(read_manifest(&d).is_none());
        assert!(read_manifest(&tmp("empty")).is_none());
        let (none, _) = (Demo::new(None, Box::new(Fake::default())), ());
        assert!(none.manifest().is_none());
    }

    #[test]
    fn the_bundled_song_is_installed_once_per_version_keeping_its_heart() {
        let store = Store::new(tmp("store"));
        let d = demo_dir("install", "1");
        let m = read_manifest(&d).unwrap();
        assert!(install(&d, &m, &store).unwrap());
        assert_eq!(std::fs::read_to_string(store.song_file(&m.id, "bin")).unwrap(), "bin 1");
        assert_eq!(std::fs::read_to_string(store.song_file(&m.id, "syl.json")).unwrap(), "syl 1");
        let meta: Value = serde_json::from_slice(&std::fs::read(store.song_file(&m.id, "json")).unwrap()).unwrap();
        // the manifest says what the song is; the words stay kept with it (no lyrics lookup for the demo)
        assert_eq!(meta["track"]["id"], "demo:test-song");
        assert_eq!(meta["track"]["name"], "Test Song");
        assert_eq!(meta["demo_version"], "1");
        assert!(matches!(store.kept_lyrics(&m.id), Some(crate::store::KeptLyrics::Text(l)) if l[0].1 == "Some words"));
        // the same version again: nothing written
        assert!(!install(&d, &m, &store).unwrap());
        // hearted, then a new version ships: replaced, the heart kept, and an old file the new one hasn't goes
        store.set_favorite(&m.track(), true);
        std::fs::write(store.song_file(&m.id, "words"), "old words").unwrap();
        let d2 = demo_dir("install2", "2");
        let m2 = read_manifest(&d2).unwrap();
        assert!(install(&d2, &m2, &store).unwrap());
        assert_eq!(std::fs::read_to_string(store.song_file(&m.id, "bin")).unwrap(), "bin 2");
        assert!(!store.song_file(&m.id, "words").exists());
        assert!(store.is_favorite(&m.track()));
        // it's a song in the Library like any other
        assert!(store.library().iter().any(|r| r["track"]["id"] == "demo:test-song"));
        // never merged with a song of the same name learned from an app (or that one into it)
        let same = Track { id: "spotify:track:abc".into(), ..m.track() };
        assert!(!crate::store::same_title(&m.track(), &same));
        assert_eq!(store.canonical(&m.track()).id, "demo:test-song");
        assert_eq!(store.canonical(&same).id, "spotify:track:abc");
    }

    #[test]
    fn the_demo_is_followed_by_its_players_clock() {
        let store = Store::new(tmp("clock-store"));
        let d = demo_dir("clock", "1");
        let (mut demo, f) = demo(&d);
        let t0 = Instant::now();
        assert!(!demo.on() && demo.state(t0).is_none());
        let track = demo.start(&store).unwrap();
        assert_eq!(track.id, "demo:test-song");
        assert_eq!(f.0.lock().unwrap().opened, vec![d.join("demo.m4a")]);
        assert!(store.load_song(&track.id).is_none(), "the stand-in bin isn't a real song file");   // (installed all the same)
        assert!(store.song_file(&track.id, "bin").is_file());
        // the position is the player's own, exactly: no listening, no fingerprint guess
        f.advance(12.34);
        let s = demo.state(t0).unwrap();
        assert!(s.playing && s.source == Source::Demo && s.track.id == track.id);
        assert!((s.position - 12.34).abs() < 1e-9);
        // paused, sought and played on: the clock follows
        demo.control("playpause");
        f.advance(5.0);
        let s = demo.state(t0).unwrap();
        assert!(!s.playing && (s.position - 12.34).abs() < 1e-9);
        demo.seek(60.0);
        demo.control("playpause");
        f.advance(1.0);
        let s = demo.state(t0).unwrap();
        assert!(s.playing && (s.position - 61.0).abs() < 1e-9);
        // start over
        demo.control("restart");
        assert!(demo.state(t0).unwrap().position.abs() < 1e-9);
        assert!(demo.ended_take().is_none(), "not over while it plays");
    }

    #[test]
    fn the_end_of_the_demo_finishes_its_take_once() {
        let store = Store::new(tmp("end-store"));
        let (mut demo, f) = demo(&demo_dir("end", "1"));
        let t0 = Instant::now();
        demo.start(&store).unwrap();
        f.advance(199.0);
        assert!(demo.state(t0).unwrap().playing);
        f.advance(2.0);     // past the end: the player stops and goes back to 0
        let s = demo.state(t0).unwrap();
        assert!(!s.playing && s.position == 200.0, "at its end, not back at the start: {}", s.position);
        assert!(demo.on(), "still followed until its take is finished");
        assert_eq!(demo.ended_take().map(|t| t.id), Some("demo:test-song".into()));
        assert!(demo.ended_take().is_none(), "once");
        assert!(!demo.on() && demo.state(t0).is_none(), "then OnKey listens for your music app again");
        assert!(!f.0.lock().unwrap().open, "the recording is let go");
        // "next" ends it too
        demo.start(&store).unwrap();
        demo.control("next");
        assert!(!demo.state(t0).unwrap().playing);
        assert!(demo.ended_take().is_some());
    }

    #[test]
    fn a_long_pause_or_another_song_puts_the_demo_away() {
        let store = Store::new(tmp("away-store"));
        let (mut demo, f) = demo(&demo_dir("away", "1"));
        let t0 = Instant::now();
        demo.start(&store).unwrap();
        demo.control("pause");
        assert!(demo.state(t0).is_some_and(|s| !s.playing));
        assert!(demo.state(t0 + Duration::from_secs(30)).is_some(), "a short pause keeps it");
        assert!(demo.state(t0 + PAUSED_PUT_AWAY).is_none(), "paused past the engine's minute: put away");
        assert!(!demo.on() && !f.0.lock().unwrap().open);
        assert!(demo.ended_take().is_none(), "put away isn't an ending: the engine ended that take after its minute");
        demo.start(&store).unwrap();
        demo.stop();
        assert!(!demo.on() && demo.state(t0).is_none());
        // controls with no demo playing do nothing
        demo.control("playpause");
        assert!(!f.0.lock().unwrap().playing);
    }

    #[test]
    fn no_demo_no_start() {
        let store = Store::new(tmp("none-store"));
        let mut demo = Demo::new(Some(tmp("none")), Box::new(Fake::default()));
        assert!(demo.start(&store).is_err());
        assert!(!demo.on());
        let mut demo = Demo::new(None, Box::new(Fake::default()));
        assert!(demo.start(&store).is_err());
    }
}
