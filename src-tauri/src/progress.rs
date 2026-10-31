//! How far each song being learned has got, for the window's progress bars and for keeping a song from being
//! started again before it's learned. Learning reports real work done (seconds of music separated out of the
//! take's, 20 s pieces of singer word-timed, lyric lines made into bars, pieces of singer transcribed into
//! notes); the stages' shares of the whole (`PLAN`) are what they took in the app's log.
//!
//! Cheap for the learner: reporting only writes a few numbers under a lock nobody holds for long. One small
//! thread sends the window the state of every learn (event "learn_progress", the whole list) when something
//! changed, at most `EVERY` apart, and sleeps otherwise.
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// At most this often the window is told (4 a second).
const EVERY: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// the song is still playing (learned as it plays, or kept to be learned after it ends)
    Playing,
    /// waiting its turn for the separation models (another song is being learned)
    Waiting,
    /// separating the singer from the music
    Separating,
    /// tracking the singer's pitch (on each separated piece, as it's separated)
    Pitch,
    /// scoring the take and saving the learned melody
    Scoring,
    /// timing the words: where each lyric line is sung
    Words,
    /// one bar per sung syllable
    Bars,
    /// each syllable's note (GAME)
    Notes,
    /// keeping it all with the song
    Saving,
}

impl Stage {
    pub fn word(self) -> &'static str {
        match self {
            Stage::Playing => "playing", Stage::Waiting => "waiting", Stage::Separating => "separating", Stage::Pitch => "pitch",
            Stage::Scoring => "scoring", Stage::Words => "words", Stage::Bars => "bars", Stage::Notes => "notes", Stage::Saving => "saving",
        }
    }

    /// Its share of the learn after the song ends, as (from, to) of the whole. Separating and the pitch go piece by
    /// piece together (waiting for a turn too), so they share one span. From the log of a 32 GB M-series Mac: a
    /// 4-minute song's separation and pitch take about as long as its word timing, bars and notes together
    /// (40-100 s), of which timing the words is about a third.
    fn span(self) -> (f64, f64) {
        match self {
            Stage::Playing | Stage::Waiting | Stage::Separating | Stage::Pitch => (0.0, 0.45),
            Stage::Scoring => (0.45, 0.5),
            Stage::Words => (0.5, 0.67),
            Stage::Bars => (0.67, 0.82),
            Stage::Notes => (0.82, 0.97),
            Stage::Saving => (0.97, 1.0),
        }
    }
}

/// The whole learn's fraction done, at `within` (0..1) of `stage`.
pub fn overall(stage: Stage, within: f64) -> f64 {
    let (a, b) = stage.span();
    a + (b - a) * within.clamp(0.0, 1.0)
}

struct Entry {
    key: u64,
    track_id: String,
    name: String,
    artist: String,
    stage: Stage,
    within: f64,
    /// what the window was told: never goes back (a piece's estimate may)
    shown: f64,
    /// the take is still playing
    playing: bool,
    /// waiting at a safe point while someone sings (learn::yield_to_singing)
    paused: bool,
    /// the song ended and waits to be learned while a song before it is (learn::Queued)
    queued: bool,
    /// since the song ended: seconds in each stage, by its word (and "queued", "paused"), and when this lap began
    took: Vec<(&'static str, f64)>,
    lap: Option<Instant>,
}

impl Entry {
    /// The time since the last lap, to what it's doing now (before that changes).
    fn lap(&mut self) {
        let Some(t) = self.lap else { return };
        self.lap = Some(Instant::now());
        let doing = if self.queued { "queued" } else if self.paused { "paused" } else { self.stage.word() };
        let s = t.elapsed().as_secs_f64();
        match self.took.iter_mut().find(|(w, _)| *w == doing) {
            Some((_, t)) => *t += s,
            None => self.took.push((doing, s)),
        }
    }
}

/// "separating 41 s, pitch 12 s, words 20 s, …; 93 s in all": how long a learn took after its song ended, in the
/// order it got to each stage ("queued": waiting for a song before it; "paused": waiting while you sang).
fn times(took: &[(&str, f64)]) -> String {
    let all: f64 = took.iter().map(|t| t.1).sum();
    format!("{}; {all:.0} s in all", took.iter().map(|(w, s)| format!("{w} {s:.0} s")).collect::<Vec<_>>().join(", "))
}

static JOBS: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);
static DIRTY: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
type Sink = Box<dyn Fn(Value) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();
/// someone is singing: learning waits (learn::set_singing); one waiting its turn is said to wait for that too
static SINGING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_singing(on: bool) {
    SINGING.store(on, Ordering::SeqCst);
    changed();
}

