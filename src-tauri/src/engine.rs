//! The live engine: follows the music app (Spotify or Apple Music, see source.rs) song to song, captures the
//! music and your mic on one clock, runs the current take (see take.rs) and streams it to the window as "onkey"
//! events. When a take ends (the track changes, the song restarts, or you resume after a pause) it is handed to
//! a worker for the full score, and learned if it was a first play, while the next take starts at once.
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::analysis::{self, Reference};
use crate::capture::{self, Capture, CaptureError, CHANNELS};
use crate::learn::Learner;
use crate::pitch::Crepe;
use crate::source::{self, Source};
use crate::store::{self, Store};
use crate::take::{Mode, Take};

/// Shorter takes (skipped tracks) aren't scored.
const MIN_TAKE_S: f64 = 20.0;
/// A take paused this long ends and is scored (it can't be known whether the song will be resumed: a resume within
/// it carries on the same take). The window counts it down, with Finish to score it now (LiveBits PausedFinish).
const PAUSE_ENDS_S: u64 = 60;

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
/// listening mode keeps this much of the captured sound (more than the listener's, so a take can begin with all of it)
const HEARD_S: f64 = 28.0;
/// listening mode: a song that stops being recognised (not at its end) keeps its take this long, in case it's found again
const LOST_HOLD_S: u64 = 20;
/// control mode: the music app gone quiet and saying nothing plays (stopped, or quit) this long ends the take
const STOPPED_S: u64 = 5;
/// a jump ahead in the song up to this long is filled with silence in the take; a longer one ends it (s)
const MAX_FILL_S: f64 = 180.0;
/// The longest one take runs (see its use): longer than nearly every song, short enough that its audio stays in memory
const MAX_TAKE_S: f64 = 20.0 * 60.0;
/// a song's own silence at its very start, kept in its take (listen.rs counts it as the song too)
const LEAD_SILENCE_S: f64 = 2.0;
/// with nothing playing, the music apps are looked at this often (ms)
const IDLE_POLL_MS: u64 = 500;
/// nothing played (and no take) for this long: the capture stops, letting go of the microphone and the music app's
/// audio, so OnKey left open costs the Mac (Core Audio, the mic) nothing; it starts again when music plays
const REST_AFTER: Duration = Duration::from_secs(30);
/// a YouTube video that may not be a song (youtube::might_not_be_song) is checked for talk this far in (notmusic.rs)
const TALK_CHECK_S: f64 = 60.0;

pub struct Shared {
    pub blind: bool,
    /// remove the music's sound from the microphone (on by default; off to test with music from speakers)
    pub echo_cancel: bool,
    /// "skip to singing": jump the song to this time (seconds)
    pub skip_to: Option<f64>,
    /// "finish": score the play now (the lyrics are over), without waiting for the track to end
    pub finish: bool,
    /// listening back to a take with the song: this track plays in its app but isn't a new take
    pub play_along: Option<String>,
    /// the track listened back to has been heard playing (until then, another song still playing doesn't end
    /// it: in listening mode you start the song yourself, after clicking)
    pub along_heard: bool,
    /// Stop was clicked: listening back ends once the music is paused or moves on (until then the song still
    /// playing isn't a new take: live, one started in the moment before Spotify paused, and was scored "unsung")
    pub along_stopping: bool,
    /// what the music app is playing: (track id, playing, position s, when that was read)
    pub player_now: Option<(String, bool, f64, Instant)>,
    /// listening back: the app was told where to play from at this moment (None: not yet), and the song is placed
    /// from what's read after it (see `Along`)
    pub along_from: Option<Instant>,
    /// listening back: where the song is (song s, at the end of the audio captured), placed as a take places it,
    /// and when that was; None while it isn't playing or not yet placed. Your recording plays there (player_now)
    pub along_place: Option<(f64, Instant)>,
    /// the take under way: where the song is (song s, at the end of the audio captured) and when that was; None
    /// before it's placed (for development tools: player_now's `take`)
    pub take_place: Option<(f64, Instant)>,
    pub takes: Vec<Value>,
}

/// Makes a learner for a first play, if the separation models are available.
pub type LearnerFactory = Arc<dyn Fn(f64) -> Option<Learner> + Send + Sync>;

/// Where the word-alignment model for syllable bars is (None: no syllable bars, the pitch-based ones are used).
pub static WORD_MODEL: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// The word-alignment model, loaded once (its lock taken: `WORD_ALIGNER.lock()`, or `try_lock()` not to wait):
/// for the syllable bars and for lining up a first play's lyrics while it plays. None without the model.
static WORD_ALIGNER: Mutex<Option<crate::syllables::WordAligner>> = Mutex::new(None);
/// GAME (note transcription for the syllable bars), loaded once it's needed
static GAME: Mutex<Option<crate::game::Game>> = Mutex::new(None);

/// Let go of the learning models nothing is using now (the word timer, GAME and the separation sets), unless
/// they're kept ready (lowmem::keep_learning_models), or `always` (Settings just said not to keep them). A model
/// busy now (a live line-up, a song's bars being made) is left for that work to let go when it's done.
pub fn release_learning_models(always: bool) {
    // (another song being learned now will want them shortly: they're let go after it)
    if crate::learn::running() > 0 || (!always && crate::lowmem::keep_learning_models()) {
        return;
    }
    let mut freed = Vec::new();
    if let Ok(mut g) = WORD_ALIGNER.try_lock() {
        if g.take().is_some() { freed.push("the word timer"); }
    }
    if let Ok(mut g) = GAME.try_lock() {
        if g.take().is_some() { freed.push("GAME"); }
    }
    if !freed.is_empty() {
        log(&format!("{} let go", freed.join(" and ")));
    }
    crate::learn::release_idle();
}

fn word_aligner(mut g: std::sync::MutexGuard<'static, Option<crate::syllables::WordAligner>>)
                -> std::sync::MutexGuard<'static, Option<crate::syllables::WordAligner>> {
    if g.is_none() {
        if let Some(path) = WORD_MODEL.lock().unwrap().clone() {
            *g = crate::syllables::WordAligner::load(&path, 2).map_err(|e| log(&e)).ok();
        }
    }
    g
}

/// A first play's lyrics lined up with the singer separated so far (syllables::line_up_live), with the word timer
/// shared with the syllable bars (one copy in memory). (None, true): it's busy making the last song's; later.
pub fn line_up_now(vocal: &[f32], off: f64, given: &[(f64, String)], shown: &[(f64, String)], now: f64, prior: f64)
                   -> (Option<crate::syllables::LiveLineUp>, bool) {
    let Ok(g) = WORD_ALIGNER.try_lock() else { return (None, true) };
    (word_aligner(g).as_mut().and_then(|a| crate::syllables::line_up_live(a, vocal, off, given, shown, now, prior)), false)
}

/// The singer separated during a play, each stretch between jumps where it was heard in the song (Take::one_place),
/// so the syllable bars made from it can take the take's place now throughout.
fn one_place(take: &Take, l: &crate::learn::Learnt) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    (take.one_place(&l.vocal16, crate::pitch::HOP, 0.0), take.one_place(&l.midi, 1, f32::NAN), take.one_place(&l.periodicity, 1, 0.0))
}

/// One syllable bar per sung syllable, made after a song is learned (about a minute, on this thread).
/// `covered`: the song frames this play heard, when the song may already have lyric times and bars from earlier
/// plays. The play then starts from the song's own lyric times (not the ones it fetched when it began, which
/// may predate an earlier play's placing them: The Kill, 19 Oct, put back LRCLIB's times 6 s late), and its
/// lines and bars replace the song's only where it heard the song (syllables::merge_play).
fn make_syllables(store: &Store, take: &Take, learnt: &(Vec<f32>, Vec<f32>, Vec<f32>), covered: Option<&[bool]>) {
    let id = &take.track.id;
    let kept = match store.kept_lyrics(id) {
        Some(store::KeptLyrics::Times(k)) => store::lyrics_on_kept_times(&k, &take.lyrics),
        _ => None,
    };
    let given = kept.clone().unwrap_or_else(|| take.lyrics.clone());
    let before = covered.and(kept.as_ref()).map(|k| (k.clone(), store.syllables(id, Some(k)).unwrap_or_default()));
    let placed_before = store.lyrics_placed(id);
    let made = syllable_bars(store, &take.track, &given, take.song_offset.unwrap_or(0.0), learnt);
    let (Some((old_lyrics, old_bars)), Some(covered)) = (before, covered) else { return };
    // what syllable_bars kept for this play alone, merged into what the song had
    let new_lyrics = match store.kept_lyrics(id) {
        Some(store::KeptLyrics::Times(k)) => store::lyrics_on_kept_times(&k, &given).unwrap_or_else(|| given.clone()),
        _ => given.clone(),
    };
    let new_bars = if made > 0 { store.syllables(id, Some(&new_lyrics)).unwrap_or_default() } else { Vec::new() };
    let (lyrics, bars) = crate::syllables::merge_play(&old_lyrics, &new_lyrics, placed_before, &old_bars, &new_bars, covered);
    store.save_lyrics(id, &lyrics);
    crate::lyrics::keep_learned(id, &lyrics);
    if !bars.is_empty() {
        store.save_syllables(id, &bars, &lyrics);
    }
    log(&format!("{}: this play's lyric times and bars merged where it heard the song ({} bars: {} before, {} from this play)",
                 take.track.name, bars.len(), old_bars.len(), new_bars.len()));
}

/// A learned song's lyric line times and syllable bars, made from its separated singer (`learnt`: the vocal at
/// 16 kHz, its raw pitch and periodicity, starting `off` s into the song) and its lyrics, and kept with the
/// song. Also how the lab rebuilds a library after the method improves (examples/rebuild). The number of bars
/// kept (0: none made).
pub fn syllable_bars(store: &Store, track: &crate::source::Track, given: &[(f64, String)], off: f64,
                     learnt: &(Vec<f32>, Vec<f32>, Vec<f32>)) -> usize {
    static SYLLABIFIER: Mutex<Option<crate::syllables::Syllabifier>> = Mutex::new(None);
    use crate::learn::yield_to_singing;
    yield_to_singing(None);     // (someone singing: this waits until they stop, before each step)
    // why none were made, in the log (it was silent: songs went without bars and nothing said why)
    let none = |why: &str| {
        log(&format!("{}: no syllable bars: {why}", track.name));
        0
    };
    let Some(path) = WORD_MODEL.lock().unwrap().clone() else { return none("no word-alignment model") };
    let Some(song) = store.load_song(&track.id) else { return none("the song isn't in the library") };
    if given.len() < 4 {
        return none(&format!("{} lyric lines (4 needed)", given.len()));
    }
    // [lyrics-ml] lyrics in another script (Mandarin, Japanese, Korean, Hindi...): the word timer hears English
    // letters only, so no model pass; LRCLIB's lines are kept as they are and shown without bars (lang::alignable)
    if !crate::lang::alignable(given) {
        store.save_lyrics(&track.id, given);
        crate::lyrics::keep_learned(&track.id, given);
        return none(&format!("the lyrics aren't in Latin letters ({:?}): the word timer hears English letters only; LRCLIB's lines are shown as they are",
                             crate::lang::song_script(given)));
    }
    let mut models = word_aligner(WORD_ALIGNER.lock().unwrap());
    let Some(aligner) = models.as_mut() else { return none("the word-alignment model didn't load") };
    let mut syl = SYLLABIFIER.lock().unwrap();
    let syl = syl.get_or_insert_with(crate::syllables::Syllabifier::new);
    let line = crate::notes::clean_line(&song.midi);
    let t = Instant::now();
    use crate::progress::{stage, Stage};
    stage(Stage::Words);
    // each lyric line where it's really sung: the whole song's words aligned at once (the given timing can be
    // seconds off, or made for another version of the song); kept with the song, so the lyrics show there too
    // (believed only where consistent: see place_lines_checked)
    let (lyrics, heard) = match crate::syllables::place_lines_checked_and_heard(aligner, &learnt.0, off, given) {
        Some(((l, heard, fit), all)) => {
            // the words heard, kept with the song (times and places only), so its lyrics can be checked later
            store.save_words(&track.id, given, &all, &heard);
            let mut moved: Vec<f32> = l.iter().zip(given).map(|(a, b)| (a.0 - b.0).abs() as f32).collect();
            let far = moved.iter().filter(|m| **m > 1.0).count();
            crate::records::note("learn_fit", json!({"track": track.id, "check": store::lyrics_check(given), "lines": given.len(),
                "heard": fit.heard, "agree": fit.agree, "offset": fit.offset, "drift": fit.drift, "believed": fit.believed, "sections": fit.sections,
                "median_move": crate::dsp::median(&mut moved.clone()), "moved_over_1s": far}));
            log(&format!("{}: lyric lines placed by the words heard (median move {:.2} s, {far} moved over 1 s; {} of {} lines heard agree, on {:+.2} s{}{})",
                         track.name, crate::dsp::median(&mut moved), fit.agree, fit.heard, fit.offset,
                         if fit.drift != 0.0 { format!(" drifting {:+.1}%", fit.drift * 100.0) } else { String::new() },
                         if fit.sections > 1 { format!(" in the first of {} sections, each moved on its own", fit.sections) }
                         else if fit.believed { String::new() } else { ": not consistent, LRCLIB's times kept".to_string() }));
            store.save_lyrics(&track.id, &l);
            if fit.believed {
                store.mark_lyrics_placed(&track.id);
            }
            crate::lyrics::keep_learned(&track.id, given);
            (l, heard)
        }
        None => (given.to_vec(), Vec::new()),
    };
    yield_to_singing(None);
    stage(Stage::Bars);
    let bars = crate::syllables::make(aligner, syl, &crate::syllables::Inputs {
        vocal16: &learnt.0, song_offset: off, lyrics: &lyrics, line: &line,
        raw_midi: &learnt.1, raw_per: &learnt.2, heard: (!heard.is_empty()).then_some(&heard[..]) });
    let made = bars.len();
    // each syllable's note (and runs split into notes) from GAME's transcription of the singer
    let mut game = GAME.lock().unwrap();
    if game.is_none() {
        if let Some(dir) = path.parent().map(|d| d.join("game")) {
            *game = crate::game::Game::load(&dir, 2).map_err(|e| log(&e)).ok();
        }
    }
    let mut game_notes = None;
    yield_to_singing(None);
    stage(Stage::Notes);
    let bars = match game.as_mut().map(|g| g.notes(&learnt.0)) {
        Some(Ok(notes)) => {
            game_notes = Some(notes.len());
            let notes: Vec<(f64, f64, f32)> = notes.iter().map(|n| ((n.0 + off) * 100.0, (n.1 + off) * 100.0, n.2)).collect();
            let failed: Vec<(f64, f64)> = game.as_ref().map_or(Vec::new(), |g| g.failed.iter().map(|f| ((f.0 + off) * 100.0, (f.1 + off) * 100.0)).collect());
            crate::syllables::refine_with_notes_where_heard(&bars, &notes, &failed)
        }
        Some(Err(e)) => { log(&format!("GAME: {e}")); bars }
        None => bars,
    };
    stage(Stage::Saving);
    let kept = if bars.len() >= 10 {
        store.save_syllables(&track.id, &bars, &lyrics);
        log(&format!("{}: {} syllable bars ({:.0} s)", track.name, bars.len(), t.elapsed().as_secs_f64()));
        bars.len()
    } else {
        // the singer's level and how much of it was voiced say whether the separation gave a singer at all
        let peak = learnt.0.iter().fold(0f32, |m, v| m.max(v.abs()));
        let voiced = learnt.2.iter().filter(|p| **p > 0.45).count() as f64 / learnt.2.len().max(1) as f64;
        none(&format!("{} made ({made} from the words heard, {} kept after GAME's {} notes; 10 needed): {} of {} lyric lines \
                       placed by the words heard, singer {:.0} s at peak {:.1} dBFS, {:.0}% voiced",
                      bars.len(), bars.len(), game_notes.map_or("no".to_string(), |n| n.to_string()),
                      heard.iter().filter(|h| h.is_some()).count(), lyrics.len(), learnt.0.len() as f64 / 16_000.0,
                      20.0 * (peak + 1e-9).log10(), 100.0 * voiced))
    };
    // the word timer and GAME (hundreds of MB loaded) are let go until the next song needs them, unless the
    // learning models are kept ready (lowmem::keep_learning_models)
    if !crate::lowmem::keep_learning_models() && crate::learn::running() == 0 {
        *models = None;
        *game = None;
    }
    kept
}

/// A song's length when its play didn't know it: Apple's figure when it has one; otherwise as far as any play has
/// heard it, never shorter than an earlier play found (a play that stopped at 3:17 made a 3:50 song 3:17 long,
/// and its 3:50 learned 116%).
fn guessed_length(apple: Option<f64>, heard_to: f64, before: Option<f64>) -> f64 {
    apple.filter(|d| *d > 0.0).unwrap_or_else(|| heard_to.max(before.unwrap_or(0.0)))
}

/// The log is kept to this size: past it, it becomes onkey.log.1 (replacing the one before) and starts afresh.
const LOG_MAX: u64 = 5 << 20;

/// A plain-text log in the temp folder ($TMPDIR/onkey.log): what the engine did, for diagnosing problems.
/// At most ~10 MB with the one older file.
pub fn log(msg: &str) {
    static LOCK: Mutex<()> = Mutex::new(());
    let _one_at_a_time = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let line = format!("{} {msg}", store::now_iso());
    append_log(&std::env::temp_dir().join("onkey.log"), &line, LOG_MAX);
    crate::records::log_line(&line);       // and today's file in the records (kept 30 days)
}

fn append_log(path: &std::path::Path, line: &str, max: u64) {
    use std::io::Write;
    if std::fs::metadata(path).is_ok_and(|m| m.len() >= max) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Send the window an event (and, in development builds, to the event log and the live mirror). Each is also kept
/// for catching up (`catch_up`).
pub(crate) fn emit(app: &AppHandle, ev: Value) {
    crate::devtest::record(&ev);
    keep_for_catch_up(&ev);
    let _ = app.emit("onkey", ev);
}

/// What a window that has just loaded needs to show what's happening now: the song playing (its "song" event and
/// the events for it since, but only the latest "update" and none of the ribbon's "viz"), and the latest of each
/// other kind (status, takes, the status line…). Without it, a take that starts as OnKey launches, before the
/// window is listening, never reaches it: the Sing page said "Getting ready…" for the whole song (19 Oct).
#[derive(Default)]
struct CatchUp {
    song: Vec<Value>,
    update: Option<Value>,
    latest: std::collections::BTreeMap<String, Value>,
}
static CATCH_UP: std::sync::Mutex<Option<CatchUp>> = std::sync::Mutex::new(None);
/// Most events kept for the song playing (lyrics, notes, art, phase: a few per song)
const CATCH_UP_KEEP: usize = 2000;

fn keep_for_catch_up(ev: &Value) {
    let Some(kind) = ev["type"].as_str() else { return };
    let mut g = CATCH_UP.lock().unwrap_or_else(|e| e.into_inner());
    let c = g.get_or_insert_with(CatchUp::default);
    match kind {
        "song" => { c.song = vec![ev.clone()]; c.update = None; }
        "update" => if !c.song.is_empty() { c.update = Some(ev.clone()) },
        "viz" => {}
        "lyrics" | "notes" | "art" | "track" | "phase" => {
            if !c.song.is_empty() && c.song.len() < CATCH_UP_KEEP { c.song.push(ev.clone()); }
        }
        _ => { c.latest.insert(kind.to_string(), ev.clone()); }
    }
}

/// The events to catch a newly loaded window up, in the order to apply them.
pub fn catch_up() -> Vec<Value> {
    let g = CATCH_UP.lock().unwrap_or_else(|e| e.into_inner());
    let Some(c) = g.as_ref() else { return vec![] };
    let mut out: Vec<Value> = c.latest.values().cloned().collect();
    out.extend(c.song.iter().cloned());
    out.extend(c.update.iter().cloned());
    out
}

#[cfg(test)]
mod catch_up_tests {
    use super::*;
    #[test]
    fn a_window_loaded_mid_song_gets_the_song_its_events_and_the_latest_update() {
        keep_for_catch_up(&json!({"type": "status", "n": 1}));
        keep_for_catch_up(&json!({"type": "song", "version": 1}));
        keep_for_catch_up(&json!({"type": "update", "version": 1, "tap_now": 1}));
        keep_for_catch_up(&json!({"type": "song", "version": 2}));
        keep_for_catch_up(&json!({"type": "lyrics", "version": 2}));
        keep_for_catch_up(&json!({"type": "viz"}));
        keep_for_catch_up(&json!({"type": "update", "version": 2, "tap_now": 5}));
        keep_for_catch_up(&json!({"type": "update", "version": 2, "tap_now": 6}));
        keep_for_catch_up(&json!({"type": "status", "n": 2}));
        let kinds: Vec<String> = catch_up().iter().map(|e| format!("{}{}", e["type"].as_str().unwrap(),
            e.get("n").or(e.get("tap_now")).or(e.get("version")).map(|v| v.to_string()).unwrap_or_default())).collect();
        assert_eq!(kinds, ["status2", "song2", "lyrics2", "update6"]);
    }
}

struct Current {
    take: Take,
    version: u64,
    learner: Option<Learner>,
    score: Option<f32>,
    score_at: Instant,
    /// the ribbon's frames go to the window in batches, about every 100 ms
    viz_at: Instant,
    /// a learned song: its lyrics are already lined up with the singer
    lyrics_fixed: bool,
    /// first play: lyrics being lined up with the singer separated so far (on its own thread): the line-up, the
    /// LRCLIB lines it was made from, and whether the word aligner was busy (then it's tried again shortly)
    lyrics_job: Option<std::sync::mpsc::Receiver<LineUpJob>>,
    /// seconds of separated singer the last line-up had, and not before when the next may start
    lyrics_tried_at: f64,
    lyrics_retry: Instant,
    /// which of those line-ups to believe
    line_ups: crate::syllables::LineUps,
    /// a learned song keeps only its lyrics' times (and a check of the words): the words come from LRCLIB
    lyric_times: Option<store::Kept>,
    /// how sure its place was, as last told to the window (logged when it changes)
    place_said: &'static str,
    /// the last update sent to the window, since when it's been the same (paused) and when it was last sent
    said: String,
    same_since: Instant,
    said_at: Instant,
    /// a video that may not be a song: its first minute checked for talk (on its own thread), the answer to come
    talk_checked: bool,
    talk_job: Option<std::sync::mpsc::Receiver<Option<crate::notmusic::Verdict>>>,
}

/// Paused (the same update over and over) for longer than this, the window is told only every UPDATE_STILL_EVERY:
/// it already shows the song stopped, and each update wakes it
const UPDATE_STILL_AFTER: Duration = Duration::from_secs(1);
const UPDATE_STILL_EVERY: Duration = Duration::from_millis(500);

/// A live line-up done on its own thread: the result, the LRCLIB lines it was made from, and whether the word
/// aligner was busy.
type LineUpJob = (Option<crate::syllables::LiveLineUp>, Vec<(f64, String)>, bool);

/// A finished take on its way to being scored.
struct Done {
    take: Take,
    learner: Option<Learner>,
    live: Option<f32>,
    entry: usize,
    /// the take's version (its record, records.rs)
    version: u64,
    /// its place in the queue of songs to learn (learn::Queued), taken as it ended: one song is learned at a time
    queued: Option<crate::learn::Queued>,
}

/// The window shows a song playing and can be seen (lib.rs ui_moving, from the window's own idea of it): with a
/// learned song's take going on, learning waits (learn::set_singing).
pub static WINDOW_MOVING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Takes handed over to be learned or scored and not yet done.
static PENDING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// OnKey is quitting: the take playing ends now and is kept (learned so far), and nothing new starts.
static QUITTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The engine was started, and has handed over its last take.
static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STOPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// macOS Low Power Mode is on: the window draws less (lane at 30 frames a second, no glow).
fn low_power() -> bool {
    crate::platform::sys::low_power()
}

/// The Mac is running on its battery (IOKit's power source; a desktop Mac never is).
fn on_battery() -> bool {
    if let Some(b) = crate::devtest::force_battery() {
        return b;
    }
    crate::platform::sys::on_battery()
}

/// The lane draws at most 30 times a second: Low Power Mode, or on battery unless Settings says otherwise
/// ("battery_30fps", on by default). The notes still move smoothly; WebKit and the window server do half the work.
fn draw_30() -> bool {
    low_power() || (crate::settings::get_opt("battery_30fps").unwrap_or(true) && on_battery())
}

/// Takes are still being learned or scored (a restart to update waits for them).
pub fn learning() -> bool {
    PENDING.load(std::sync::atomic::Ordering::SeqCst) > 0
}

/// "Pause listening" switched (lib.rs `set_listening_paused`), or anything else the window should hear about at
/// once: the engine's loop, if it's napping (no capture, or paused), goes round now, and sends its status.
static WAKE: (Mutex<bool>, std::sync::Condvar) = (Mutex::new(false), std::sync::Condvar::new());
static STATUS_NOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn wake() {
    STATUS_NOW.store(true, std::sync::atomic::Ordering::SeqCst);
    *WAKE.0.lock().unwrap() = true;
    WAKE.1.notify_all();
}

/// Sleep for `d`, or until `wake`.
fn nap(d: Duration) {
    let woken = WAKE.0.lock().unwrap();
    let (mut woken, _) = WAKE.1.wait_timeout_while(woken, d, |w| !*w).unwrap();
    *woken = false;
}

/// What the engine's loop does about "Pause listening", from whether it was paused last time round and is now.
#[derive(Debug, PartialEq)]
enum Pause {
    /// not paused: everything as usual
    Listen,
    /// just paused: the take under way ends (scored as usual), the capture stops, the song followed is forgotten
    Stop,
    /// still paused: nothing captured, named, recognised or learned; the loop naps
    Idle,
    /// just resumed: everything starts again as usual (capture once music plays)
    Resume,
}

fn pause_step(was: bool, now: bool) -> Pause {
    match (was, now) {
        (false, false) => Pause::Listen,
        (false, true) => Pause::Stop,
        (true, true) => Pause::Idle,
        (true, false) => Pause::Resume,
    }
}

pub fn quitting() -> bool {
    QUITTING.load(std::sync::atomic::Ordering::SeqCst)
}

/// OnKey is quitting (from the menu, or told to stop): the take playing is ended and kept, what its learner has
/// is learned and saved, and every take being learned is finished, waiting at most `max` (a quit mid-song lost
/// its first minutes before). Syllable bars aren't made now: the song's next play makes them.
pub fn shutdown(max: Duration) {
    use std::sync::atomic::Ordering::SeqCst;
    let now = Instant::now();
    // a second quit (told to stop, a restart, ⌘Q again) joins the wait already going, for no longer than the
    // first one has left nor its own `max`, rather than exiting at once and dropping what's being learned
    let first = !QUITTING.swap(true, SeqCst);
    let until = {
        let mut d = QUIT_DEADLINE.lock().unwrap();
        let until = quit_until(*d, now, max);
        d.get_or_insert(until);
        until
    };
    log(if first { "quitting: keeping what's been learned" } else { "quitting again: waiting with the quit already going" });
    while Instant::now() < until {
        // (an engine that never started has nothing to hand over)
        if (STOPPED.load(SeqCst) || !STARTED.load(SeqCst)) && PENDING.load(SeqCst) == 0 {
            log(&format!("quitting: done in {:.1} s", now.elapsed().as_secs_f64()));
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let left = PENDING.load(SeqCst);
    log(&format!("quitting: gave up waiting after {:.0} s ({left} take{} not finished)", now.elapsed().as_secs_f64(), if left == 1 { "" } else { "s" }));
}

/// When the first quit's wait ends (set by the first call to `shutdown`).
static QUIT_DEADLINE: Mutex<Option<Instant>> = Mutex::new(None);

/// How long a quit waits: `max` from now, but a later quit no longer than the first one's wait has left.
fn quit_until(first: Option<Instant>, now: Instant, max: Duration) -> Instant {
    let own = now + max;
    first.map_or(own, |f| f.min(own))
}

pub fn spawn(app: AppHandle, shared: Arc<Mutex<Shared>>, store: Store, learners: LearnerFactory) {
    STARTED.store(true, std::sync::atomic::Ordering::SeqCst);
    // how far each song being learned has got: to the window, at most a few times a second (progress.rs)
    {
        let app = app.clone();
        crate::progress::set_sink(move |ev| emit(&app, ev));
    }
    // each finished take is scored on its own thread: a learned song's score is ready at once and never
    // waits behind a first play that's still being learned
    let (tx, rx) = channel::<Done>();
    {
        let (app, shared, store) = (app.clone(), shared.clone(), store.clone());
        std::thread::Builder::new().name("onkey-score".into()).spawn(move || {
            for done in rx {
                let (app, shared, store) = (app.clone(), shared.clone(), store.clone());
                let entry = done.entry;
                // a panic scoring it (or a thread that couldn't start) still counts the take done, so a quit or the
                // updater doesn't wait on it for good, and it shows as "couldn't score" rather than "scoring" forever
                let failed = move |app: &AppHandle, shared: &Arc<Mutex<Shared>>| {
                    if let Some(e) = shared.lock().unwrap_or_else(|e| e.into_inner()).takes.get_mut(entry) {
                        e["status"] = json!("failed");
                    }
                    takes_changed(app, shared);
                    PENDING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                };
                let (app2, shared2) = (app.clone(), shared.clone());
                let started = std::thread::Builder::new().name("onkey-scoring".into()).spawn(move || {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| score_take(&app, &shared, &store, done))) {
                        Ok(()) => { PENDING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst); }
                        Err(p) => {
                            let why = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                            log(&format!("scoring a take failed (a panic: {why})"));
                            failed(&app, &shared);
                        }
                    }
                });
                if let Err(e) = started {
                    log(&format!("scoring a take couldn't start: {e}"));
                    failed(&app2, &shared2);
                }
            }
        }).unwrap();
    }
    std::thread::Builder::new().name("onkey-engine".into()).spawn(move || run(app, shared, store, learners, tx)).unwrap();
}

/// A take of `took` is over though nothing is playing, by the app's paused state `s`: `took` played on to within
/// a few seconds of its end (`last_pos`) and the app now shows it back at its start, or shows another track.
fn came_to_end(took: &source::Track, s: &source::State, last_pos: f64) -> bool {
    let near_end = took.duration > 0.0 && last_pos >= took.duration - 5.0;
    near_end && (s.track.id != took.id || s.position < 1.0)
}

/// Songs forgotten from the Library, and the newest take then (TAKE_VERSION): a play of one already under way
/// is still scored and kept in History, but doesn't learn the song back or save its bars (`forgotten_during`).
static FORGOTTEN: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

/// The Library forgot song `id` (lib.rs forget_song).
pub fn forgotten(id: &str) {
    let v = TAKE_VERSION.load(std::sync::atomic::Ordering::Relaxed);
    FORGOTTEN.lock().unwrap_or_else(|e| e.into_inner()).push((id.to_string(), v));
}

/// Was any of `ids` forgotten after take `version` began?
fn forgotten_during(ids: &[&str], version: u64) -> bool {
    FORGOTTEN.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|(id, v)| version <= *v && ids.contains(&id.as_str()))
}

fn takes_changed(app: &AppHandle, shared: &Arc<Mutex<Shared>>) {
    let takes = shared.lock().unwrap().takes.clone();
    emit(app, json!({"type": "takes", "takes": takes.iter().filter(|t| t["status"] != "unsung" && t["status"] != "discarded").collect::<Vec<_>>()}));
}

/// Stop learning a song (the Library's "Stop learning and delete"): the play of it under way ends with nothing
/// kept, a learn of it waiting or under way stops, what was learned of it before is forgotten, and it isn't
/// learned again while it plays (a YouTube video: never). See discard.rs.
/// Err if what was learned couldn't be moved to the Trash (it stays; the learning still stops).
pub fn stop_learning(app: &AppHandle, shared: &Arc<Mutex<Shared>>, store: &Store, ids: &[String]) -> Result<(), String> {
    let version = TAKE_VERSION.load(std::sync::atomic::Ordering::Relaxed);
    let mut forgot = Ok(());
    for id in ids {
        log(&format!("stopped learning {id} (from the Library)"));
        crate::discard::discard(id, version);
        if let Err(e) = store.forget(id) {
            log(&format!("{id}: {e}"));
            forgot = Err(e);
        }
    }
    let mut changed = false;
    for t in shared.lock().unwrap().takes.iter_mut() {
        if t["status"] == "learning" && t["track_id"].as_str().is_some_and(|i| ids.iter().any(|x| x == i)) {
            t["status"] = json!("discarded");
            changed = true;
        }
    }
    if changed {
        takes_changed(app, shared);
    }
    forgot
}