fn changed() {
    let (m, cv) = &DIRTY;
    *m.lock().unwrap_or_else(|e| e.into_inner()) = true;
    cv.notify_one();
}

/// Where the window's events go (the engine's emit): starts the thread that sends them.
pub fn set_sink(f: impl Fn(Value) + Send + Sync + 'static) {
    if SINK.set(Box::new(f)).is_err() {
        return;
    }
    std::thread::Builder::new().name("onkey-progress".into()).spawn(|| loop {
        {
            let (m, cv) = &DIRTY;
            let mut d = cv.wait_while(m.lock().unwrap_or_else(|e| e.into_inner()), |d| !*d).unwrap_or_else(|e| e.into_inner());
            *d = false;
        }
        if let Some(sink) = SINK.get() {
            sink(snapshot());
        }
        std::thread::sleep(EVERY);
    }).expect("progress thread");
}

/// Every learn going on: {"type": "learn_progress", "songs": [{track_id, name, artist, stage, fraction}]}
/// (fraction null while the song still plays; stage "queued": waiting to learn while a song before it is learned,
/// its fraction not moving and not to be shown as if it were; "paused": waiting while you sing).
pub fn snapshot() -> Value {
    let jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    let singing = SINGING.load(Ordering::SeqCst);
    json!({"type": "learn_progress", "songs": jobs.iter().map(|e| json!({
        "track_id": e.track_id, "name": e.name, "artist": e.artist,
        "stage": if e.playing { "playing" } else if e.queued { "queued" } else if e.paused || (singing && e.stage == Stage::Waiting) { "paused" } else { e.stage.word() },
        "fraction": if e.playing { Value::Null } else { json!((e.shown * 1000.0).round() / 1000.0) },
    })).collect::<Vec<_>>()})
}

fn with(key: u64, f: impl FnOnce(&mut Entry)) {
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = jobs.iter_mut().find(|e| e.key == key) {
        f(e);
        if !e.playing {
            e.shown = e.shown.max(overall(e.stage, e.within));
        }
        drop(jobs);
        changed();
    }
}

/// Stop showing the learns of `track_id` (stopped from the Library: discard.rs); their work stops on its own.
pub fn drop_learns(track_id: &str) {
    JOBS.lock().unwrap_or_else(|e| e.into_inner()).retain(|e| e.track_id != track_id);
    changed();
}

struct Key(u64);

impl Drop for Key {
    /// The last handle on a learn gone: it's over (learned, failed or dropped). On Windows its stage times are
    /// logged, with where the models ran (gpu.rs): how long learning takes on a tester's PC, and on what.
    fn drop(&mut self) {
        let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
        let done = jobs.iter().position(|e| e.key == self.0).map(|i| jobs.remove(i));
        drop(jobs);
        changed();
        if let Some(mut e) = done.filter(|e| crate::platform::diag::LEARN_TIMES && e.lap.is_some()) {
            e.lap();
            crate::engine::log(&format!("learn times: {}: {} ({})", e.name, times(&e.took), crate::gpu::places()));
        }
    }
}

/// One song's learn, from its first play's start until it's saved: shown while any handle is alive.
#[derive(Clone)]
pub struct Job(Arc<Key>);

impl Job {
    pub fn new(track: &crate::source::Track) -> Job {
        let key = NEXT.fetch_add(1, Ordering::Relaxed);
        JOBS.lock().unwrap_or_else(|e| e.into_inner()).push(Entry {
            key, track_id: track.id.clone(), name: track.name.clone(), artist: track.artist.clone(),
            stage: Stage::Playing, within: 0.0, shown: 0.0, playing: true, paused: false, queued: false, took: Vec::new(), lap: None });
        changed();
        Job(Arc::new(Key(key)))
    }