fn run(app: AppHandle, shared: Arc<Mutex<Shared>>, store: Store, learners: LearnerFactory, done_tx: Sender<Done>) {
    // (the pitch model not loading, a damaged install say: said in the window, which may open after this, so said
    // every second, and the engine does nothing more, stopping as soon as OnKey quits; it used to die silently here,
    // the window saying "Getting ready…" for good and every quit waiting its full time)
    let mut crepe = match Crepe::new() {
        Ok(c) => c,
        Err(e) => {
            log(&format!("the pitch model couldn't load: {e}"));
            crate::models::set_state(json!({"state": "broken", "code": "pitch_model", "error": e.to_string()}));
            while !quitting() {
                emit(&app, json!({"type": "status", "models": crate::models::state(), "listening": crate::settings::listening(), "listening_paused": crate::settings::listening_paused()}));
                std::thread::sleep(Duration::from_secs(1));
            }
            STOPPED.store(true, std::sync::atomic::Ordering::SeqCst);
            return;
        }
    };

    // the music apps are polled on their own thread: AppleScript calls take ~50-100 ms
    let state: Arc<Mutex<Option<(source::State, Instant)>>> = Arc::new(Mutex::new(None));
    {
        let state = state.clone();
        std::thread::spawn(move || loop {
            // the demo song (demo.rs), while it plays: OnKey's own player says where it is, in every mode
            let demo = crate::demo::on();
            // listening paused (and no demo song, which you start yourself): nothing is followed, asked or listened to
            if (crate::settings::listening_paused() || crate::tuner::on()) && !demo {
                *state.lock().unwrap() = None;
                std::thread::sleep(Duration::from_millis(IDLE_POLL_MS));
                continue;
            }
            // listening mode: followed by its sound (and asked only if you've allowed control of that app)
            let listening = crate::settings::listening() && !demo;
            // (a podcast episode or an ad is never a song: no take)
            let s = if demo { crate::demo::state() } else if listening { crate::listen::follow() } else { source::poll() }
                .filter(|s| source::is_song(&s.track.id));
            // (a track stopped from the Library, or a video found not to be a song, isn't followed: no take of it)
            if let Some(s) = &s {
                crate::discard::now_playing(&s.track.id);
            }
            crate::discard::note_playing(s.as_ref().filter(|s| s.playing).map(|s| (s.track.id.as_str(), s.track.name.as_str())));
            let s = s.filter(|s| !crate::discard::ignored(&s.track.id));
            // listening is cheap to read (no AppleScript), so it's read often: a pause or a jump shows sooner.
            // With nothing playing, half as often as that or less: each look asks Core Audio (or the music app)
            let active = s.as_ref().is_some_and(|s| s.playing) || (listening && crate::listen::sounding_now());
            *state.lock().unwrap() = s.map(|s| (s, Instant::now()));
            std::thread::sleep(Duration::from_millis(match (listening, active) {
                (true, true) => 150,
                (false, true) => 400,
                (_, false) => IDLE_POLL_MS,
            }));
        });
    }

    let mut capture: Option<Capture> = None;
    // the processes being captured (a source's bundle prefix: YouTube's changes with the browser playing it)
    let mut capture_from = Source::Spotify.bundle();
    let mut cur: Option<Current> = None;
    let mut version = 0u64;
    let mut last_status = Instant::now().checked_sub(Duration::from_secs(10)).unwrap_or_else(Instant::now);     // (checked: Windows, just after boot)
    let mut last_pos = 0.0f64;
    let mut paused_since: Option<Instant> = None;
    // when a paused take will end and be scored (epoch ms), for the window's countdown
    let mut pause_ends_ms: Option<u64> = None;
    let mut state_changed = Instant::now();
    // the sound output's device (see where its silence is left out)
    let mut out_uid = capture::output_uid();
    let mut placed_pending = false;
    // listening back to a take: where the song is (see `Along`)
    let mut along: Option<Along> = None;
    let mut block = Vec::new();
    let mut watchdog = capture::Watchdog::default();
    // when the watchdog dropped the capture (a take running on gets the time without it as silence)
    let mut capture_lost: Option<Instant> = None;
    // the captured music's level (too quiet to trust: not scored, and the window says to turn the app up)
    let mut music_level = capture::MusicLevel::default();
    let mut last_problem = None;
    let mut sim: Option<crate::devtest::SimSinger> = None;
    let mut skipped_at = Instant::now().checked_sub(Duration::from_secs(60)).unwrap_or_else(Instant::now);
    let mut finished: Option<String> = None;
    let mut lost_since: Option<Instant> = None;
    // the sound while the song under way is thought paused, and which take's (see RESUME_HOLD_S)
    let mut held: Vec<f32> = Vec::new();
    let mut held_for: Option<u64> = None;
    // listening mode: the last seconds captured (as the listener heard them), to start a take from its song's start
    let mut heard_audio: std::collections::VecDeque<f32> = std::collections::VecDeque::new();
    // listening mode: the status line shown (kind, params and words: messages.rs `Line`)
    let mut status_line: Option<crate::messages::Line> = None;
    // the capture let go while nothing played (REST_AFTER), and since when nothing has
    let mut rested = false;
    let mut quiet_since: Option<Instant> = None;
    // "Pause listening" as it was last time round (see `Pause`); paused at launch: as you left it
    let mut was_paused = (crate::settings::listening_paused() || crate::tuner::on()) && !crate::demo::on();
    if was_paused {
        log("listening paused by you (as it was left)");
    }
    // development builds: the live pitch's latency and cadence (ONKEY_PERF)
    #[allow(clippy::default_constructed_unit_structs)]     // (a unit struct in release builds)
    let mut dot = crate::devtest::DotClock::default();

    let end_take = |cur: Option<Current>, shared: &Arc<Mutex<Shared>>, why: &str| {
        let Some(c) = cur else { return };
        TAKE_ENDED.store(c.version, std::sync::atomic::Ordering::Relaxed);     // (its lyrics stop being looked for)
        LYRICS.lock().unwrap().retain(|(v, _)| *v != c.version);
        LEARNED_LYRICS.lock().unwrap().retain(|(v, _)| *v != c.version);
        let t = &c.take;
        crate::records::take_end(c.version);
        crate::records::take_set(c.version, "ended", json!({"why": why, "seconds": (t.seconds() * 10.0).round() / 10.0,
            "song_offset": t.song_offset, "lag_ms": t.lag * 10, "output_ms": t.heard * 10, "aligned": t.aligned,
            "fingerprint_placed": t.fingerprint_placed(), "align_conf": t.align_conf, "blind": t.blind, "quiet_s": t.quiet_s,
            "lyric_lines_shown": t.lyrics.len()}));
        // (Windows, Linux) how far the music app's position was from where the sound placed the song (digest.rs)
        #[cfg(any(windows, target_os = "linux"))]
        if let Some((app, g)) = crate::digest::gap_finish(c.version) {
            if let (Some(line), Some(mut sum)) = (g.line(&t.track.name, app), g.summary()) {
                log(&line);
                sum["app"] = json!(app);
                crate::records::take_set(c.version, "app_vs_sound", sum);
            }
        }
        // the times shown at the end (a first play's lyrics move as they're lined up with the singer)
        if !t.lyrics.is_empty() {
            crate::records::take_set(c.version, "lyrics", json!({"times": t.lyrics.iter().map(|l| (l.0 * 100.0).round() / 100.0).collect::<Vec<_>>(),
                "check": store::lyrics_check(if t.lyrics_given.is_empty() { &t.lyrics } else { &t.lyrics_given })}));
        }
        // stopped from the Library, or found not to be a song: nothing kept (its learner stops as it's dropped)
        if crate::discard::dropped(&[t.track.id.as_str()], c.version) {
            crate::records::take_finish(c.version, json!({"status": "discarded"}), Some(&store));
            log(&format!("take of {} dropped ({why}): stopped learning it", t.track.name));
            return;
        }
        if t.seconds() < MIN_TAKE_S || t.peak <= 1e-4 || t.song_offset.is_none() {
            crate::records::take_finish(c.version, json!({"status": "dropped", "why": if t.seconds() < MIN_TAKE_S { "too short" } else if t.peak <= 1e-4 { "silent" } else { "never placed" }}), Some(&store));
            log(&format!("take of {} dropped ({why}; {:.0} s{}){}", t.track.name, t.seconds(),
                         if t.seconds() < MIN_TAKE_S { ", too short to keep" } else if t.peak <= 1e-4 { ", silent" } else if t.song_offset.is_none() { ", never placed" } else { "" },
                         if c.learner.is_some() { "; its learner stopped" } else { "" }));
            return;
        }
        if t.mode == Mode::Learn && c.learner.is_none() {
            log(&format!("take of {} ended ({why}) but not learned or scored: no learner", t.track.name));
            crate::records::take_finish(c.version, json!({"status": "dropped", "why": "no learner"}), Some(&store));
            return;
        }
        let job = c.learner.as_ref().and_then(|l| l.job());
        if let Some(j) = &job {
            j.set_track(&t.track);
            j.ended();
        }
        // songs are learned one at a time, in the order they ended (learn::Queued); what the take heard is all kept
        let queued = c.learner.is_some().then(|| crate::learn::Queued::join(job));
        let live = if t.blind { None } else { t.running() };
        let entry = {
            let mut s = shared.lock().unwrap();
            s.takes.push(json!({"name": t.track.name, "artist": t.track.artist, "track_id": t.track.id,
                "mode": t.mode.name(), "blind": t.blind,
                "status": if t.mode == Mode::Learn { "learning" } else { "scoring" },
                "live": live.map(|v| v.round()), "score": null, "report": null}));
            s.takes.len() - 1
        };
        takes_changed(&app, shared);
        log(&format!("take of {} ended ({why}): {:.0} s from {:.1} s, to be {}", t.track.name, t.seconds(), t.song_offset.unwrap_or(0.0),
                     if c.learner.is_some() { "learned" } else { "scored" }));
        PENDING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);     // (counted until scored: a quit waits for it)
        let _ = done_tx.send(Done { take: c.take, learner: c.learner, live, entry, version: c.version, queued });
    };

    // the records (records.rs): when the loop last went round, and the take's place as last noted
    let mut loop_at = Instant::now();
    let mut noted_place: (u64, Option<f64>, &str) = (0, None, "");
    loop {
        crate::records::engine_tick(std::mem::replace(&mut loop_at, Instant::now()).elapsed());
        // quitting: the take playing is kept (handed over to be learned, with what its learner has), and that's all
        if quitting() {
            end_take(cur.take(), &shared, "OnKey is quitting");
            STOPPED.store(true, std::sync::atomic::Ordering::SeqCst);
            return;
        }
        // the demo song came to its end (no next song follows it): its take is scored now, and OnKey listens for
        // your music app again
        if let Some(t) = crate::demo::ended_take() {
            if cur.as_ref().is_some_and(|c| c.take.track.id == t.id) {
                finished = Some(t.id.clone());
                end_take(cur.take(), &shared, "the demo song came to its end");
                emit(&app, json!({"type": "phase", "phase": "ended"}));
            }
        }
        // singing a learned song, watching it: learning waits (it made the lane stutter on an M2 Air)
        crate::learn::set_singing(cur.as_ref().is_some_and(|c| c.take.mode == Mode::Sing)
            && WINDOW_MOVING.load(std::sync::atomic::Ordering::Relaxed));
        let st = state.lock().unwrap().clone();
        {
            let mut sh = shared.lock().unwrap();
            sh.player_now = st.as_ref().map(|(s, at)| (s.track.id.clone(), s.playing, s.position, *at));
            sh.take_place = cur.as_ref().and_then(|c| c.take.song_time()).map(|p| (p, Instant::now()));
            along_follows(&mut sh, st.as_ref().map(|(s, _)| (s.track.id.as_str(), s.playing)));
        }

        if last_status.elapsed() > Duration::from_secs(3) || STATUS_NOW.swap(false, std::sync::atomic::Ordering::SeqCst) {
            last_status = Instant::now();
            // headphones connected or disconnected mid-song (or their timing just checked in Settings): what you
            // hear moves, so the display follows
            let (heard, reported) = crate::timing::heard_frames();
            if let Some(c) = cur.as_mut() {
                if (heard - c.take.heard).abs() >= 2 {
                    log(&format!("output latency changed: {} -> {} ms{}", c.take.heard * 10, heard * 10,
                                 if heard != reported { format!(" (Core Audio says {} ms; your timing check adds the rest)", reported * 10) } else { String::new() }));
                    if c.take.lag == c.take.heard.max(5) {
                        c.take.lag = heard.max(5);     // not yet learned from your singing: follow the device
                    }
                    c.take.heard = heard;
                    c.take.speakers = reported < 10;
                }
            }
            // the microphone changed (one plugged in or out, the Mac's input changed, another chosen in Settings):
            // the take so far is kept and the capture restarts on the new one
            let mic_now = capture::chosen_mic();
            if let Some(c) = capture.as_ref().filter(|c| mic_now.as_ref().map_or("", |m| m.uid.as_str()) != c.mic_uid) {
                log(&format!("microphone now '{}' (was '{}'): restarting capture", mic_now.as_ref().map_or("none", |m| m.name.as_str()), c.mic_name));
                crate::records::note("capture", json!({"what": "restart", "why": "the microphone changed"}));
                end_take(cur.take(), &shared, "the microphone changed");
                capture = None;
            }
            // what OnKey can't do and why, for the window to say (a refused permission, no mic, missing models)
            let (mic, audio) = crate::permissions::granted_now();
            // the music captured too quiet to hear well while the app plays (its own volume turned right down): say
            // so, with Spotify's volume when OnKey may read it
            // (only where the mic could be hearing the song: MusicLevel::untrusted)
            let quiet_music = (capture.is_some() && music_level.untrusted(capture::output_latency_ms() < 100, crate::devtest::own_voice())
                && (st.as_ref().is_some_and(|(s, _)| s.playing) || capture::app_sounding(capture_from)))
                .then(|| json!({"app": crate::listen::app_name_known(capture_from), "level_db": music_level.level_db().round(),
                                "volume": if capture_from == Source::Spotify.bundle() { crate::spotify::volume_now() } else { None }}));
            // (read on its own: a guard taken inside the json! would hold the lock through the slow parts below,
            // the Automation check and browser_unheard's Core Audio queries, and the emit, blocking player_now)
            let echo_cancel = shared.lock().unwrap().echo_cancel;
            emit(&app, json!({"type": "status", "music_quiet": quiet_music, "mic_device": mic_now.map(|m| m.name), "output_ms": crate::timing::heard_ms(), "output": crate::timing::status(), "echo_cancel": echo_cancel, "youtube": crate::youtube::enabled(), "listening": crate::settings::listening(), "listening_paused": crate::settings::listening_paused(), "low_power": low_power(), "draw_30": draw_30(),
                "line": status_line.as_ref().map(|l| l.json()),
                "mic": mic, "audio": audio, "automation_denied": source::automation_denied(), "not_answering": source::not_answering(), "lyrics_down": crate::settings::show_lyrics() && crate::lyrics::service_down(), "models": crate::models::state(),
                // paused with a take open: when it ends and is scored, unless the song resumes first
                "pause_ends_ms": pause_ends_ms.filter(|_| cur.is_some()),
                "player": st.as_ref().map(|(s, _)| json!({"state": if s.playing {"playing"} else {"paused"},
                    "name": s.track.name, "artist": s.track.artist, "app": (s.source != Source::Other).then(|| s.source.name())})),
                // YouTube is playing in a browser whose page OnKey isn't allowed to read: which setting to turn on
                "youtube_blocked": crate::youtube::js_blocked().map(|(browser, setting, kind)| json!({"browser": browser, "setting": setting, "setting_kind": kind})),
                // YouTube off and a browser playing sound: which, so the window can offer to turn YouTube on
                "browser_sounding": crate::listen::browser_unheard().map(|(browser, works)| json!({"browser": browser, "works": works}))}));
        }

        // "Pause listening" (lib.rs `set_listening_paused`): the take under way ends and is scored as usual, the
        // capture stops (as after REST_AFTER), and until you resume nothing is captured, named, recognised or
        // learned, and no take starts; the status above is still sent, so the window shows it. The demo song,
        // which you start yourself, plays on
        // (and while the Tuner page listens to the mic on its own: tuner.rs)
        let paused = (crate::settings::listening_paused() || crate::tuner::on()) && !crate::demo::on();
        match pause_step(std::mem::replace(&mut was_paused, paused), paused) {
            Pause::Stop => {
                log(if crate::tuner::on() { "listening paused while the tuner is open" } else { "listening paused by you" });
                if cur.is_some() {
                    end_take(cur.take(), &shared, "listening paused");
                    emit(&app, json!({"type": "phase", "phase": "ended"}));
                }
                if capture.take().is_some() {
                    crate::records::note("capture", json!({"what": "stopped", "why": "listening paused"}));
                }
                crate::listen::reset();
                if status_line.take().is_some() {
                    emit(&app, json!({"type": "listen", "line": null}));
                }
                capture_lost = None;
                rested = true;
                quiet_since = None;
                paused_since = None;
                pause_ends_ms = None;
                held.clear();
                held_for = None;
                heard_audio.clear();
            }
            Pause::Resume => log("listening resumed"),
            Pause::Listen | Pause::Idle => {}
        }
        if paused {
            nap(Duration::from_millis(500));
            continue;
        }

        // another app started playing: listen to that one instead (the take of the last one ends below, as
        // its track id differs)
        // (the demo song, while it plays, is followed as an app OnKey controls, whatever the mode: demo.rs)
        let listening = crate::settings::listening() && !crate::demo::on();
        // not listening (the demo song playing): listening's status line ("Still listening", …) goes
        if !listening && status_line.take().is_some() {
            emit(&app, json!({"type": "listen", "line": null}));
        }
        // what to capture: the app being followed; in listening mode, the app playing sound (found without asking it)
        let want: Option<(&'static str, &'static str)> = if listening {
            crate::listen::want_bundle().map(|b| (b, crate::listen::app_name(b)))
        } else {
            st.as_ref().filter(|(s, _)| s.playing).map(|(s, _)| (s.source.bundle(), s.source.name()))
        };
        if let Some((b, name)) = want {
            if capture.is_some() && b != capture_from {
                log(&format!("now following {name} ({b})"));
                capture = None;
            }
        } else if listening && capture.is_some() && !crate::listen::may_listen(capture_from) {
            log(&format!("stopped listening to {}", crate::listen::app_name(capture_from)));
            capture = None;         // a browser once YouTube is over: not even captured
        }
        // capture runs once the app has played (the tap only delivers while the app plays audio); stopped while
        // nothing played (resting), it starts again only once the app sounds
        if capture.is_none() {
            // (or at once to listen back: its place is found from the music, as a take's is: `Along`)
            let along = shared.lock().unwrap().play_along.is_some();
            if let Some((bundle, name)) = want.filter(|_| !rested || !listening || along || crate::listen::sounding_now()) {
                match Capture::start(bundle) {
                    Ok(c) => {
                        crate::records::note("capture", json!({"what": "started", "app": bundle, "rate": c.rate, "mic": !c.mic_uid.is_empty(),
                                                               "after_s": capture_lost.map(|l| (l.elapsed().as_secs_f64() * 100.0).round() / 100.0)}));
                        log(&format!("capture started: {name}, {} Hz, mic '{}'", c.rate,
                                     if c.mic_uid.is_empty() { "none: no input device, takes can't be sung" } else { &c.mic_name }));
                        // restarted under a take: the time without capture goes into it as silence, so its clock
                        // stays the song's (a long one ends it: the song may have moved on)
                        if let Some(lost) = capture_lost.take().filter(|_| bundle == capture_from) {
                            let gap = lost.elapsed().as_secs_f64();
                            if gap > MAX_CAPTURE_GAP_S {
                                end_take(cur.take(), &shared, "the capture was down too long");
                            } else if let Some(c) = cur.as_mut() {
                                c.take.gap(gap);
                                if let Some(l) = &c.learner {
                                    l.push_silence(gap, c.take.rate);
                                }
                                log(&format!("{}: {:.2} s without capture kept as silence", c.take.track.name, gap));
                            }
                        }
                        if bundle != capture_from {
                            music_level = capture::MusicLevel::default();     // another app: its own level
                        }
                        capture_from = bundle;
                        watchdog.started();
                        heard_audio.clear();
                        // (None unless ONKEY_SIM is set, in devtest builds; release builds have no simulated singer)
                        sim = crate::devtest::SimSinger::from_env(c.rate);
                        capture = Some(c);
                        rested = false;
                        quiet_since = None;
                        if last_problem == Some(CaptureError::TapRefused) {
                            emit(&app, json!({"type": "problem", "kind": null}));     // allowed now
                        }
                        last_problem = None;
                    }
                    Err(e) => {
                        if last_problem != Some(e) {
                            crate::records::note("capture", json!({"what": "failed", "app": bundle, "error": format!("{e:?}")}));
                            log(&format!("capture failed: {e:?}"));
                            if e == CaptureError::TapRefused {
                                emit(&app, json!({"type": "problem", "kind": "permission"}));
                            }
                            last_problem = Some(e);
                        }
                    }
                }
            }
            if capture.is_none() {
                // (a Finish pressed while nothing is heard has no play to finish: not kept for the next one)
                shared.lock().unwrap().finish = false;
                nap(Duration::from_millis(500));
                continue;
            }
        }
        let rate = capture.as_ref().unwrap().rate;

        // finish now: score this play, and don't start another of the same track until it changes or restarts
        if std::mem::take(&mut shared.lock().unwrap().finish) {
            // (also when the last poll found nothing, e.g. the song just ended or is briefly lost: the take ends
            // where it was last heard rather than being dropped unscored)
            if let Some(c) = cur.take() {
                finished = Some(c.take.track.id.clone());
                if let Some((s, _)) = &st {
                    last_pos = s.position;
                }
                log(&format!("finished {} early", c.take.track.name));
                end_take(Some(c), &shared, "finished early");
                emit(&app, json!({"type": "phase", "phase": "ended"}));
            }
        }

        // a new take when the track changes, the song restarts (repeat), or you resume after a pause
        if let Some((s, _)) = &st {
            let mut new_song = cur.as_ref().map_or(true, |c| c.take.track.id != s.track.id);
            // (a take whose place is still settling isn't restarted by it: its place moves with it, below)
            let settling = listening && cur.as_ref().is_some_and(|c| c.take.track.id == s.track.id && !c.take.place_sure && !c.take.exact_place && !c.take.app_guess);
            if s.position < last_pos - 5.0 && !settling {
                new_song = true;
            } else if cur.is_none() && finished.as_deref() == Some(s.track.id.as_str()) {
                new_song = false;                   // the rest of a song you finished early
            }
            if shared.lock().unwrap().play_along.as_deref() == Some(s.track.id.as_str()) {
                new_song = false;                   // listening back to a take: not a new take
                if let Some(c) = cur.take() {
                    end_take(Some(c), &shared, "listening back to a take");
                }
            }
            if finished.as_deref().is_some_and(|f| f != s.track.id.as_str()) || s.position < last_pos - 5.0 {
                finished = None;
            }
            // a pause doesn't end the take: the silence the app plays while paused is left out (below), so the
            // take's clock stops with the song and carries on when you resume
            if s.playing {
                if paused_since.is_some() {
                    state_changed = Instant::now();
                    // silence from before the pause was noticed can slip into the take: re-check the place
                    if let Some(c) = cur.as_mut().filter(|c| c.take.track.id == s.track.id) {
                        c.take.resumed();
                    }
                }
                paused_since = None;
                pause_ends_ms = None;
            } else if paused_since.is_none() {
                paused_since = Some(Instant::now());
                pause_ends_ms = Some(now_ms() + PAUSE_ENDS_S * 1000);
                state_changed = Instant::now();
            }
            // the song came to its end and the app stopped there: back at its start, paused (Spotify with nothing
            // queued), or another track paused (it moved on, then stopped). Scored now, not after a minute's pause
            if !s.playing && cur.as_ref().is_some_and(|c| came_to_end(&c.take.track, s, last_pos)) {
                end_take(cur.take(), &shared, "the song came to its end");
                emit(&app, json!({"type": "phase", "phase": "ended"}));
            }
            if new_song && s.playing {
                // a first play just started turns out to be a song learned here, now named as it is (its app's id
                // first, then the learned one): sung from now on, from the same start, the sound so far kept
                let carry = cur.as_ref().filter(|c| listening && c.take.mode == Mode::Learn && c.take.track.id != s.track.id
                        && store::same_title(&c.take.track, &s.track) && c.take.seconds() < HEARD_S - 0.5
                        && store.load_song(&s.track.id).is_some_and(|l| l.ready(s.track.duration)))
                    .map(|c| c.take.seconds());
                let why = match &cur {
                    _ if carry.is_some() => format!("it's {}, learned here: sung from now on", s.track.id),
                    Some(c) if c.take.track.id != s.track.id => format!("the song is now {}", s.track.name),
                    Some(_) => format!("the song moved back from {last_pos:.1} s to {:.1} s", s.position),
                    None => String::new(),
                };
                // the music app says the song moved back (you sought there): listening goes there too, at once
                if listening && crate::listen::from_app() && cur.as_ref().is_some_and(|c| c.take.track.id == s.track.id) && carry.is_none() {
                    crate::listen::app_moved(s.position - player_lead(s.source));     // (what's heard, not Spotify's lead: from the app here)
                }
                end_take(cur.take(), &shared, &why);
                version += 1;
                TAKE_VERSION.store(version, std::sync::atomic::Ordering::Relaxed);
                last_pos = s.position;
                let blind = shared.lock().unwrap().blind;
                // audio that queued up before this take (e.g. while the last one was handed over) isn't part of it;
                // in listening mode it is: the song was named a few seconds in, and the take starts from its start
                if !listening {
                    let mut stale = Vec::new();
                    capture.as_mut().unwrap().drain_into(&mut stale);
                }
                // what you hear is this far behind what's captured: Core Audio's figure, with your timing check of
                // the device when there's one (timing.rs); the bleed guard goes by the device, as before
                let (heard, reported) = crate::timing::heard_frames();
                // "Learn again" asked for: learned from scratch, as a first play is (store::learning_again)
                let again = store.learning_again(&s.track.id);
                if again {
                    log(&format!("{}: learning it again (asked for), from scratch", s.track.name));
                }
                let mut take = Take::new(s.track.clone(), if again { None } else { store.load_song(&s.track.id) }, blind, rate, heard);
                // the song's stretches marked "Not the melody" on a take's page: left out of the score live too
                take.leave_out(store.not_melody(&s.track.id));
                if heard != reported {
                    take.set_speakers(reported < 10);
                }
                // the app it plays in: the one asked, else (listening mode) the one heard
                take.app = match s.source {
                    crate::source::Source::Other => crate::listen::want_bundle().map_or(crate::source::Source::Other, crate::listen::source_of),
                    app => app,
                };
                // a learned song with no syllable bars (learned when no lyrics were found): separate the singer
                // during this play too, to make them afterwards (the melody learned already is left as it is)
                // (not with "Show lyrics" off: the bars' syllables come from the lyrics, so none could be made)
                // none, or a sliver left by an earlier play (store::thin_bars): this play makes them
                // (nor for lyrics in another script, which can't give bars: lang::alignable, syllable_bars)
                let needs_bars = take.mode == Mode::Sing && store.needs_bars(&take.track.id, take.track.duration) && crate::settings::show_lyrics()
                    && crate::lyrics::learned(&take.track.id).map_or(true, |l| crate::lang::alignable(&l));
                // never refused: a song that ended still being learned (or waiting to be: learn::Queued) doesn't
                // stop this take's learner starting; it keeps the music until its own turn
                let learner = if take.mode != Mode::Learn && !needs_bars {
                    None
                } else {
                    let finishing = crate::learn::running();
                    let l = learners(rate);
                    if l.is_some() && finishing > 0 {
                        log(&format!("learner started while {finishing} other{} finishing", if finishing == 1 { " is" } else { "s are" }));
                    }
                    if l.is_none() {
                        log("no separation model: this song can be sung but not learned");
                        // there but unusable (or not there, which the window already says): the window says why
                        if crate::models::state()["state"] == "ready" {
                            crate::models::set_state(json!({"state": "missing", "code": "unloadable", "error": "they couldn't be loaded"}));
                        }
                    }
                    if let Some(l) = &l {
                        l.report_to(crate::progress::Job::new(&take.track));     // the window shows how far it's got
                    }
                    l
                };
                // listening mode: where the song is comes from one place: the music app's own
                // position when it says, else listening's clock. The song was named some seconds after it started;
                // the sound since its start was kept, and the take (and its learner) begin with it
                // (the music app's position runs ahead of what's heard; listening's doesn't)
                let (p, at) = first_place(listening, s);
                let lead = match carry {
                    Some(had) => had,
                    None if listening => p.lead,
                    None => 0.0,
                };
                let frames = ((lead * rate) as usize).min(heard_audio.len() / CHANNELS);
                if frames > 0 {
                    let back: Vec<f32> = heard_audio.iter().skip(heard_audio.len() - frames * CHANNELS).copied().collect();
                    if let Some(l) = &learner {
                        l.push(back.chunks_exact(CHANNELS).flat_map(|f| [f[0], f[1]]).collect());
                    }
                    take.push(&back);
                }
                take.place(at);
                // the music app said where: the notes show at once, but it's only a first guess that the music's own
                // fingerprint moves (where the song is comes from the music, one way for every source)
                take.place_sure = listening && p.sure;
                if listening && p.exact {
                    take.app_guess = true;
                    take.aligned = true;
                    take.provisional = true;
                }
                // re-read once the take has some audio (setting up takes a moment); listening's own place is exact
                placed_pending = frames == 0 || !listening || !p.heard;
                log(&format!("new take ({}): {} - {} from {:.1}s{} ({}; {})", take.mode.name(), take.track.artist, take.track.name,
                             take.song_offset.unwrap_or(at),
                             if frames > 0 { format!(", with the {:.1} s heard before it was named", frames as f64 / rate) } else { String::new() },
                             if why.is_empty() { "a song started" } else { why.as_str() },
                             if take.mode == Mode::Learn || needs_bars { if learner.is_some() { "learner started" } else { "no learner" } } else { "sing" }));
                // lyrics: already lined up with the singer if the song has been learned; otherwise from LRCLIB
                let kept = store.kept_lyrics(&take.track.id);
                let lyrics_fixed = kept.is_some();
                let (text, lyric_times) = match kept {
                    Some(store::KeptLyrics::Text(l)) => (Some(l), None),
                    Some(store::KeptLyrics::Times(k)) => (None, Some(k)),
                    None => (None, None),
                };
                // its record (records.rs): what it is and how it was placed, and where its lyrics come from
                let echo_cancel = shared.lock().unwrap().echo_cancel;      // (not held through what follows)
                crate::records::take_begin(version, &take.track.id, json!({
                    "track": {"id": take.track.id, "name": take.track.name, "artist": take.track.artist, "duration": take.track.duration},
                    "mode": take.mode.name(), "blind": take.blind, "source_app": take.app.name(), "listening": listening,
                    // how the song is followed: the music app's own position, or its sound (listening's clock)
                    "followed_by": if !listening { "app (control)" } else if p.exact { "app position" } else if p.heard { "sound" } else { "app position, not yet heard" },
                    "start": {"why": if why.is_empty() { "a song started" } else { why.as_str() }, "app_pos": s.position, "at": at,
                              "lead_s": lead, "kept_s": frames as f64 / rate, "exact": p.exact, "sure": p.sure, "heard": p.heard,
                              "song_offset": take.song_offset, "carried": carry.is_some()},
                    "learner": learner.is_some(), "needs_bars": needs_bars,
                    "lyrics": {"kept": match (&text, &lyric_times) { (Some(_), _) => "words (old songs)", (_, Some(_)) => "times", _ => "none: LRCLIB" },
                               "kept_check": lyric_times.as_ref().map(|k| k.check.clone()),
                               "pinned": lyric_times.as_ref().map(|k| crate::lyrics::learned(&take.track.id).is_some_and(|l| store::lyrics_check(&l) == k.check))},
                    "output_ms": take.heard * 10, "echo_cancel": echo_cancel, "low_power": low_power()}));
                noted_place = (version, take.song_offset, "");
                // syllable bars come back without words until the lyrics are fetched (below)
                if let Some(syl) = store.syllables(&take.track.id, text.as_deref()).filter(|_| take.mode == Mode::Sing) {
                    take.use_syllables(syl);
                }
                crate::records::take_set(version, "bars", json!(take.bar_text.len()));
                if let Some(l) = text {
                    take.lyrics = l;
                    take.gate_by_lyrics();
                }
                emit(&app, take.song_event(version));
                if let Some(k) = &lyric_times {
                    // a learned song's words: the entry pinned when it was learned (LRCLIB asked only when that isn't
                    // the one its kept times were made for), shown once its kept times are put back on them
                    let (track, v, check) = (take.track.clone(), version, k.check.clone());
                    std::thread::spawn(move || {
                        let Some(c) = learned_candidates(&track, &check, v) else { return };
                        keep_for_take(&LEARNED_LYRICS, v, c);
                    });
                } else if !lyrics_fixed {
                    let (app2, track, v) = (app.clone(), take.track.clone(), version);
                    std::thread::spawn(move || {
                        let Some(l) = lyrics_for_take(&track, v) else { return };
                        emit(&app2, json!({"type": "lyrics", "version": v, "lyrics": l}));
                        keep_for_take(&LYRICS, v, l);
                    });
                }
                *CURRENT_TRACK.lock().unwrap() = Some((version, take.track.clone()));
                cur = Some(Current { take, version, learner, score: None, score_at: Instant::now(), viz_at: Instant::now(),
                                     lyrics_fixed, lyrics_job: None, lyrics_tried_at: 0.0, lyrics_retry: Instant::now(),
                                     line_ups: Default::default(), lyric_times, place_said: "", said: String::new(),
                                     same_since: Instant::now(), said_at: Instant::now(), talk_checked: false, talk_job: None });
            } else if s.playing {
                last_pos = last_pos.max(s.position);
            }
            // a cover found after the take started (Apple Music's are looked up as the song begins): kept with it
            if let Some(c) = cur.as_mut().filter(|c| c.take.track.id == s.track.id && c.take.track.art.is_none() && s.track.art.is_some()) {
                c.take.track.art.clone_from(&s.track.art);
                c.take.track.link.clone_from(&s.track.link);
                emit(&app, json!({"type": "art", "version": c.version, "art": s.track.art, "link": s.track.link}));    // and shown straight away
            }
            // you renamed the YouTube video playing (its artist and title were read wrong): shown straight away,
            // and its lyrics looked up again under the new name unless the song is already learned with lyrics
            if let Some(c) = cur.as_mut().filter(|c| c.take.track.id == s.track.id && s.source == Source::YouTube
                    && (c.take.track.name != s.track.name || c.take.track.artist != s.track.artist)
                    && s.track.id.strip_prefix(crate::youtube::ID_PREFIX).and_then(crate::youtube::named).is_some()) {
                c.take.track.name.clone_from(&s.track.name);
                c.take.track.artist.clone_from(&s.track.artist);
                *CURRENT_TRACK.lock().unwrap() = Some((c.version, c.take.track.clone()));
                emit(&app, json!({"type": "track", "version": c.version, "name": s.track.name, "artist": s.track.artist}));
                if !c.lyrics_fixed {
                    log(&format!("renamed: {} - {}; looking up its lyrics again", s.track.artist, s.track.name));
                    c.take.lyrics.clear();
                    c.take.lyrics_given.clear();
                    c.lyrics_job = None;
                    let (app2, track, v) = (app.clone(), c.take.track.clone(), c.version);
                    std::thread::spawn(move || {
                        let Some(l) = lyrics_for_take(&track, v) else { return };
                        emit(&app2, json!({"type": "lyrics", "version": v, "lyrics": l}));
                        LYRICS.lock().unwrap().retain(|(x, _)| *x != v);
                        keep_for_take(&LYRICS, v, l);
                    });
                }
            }
            if paused_since.is_some_and(|p| p.elapsed() > Duration::from_secs(PAUSE_ENDS_S)) && cur.is_some() {
                end_take(cur.take(), &shared, "paused for a minute");
                // (the window shows it ended, as every other end does: else History's card for the play under way and
                // the Sing page kept showing it, paused, after it was scored)
                emit(&app, json!({"type": "phase", "phase": "ended"}));
            }
        }

        // a take is at most MAX_TAKE_S long: a live stream or an hours-long mix is one "track" that never changes,
        // and its take (and learner) would hold ~0.5 MB of audio a second until the Mac swapped. It ends there, as
        // "finish" does: no new take of that track until it changes or restarts
        if let Some(c) = cur.take_if(|c| c.take.seconds() > MAX_TAKE_S) {
            finished = Some(c.take.track.id.clone());
            log(&format!("{} has played for {} minutes: its take ends there", c.take.track.name, MAX_TAKE_S / 60.0));
            end_take(Some(c), &shared, "it played for 20 minutes");
            emit(&app, json!({"type": "phase", "phase": "ended"}));
        }
        // the song playing was stopped from the Library (discard.rs): its take ends here, nothing kept
        if cur.as_ref().is_some_and(|c| crate::discard::dropped(&[c.take.track.id.as_str()], c.version)) {
            end_take(cur.take(), &shared, "stopped from the Library");
            emit(&app, json!({"type": "phase", "phase": "ended"}));
        }
        // a YouTube video that may not be a song (not in YouTube's Music category), with no lyrics found for it by
        // its first minute: is that minute someone talking (notmusic.rs)? Judged on its own thread
        if let Some(c) = cur.as_mut().filter(|c| c.take.mode == Mode::Learn && !c.talk_checked && c.take.lyrics.is_empty()
                && c.take.seconds() >= TALK_CHECK_S && crate::youtube::might_not_be_song(&c.take.track.id)) {
            c.talk_checked = true;
            let n = (TALK_CHECK_S as usize) * crate::notmusic::SR;
            let minute = c.take.ref16[c.take.ref16.len().saturating_sub(n)..].to_vec();
            let (tx, rx) = std::sync::mpsc::channel();
            c.talk_job = Some(rx);
            std::thread::spawn(move || { let _ = tx.send(crate::notmusic::judge(&minute)); });
        }
        if let Some(v) = cur.as_ref().and_then(|c| c.talk_job.as_ref()?.try_recv().ok()) {
            let c = cur.as_mut().unwrap();
            c.talk_job = None;
            let (id, name) = (c.take.track.id.clone(), c.take.track.name.clone());
            let category = crate::youtube::category(&id);
            crate::records::note_for(c.version, "not_a_song", json!({"category": category,
                "persist": v.map(|v| (v.persist * 1000.0).round() / 1000.0), "beat": v.map(|v| (v.beat * 1000.0).round() / 1000.0),
                "talk": v.is_some_and(|v| v.talk())}));
            match v {
                Some(v) if v.talk() => {
                    log(&format!("{name}: not a song (YouTube category '{category}', talk: held tones {:.3}, beat {:.3}): stopped learning it", v.persist, v.beat));
                    crate::discard::discard(&id, c.version);
                    end_take(cur.take(), &shared, "not a song");
                    emit(&app, json!({"type": "phase", "phase": "ended"}));
                    emit(&app, json!({"type": "not_a_song", "track_id": id, "name": name}));
                }
                Some(v) => log(&format!("{name}: a song, by its first minute (YouTube category '{category}'; held tones {:.3}, beat {:.3})", v.persist, v.beat)),
                None => log(&format!("{name}: its first minute was too quiet to tell whether it's a song")),
            }
        }

        // pull audio
        block.clear();
        capture.as_mut().unwrap().drain_into(&mut block);
        // development builds: the simulated singer, on every frame captured (its clock is the capture's)
        if let Some(s) = sim.as_mut() {
            s.apply(&mut block);
        }
        // the tap is alive (judged on the block as delivered, before listening mode leaves out what isn't the song)
        let tap_heard = capture::any_sound(&block);
        // how loud the music is captured (the app's own volume turned right down: a take isn't scored, and the window says so)
        music_level.measure(&block, rate);
        if listening {
            crate::listen::feed(&block, rate, capture_from);
            heard_audio.extend(block.iter().copied());
            let keep = (HEARD_S * rate) as usize * CHANNELS;
            if heard_audio.len() > keep {
                heard_audio.drain(..heard_audio.len() - keep);
            }
            // sound that isn't the song being sung (paused, not yet recognised, or something else now) isn't part of the take
            // (but at a pause of the song under way, the music app says it's paused a moment before its sound stops,
            // and playing a moment after it starts again: that sound is the song's. Dropped, live, a take came back
            // 0.565 s out of step after a pause, about half lost before it and half after)
            // (only when the app says so: the listener's own pause and resume come from the sound, and its clock
            // already allows for what it heard)
            if !st.as_ref().is_some_and(|(s, _)| s.playing) {
                let paused_take = cur.as_ref().filter(|c| crate::listen::from_app() && st.as_ref().is_some_and(|(s, _)| s.track.id == c.take.track.id)).map(|c| c.version);
                if paused_take.is_none() || held_for != paused_take {
                    held.clear();
                }
                held_for = paused_take;
                if held_for.is_some() && paused_since.is_some_and(|p| p.elapsed().as_secs_f64() < RESUME_HOLD_S) {
                    // just paused: the song may still be sounding, and goes in (the app's silence after it is left
                    // out below, as around any pause)
                    held.clear();
                } else {
                    if held_for.is_some() {
                        held.extend_from_slice(&block);
                        let keep = (RESUME_HOLD_S * rate) as usize * CHANNELS;
                        if held.len() > keep {
                            held.drain(..held.len() - keep);
                        }
                    }
                    block.clear();
                }
            } else if !held.is_empty() {
                // playing again: the music that restarted before OnKey heard so goes in first
                if cur.as_ref().is_some_and(|c| Some(c.version) == held_for && st.as_ref().is_some_and(|(s, _)| s.track.id == c.take.track.id)) {
                    let mut tail = resumed_tail(&held);
                    if !tail.is_empty() {
                        log(&format!("resumed: {:.2} s of the song from before OnKey heard it play again kept in the take", tail.len() as f64 / CHANNELS as f64 / rate));
                        tail.extend_from_slice(&block);
                        block = tail;
                    }
                }
                held.clear();
            }
            // the song stopped being recognised: it came to its end (that take is over), or it's lost for now (a
            // check that couldn't place it). A lost song is held for a while: found again, its take carries on,
            // learner and all, with the sound heard meanwhile (above); not found, the take is over
            if st.is_none() && cur.is_some() {
                let since = *lost_since.get_or_insert_with(Instant::now);
                let over = cur.as_ref().is_some_and(|c| crate::listen::is_over(&c.take.track.id));
                if over || since.elapsed() > Duration::from_secs(LOST_HOLD_S) {
                    end_take(cur.take(), &shared, if over { "the song came to its end" } else { "the song was lost for 20 s" });
                    // the window is told, as at every other end of a take: it kept showing the song as still playing,
                    // with a "Finish and see your score" button that did nothing since no take was open (issue #2:
                    // YouTube moved on to the next video after the song). The status line still says what's next;
                    // the page shows it instead of the ended play while a song is being found
                    emit(&app, json!({"type": "phase", "phase": "ended"}));
                }
            } else {
                lost_since = None;
            }
            // the status line: what listening is doing (finding the song, a new song, couldn't name it), or with a
            // first play, that it's being learned
            // (a take of what's playing comes first: when the music app itself says what plays, the listener may
            // still be naming it, and "Couldn't name this song" over a song being learned was wrong)
            let following = cur.as_ref().filter(|c| st.as_ref().is_some_and(|(s, _)| s.track.id == c.take.track.id));
            let stopped = crate::discard::playing_ignored();
            use crate::messages::Line;
            let line = match following {
                Some(c) if c.take.mode == Mode::Learn => Some(Line::new("learning", "Learning: sing along, it’s scored from the next play")),     // the title is right above
                Some(_) => None,
                // a song you stopped learning still playing: not "finding the next song"
                None if stopped.is_some() => stopped.map(|n| Line::new("stopped", format!("Not learning {n}")).with("song", n)),
                None if crate::listen::not_song() => Some(Line::new("podcast", "Podcast playing: OnKey sings along to music")),
                None => crate::listen::status_line(),
            };
            if line != status_line {
                log(&format!("status line: {}", line.as_ref().map_or("(none)", |l| l.text.as_str())));
                emit(&app, json!({"type": "listen", "line": line.as_ref().map(|l| l.json())}));
                status_line = line;
            }
        } else if st.is_none() && cur.is_some() && !tap_heard {
            // control mode: the music app stopped or quit mid-song (Apple Music at the end of its queue says "stopped",
            // Spotify quit): no player and no sound for STOPPED_S, the song came to its end (a single missed poll is
            // back within half a second)
            if lost_since.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(STOPPED_S) {
                end_take(cur.take(), &shared, "the music app stopped");
                emit(&app, json!({"type": "phase", "phase": "ended"}));
            }
        } else if !listening {
            lost_since = None;
        }
        // while the music is paused (and just around pausing and resuming, as its state is read every 0.4 s), the
        // tap delivers exact digital silence: leave those 10 ms pieces out of the take entirely
        // (for 3 s after a change: a browser may rebuffer for a couple of seconds after resuming)
        // (not a song's own silence in its first seconds: listening counts it as the song, and the music app's
        // position runs on through it; dropped, it left a song OnKey started 0.45 s behind until re-placed)
        // the sound output switched (headphones connected or gone): the music app goes quiet while it moves over,
        // and the tap delivers exact silence that isn't the song (27 Oct: 1.26 s of it learned into Take On Me,
        // which then ran 1.27 s off from 2:01 on every play): left out, as around a pause
        let out_now = capture::output_uid();
        if out_now != out_uid {
            if cur.is_some() {
                // (by name, as the mic is, so a report takes it out; a Bluetooth device's id is its hardware address)
                log(&format!("output switched (now '{}'): its silence is left out, as around a pause", capture::output_name()));
            }
            out_uid = out_now;
            state_changed = Instant::now();
        }
        let lead_silence = listening && paused_since.is_none()
            && cur.as_ref().is_some_and(|c| c.take.song_time().is_some_and(|t| t < LEAD_SILENCE_S));
        let around_pause = (paused_since.is_some() || state_changed.elapsed() < Duration::from_millis(3000)) && !lead_silence;
        if around_pause && !block.is_empty() {
            let mut kept = Vec::with_capacity(block.len());
            for piece in block.chunks(480 * CHANNELS) {
                if piece.chunks_exact(CHANNELS).any(|f| f[0] != 0.0 || f[1] != 0.0) {
                    kept.extend_from_slice(piece);
                }
            }
            block = kept;
        }
        follow_along(&mut along, &shared, st.as_ref(), listening, &store, rate, &block);
        let mut jumped_far: Option<f64> = None;
        if let Some(c) = cur.as_mut() {
            c.take.music_quiet = music_level.untrusted(c.take.speakers, crate::devtest::own_voice());
            // you dragged the app's progress bar forward: the time skipped becomes silence in the take. In listening
            // mode also the song found again after it was lost a moment (the take carries on, with its learner):
            // the sound heard since it was lost, kept, goes in rather than silence
            if let (Some((s, at)), Some(now)) = (&st, c.take.song_time()) {
                let polled = s.position + at.elapsed().as_secs_f64();
                let p = crate::listen::position(&s.track.id, polled - if crate::listen::from_app() { player_lead(s.source) } else { 0.0 });
                let actual = if listening { p.pos } else { polled };
                let ahead = s.playing && s.track.id == c.take.track.id && !placed_pending && at.elapsed() < Duration::from_millis(300)
                    && skipped_at.elapsed() > Duration::from_millis(2500) && actual - now > 3.0;
                // (moved minutes ahead, in an hours-long track: not filled with silence, it ends the take, as a long
                // capture gap does, and the next starts where the song is)
                if ahead && actual - now > MAX_FILL_S {
                    jumped_far = Some(actual - now);
                } else if ahead {
                    // (the music app says it moved on: listening goes there too, and none of the sound before is the new place's)
                    if listening && p.exact {
                        crate::listen::app_moved(actual);
                    }
                    // the sound in `block` is the newest, after what's kept: it goes in after
                    let kept = (heard_audio.len() / CHANNELS).saturating_sub(block.len() / CHANNELS);
                    let real = if listening && !p.exact { (p.lead - block.len() as f64 / CHANNELS as f64 / rate).max(0.0).min(actual - now) } else { 0.0 };
                    let frames = ((real * rate) as usize).min(kept);
                    let silent = actual - now - frames as f64 / rate - block.len() as f64 / CHANNELS as f64 / rate;
                    if silent > 0.0 {
                        c.take.moved_ahead(silent);
                        if let Some(l) = &c.learner {
                            l.push_silence(silent, c.take.rate);
                        }
                    }
                    if frames > 0 {
                        let back: Vec<f32> = heard_audio.iter().skip((kept - frames) * CHANNELS).take(frames * CHANNELS).copied().collect();
                        if let Some(l) = &c.learner {
                            l.push(back.chunks_exact(CHANNELS).flat_map(|f| [f[0], f[1]]).collect());
                        }
                        c.take.push(&back);
                    }
                    skipped_at = Instant::now();
                    crate::records::note_for(c.version, "placement", json!({"what": "moved ahead", "by_s": ((actual - now) * 100.0).round() / 100.0,
                        "kept_s": frames as f64 / rate, "take_s": c.take.seconds()}));
                    log(&format!("{} moved ahead {:.1} s in {} ({:.1} s of it heard and kept, the rest left silent)",
                                 c.take.track.name, actual - now, s.source.name(), frames as f64 / rate));
                }
            }
            if !block.is_empty() {
                if let Some(l) = &c.learner {
                    l.push(block.chunks_exact(CHANNELS).flat_map(|f| [f[0], f[1]]).collect());
                }
                c.take.push(&block);
                dot.pushed(c.take.clean16.len());
            }
            // (Windows, Linux) the music app's position against the place the sound found, once a second: "app vs sound"
            // in the log when the take ends (digest.rs)
            #[cfg(any(windows, target_os = "linux"))]
            if let (Some((s, at)), Some(sound)) = (&st, c.take.song_time()) {
                if s.playing && c.take.fingerprint_placed() && (s.track.id == c.take.track.id || s.track.name.eq_ignore_ascii_case(&c.take.track.name)) {
                    crate::digest::gap_sample(c.version, s.source.name(), s.position + at.elapsed().as_secs_f64(), sound, crate::records::now());
                } else {
                    crate::digest::gap_pause(c.version);
                }
            }
            // the app's position against the audio this take has: a better first placement
            if placed_pending && c.take.seconds() > 1.0 {
                if let Some((s, at)) = &st {
                    if s.track.id == c.take.track.id && s.playing && at.elapsed() < Duration::from_millis(300) {
                        c.take.place(app_place(listening, s, *at));
                        placed_pending = false;
                    }
                }
            }
            // skip to singing: jump the song ahead, and fill the take with the time skipped
            let skip_to = shared.lock().unwrap().skip_to.take();
            if let (Some(target), Some(now)) = (skip_to, c.take.song_time()) {
                // listening mode: the app playing it, if you've allowed OnKey to control it (else no skip)
                let app = if listening { st.as_ref().map_or(Source::Other, |(s, _)| s.source) } else { Source::of_id(&c.take.track.id) };
                if target > now + 1.0 && target < c.take.track.duration - 1.0 && source::controllable(app) {
                    app.seek(target);
                    c.take.skip(target - now);
                    if let Some(l) = &c.learner {
                        l.push_silence(target - now, c.take.rate);
                    }
                    skipped_at = Instant::now();
                    crate::records::note_for(c.version, "placement", json!({"what": "skip to singing", "by_s": target - now, "to_s": target}));
                    log(&format!("skipped {:.1} s to {:.1} s", target - now, target));
                }
            }
            if c.take.lyrics.is_empty() {
                let learned = c.lyric_times.as_ref().and_then(|_| {
                    let mut got = LEARNED_LYRICS.lock().unwrap();
                    let i = got.iter().position(|(v, _)| *v == c.version)?;
                    Some(got.swap_remove(i).1)
                });
                let fetched = if c.lyric_times.is_some() { None } else {
                    LYRICS.lock().unwrap().iter().find(|(v, _)| *v == c.version).map(|x| x.1.clone())
                };
                let shown = match (learned, c.lyric_times.take()) {
                    (Some(cands), Some(kept)) => {
                        // a learned song: its kept times on the entry they were made for, the same every play
                        let l = learned_lyrics(&store, &c.take.track, &kept, &cands);
                        if let Some(syl) = store.syllables(&c.take.track.id, Some(&l)).filter(|s| s.len() == c.take.bar_text.len()) {
                            c.take.bar_text = std::sync::Arc::new(syl.into_iter().map(|b| b.3).collect());
                            emit(&app, c.take.notes_event(c.version));
                        }
                        emit(&app, json!({"type": "lyrics", "version": c.version, "lyrics": l}));
                        Some(l)
                    }
                    (None, kept) => { c.lyric_times = kept; fetched }
                    (Some(_), None) => None,
                };
                if let Some(l) = shown {
                    crate::records::note_for(c.version, "lyric_events", json!({"what": "shown", "check": store::lyrics_check(&l), "lines": l.len(),
                        "at_song_s": c.take.song_time().map(|t| (t * 100.0).round() / 100.0),
                        "times": l.iter().map(|x| (x.0 * 100.0).round() / 100.0).collect::<Vec<_>>()}));
                    c.take.lyrics = l.clone();
                    c.take.lyrics_given = l.clone();
                    if c.take.gate_by_lyrics() {
                        emit(&app, c.take.notes_event(c.version));
                    }
                }
            }
            c.take.step(&mut crepe);
            // first play: the learner separates the singer as the song plays, 5 s at a time, until the lyrics' line-up
            // has settled (at most the first 90 s: learn::chunk_s; the rest is learned after the song, [sing-first]);
            // each time another stretch has been separated the lyrics are lined up with the words heard
            // (syllables::line_up_live); lines already shown keep their time. Not on a small Mac (lowmem.rs): no
            // separating models in memory beside the singing there. Not while the song before is being learned
            // (learn::learning_now: one learner at a time); after, only while the play is still in its first 90 s or
            // so, catching up; past that its lyrics are placed by the words heard once it's learned (syllable_bars).
            // Nor for lyrics in another script (Mandarin, Japanese...), which the English word timer can't hear
            // (lang::alignable): LRCLIB's times stay, and the singer is separated at the usual pace.
            if !c.lyrics_fixed && !c.take.lyrics_given.is_empty() && crate::lang::alignable(&c.take.lyrics_given) {
                if let Some(l) = &c.learner {
                    l.quick(!c.line_ups.settled() && !crate::lowmem::low_memory());
                }
                if let Some(rx) = &c.lyrics_job {
                    if let Ok((result, given, busy)) = rx.try_recv() {
                        c.lyrics_job = None;
                        if busy {
                            c.lyrics_tried_at -= 5.0;           // the aligner was busy with the last song: again soon
                            c.lyrics_retry = Instant::now() + Duration::from_secs(3);
                        }
                        let now = c.take.song_time().unwrap_or(0.0);
                        let fits = given == c.take.lyrics_given && c.take.lyrics.len() == given.len();
                        if let Some(a) = &result {
                            crate::records::note_for(c.version, "lyric_events", json!({"what": "live line-up", "heard": a.heard, "moved": (a.moved * 100.0).round() / 100.0,
                                "fits": fits, "at_song_s": (now * 100.0).round() / 100.0}));
                        }
                        if let Some(a) = result.filter(|a| fits && c.line_ups.accept(a.moved, a.heard)) {
                            crate::records::note_for(c.version, "lyric_events", json!({"what": "live line-up shown", "moved": (a.moved * 100.0).round() / 100.0}));
                            let lyrics = crate::syllables::keep_shown(&c.take.lyrics, &a.lyrics, now);
                            if lyrics != c.take.lyrics {
                                log(&format!("lyrics lined up with the singer: {} lines heard, moved {:+.2} s", a.heard, a.moved));
                                c.take.lyrics = lyrics.clone();
                                emit(&app, json!({"type": "lyrics", "version": c.version, "lyrics": lyrics, "pace": a.pace}));
                            }
                        }
                    }
                } else if let Some(l) = &c.learner {
                    let have = l.vocal_so_far.lock().unwrap().len() as f64 / 16_000.0;
                    if have >= c.lyrics_tried_at + 5.0 && Instant::now() >= c.lyrics_retry && c.take.song_offset.is_some()
                        && c.take.lyrics.len() == c.take.lyrics_given.len() {
                        c.lyrics_tried_at = have;
                        let vocal = l.vocal_so_far.lock().unwrap().clone();
                        let (off, now, prior) = (c.take.song_offset.unwrap_or(0.0), c.take.song_time().unwrap_or(0.0), c.line_ups.believed());
                        let (given, shown) = (c.take.lyrics_given.clone(), c.take.lyrics.clone());
                        let (tx, rx) = channel();
                        std::thread::spawn(move || {
                            crate::learn::lower_priority();     // (background work, as learning is)
                            let (r, busy) = line_up_now(&vocal, off, &given, &shown, now, prior);
                            let _ = tx.send((r, given, busy));
                        });
                        c.lyrics_job = Some(rx);
                    }
                }
            }
            if c.score_at.elapsed() > Duration::from_millis(500) {
                c.score_at = Instant::now();
                c.score = c.take.running();
            }
            let (blind, cancel) = { let s = shared.lock().unwrap(); (s.blind, s.echo_cancel) };
            c.take.set_echo_cancel(cancel);
            if c.take.mode == Mode::Sing {
                c.take.blind |= blind;     // once a take was blind for any part, it counts as blind
            }
            // (a listening take's place settles once listening's does)
            // Until then its place follows listening's as that settles (a first guess on answers that disagreed,
            // then the place checks agree on): the take's sound runs on in order, so the whole take moves with it
            if listening && !c.take.place_sure && !c.take.exact_place && !c.take.app_guess {
                let p = crate::listen::position(&c.take.track.id, 0.0);
                if let (true, Some(now)) = (p.heard, c.take.song_time()) {
                    if (p.pos - now).abs() > 0.2 {
                        log(&format!("{}: take moved {:+.1} s with listening's place, while it settles", c.take.track.name, p.pos - now));
                        c.take.song_offset = Some(p.pos - c.take.seconds());
                        last_pos = p.pos;
                    }
                }
                c.take.place_sure = p.sure;
            }
            let sent_from = c.take.sent_usr();
            let mut ev = c.take.update_event(c.version, c.score);
            // how sure the place is: the window shows lyrics and notes once it's settled (or after 4 s)
            let place = if c.take.exact_place || !listening { "exact" } else if c.take.place_sure || c.take.fingerprint_placed() { "confirmed" } else if c.take.app_guess { "guess" } else { "provisional" };
            if place != c.place_said {
                log(&format!("{}: place {place} at {:.1} s into the take (song {:.1} s)", c.take.track.name, c.take.seconds(),
                             c.take.song_time().unwrap_or(0.0)));
                c.place_said = place;
            }
            ev["place"] = json!(place);
            ev["blind"] = json!(blind || c.take.mode == Mode::Learn);
            // the records: each change of the take's place (by the app, listening or the fingerprint), and how far
            // its live pitch lags the audio it has; when this update left, for the window to time its delivery
            for m in std::mem::take(&mut c.take.moves) {
                let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
                crate::records::note_for(c.version, "placement", json!({"what": "fingerprint move", "take_s": r3(m.take_s), "from": r3(m.from),
                    "to": r3(m.to), "moved": r3(m.to - m.from), "conf": (m.conf * 100.0).round() / 100.0, "rule": m.rule, "two_checks_4s_apart": m.apart,
                    "short_music": m.short, "place_from": if m.exact_place || !listening { "music app" } else if m.place_sure { "listening (settled)" } else { "first guess" }}));
                // the music app's place against the music's own: its real lead on this output (beyond the lead allowed)
                if m.exact_place || !listening {
                    crate::records::take_set(c.version, "app_vs_fingerprint", json!({"app_minus_fingerprint_s": r3(m.from - m.to),
                        "lead_allowed_s": player_lead(c.take.app), "output_ms": c.take.heard * 10, "at_take_s": r3(m.take_s), "conf": (m.conf * 100.0).round() / 100.0}));
                }
            }
            if noted_place.0 != c.version || place != noted_place.2
                || c.take.song_offset.zip(noted_place.1).map_or(c.take.song_offset.is_some() != noted_place.1.is_some(), |(a, b)| (a - b).abs() > 0.03) {
                let r2 = |x: f64| (x * 100.0).round() / 100.0;
                crate::records::note_for(c.version, "placement", json!({"take_s": r2(c.take.seconds()), "song_offset": c.take.song_offset.map(r2),
                    "moved": c.take.song_offset.zip(noted_place.1).map(|(a, b)| r2(a - b)), "place": place, "aligned": c.take.aligned,
                    "provisional": c.take.provisional, "fingerprint": c.take.fingerprint_placed(), "conf": (c.take.align_conf * 100.0).round() / 100.0,
                    "exact": c.take.exact_place, "sure": c.take.place_sure}));
                noted_place = (c.version, c.take.song_offset, place);
            }
            crate::records::pitch_lag(((c.take.seconds() / crate::dsp::FRAME_S) - c.take.usr_done as f64).max(0.0) * crate::dsp::FRAME_S * 1000.0);
            let said = ev.to_string();
            if said != c.said {
                c.said = said;
                c.same_since = Instant::now();
            }
            if c.same_since.elapsed() < UPDATE_STILL_AFTER || c.said_at.elapsed() >= UPDATE_STILL_EVERY {
                c.said_at = Instant::now();
                // when it left (epoch ms): the window times how late updates reach it (records.rs, its frame stats)
                ev["at_ms"] = json!(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64));
                emit(&app, ev);
                dot.emitted(&c.take.usr, sent_from, c.take.sent_usr());
            }
            if c.viz_at.elapsed() >= Duration::from_millis(100) {
                c.viz_at = Instant::now();
                if let Some((f0, frames)) = c.take.viz.take_frames() {
                    emit(&app, crate::viz::event(c.version, f0, &frames));
                }
            }
        }

        if let Some(by) = jumped_far {
            end_take(cur.take(), &shared, &format!("the song moved {by:.0} s ahead"));
        }

        // watchdog: the Mac woke, or the tap dead while the app plays (capture::Watchdog): restart it
        let playing = matches!(&st, Some((s, _)) if s.playing && s.position > 5.0);
        if let Some(why) = watchdog.check(tap_heard, playing, capture_from) {
            crate::records::note("capture", json!({"what": "restart", "why": why.to_string()}));
            log(&format!("{why}: restarting the capture"));
            capture = None;
            capture_lost = Some(Instant::now());
            continue;
        }
        // nothing playing and no take for a while: let go of the capture (see REST_AFTER)
        let quiet = cur.is_none() && if listening { !crate::listen::sounding_now() } else { !st.as_ref().is_some_and(|(s, _)| s.playing) };
        if !quiet {
            quiet_since = None;
            busy_now();
        } else if quiet_since.get_or_insert_with(Instant::now).elapsed() >= REST_AFTER {
            log(&format!("nothing played for {} s: capture stopped until music plays", REST_AFTER.as_secs()));
            capture = None;
            rested = true;
            quiet_since = None;
            continue;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// When a take or the music was last going (ms since `CLOCK` started): background chores wait for quiet.
static LAST_BUSY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CLOCK: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn busy_now() {
    let ms = CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u64;
    LAST_BUSY_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
}

/// How long nothing has been sung or played (no take, the music app quiet): since launch if nothing yet.
pub fn quiet_for() -> Duration {
    let now = CLOCK.get_or_init(Instant::now).elapsed();
    now.saturating_sub(Duration::from_millis(LAST_BUSY_MS.load(std::sync::atomic::Ordering::Relaxed)))
}

/// Longer than this without capture (restarting it kept failing) and the take running ends.
const MAX_CAPTURE_GAP_S: f64 = 30.0;

static LYRICS: Mutex<Vec<(u64, Vec<(f64, String)>)>> = Mutex::new(Vec::new());

/// The take playing now (its version), for lyrics still being looked for to know when to stop.
static TAKE_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The last take that ended (its version): with no new take after it, TAKE_VERSION alone would keep it going.
static TAKE_ENDED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Lyrics found for take `v`, kept for it while it goes on (a take that ended first, or any before it, has its
/// dropped: kept forever, they grew with every first play of a long session).
fn keep_for_take<T>(list: &Mutex<Vec<(u64, T)>>, v: u64, lyrics: T) {
    let mut l = list.lock().unwrap();
    l.retain(|(x, _)| take_on(*x));
    if take_on(v) {
        l.push((v, lyrics));
    }
}

/// The take `version` is still going on.
fn take_on(version: u64) -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    TAKE_VERSION.load(Relaxed) == version && TAKE_ENDED.load(Relaxed) != version
}

/// The take playing now: its version and track (as named now: a renamed video changes it), for "Look again".
static CURRENT_TRACK: Mutex<Option<(u64, crate::source::Track)>> = Mutex::new(None);

/// "Look again" under "No lyrics for this song yet" (the Sing page, a song's page): ask LRCLIB about the song once
/// more, forgetting the "none" answers kept for it (lyrics::look_again). Found: the play under way shows them as it
/// would have (the live path: engine loop, LYRICS), and a learned song keeps them (lyrics::keep_learned) so its next
/// play lines them up and makes its syllable bars (needs_bars). "found", "none", "unreachable" (LRCLIB couldn't be
/// asked), or "unknown" (no such song: not playing, not learned).
pub fn look_again(app: &AppHandle, store: &Store, id: &str) -> &'static str {
    let playing = CURRENT_TRACK.lock().unwrap().clone().filter(|(v, t)| t.id == id && take_on(*v));
    let learned = store.learned_tracks().into_iter().find(|t| t.id == id);
    let Some(track) = playing.as_ref().map(|(_, t)| t.clone()).or(learned.clone()) else { return "unknown" };
    log(&format!("{}: looking for its lyrics again (asked for)", track.name));
    match crate::lyrics::look_again(&track) {
        crate::lyrics::LookAgain::Found(l) => {
            log(&format!("{}: lyrics found on looking again ({} lines)", track.name, l.len()));
            if let Some((v, _)) = playing {
                emit(app, json!({"type": "lyrics", "version": v, "lyrics": l}));
                // (the loop adopts them: the empty answer kept for the take goes first)
                LYRICS.lock().unwrap().retain(|(x, _)| *x != v);
                keep_for_take(&LYRICS, v, l.clone());
            }
            // a learned song without lyrics: pinned to it, as a song learned with them has (never over the entry a
            // song with kept times was learned with)
            if learned.is_some() && store.kept_lyrics(id).is_none() {
                crate::lyrics::keep_learned(id, &l);
            }
            "found"
        }
        crate::lyrics::LookAgain::None => "none",
        crate::lyrics::LookAgain::Unreachable => "unreachable",
    }
}