    /// The song turned out to be another (renamed, or kept under its one id).
    pub fn set_track(&self, track: &crate::source::Track) {
        with(self.0 .0, |e| {
            e.track_id = track.id.clone();
            e.name = track.name.clone();
            e.artist = track.artist.clone();
        });
    }

    /// The song's play is over: what's left of learning it shows from here.
    pub fn ended(&self) {
        with(self.0 .0, |e| {
            crate::records::note("stage", json!({"track": e.track_id, "stage": "song ended"}));
            e.playing = false;
            if e.stage == Stage::Playing {
                e.stage = Stage::Waiting;
            }
            e.lap.get_or_insert_with(Instant::now);
        });
    }

    pub fn stage(&self, s: Stage) {
        with(self.0 .0, |e| {
            if e.stage != s {
                crate::records::note("stage", json!({"track": e.track_id, "stage": s.word()}));     // its record (records.rs)
            }
            if e.stage.span() != s.span() {
                e.within = 0.0;
            }
            e.lap();
            e.stage = s;
        });
    }

    /// The song waits to be learned while a song before it is (true), or its turn has come (false).
    pub fn queued(&self, on: bool) {
        with(self.0 .0, |e| {
            if on && !e.queued {
                crate::records::note("stage", json!({"track": e.track_id, "stage": "queued"}));
            }
            e.lap();
            e.queued = on;
        });
    }

    pub fn within(&self, f: f64) {
        with(self.0 .0, |e| e.within = f);
    }

    /// Reports from this thread (`stage`, `advance`) go to this learn until the guard is dropped.
    pub fn enter(&self) -> Entered {
        let before = CURRENT.with(|c| c.replace(Some((self.clone(), 0.0, 1.0))));
        Entered(before)
    }
}

thread_local! {
    /// the learn this thread is working for, and the part of its stage being done now (from, to)
    static CURRENT: RefCell<Option<(Job, f64, f64)>> = const { RefCell::new(None) };
}

pub struct Entered(Option<(Job, f64, f64)>);

impl Drop for Entered {
    fn drop(&mut self) {
        let before = self.0.take();
        CURRENT.with(|c| *c.borrow_mut() = before);
    }
}

/// This thread's learn is now at stage `s` (nothing when it isn't working for one: tests, tools).
pub fn stage(s: Stage) {
    CURRENT.with(|c| if let Some((j, ..)) = c.borrow().as_ref() { j.stage(s) });
}

/// This thread's learn is waiting while someone sings (true), or carrying on (false).
pub fn paused(on: bool) {
    CURRENT.with(|c| if let Some((j, ..)) = c.borrow().as_ref() { with(j.0 .0, |e| {
        e.lap();
        e.paused = on;
    }) });
}

/// `done` of `of` units of the work in hand (pieces of audio, lines): within the part set by `part`, else the stage.
pub fn advance(done: usize, of: usize) {
    CURRENT.with(|c| if let Some((j, a, b)) = c.borrow().as_ref() {
        j.within(a + (b - a) * done as f64 / of.max(1) as f64);
    });
}

/// The work until the guard is dropped is the part (from, to) of the stage: `advance` inside it moves within it.
pub fn part(from: f64, to: f64) -> Part {
    CURRENT.with(|c| {
        let mut c = c.borrow_mut();
        match c.as_mut() {
            Some((j, a, b)) => {
                let before = (*a, *b);
                (*a, *b) = (from, to);
                j.within(from);
                Part(Some(before))
            }
            None => Part(None),
        }
    })
}

pub struct Part(Option<(f64, f64)>);