/// A take's lyrics: a learned song's from what's kept with it (no network); otherwise from LRCLIB, asked again
/// (after 5 s, then 10, 20 … up to a minute) for as long as LRCLIB can't be asked and the take goes on: the
/// notes show meanwhile, and the words join them when they come. None: the take ended first.
fn lyrics_for_take(track: &crate::source::Track, version: u64) -> Option<Vec<(f64, String)>> {
    let mut wait = 5u64;
    loop {
        if let Ok(l) = crate::lyrics::fetch_answered(track) {
            return Some(l);
        }
        if !take_on(version) {
            return None;
        }
        log(&format!("{}: LRCLIB can't be asked now; its lyrics again in {wait} s (the notes show meanwhile)", track.name));
        for _ in 0..wait {
            std::thread::sleep(Duration::from_secs(1));
            if !take_on(version) {
                return None;
            }
        }
        wait = (wait * 2).min(60);
    }
}

/// LRCLIB entries for a learned song whose times are kept for the words checked as `check`: the entry pinned on this
/// Mac if it's that one, or else one kept on this Mac from an earlier answer (no network); otherwise the pinned
/// entry, LRCLIB's answer (waited for when nothing is pinned) and, unless that's the one, every entry LRCLIB has for
/// the song (see store::learned_play).
fn learned_candidates(track: &crate::source::Track, check: &str, version: u64) -> Option<Vec<Vec<(f64, String)>>> {
    let pinned = crate::lyrics::learned(&track.id);
    if let Some(p) = pinned.as_ref().filter(|p| store::lyrics_check(p) == check) {
        return Some(vec![p.clone()]);
    }
    if let Some(e) = crate::lyrics::kept_entries(track).into_iter().find(|e| store::lyrics_check(e) == check) {
        return Some(vec![e]);
    }
    let mut c: Vec<Vec<(f64, String)>> = pinned.into_iter().collect();
    if c.is_empty() {
        c.push(lyrics_for_take(track, version)?);
    } else if let Ok(l) = crate::lyrics::fetch_answered(track) {
        c.push(l);
    }
    if !c.iter().any(|e| store::lyrics_check(e) == check) {
        if let Ok(e) = crate::lyrics::entries(track) {
            c.extend(e);
        }
    }
    c.retain(|e| !e.is_empty());
    Some(c)
}

/// A learned song's lyrics for this play from its candidate entries (store::learned_play), and what's kept for its
/// next plays: the entry it was learned with, once found, is pinned (a play never depends on what LRCLIB answers
/// that day), and a checked pairing of another entry becomes the song's kept times (its next plays are exact).
fn learned_lyrics(store: &Store, track: &crate::source::Track, kept: &store::Kept, cands: &[Vec<(f64, String)>]) -> Vec<(f64, String)> {
    let voiced: Option<Vec<bool>> = store.load_song(&track.id).map(|s| s.midi.iter().map(|m| m.is_finite() && *m > 0.0).collect());
    let Some(p) = store::learned_play(kept, cands, voiced.as_deref()) else { return Vec::new() };
    let pinned = crate::lyrics::learned(&track.id);
    crate::records::note_for(TAKE_VERSION.load(std::sync::atomic::Ordering::Relaxed), "lyric_events", json!({"what": "learned entry",
        "how": format!("{:?}", p.how), "kept_check": kept.check, "entry_check": store::lyrics_check(&p.entry),
        "pinned_check": pinned.as_ref().map(|l| store::lyrics_check(l)), "candidates": cands.len(), "kept_times": kept.times.len()}));
    match p.how {
        store::LearnedHow::Exact => {
            if !pinned.as_ref().is_some_and(|l| store::lyrics_check(l) == kept.check) {
                crate::lyrics::keep_learned(&track.id, &p.entry);
                log(&format!("{}: the lyrics it was learned with found again and kept", track.name));
            }
        }
        store::LearnedHow::Repaired => {
            // the bars' words follow the new entry (their places in it kept), then its times
            if let Some(bars) = store.syllables(&track.id, Some(&p.lyrics)) {
                store.save_syllables(&track.id, &bars, &p.lyrics);
            }
            store.save_lyrics(&track.id, &p.lyrics);
            crate::lyrics::keep_learned(&track.id, &p.entry);
            log(&format!("{}: LRCLIB has other lyrics than it was learned with; their lines, paired with the kept times, fit the singing, so they're kept with the song", track.name));
        }
        store::LearnedHow::Unchecked | store::LearnedHow::Unfit => {
            if pinned.is_none() {
                crate::lyrics::keep_learned(&track.id, &p.entry);
            }
            log(&format!("{}: other lyrics than it was learned with; {}", track.name,
                         if p.how == store::LearnedHow::Unfit { "they don't pair with its kept times, so they keep LRCLIB's timing" }
                         else { "paired with its kept times, but they fit the singing worse, so they're not kept with the song" }));
        }
    }
    p.lyrics
}