impl Drop for Part {
    fn drop(&mut self) {
        if let Some(before) = self.0 {
            CURRENT.with(|c| if let Some((_, a, b)) = c.borrow_mut().as_mut() { (*a, *b) = before });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> crate::source::Track {
        crate::source::Track { id: id.into(), name: format!("Song {id}"), artist: "A".into(), album: String::new(),
                               duration: 200.0, ..Default::default() }
    }

    fn shown(id: &str) -> Option<(String, Option<f64>)> {
        snapshot()["songs"].as_array().unwrap().iter().find(|s| s["track_id"] == id)
            .map(|s| (s["stage"].as_str().unwrap().to_string(), s["fraction"].as_f64()))
    }

    #[test]
    fn a_learn_times_each_stage_from_when_its_song_ended() {
        let mut e = Entry { key: 0, track_id: "t".into(), name: "T".into(), artist: String::new(), stage: Stage::Playing, within: 0.0,
                            shown: 0.0, playing: true, paused: false, queued: false, took: Vec::new(), lap: None };
        e.lap();
        assert!(e.took.is_empty() && e.lap.is_none(), "nothing timed while the song plays");
        let ago = |s: u64| Some(Instant::now() - Duration::from_secs(s));
        e.stage = Stage::Waiting;
        e.lap = ago(3);
        e.lap();
        e.stage = Stage::Separating;
        e.lap = ago(40);
        e.lap();
        e.paused = true;
        e.lap = ago(5);
        e.lap();
        e.paused = false;
        e.lap = ago(2);
        e.lap();
        assert_eq!(times(&e.took), "waiting 3 s, separating 42 s, paused 5 s; 50 s in all");
    }

    #[test]
    fn the_stages_fill_the_bar_once_in_order() {
        let order = [Stage::Separating, Stage::Scoring, Stage::Words, Stage::Bars, Stage::Notes, Stage::Saving];
        assert_eq!(overall(order[0], 0.0), 0.0);
        assert_eq!(overall(Stage::Saving, 1.0), 1.0);
        for w in order.windows(2) {
            assert!((overall(w[0], 1.0) - overall(w[1], 0.0)).abs() < 1e-12, "{w:?}");
            assert!(overall(w[0], 0.0) < overall(w[0], 1.0));
        }
        assert_eq!(overall(Stage::Pitch, 0.5), overall(Stage::Separating, 0.5));
        assert_eq!(overall(Stage::Words, 7.0), overall(Stage::Words, 1.0));
    }

    #[test]
    fn a_learn_reports_the_work_done_and_goes_when_its_handles_do() {
        // (not while learn's test sings: a learn waiting then shows as paused)
        let _quiet = crate::learn::tests::SINGING_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let job = Job::new(&track("p-1"));
        assert_eq!(shown("p-1"), Some(("playing".into(), None)));
        job.ended();
        assert_eq!(shown("p-1"), Some(("waiting".into(), Some(0.0))));
        // another song is learned first: this one says it waits to be learned, until its turn
        job.queued(true);
        assert_eq!(shown("p-1").unwrap().0, "queued");
        job.queued(false);
        assert_eq!(shown("p-1").unwrap().0, "waiting");
        let t = std::thread::spawn({
            let job = job.clone();
            move || {
                let _in = job.enter();
                stage(Stage::Separating);
                // the second of four pieces of music, a quarter of it separated
                let _p = part(0.25, 0.5);
                advance(1, 4);
                shown("p-1").unwrap()
            }
        }).join().unwrap();
        assert_eq!(t.0, "separating");
        assert!((t.1.unwrap() - overall(Stage::Separating, 0.25 + 0.25 / 4.0)).abs() < 1e-3, "{t:?}");
        // a stage's estimate going back (more music arrived) never moves the bar back
        job.within(0.1);
        assert!(shown("p-1").unwrap().1.unwrap() >= t.1.unwrap());
        job.stage(Stage::Words);
        assert_eq!(shown("p-1").unwrap().1, Some(overall(Stage::Words, 0.0)));
        let t = std::thread::spawn({
            let job = job.clone();
            move || {
                let _in = job.enter();
                paused(true);
                let s = shown("p-1").unwrap().0;
                paused(false);
                (s, shown("p-1").unwrap().0)
            }
        }).join().unwrap();
        assert_eq!(t, ("paused".to_string(), "words".to_string()));
        job.set_track(&track("p-2"));
        assert!(shown("p-1").is_none() && shown("p-2").is_some());
        drop(job);
        assert!(shown("p-2").is_none());
        // reports with no learn on the thread go nowhere
        stage(Stage::Bars);
        advance(1, 2);
    }
}