/// Lyric lines (song s, text).
type Lines = Vec<(f64, String)>;

static LEARNED_LYRICS: Mutex<Vec<(u64, Vec<Lines>)>> = Mutex::new(Vec::new());

/// Learned songs whose lyrics aren't kept with them yet (learned before they were): found among LRCLIB's answers
/// already kept on this Mac, or else asked for once, politely, a few minutes after OnKey starts, one song at a time
/// (LRCLIB's requests go one at a time, app-wide, and wait when it asks): only the entry its kept times were made
/// for is kept. Songs it can't ask about now are tried again next start.
pub fn keep_learned_lyrics(store: Store) {
    std::thread::Builder::new().name("onkey-lyrics-keep".into()).spawn(move || {
        std::thread::sleep(Duration::from_secs(120));
        let (mut kept, mut missing) = (0, 0);
        for t in store.learned_tracks() {
            if !crate::settings::show_lyrics() {
                break;      // "Show lyrics" off: LRCLIB isn't asked
            }
            let Some(store::KeptLyrics::Times(k)) = store.kept_lyrics(&t.id) else { continue };
            if crate::lyrics::learned(&t.id).is_some_and(|l| store::lyrics_check(&l) == k.check) {
                continue;
            }
            // only the very entry its times were made for is pinned (another is paired and checked when it's played)
            let exact = |l: &Vec<(f64, String)>| store::lyrics_check(l) == k.check;
            if let Some(l) = crate::lyrics::kept_entries(&t).into_iter().find(exact) {
                crate::lyrics::keep_learned(&t.id, &l);
                kept += 1;
                continue;
            }
            match crate::lyrics::fetch_answered(&t).and_then(|l| {
                if exact(&l) { Ok(Some(l)) } else { crate::lyrics::entries(&t).map(|e| e.into_iter().find(exact)) }
            }) {
                Ok(Some(l)) => {
                    crate::lyrics::keep_learned(&t.id, &l);
                    kept += 1;
                }
                Ok(None) => missing += 1,
                Err(_) => {
                    log("learned songs' lyrics: LRCLIB can't be asked now; the rest next time");
                    break;
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        if kept + missing > 0 {
            log(&format!("learned songs' lyrics: kept for {kept} more songs ({missing} found none that fit)"));
        }
    }).ok();
}

/// How much of the sound heard while the song under way is thought paused is held (s), in case it turns out to have
/// been playing again already; also how long after a pause is heard of the sound still goes into the take.
const RESUME_HOLD_S: f64 = 1.5;

/// The music that had restarted before OnKey heard it was playing again: the sound at the end of `held` (frames as
/// the tap delivers them) after the paused app's digital silence. Empty unless that silence is there (sound all
/// through isn't a resume: something else, or a song not yet named).
fn resumed_tail(held: &[f32]) -> Vec<f32> {
    let piece = 480 * CHANNELS;
    let silent = |p: &[f32]| p.chunks_exact(CHANNELS).all(|f| f[0] == 0.0 && f[1] == 0.0);
    let pieces: Vec<&[f32]> = held.chunks(piece).collect();
    let sound = pieces.iter().rev().take_while(|p| !silent(p)).count();
    if sound == 0 || sound == pieces.len() {
        return Vec::new();
    }
    pieces[pieces.len() - sound..].concat()
}

/// Listening back: the song's place, from the audio just captured (`block`, the same as a take gets), published for
/// player_now. Started once the app plays the track after it was told where to (`along_from`), and afresh when the
/// song moves more than 3 s from its place (a seek, or a drag in the app), as a take restarts or moves ahead.
fn follow_along(along: &mut Option<Along>, shared: &Arc<Mutex<Shared>>, st: Option<&(source::State, Instant)>, listening: bool,
                store: &Store, rate: f64, block: &[f32]) {
    let want = {
        let sh = shared.lock().unwrap();
        sh.play_along.clone().zip(sh.along_from)
    };
    let st = st.filter(|(s, _)| want.as_ref().is_some_and(|(id, _)| *id == s.track.id));
    let (Some((s, read)), Some((_, from))) = (st, want) else {
        *along = None;
        shared.lock().unwrap().along_place = None;
        return;
    };
    if !s.playing {
        if let Some(a) = along.as_mut() {
            a.paused = true;
        }
        shared.lock().unwrap().along_place = None;
        return;
    }
    let fresh = read.elapsed() < Duration::from_millis(300);
    let moved = along.as_ref().and_then(|a| a.take.song_time()).is_some_and(|now| fresh && (app_place(listening, s, *read) - now).abs() > 3.0);
    if *read > from && (along.as_ref().map_or(true, |a| a.from != from) || moved) {
        *along = Some(Along::start(listening, s, store, rate, from));
    }
    let Some(a) = along.as_mut() else { return };
    if std::mem::take(&mut a.paused) {
        a.take.resumed();
    }
    a.take.push(block);
    if a.pending && a.take.seconds() > 1.0 && fresh {
        a.take.place(app_place(listening, s, *read));
        a.pending = false;
    }
    a.take.step_place();
    shared.lock().unwrap().along_place = a.take.song_time().map(|p| (p, Instant::now()));
}

/// Listening back, as the music app's state comes in (track id, playing): heard once the app plays the track, over
/// when it moves to another track (once the track was heard), and after Stop once the music is paused or gone.
pub fn along_follows(sh: &mut Shared, st: Option<(&str, bool)>) {
    let Some(p) = sh.play_along.clone() else {
        sh.along_stopping = false;
        return;
    };
    if st.is_some_and(|(id, _)| id == p) {
        sh.along_heard = true;
    }
    let other = st.is_some_and(|(id, _)| id != p);
    let stopped = sh.along_stopping && st.map_or(true, |(id, playing)| id != p || !playing);
    if (sh.along_heard && other) || stopped {
        sh.play_along = None;
        sh.along_stopping = false;
    }
}

/// How far ahead of the sound a music app's own reported position is (s), when OnKey follows the app (not
/// listening mode, whose clock is the sound itself). Spotify's runs ahead: on 6 first plays timed by it, the
/// lyrics showed 0.39-0.88 s before the singer (median 0.67 s, of which 0.15 s is the window's deliberate lead);
/// allowing 0.45 s leaves them a little early (median 0.23 s), not late.
/// Where a song just started is, to place a take from (and listening back's `Along`): in listening mode listening's
/// place (the music app's own position when it says, less its lead), else the app's position less its lead. Then
/// the music's own fingerprint places it (Take::step).
fn first_place(listening: bool, s: &source::State) -> (crate::listen::Position, f64) {
    let app_lead = if crate::listen::from_app() { player_lead(s.source) } else { 0.0 };
    let p = crate::listen::position(&s.track.id, s.position - app_lead);
    let at = if listening && !p.exact { p.pos } else { s.position - player_lead(s.source) };
    (p, at)
}

/// The music app's position now (read at `read`), as the song's place: a take's better first placement, once it has
/// a second of audio (in listening mode without the app's position, listening's clock, which has no lead).
fn app_place(listening: bool, s: &source::State, read: Instant) -> f64 {
    s.position + read.elapsed().as_secs_f64() - if listening && !crate::listen::from_app() { 0.0 } else { player_lead(s.source) }
}

/// Listening back to a take ("Play with the song"): the song's place, found exactly as a take finds it. A take of it
/// that's never scored or kept: placed first from the app (`first_place`, `app_place` after a second), then by the
/// music's fingerprint as it plays (Take::step_place), started afresh when the song moves.
struct Along {
    take: Take,
    /// the app's position is to be read again once there's a second of audio (as `placed_pending` for a take)
    pending: bool,
    /// the moment the app was told where to play from that this place started after (lib.rs play_along)
    from: Instant,
    paused: bool,
}

impl Along {
    fn start(listening: bool, s: &source::State, store: &Store, rate: f64, from: Instant) -> Along {
        let (p, at) = first_place(listening, s);
        let mut take = Take::new(s.track.clone(), store.load_song(&s.track.id), true, rate, crate::timing::heard_frames().0);
        take.set_echo_cancel(false);     // (only its place is wanted: the mic isn't listened to)
        take.place(at);
        log(&format!("play with the song: {} placed from {at:.2} s{}", s.track.name, if p.heard { " (listening's clock)" } else { "" }));
        Along { take, pending: !listening || !p.heard, from, paused: false }
    }
}

fn player_lead(source: Source) -> f64 {
    // (not learned per output: the music's own fingerprint places each take, from this as a first guess; a learned
    // lead swung between -0.4 and 1.5 s live and started songs further off)
    if source == Source::Spotify { 0.45 } else { 0.0 }
}

/// Full score for a finished take; a first play is learned too.
fn score_take(app: &AppHandle, shared: &Arc<Mutex<Shared>>, store: &Store, done: Done) {
    let Done { mut take, learner, live, entry, version, queued } = done;
    // (for the log: how long learning and scoring took after the play ended, waiting included)
    let ended = Instant::now();
    // how it ended, for its record (records.rs)
    let mut outcome = Value::Null;
    // this thread's work on the song (finishing its learner, scoring, timing the words…) shows in the window, and
    // is background work: it waits while you sing (learn::yield_to_singing), at a low priority
    let job = learner.as_ref().and_then(|l| l.job());
    let _reporting = job.as_ref().map(|j| j.enter());
    if learner.is_some() {
        crate::learn::lower_priority();
    }
    let had_learner = learner.is_some();
    // stopped from the Library meanwhile (discard.rs), by any id the song goes by: its learner stops at once, and
    // nothing more of it is saved
    let played_id = take.track.id.clone();
    let watch = learner.as_ref().map(|l| crate::discard::watch(vec![played_id.clone()], l.stop_flag(), version));
    let is_dropped = |id: &str| crate::discard::dropped(&[played_id.as_str(), id], version);
    // forgotten from the Library while this play was under way: scored, but the song isn't made again from it
    let forgot = |id: &str| forgotten_during(&[played_id.as_str(), id], version);
    let update = |fields: Value| {
        if let Some(e) = shared.lock().unwrap().takes.get_mut(entry) {
            for (k, v) in fields.as_object().unwrap() {
                e[k] = v.clone();
            }
        }
        takes_changed(app, shared);
    };
    // a song named without its length (Apple's lookup failed then): asked again, else as far as it was heard, so
    // what's learned is measured against it (with no length, a whole play counted as 0% learned)
    if take.track.duration <= 0.0 {
        let heard_to = take.song_offset.unwrap_or(0.0) + take.seconds();
        let before = store.song_duration(&store.canonical(&take.track).id);
        let apple = take.track.link.as_deref().and_then(crate::recognize::song_length);
        take.track.duration = guessed_length(apple, heard_to, before);
        // saved as a guess, so a later play's exact length or longer guess can replace it (store::keep_known)
        store::note_play(apple.is_none(), take.track.id.starts_with(crate::youtube::ID_PREFIX));
        log(&format!("{}: its length wasn't known; now {:.1} s", take.track.name, take.track.duration));
    } else if take.track.id.starts_with(crate::youtube::ID_PREFIX) {
        // a video's length (with its intro and outro) is a guess at the song's, and its title a video's: an app's
        // exact length and name stay
        store::note_play(true, true);
    }
    // kept under the song's one id (its ISRC when it has one), whatever id this play had: never a second entry
    let canon = store.canonical(&take.track);
    if canon.id != take.track.id {
        log(&format!("{}: kept as {} (played as {})", take.track.name, canon.id, take.track.id));
        take.track = canon;
        if let Some(j) = &job {
            j.set_track(&take.track);
        }
        if let Some(w) = &watch {
            w.also(&take.track.id);
        }
    }
    crate::devtest::keep_mic(&take.track, take.song_offset.unwrap_or(0.0), take.heard, &take.mic16, &take.clean16, &take.ref16);
    let mut learnt_kept: Option<(Vec<f32>, Vec<f32>, Vec<f32>)> = None;
    // its turn to be learned: one song at a time, in the order they ended (learn::Queued), shown meanwhile as
    // waiting to learn; held until it's scored and its bars are made
    let my_turn = |name: &str| if let Some(q) = &queued {
        let ahead = q.ahead();
        if ahead > 0 {
            let t = Instant::now();
            log(&format!("{name}: waiting to learn ({ahead} song{} ahead)", if ahead == 1 { "" } else { "s" }));
            q.wait();
            log(&format!("{name}: learning, after waiting {:.0} s", t.elapsed().as_secs_f64()));
        }
    };
    let result = (|| -> Result<(), String> {
        let reference = if take.mode == Mode::Sing {
            // the singer separated during this play of a song with no syllable bars: make them now, if this play
            // covered most of the song and there are lyrics (otherwise the next play tries again)
            if let Some(l) = learner {
                let most = take.seconds() >= 0.7 * take.track.duration;
                my_turn(&take.track.name);
                match l.finish() {
                    // another play of the song may have made its bars while this one waited its turn
                    Ok(_) if !store.needs_bars(&take.track.id, take.track.duration) => {
                        log(&format!("{}: no syllable bars from this play (an earlier play has made them)", take.track.name));
                    }
                    Ok(l) if most && take.lyrics.len() >= 4 => {
                        log(&format!("{}: making its syllable bars from this play", take.track.name));
                        crate::devtest::keep_audio(&take.track, take.song_offset.unwrap_or(0.0), &take.ref16, &l.vocal16, &take.lyrics, &take.lyrics,
                                                   take.music_kept.as_deref().map(|m| (m, take.rate as u32)));
                        if !quitting() && !is_dropped(&take.track.id) && !forgot(&take.track.id) {
                            make_syllables(store, &take, &one_place(&take, &l), None);
                        }
                    }
                    Ok(_) => log(&format!("{}: no syllable bars yet (needs most of the song played, with lyrics)", take.track.name)),
                    Err(e) => log(&format!("{}: separating for syllable bars failed: {e}", take.track.name)),
                }
            }
            Reference::Learned(take.reference_line())
        } else {
            let waited = Instant::now();
            let l = learner.ok_or("this song can't be learned: the separation model is missing")?;
            my_turn(&take.track.name);
            let l = l.finish()?;
            log(&format!("{}: learner finished ({:.0} s of singer, {:.0} s after the take ended; {} other learner{} running)", take.track.name,
                         l.vocal16.len() as f64 / 16_000.0, waited.elapsed().as_secs_f64(), crate::learn::running(),
                         if crate::learn::running() == 1 { "" } else { "s" }));
            let given = if take.lyrics_given.is_empty() { take.lyrics.clone() } else { take.lyrics_given.clone() };
            // (not when they were lined up by the words heard during the play: this fit, by when someone sings,
            // was 2-4 s wrong on 4 of 10 test songs whose words were right)
            let placed = one_place(&take, &l);
            if let Some(a) = (take.lyrics == given).then(|| take.aligned_lyrics(&l.vocal16)).flatten() {
                log(&format!("{}: lyrics lined up with the singer (moved {:+.2} s)", take.track.name, a.shift));
                take.lyrics = a.lyrics;
            }
            crate::devtest::keep_audio(&take.track, take.song_offset.unwrap_or(0.0), &take.ref16, &l.vocal16, &given, &take.lyrics,
                                       take.music_kept.as_deref().map(|m| (m, take.rate as u32)));
            learnt_kept = Some(placed);
            Reference::Separated { vocal16: l.vocal16, midi: l.midi, periodicity: l.periodicity }
        };
        if is_dropped(&take.track.id) {
            return Err("stopped from the Library".into());
        }
        if learnt_kept.is_some() {
            crate::learn::yield_to_singing(None);
        }
        crate::progress::stage(crate::progress::Stage::Scoring);
        // your pitch as tracked live (smoothed as a full re-track would be: the scores match, see simsession)
        let usr = take.usr_track();
        // your voice per semitone, octave as sung, kept with the take for your range (range.rs)
        let voice = crate::range::Voice::from_frames(&usr.0, &usr.1,
            &(0..usr.0.len()).map(|i| crate::dsp::frame_db(&take.mic16, i)).collect::<Vec<_>>());
        let (fingerprint, audible) = if take.mode == Mode::Learn {
            let (f, a) = take.fingerprint();
            (Some(f), Some(a))
        } else {
            (None, None)
        };
        let recorded = store::now_iso();
        let out = analysis::analyze(analysis::Input {
            track: take.track.clone(), recorded: recorded.clone(), mode: take.mode.name(), blind: take.blind,
            lyrics: take.lyrics.clone(), song_offset: take.song_offset.unwrap_or(0.0), places: take.stretches(), reference,
            usr: (usr.0, usr.1), mic16: take.mic16.clone(), voiced: None,
            live: json!({"score": live, "lag_ms": take.lag * 10, "cached": take.mode == Mode::Sing, "aligned": take.aligned}),
            fingerprint, audible,
            bars: (take.mode == Mode::Sing && !take.bar_text.is_empty()).then(|| take.bars_in_take()),
            level: take.level,
        });
        // how much of the song this play alone covered (what was learned before aside)
        let this_play = out.learned.as_ref().map(|(_, c, _)| c.iter().filter(|v| **v).count() as f64 * crate::dsp::FRAME_S / take.track.duration.max(1.0));
        if is_dropped(&take.track.id) {
            return Err("stopped from the Library".into());
        }
        if forgot(&take.track.id) && out.learned.is_some() {
            log(&format!("{}: forgotten from the Library during this play: not learned again from it", take.track.name));
        }
        if let Some((line, covered, fp)) = out.learned.filter(|_| !forgot(&take.track.id)) {
            let covered_now = covered.clone();
            // the song's one id is settled again as it's saved, with the save (two plays of one song under two ids,
            // scored at once, each checked before the other was saved and both kept: My Love, My Life, live)
            let (saved, canon) = store.save_as_one(&take.track, line, covered, fp);
            if canon.id != take.track.id {
                log(&format!("{}: kept as {} (played as {})", take.track.name, canon.id, take.track.id));
                take.track = canon;
                if let Some(j) = &job {
                    j.set_track(&take.track);
                }
                if let Some(w) = &watch {
                    w.also(&take.track.id);
                }
            }
            if !saved && store.learning_again(&take.track.id) {
                // (said by the store: too little heard to replace the copy)
            } else if !saved {
                log(&format!("{}: this play couldn't be lined up with what was learned before, so it wasn't added", take.track.name));
            } else {
                // an earlier play's lyric times and bars, merged with this play's by what it heard (else this
                // play's are the song's)
                let had_lyrics = store.kept_lyrics(&take.track.id).is_some();
                if !take.lyrics.is_empty() && !had_lyrics {
                    store.save_lyrics(&take.track.id, &take.lyrics);
                    crate::lyrics::keep_learned(&take.track.id, &take.lyrics);
                }
                if let Some(k) = &learnt_kept {
                    if quitting() {
                        log(&format!("{}: syllable bars left for its next play (quitting)", take.track.name));
                    } else {
                        make_syllables(store, &take, k, had_lyrics.then_some(&covered_now[..]));
                    }
                }
            }
        }
        // the song's Shazam signature, from this play's music (kept if longer than the one it has), so listening
        // mode recognises it on this Mac from then on; only from a play placed exactly in the song
        if let (Some(off), true) = (take.song_offset, take.aligned) {
            let secs = take.ref16.len() as f64 / crate::recognize::SR as f64;
            if secs >= 20.0 && store.load_song(&take.track.id).is_some() && store.signature(&take.track.id).map_or(true, |(_, _, had)| secs > had) {
                // (the music at the take's place now, every stretch where it was heard: Take::music_placed)
                if let Some(sig) = crate::recognize::signature(&take.music_placed()) {
                    store.save_signature(&take.track.id, &sig, off, secs);
                    crate::listen::songs_changed();
                }
            }
        }
        let mut r = out.result;
        // the song's stretches marked "Not the melody" (its learned notes there a backing vocal's or a harmony's): left
        // out of the score, as from every take of it before (store::rescore_take)
        let left_out = take.left_out();
        if !left_out.is_empty() {
            analysis::leave_out(&mut r, &left_out);
            log(&format!("{}: {} stretch{} marked not the melody left out of the score (score {} as first scored)", take.track.name,
                         left_out.len(), if left_out.len() == 1 { "" } else { "es" }, r["scored_first"]["score"]));
        }
        r["voice"] = voice.to_json();
        // the music too quiet to trust for most of the take (the app's own volume turned right down): what the mic
        // heard can't be told from the room, so it isn't scored as singing (and the window said to turn it up)
        let too_quiet = take.quiet_s > 0.5 * take.seconds();
        if too_quiet {
            log(&format!("{}: the music was too quiet to hear for {:.0} s of {:.0} s: not scored", take.track.name, take.quiet_s, take.seconds()));
        }
        let sang = r["coverage"].as_f64().unwrap_or(0.0) >= 0.1 && !too_quiet;
        let song = store.load_song(&take.track.id);
        let ready = song.as_ref().is_some_and(|s| s.ready(take.track.duration));
        let fraction = song.as_ref().map_or(0.0, |s| s.fraction(take.track.duration));
        let mut report = Value::Null;
        if sang {
            let dir = save_take(store, &take, &r).map_err(|e| e.to_string())?;
            report = json!(dir);
        }
        let status = match (take.mode, sang, ready) {
            (_, true, _) => "done",
            (Mode::Learn, false, true) => "learned",
            (Mode::Learn, false, false) => "partly",
            (Mode::Sing, false, _) => "unsung",
        };
        store.played(&take.track.id);
        if take.mode == Mode::Sing {
            if let Some((at, by)) = store.note_slips(&take.track.id, &crate::listen::take_slips(&take.track.id)) {
                log(&format!("{}: jumped {by:+.2} s at {at:.0} s on two plays: its learned copy is off there, relearn suggested", take.track.name));
            }
        }
        store.note_play(&take.track.id, &take.played_as, take.app);   // where it played, for "open in your music app"
        // (how long it took shows how a PC copes: testers' reports on Windows and Linux couldn't say, issue #1)
        log(&format!("scored {}: {} (score {}, learned {:.0}%{}; done {:.0} s after the play ended, a play of {:.0} s)", take.track.name, status,
                     r["score"], 100.0 * fraction, this_play.map_or(String::new(), |p| format!("; this play covered {:.0}%", 100.0 * p)),
                     ended.elapsed().as_secs_f64(), take.seconds()));
        outcome = json!({"status": status, "score": if sang { r["score"].clone() } else { Value::Null }, "take_id": report,
            "notes_hit": r["notes_hit"], "notes_total": r["notes_total"], "coverage": r["coverage"], "latency_ms": r["latency_ms"],
            "learned": (fraction * 1000.0).round() / 1000.0, "this_play_covered": this_play.map(|p| (p * 1000.0).round() / 1000.0),
            "too_quiet": too_quiet, "kept_as": take.track.id});
        update(json!({"status": status, "score": if sang { r["score"].clone() } else { Value::Null }, "report": report,
            "notes_hit": r["notes_hit"], "notes_total": r["notes_total"], "ready": ready, "learned": fraction}));
        Ok(())
    })();
    drop(watch);
    if is_dropped(&take.track.id) {
        // (stopped after it was saved, or while it saved: forgotten again, so nothing of it stays)
        for id in [played_id.as_str(), take.track.id.as_str()] {
            if let Err(e) = store.forget(id) {
                log(&format!("{id}: {e}"));
            }
        }
        log(&format!("{}: stopped learning it; nothing kept", take.track.name));
        outcome = json!({"status": "discarded"});
        update(json!({"status": "discarded"}));
    } else if let Err(e) = result {
        log(&format!("scoring {} failed: {e}", take.track.name));
        outcome = json!({"status": "failed", "error": e});
        update(json!({"status": "failed", "error": e}));
    }
    crate::records::take_track(version, &take.track.id);
    crate::records::take_finish(version, outcome, Some(store));
    // a song learned (or its bars made): nothing it loaded stays behind while you sing the next
    if had_learner {
        release_learning_models(false);
    }
    // the next song that ended is learned now
    drop(queued);
}

/// takes/<time>-<song>/: your voice (mic.m4a: AAC, takeaudio.rs), the full result and the take's details.
fn save_take(store: &Store, take: &Take, result: &Value) -> std::io::Result<String> {
    let slug: String = format!("{}-{}", take.track.artist, take.track.name).chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>()
        .split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
    let name = format!("{}-{}", store::format_time(std::time::SystemTime::now(), "%Y%m%d-%H%M%S"), &slug[..slug.len().min(60)]);
    // its own folder even when another take of the song is saved the same second; each file written atomically,
    // result.json (what History lists) last
    let (dir, name) = store.new_take_dir(&name)?;
    let started = Instant::now();
    if let Err(e) = crate::takeaudio::write_m4a(&dir.join(crate::takeaudio::M4A), &take.mic, take.rate, crate::takeaudio::TAKE_BIT_RATE) {
        // never lose the recording: as WAV, as before (converted later)
        log(&format!("take audio: AAC encoding failed ({e}): kept as WAV"));
        write_wav(&dir.join(crate::takeaudio::WAV), &take.mic, take.rate as u32)?;
    } else {
        log(&format!("take audio: {:.0} s saved as AAC in {:.2} s", take.mic.len() as f64 / take.rate, started.elapsed().as_secs_f64()));
    }
    // the app it played in, for the app you usually sing from
    let mut result = result.clone();
    if let Some(k) = store::app_key(take.app) {
        result["app"] = json!(k);
    }
    store::write_atomic(&dir.join("meta.json"), json!({"track": take.track, "recorded": store::now_iso(),
        "song_offset": take.song_offset, "mode": take.mode.name(), "blind": take.blind, "lyrics": take.lyrics,
        // each part of the take's place in the song (between skips), for leaving a stretch of the song out later
        "places": take.stretches().iter().map(|p| [p.0, p.1, p.2]).collect::<Vec<_>>()}).to_string().as_bytes())?;
    store::write_atomic(&dir.join("result.json"), result.to_string().as_bytes())?;
    Ok(name)
}

/// 16-bit mono WAV.
pub fn write_wav(path: &std::path::Path, samples: &[i16], rate: u32) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    store::write_atomic(path, &b)
}

#[cfg(test)]
mod tests {
    #[test]
    fn pausing_listening_stops_once_idles_while_paused_and_resumes_once() {
        use super::{pause_step, Pause};
        assert_eq!(pause_step(false, false), Pause::Listen);
        assert_eq!(pause_step(false, true), Pause::Stop, "the take ends and the capture stops once, as you pause");
        assert_eq!(pause_step(true, true), Pause::Idle, "then nothing, round after round");
        assert_eq!(pause_step(true, false), Pause::Resume);
        // a pause and resume, round by round: one stop and one resume, however long it lasts
        let rounds = [false, true, true, true, false, false];
        let steps: Vec<Pause> = rounds.windows(2).map(|w| pause_step(w[0], w[1])).collect();
        assert_eq!(steps, [Pause::Stop, Pause::Idle, Pause::Idle, Pause::Resume, Pause::Listen]);
    }

    #[test]
    fn a_nap_ends_when_woken() {
        let t = std::time::Instant::now();
        std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_millis(50)); super::wake(); });
        super::nap(std::time::Duration::from_secs(5));
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn a_guessed_length_never_shrinks() {
        // The Kill: a first play heard to 229.7 s, a second paused at 197.2 s
        assert_eq!(super::guessed_length(None, 197.2, Some(229.7)), 229.7);
        assert_eq!(super::guessed_length(None, 240.0, Some(229.7)), 240.0);
        assert_eq!(super::guessed_length(None, 197.2, None), 197.2);
        assert_eq!(super::guessed_length(Some(231.0), 197.2, Some(229.7)), 231.0);
    }

    use super::*;

    #[test]
    fn a_song_forgotten_during_a_play_isnt_learned_back_from_it() {
        // (TAKE_VERSION is the app's one counter, which other tests move on: only ever forward)
        let now = TAKE_VERSION.load(std::sync::atomic::Ordering::Relaxed);
        forgotten("spotify:track:forgot");
        assert!(forgotten_during(&["spotify:track:forgot"], now), "the play under way");
        assert!(forgotten_during(&["spotify:track:x", "spotify:track:forgot"], now.saturating_sub(5)), "by either id");
        assert!(!forgotten_during(&["spotify:track:forgot"], u64::MAX), "a play after it: learns it afresh");
        assert!(!forgotten_during(&["spotify:track:other"], now));
    }

    #[test]
    fn a_song_that_played_to_its_end_and_stopped_is_over_at_once() {
        let t = |id: &str| source::Track { id: id.into(), name: "Hello".into(), duration: 240.0, ..Default::default() };
        let paused = |id: &str, position: f64| source::State { playing: false, track: t(id), position, source: Source::Spotify };
        // Spotify, nothing queued: back at 0:00, paused
        assert!(came_to_end(&t("a"), &paused("a", 0.0), 239.6));
        // it moved on to the next track, then stopped
        assert!(came_to_end(&t("a"), &paused("b", 0.0), 238.0));
        // a pause mid-song, or a restart while paused mid-song: a pause, not the end
        assert!(!came_to_end(&t("a"), &paused("a", 120.0), 120.0));
        assert!(!came_to_end(&t("a"), &paused("a", 0.0), 120.0));
        assert!(!came_to_end(&t("a"), &paused("b", 0.0), 120.0));
        // paused near the end, still there: a pause
        assert!(!came_to_end(&t("a"), &paused("a", 238.0), 238.0));
        // length unknown: never guessed
        let mut live = t("a");
        live.duration = 0.0;
        assert!(!came_to_end(&live, &paused("a", 0.0), 3600.0));
    }

    /// Listening back places the song as a take does: from the app's position once it has been told where to play,
    /// on with the audio, nowhere while paused, and afresh when the song moves.
    #[test]
    fn listening_back_places_the_song_as_a_take_does() {
        let store = Store::new(std::env::temp_dir().join(format!("onkey-along-test-{}", std::process::id())));
        let shared = Arc::new(Mutex::new(along("a")));
        let track = source::Track { id: "a".into(), name: "A".into(), artist: String::new(), album: String::new(), duration: 300.0, art: None, link: None };
        let state = |playing, position| (source::State { playing, track: track.clone(), position, source: Source::Music }, Instant::now());
        let half = vec![0f32; 24_000 * CHANNELS];
        let mut a = None;
        let place = |sh: &Arc<Mutex<Shared>>| sh.lock().unwrap().along_place.map(|p| p.0);
        // not yet told where to play: what the app says is the old place
        follow_along(&mut a, &shared, Some(&state(true, 30.0)), false, &store, 48_000.0, &half);
        assert!(a.is_none() && place(&shared).is_none());
        shared.lock().unwrap().along_from = Some(Instant::now());
        follow_along(&mut a, &shared, Some(&state(true, 30.0)), false, &store, 48_000.0, &half);
        assert!(place(&shared).is_some_and(|p| (p - 30.5).abs() < 1e-6), "{:?}", place(&shared));
        follow_along(&mut a, &shared, Some(&state(true, 30.5)), false, &store, 48_000.0, &half);
        assert!(place(&shared).is_some_and(|p| (p - 31.0).abs() < 1e-6), "on with the audio: {:?}", place(&shared));
        follow_along(&mut a, &shared, Some(&state(false, 31.0)), false, &store, 48_000.0, &[]);
        assert!(place(&shared).is_none(), "paused: not placed");
        // dragged ahead in the app: placed afresh there
        follow_along(&mut a, &shared, Some(&state(true, 100.0)), false, &store, 48_000.0, &half);
        assert!(place(&shared).is_some_and(|p| (p - 100.5).abs() < 1e-6), "{:?}", place(&shared));
        // another track: over
        shared.lock().unwrap().play_along = None;
        follow_along(&mut a, &shared, Some(&state(true, 101.0)), false, &store, 48_000.0, &half);
        assert!(a.is_none() && place(&shared).is_none());
    }

    fn along(id: &str) -> Shared {
        Shared { blind: false, echo_cancel: true, skip_to: None, finish: false, play_along: Some(id.into()), along_heard: false,
                 along_stopping: false, player_now: None, along_from: None, along_place: None, take_place: None, takes: Vec::new() }
    }

    #[test]
    fn listening_back_ends_when_the_song_moves_on() {
        let mut sh = along("a");
        // the song before still plays a moment: not the end
        along_follows(&mut sh, Some(("b", true)));
        assert_eq!(sh.play_along.as_deref(), Some("a"));
        along_follows(&mut sh, Some(("a", true)));
        along_follows(&mut sh, Some(("a", false)));
        assert_eq!(sh.play_along.as_deref(), Some("a"), "a pause while listening back doesn't end it");
        along_follows(&mut sh, Some(("b", true)));
        assert_eq!(sh.play_along, None);
    }

    #[test]
    fn after_stop_the_song_playing_on_is_no_take() {
        // live, 22 Oct: Stop, and a take of The A Team started before Spotify had paused
        let mut sh = along("a");
        along_follows(&mut sh, Some(("a", true)));
        sh.along_stopping = true;
        along_follows(&mut sh, Some(("a", true)));
        assert_eq!(sh.play_along.as_deref(), Some("a"), "still playing: still listening back");
        along_follows(&mut sh, Some(("a", false)));
        assert_eq!(sh.play_along, None);
        assert!(!sh.along_stopping);
        // stopped before the app played it, or with nothing playing: over at once
        let mut sh = along("a");
        sh.along_stopping = true;
        along_follows(&mut sh, Some(("b", true)));
        assert_eq!(sh.play_along, None);
        let mut sh = along("a");
        sh.along_stopping = true;
        along_follows(&mut sh, None);
        assert_eq!(sh.play_along, None);
    }

    #[test]
    fn a_second_quit_joins_the_first_ones_wait() {
        let t = Instant::now();
        let s = Duration::from_secs;
        assert_eq!(quit_until(None, t, s(150)), t + s(150));
        // told to stop 100 s into a 150 s quit: waits the 25 s it may, inside the 50 s left
        assert_eq!(quit_until(Some(t + s(150)), t + s(100), s(25)), t + s(125));
        // ... 140 s in: only the 10 s left
        assert_eq!(quit_until(Some(t + s(150)), t + s(140), s(25)), t + s(150));
        // past the first wait: done at once
        assert!(quit_until(Some(t + s(150)), t + s(160), s(25)) <= t + s(160));
    }

    #[test]
    fn a_takes_lyrics_stop_being_looked_for_when_it_ends() {
        use std::sync::atomic::Ordering::Relaxed;
        TAKE_VERSION.store(7, Relaxed);
        assert!(take_on(7) && !take_on(6));
        TAKE_ENDED.store(7, Relaxed);       // the last take ended, none after it
        assert!(!take_on(7));
        TAKE_VERSION.store(8, Relaxed);
        assert!(take_on(8));
        // lyrics that came after their take ended aren't kept, and keeping one drops any left from before
        let list: Mutex<Vec<(u64, u8)>> = Mutex::new(vec![(6, 0)]);
        keep_for_take(&list, 7, 1);
        assert_eq!(*list.lock().unwrap(), vec![]);
        keep_for_take(&list, 8, 2);
        assert_eq!(*list.lock().unwrap(), vec![(8, 2)]);
    }

    #[test]
    fn log_is_capped_with_one_older_file() {
        let dir = std::env::temp_dir().join(format!("onkey-logtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("onkey.log");
        let line = "x".repeat(99);
        for _ in 0..1000 {
            append_log(&path, &line, 10_000);       // 100 KB written through a 10 KB cap
        }
        let size = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        assert!(size(&path) <= 10_000, "{}", size(&path));
        assert!(size(&dir.join("onkey.log.1")) >= 10_000);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
