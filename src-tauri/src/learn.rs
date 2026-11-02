//! Learning a song while it plays: the music is separated in 20 s chunks (with 2 s of context either side)
//! on a background thread as it arrives, and each chunk's lead vocal is pitch-tracked straight away, so
//! little is left to do when the song ends. Which separator and which pitch tracker do the work is
//! pluggable (they're chosen by measurement; see models/README.md).
//! On a small Mac (lowmem.rs) the music is only kept, 16-bit, while the song plays, and separated after it
//! ends, with the one set of models let go after: a song's learning never sits in memory beside its singing.
//! The app does that on every Mac now (its first play smooth too; but a first play separates its first minute or so
//! as it plays, 5 s at a time, until its lyrics are lined up with the singer: chunk_s), and learning waits while you sing a learned
//! song with the window showing (`set_singing`): learning the last song on every core made the lane and the
//! live pitch line stutter on an M2 Air. It runs at a low priority (utility QoS) in any case.
//! One song is learned at a time, on every Mac (`Queued`): songs that ended wait their turn, in the order they
//! ended, and a first play's separating as it plays waits while another song is learned (`learning_now`).
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

use crate::pitch::{HOP, SR};

const CHUNK_S: f64 = 20.0;
const PAD_S: f64 = 2.0;

/// A first play lines LRCLIB's lyrics up with the singer as it's separated (syllables::line_up_live), and LRCLIB's
/// times can be seconds off. So while a learner is `quick` (Learner::quick: a first play, until its line-up has
/// settled), and for at most QUICK_UNTIL_S of the take, it separates the music as it arrives, QUICK_CHUNK_S at a
/// time, even a learner that otherwise keeps the music to learn after the song (all of them since [sing-first]);
/// once it isn't, the rest waits for the song's end as before. (In 20 s chunks the first line-up came 26-70 s in,
/// after the first 5-20 lines had come up at LRCLIB's times. A 5 s chunk separates 9 s with its context: about
/// twice the work of 20 s chunks, for the first minute or so of a first play only.)
const QUICK_CHUNK_S: f64 = 5.0;
const QUICK_UNTIL_S: f64 = 90.0;
/// While another song is learned (`learning_now`) a first play doesn't separate as it plays; once that's done it
/// carries on (catching up, 5 s at a time) only while the music heard is still within its first QUICK_UNTIL_S
/// (and the last 5 s piece's music after it): later, it's all learned after the song, and its lyrics are lined up
/// with the singer then (engine.rs score_take, syllable_bars).
const QUICK_HEARD_S: f64 = QUICK_UNTIL_S + QUICK_CHUNK_S + PAD_S;

/// Seconds of music in the chunk that starts `at` s into the take.
pub fn chunk_s(at: f64, quick: bool) -> f64 {
    if quick && at < QUICK_UNTIL_S - 0.01 { QUICK_CHUNK_S } else { CHUNK_S }
}

/// Separates the lead vocal from stereo music.
pub trait Separator: Send {
    /// `stereo`: interleaved left/right at `rate`. Returns the lead vocal, mono, at 16 kHz, same duration.
    fn separate(&mut self, stereo: &[f32], rate: f64) -> Result<Vec<f32>, String>;
}

/// Pitch of a separated vocal: (midi, periodicity) per 10 ms frame, one frame per 160 samples.
pub trait VocalPitch: Send {
    fn track(&mut self, vocal16: &[f32]) -> Result<(Vec<f32>, Vec<f32>), String>;
}

/// A separator and a pitch tracker, as a learner uses them.
pub type Models = (Box<dyn Separator>, Box<dyn VocalPitch>);

pub struct Learnt {
    pub vocal16: Vec<f32>,
    pub midi: Vec<f32>,
    pub periodicity: Vec<f32>,
}

/// Learners alive now: learning a song as it plays, or finishing one that ended (for the log).
static RUNNING: AtomicUsize = AtomicUsize::new(0);

pub fn running() -> usize {
    RUNNING.load(Ordering::SeqCst)
}

/// One learner separates music at a time, on every Mac (one set of models in memory, ever); another waits its
/// turn, chunk by chunk. (Two at once, as 16 GB Macs and up did until [one-learner]: live, playing new songs back
/// to back in Spotify, 120-200% CPU and 2.4-3.8 GB while two songs learned, ~14% and ~1.9 GB with one, and the
/// song that ended took up to 197 s to be separated, its progress stuck at 42% for minutes.)
const SEPARATING_AT_ONCE: usize = 1;
static TURNS: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

fn at_once() -> usize {
    SEPARATING_AT_ONCE
}

/// The songs that ended and are to be learned, in the order they ended: the first is learned (its learner
/// finished, its take scored, its bars made: engine.rs score_take), the others wait for it. Never refused: every
/// song that ended is learned in turn. What a take hears is never held up: only the work after the song waits.
static QUEUE: (Mutex<VecDeque<u64>>, Condvar) = (Mutex::new(VecDeque::new()), Condvar::new());
static TICKETS: AtomicU64 = AtomicU64::new(1);

/// A song that ended, in the queue to be learned (`QUEUE`) from when it ended until this is dropped (learned,
/// failed or dropped).
pub struct Queued {
    n: u64,
    job: Option<crate::progress::Job>,
}

impl Queued {
    /// A song just ended: its place in the queue. `job`, its learn as shown, says it waits to be learned while a
    /// song before it is.
    pub fn join(job: Option<crate::progress::Job>) -> Queued {
        let n = TICKETS.fetch_add(1, Ordering::SeqCst);
        let mut q = QUEUE.0.lock().unwrap_or_else(|e| e.into_inner());
        if !q.is_empty() {
            if let Some(j) = &job {
                j.queued(true);
            }
        }
        q.push_back(n);
        Queued { n, job }
    }

    /// Wait until this song is the one learned (at once if it is, or if OnKey is quitting: then the songs left are
    /// learned together, as fast as they can be, before it goes).
    pub fn wait(&self) {
        let (m, cv) = &QUEUE;
        let mut q = m.lock().unwrap_or_else(|e| e.into_inner());
        while q.front() != Some(&self.n) && !crate::engine::quitting() {
            q = cv.wait_timeout(q, std::time::Duration::from_millis(500)).unwrap_or_else(|e| e.into_inner()).0;
        }
        drop(q);
        if let Some(j) = &self.job {
            j.queued(false);
        }
    }

    /// Songs ahead of this one in the queue.
    pub fn ahead(&self) -> usize {
        QUEUE.0.lock().unwrap_or_else(|e| e.into_inner()).iter().take_while(|n| **n != self.n).count()
    }
}

impl Drop for Queued {
    fn drop(&mut self) {
        QUEUE.0.lock().unwrap_or_else(|e| e.into_inner()).retain(|n| *n != self.n);
        QUEUE.1.notify_all();
    }
}

/// A song that ended is being learned, or waits to be (`Queued`).
pub fn learning_now() -> bool {
    !QUEUE.0.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
}

/// Someone is singing a learned song and watching it (the window showing it, playing): learning waits at its next
/// safe point (between pieces of music, between stages) and carries on when the song pauses or ends, between
/// songs, or when the window is hidden. Nothing is lost: the piece being separated is finished first.
static SINGING: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Learning waits (true) or may go on (false): set by the engine as the take and the window change.
pub fn set_singing(on: bool) {
    let (m, cv) = &SINGING;
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    if *g != on {
        *g = on;
        drop(g);
        crate::progress::set_singing(on);
        cv.notify_all();
    }
}

pub fn singing() -> bool {
    *SINGING.0.lock().unwrap_or_else(|e| e.into_inner())
}

/// A safe point in learning: while someone sings (see `SINGING`), wait, shown as waiting while you sing. Returns at
/// once otherwise, and stops waiting when `cancel` is set or OnKey is quitting.
pub fn yield_to_singing(cancel: Option<&AtomicBool>) {
    let (m, cv) = &SINGING;
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    if !*g {
        return;
    }
    crate::progress::paused(true);
    while *g && !cancel.is_some_and(|c| c.load(Ordering::SeqCst)) && !crate::engine::quitting() {
        g = cv.wait_timeout(g, std::time::Duration::from_millis(500)).unwrap_or_else(|e| e.into_inner()).0;
    }
    drop(g);
    crate::progress::paused(false);
}

/// This thread's work is background work (learning): macOS runs it at utility QoS, after the window, the audio and
/// the live pitch, and on the efficiency cores when the others are busy. Threads it starts (the models' own) are
/// made at that QoS too, so the models are loaded after this.
pub fn lower_priority() {
    crate::platform::sys::utility_thread();
}

/// A turn to separate (see `SEPARATING_AT_ONCE`), given back when dropped.
struct Turn;

impl Turn {
    fn take() -> Turn {
        let (n, cv) = &TURNS;
        let n = n.lock().unwrap();
        if *n >= at_once() {
            crate::progress::stage(crate::progress::Stage::Waiting);     // (shown: waiting its turn)
        }
        let mut n = cv.wait_while(n, |n| *n >= at_once()).unwrap();
        *n += 1;
        Turn
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        *TURNS.0.lock().unwrap() -= 1;
        TURNS.1.notify_one();
    }
}

/// The app's learners share at most `SEPARATING_AT_ONCE` loaded sets of models (each set is hundreds of MB): a
/// learner takes a set for each chunk it separates and gives it back after, loading one only while fewer are
/// loaded (live, every learner loading its own made the Mac swap 13 GB with four songs finishing).
static SETS: (Mutex<(Vec<Models>, usize)>, Condvar) = (Mutex::new((Vec::new(), 0)), Condvar::new());

/// Where a learner's models come from: its own (tests, tools), or the app's shared sets.
enum Workers {
    Own(Models),
    Shared(Option<Box<dyn FnOnce() -> Result<Models, String> + Send>>),
}

impl Workers {
    /// Run `f` with a separator and pitch tracker, waiting for a turn (or a free set) first.
    fn with<R>(&mut self, f: impl FnOnce(&mut dyn Separator, &mut dyn VocalPitch) -> Result<R, String>) -> Result<R, String> {
        match self {
            Workers::Own((sep, pitch)) => {
                let _turn = Turn::take();
                f(&mut **sep, &mut **pitch)
            }
            Workers::Shared(load) => {
                let (lock, cv) = &SETS;
                let mut g = lock.lock().unwrap();
                let mut set = loop {
                    if let Some(s) = g.0.pop() {
                        drop(g);     // not held while separating
                        break s;
                    }
                    if g.1 < at_once() {
                        if let Some(load) = load.take() {
                            g.1 += 1;
                            drop(g);
                            match load() {
                                Ok(s) => break s,
                                Err(e) => {
                                    lock.lock().unwrap().1 -= 1;
                                    cv.notify_one();
                                    return Err(e);
                                }
                            }
                        }
                    }
                    crate::progress::stage(crate::progress::Stage::Waiting);     // (shown: waiting its turn)
                    g = cv.wait(g).unwrap();
                };
                // a panic in the models (a model file not shaped as the code expects, say) mustn't keep the one slot
                // for good: every later song would wait at "Waiting" forever. The set is let go and its slot freed
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut *set.0, &mut *set.1))) {
                    Ok(r) => {
                        lock.lock().unwrap_or_else(|e| e.into_inner()).0.push(set);
                        cv.notify_one();
                        r
                    }
                    Err(_) => {
                        drop(set);
                        lock.lock().unwrap_or_else(|e| e.into_inner()).1 -= 1;
                        cv.notify_one();
                        crate::engine::log("learning: the models failed (a panic): that set let go");
                        Err("learning crashed".into())
                    }
                }
            }
        }
    }

    /// This learner is done: unless the models are kept ready (lowmem::keep_learning_models: Macs with 32 GB or more,
    /// or as chosen in Settings), and no other learner is still at work, the shared sets nobody is using are let go
    /// (hundreds of MB each), so a learned song leaves nothing loaded behind it while you sing. (The next song loads
    /// a set again: seconds, from the cache.)
    fn release(&self) {
        if !matches!(self, Workers::Shared(_)) || crate::lowmem::keep_learning_models() || running() > 1 {
            return;
        }
        release_idle();
    }
}

/// Let go of the shared sets of learning models no learner is using now.
pub fn release_idle() {
    let (lock, cv) = &SETS;
    let mut g = lock.lock().unwrap();
    let idle = std::mem::take(&mut g.0);
    g.1 -= idle.len();
    drop(g);
    if !idle.is_empty() {
        crate::engine::log(&format!("learning models let go ({} set{})", idle.len(), if idle.len() == 1 { "" } else { "s" }));
    }
    drop(idle);
    cv.notify_all();
}

pub struct Learner {
    /// the lead vocal separated so far (16 kHz, from the take's start), for correcting lyrics during the play
    pub vocal_so_far: std::sync::Arc<std::sync::Mutex<Vec<f32>>>,
    tx: Option<Sender<Vec<f32>>>,
    handle: Option<JoinHandle<Result<Learnt, String>>>,
    /// dropped without finishing (its take was too short to keep): stop separating at once, freeing its turn
    cancel: Arc<AtomicBool>,
    /// the song's learn, as shown in the window (see progress.rs): set by whoever started the learner
    job: Arc<Mutex<Option<crate::progress::Job>>>,
    /// separate in short chunks for now (chunk_s)
    quick: Arc<AtomicBool>,
}

impl Learner {
    pub fn start(sep: Box<dyn Separator>, pitch: Box<dyn VocalPitch>, rate: f64) -> Learner {
        Learner::spawn(Workers::Own((sep, pitch)), rate, crate::lowmem::low_memory())
    }

    /// A learner using the app's shared models (see `SETS`), `load` making a set if one is needed: it takes music
    /// at once, so a take never waits for it (what's pushed meanwhile queues up). It keeps the music while the
    /// song plays and learns it after (as on a small Mac, on every Mac now: the song being sung gets the Mac).
    pub fn start_with(load: Box<dyn FnOnce() -> Result<Models, String> + Send>, rate: f64) -> Learner {
        Learner::spawn(Workers::Shared(Some(load)), rate, true)
    }

    /// A learner that only keeps the music while the song plays and separates it all once `finish` is called
    /// (what every learner does on a small Mac; here for tests and tools on any Mac).
    pub fn start_deferred(sep: Box<dyn Separator>, pitch: Box<dyn VocalPitch>, rate: f64) -> Learner {
        Learner::spawn(Workers::Own((sep, pitch)), rate, true)
    }

    fn spawn(mut workers: Workers, rate: f64, defer: bool) -> Learner {
        let (tx, rx) = channel::<Vec<f32>>();
        RUNNING.fetch_add(1, Ordering::SeqCst);
        let vocal_so_far = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let shared = vocal_so_far.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let job: Arc<Mutex<Option<crate::progress::Job>>> = Arc::new(Mutex::new(None));
        let reports = job.clone();
        let quick = Arc::new(AtomicBool::new(false));
        let quick_run = quick.clone();
        let handle = std::thread::Builder::new().name("onkey-learn".into()).spawn(move || {
            lower_priority();
            // (a panic anywhere in it still counts this learner done, so the models can be let go and a quit
            // doesn't wait on it)
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut workers, rate, rx, &shared, &stop, defer, &reports, &quick_run)))
                .unwrap_or_else(|_| Err("learning crashed".into()));
            reports.lock().unwrap_or_else(|e| e.into_inner()).take();
            workers.release();
            RUNNING.fetch_sub(1, Ordering::SeqCst);
            r
        }).expect("learner thread");
        Learner { vocal_so_far, tx: Some(tx), handle: Some(handle), cancel, job, quick }
    }

    /// Separate in short chunks (chunk_s) while `on`: a first play lining its lyrics up with the singer, until
    /// that has settled (and not while another song is learned: `learning_now`).
    pub fn quick(&self, on: bool) {
        self.quick.store(on, Ordering::SeqCst);
    }

    /// Report this learner's work to `job` (the song's learn, shown in the window).
    pub fn report_to(&self, job: crate::progress::Job) {
        *self.job.lock().unwrap() = Some(job);
    }

    /// The song's learn this learner reports to, if any.
    pub fn job(&self) -> Option<crate::progress::Job> {
        self.job.lock().unwrap().clone()
    }

    /// Set, it stops this learner at its next safe point (`finish` then says it was stopped): see discard.rs.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// Stereo interleaved music, as it arrives.
    pub fn push(&self, stereo: Vec<f32>) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(stereo);
        }
    }

    /// `seconds` of silence (time skipped in the song), in pieces of at most 10 s rather than one buffer.
    pub fn push_silence(&self, seconds: f64, rate: f64) {
        let mut left = (seconds.max(0.0) * rate) as usize;
        let piece = (10.0 * rate) as usize;
        while left > 0 {
            let n = left.min(piece.max(1));
            self.push(vec![0.0; n * 2]);
            left -= n;
        }
    }

    /// The song is over: finish the last chunk and return everything learned.
    pub fn finish(mut self) -> Result<Learnt, String> {
        self.tx = None;
        self.handle.take().expect("learner").join().map_err(|_| "learning crashed".to_string())?
    }
}

impl Drop for Learner {
    /// Dropped without `finish`: what it has isn't wanted, so it stops now rather than separating the rest.
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.cancel.store(true, Ordering::SeqCst);
            self.job.lock().unwrap().take();     // no longer shown as learning
        }
    }
}

/// `defer`: nothing is separated until the song ends (the music is kept meanwhile, 16-bit: a 4-minute song is
/// 46 MB rather than 92), except while `quick` (chunk_s) and no other song is being learned (`learning_now`), within
/// the first QUICK_HEARD_S heard; the chunks are the same ones either way, so what's learned is too.
#[allow(clippy::too_many_arguments)]
fn run(workers: &mut Workers, rate: f64, rx: Receiver<Vec<f32>>,
       so_far: &std::sync::Mutex<Vec<f32>>, cancel: &AtomicBool, defer: bool,
       job: &Mutex<Option<crate::progress::Job>>, quick: &AtomicBool) -> Result<Learnt, String> {
    use crate::progress::{part, stage, Stage};
    let quick_now = |done: usize, total: usize| quick.load(Ordering::SeqCst) && chunk_s(done as f64 / rate, true) < CHUNK_S
        && (total as f64) < QUICK_HEARD_S * rate && !learning_now();
    let chunk = |done: usize, quick: bool| (chunk_s(done as f64 / rate, quick) * rate).round() as usize;
    let pad = (PAD_S * rate) as usize;
    let mut buf: Vec<f32> = Vec::new();     // stereo interleaved, starting at sample `base`
    let mut held: Vec<i16> = Vec::new();    // deferred: the music after `buf`, not yet needed
    let mut base = 0usize;
    let mut done = 0usize;                  // samples separated so far
    let mut total = 0usize;
    let mut out = Learnt { vocal16: Vec::new(), midi: Vec::new(), periodicity: Vec::new() };
    let mut finished = false;
    let keep = |b: Vec<f32>, buf: &mut Vec<f32>, held: &mut Vec<i16>| -> usize {
        let n = b.len() / 2;
        if defer {
            held.extend(b.iter().map(|v| (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16));
        } else {
            buf.extend(b);
        }
        n
    };
    loop {
        if !finished {
            match rx.recv() {
                Ok(b) => total += keep(b, &mut buf, &mut held),
                Err(_) => finished = true,
            }
            while let Ok(b) = rx.try_recv() {
                total += keep(b, &mut buf, &mut held);
            }
        }
        loop {
            // (quick: as the engine says now; the song over, the rest is learned in 20 s chunks)
            let quick_here = !finished && quick_now(done, total);
            let c = chunk(done, quick_here);
            if !((finished && done < total) || ((!defer || quick_here) && total >= done + c + pad)) {
                break;
            }
            let end = if finished { (done + c).min(total) } else { done + c };
            let s0 = done.saturating_sub(pad);
            let s1 = (end + pad).min(total);
            // deferred: the music this chunk needs comes out of what was held
            let have = base + buf.len() / 2;
            if s1 > have {
                buf.extend(held.drain(..2 * (s1 - have)).map(|v| v as f32 / 32767.0));
            }
            let seg = &buf[2 * (s0 - base)..2 * (s1 - base)];
            let keep0 = ((done - s0) as f64 / rate * SR as f64).round() as usize;
            let want = ((end as f64 / rate * SR as f64).round() as usize).saturating_sub(out.vocal16.len());
            // how far this learn has got: music separated (and pitch-tracked) of the music heard
            let reporting = job.lock().unwrap().clone();
            let _in = reporting.as_ref().map(|j| j.enter());
            // someone singing: this piece waits until they stop (a safe point: no piece half done)
            yield_to_singing(Some(cancel));
            let (at, sep_to, to) = piece_parts(done, end, total);
            let _whole = part(at, to);
            let (piece, m, p) = workers.with(|sep, pitch| {
                // (a learner dropped while it waited its turn stops here)
                if cancel.load(Ordering::SeqCst) {
                    return Err("stopped: its take wasn't kept".to_string());
                }
                stage(Stage::Separating);
                let sep_part = part(at, sep_to);
                let voc = if s1 - s0 < (rate / 10.0) as usize { vec![0.0; ((s1 - s0) as f64 / rate * SR as f64) as usize] } else { sep.separate(seg, rate)? };
                let piece: Vec<f32> = (0..want).map(|i| voc.get(keep0 + i).copied().unwrap_or(0.0)).collect();
                drop(sep_part);
                // pitch-track this chunk now (frames are 160 samples; a chunk is a whole number of them, as 5 s and 20 s are)
                stage(Stage::Pitch);
                let _pitch_part = part(sep_to, to);
                let (m, p) = pitch.track(&piece)?;
                Ok((piece, m, p))
            })?;
            let nf = piece.len() / HOP;
            out.midi.extend(m.into_iter().chain(std::iter::repeat(f32::NAN)).take(nf));
            out.periodicity.extend(p.into_iter().chain(std::iter::repeat(0.0)).take(nf));
            so_far.lock().unwrap().extend_from_slice(&piece);
            out.vocal16.extend(piece);
            done = end;
            drop(_whole);
            if let Some(j) = &reporting {
                j.within(to);
            }
            // drop audio no later chunk needs
            let keep_from = done.saturating_sub(pad);
            if keep_from > base {
                buf.drain(..2 * (keep_from - base));
                base = keep_from;
            }
        }
        if finished {
            return if cancel.load(Ordering::SeqCst) { Err("stopped: its take wasn't kept".into()) } else { Ok(out) };
        }
    }
}

/// Of a learner's work so far (`total` samples of music), the piece from `done` to `end`: where it starts, where
/// its separation ends and its pitch-tracking (about a seventh of a piece's time) begins, and where it ends.
fn piece_parts(done: usize, end: usize, total: usize) -> (f64, f64, f64) {
    let t = total.max(1) as f64;
    let (a, b) = (done as f64 / t, end as f64 / t);
    (a, a + 0.85 * (b - a), b)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A separator that takes `secs` per chunk and returns silence.
    struct Slow(f64);

    impl Separator for Slow {
        fn separate(&mut self, stereo: &[f32], rate: f64) -> Result<Vec<f32>, String> {
            std::thread::sleep(Duration::from_secs_f64(self.0));
            Ok(vec![0.0; (stereo.len() as f64 / 2.0 / rate * SR as f64) as usize])
        }
    }

    struct NoPitch;

    impl VocalPitch for NoPitch {
        fn track(&mut self, v: &[f32]) -> Result<(Vec<f32>, Vec<f32>), String> {
            Ok((vec![f32::NAN; v.len() / HOP], vec![0.0; v.len() / HOP]))
        }
    }

    fn learner(secs: f64) -> Learner {
        Learner::start(Box::new(Slow(secs)), Box::new(NoPitch), 8000.0)
    }

    #[test]
    fn learners_share_one_set_of_models() {
        static LOADED: AtomicUsize = AtomicUsize::new(0);
        let start = || Learner::start_with(Box::new(|| {
            LOADED.fetch_add(1, Ordering::SeqCst);
            Ok((Box::new(Slow(0.05)) as Box<dyn Separator>, Box::new(NoPitch) as Box<dyn VocalPitch>))
        }), 8000.0);
        // a playlist's worth: five songs learning or finishing at once, one set of models loaded
        let ls: Vec<Learner> = (0..5).map(|_| { let l = start(); l.push(vec![0.1; 2 * 8000 * 50]); l }).collect();
        for l in ls {
            assert_eq!(l.finish().expect("learned").vocal16.len(), 50 * SR);
        }
        assert!(LOADED.load(Ordering::SeqCst) <= SEPARATING_AT_ONCE, "{} sets loaded", LOADED.load(Ordering::SeqCst));
    }

    #[test]
    fn a_panic_in_the_shared_models_frees_their_slot() {
        struct Panics;
        impl Separator for Panics {
            fn separate(&mut self, _: &[f32], _: f64) -> Result<Vec<f32>, String> {
                panic!("a model shaped other than expected");
            }
        }
        let bad = Learner::start_with(Box::new(|| Ok((Box::new(Panics) as Box<dyn Separator>, Box::new(NoPitch) as Box<dyn VocalPitch>))), 8000.0);
        bad.push(vec![0.1; 2 * 8000 * 30]);
        assert!(bad.finish().is_err());
        // the next song gets models (it waited forever for the slot before)
        let good = Learner::start_with(Box::new(|| Ok((Box::new(Slow(0.0)) as Box<dyn Separator>, Box::new(NoPitch) as Box<dyn VocalPitch>))), 8000.0);
        good.push(vec![0.1; 2 * 8000 * 30]);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(good.finish().map(|o| o.vocal16.len())); });
        assert_eq!(rx.recv_timeout(Duration::from_secs(20)).expect("not stuck waiting"), Ok(30 * SR));
    }

    #[test]
    fn learning_waits_while_you_sing_and_carries_on_after() {
        let _one = SINGING_TESTS.lock().unwrap();
        let l = learner(0.01);
        set_singing(true);
        l.push(vec![0.1; 2 * 8000 * 70]);          // 70 s: four pieces, none separated while singing
        std::thread::sleep(Duration::from_millis(400));
        let held = l.vocal_so_far.lock().unwrap().len();
        set_singing(false);
        let out = l.finish().expect("learned");
        assert_eq!(held, 0, "separated while someone sang");
        assert_eq!(out.vocal16.len(), 70 * SR);
    }

    /// (tests that sing don't overlap: SINGING is the app's one flag; progress's tests read it too)
    pub(crate) static SINGING_TESTS: Mutex<()> = Mutex::new(());

    #[test]
    fn a_deferred_learner_separates_nothing_until_the_song_ends() {
        let l = Learner::start_deferred(Box::new(Slow(0.01)), Box::new(NoPitch), 8000.0);
        l.push(vec![0.1; 2 * 8000 * 50]);          // 50 s: two chunks' worth, and the rest
        std::thread::sleep(Duration::from_millis(300));
        assert!(l.vocal_so_far.lock().unwrap().is_empty(), "separated while the song played");
        let out = l.finish().expect("learned");
        assert_eq!(out.vocal16.len(), 50 * SR);
        assert_eq!(out.midi.len(), 50 * SR / HOP);
    }

    #[test]
    fn a_first_play_separates_its_start_as_it_plays_until_told_to_stop() {
        let _one = SINGING_TESTS.lock().unwrap();     // (nobody sings meanwhile)
        let l = Learner::start_deferred(Box::new(Slow(0.01)), Box::new(NoPitch), 8000.0);
        l.quick(true);
        l.push(vec![0.1; 2 * 8000 * 12]);          // 12 s: two 5 s chunks ready (each needs 2 s of music after it)
        let t = Instant::now();
        while l.vocal_so_far.lock().unwrap().len() < 10 * SR && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(l.vocal_so_far.lock().unwrap().len(), 10 * SR);
        l.quick(false);                            // lined up: the rest waits for the song's end
        l.push(vec![0.1; 2 * 8000 * 40]);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(l.vocal_so_far.lock().unwrap().len(), 10 * SR, "separated after it was told to stop");
        let out = l.finish().expect("learned");
        assert_eq!(out.vocal16.len(), 52 * SR);
        assert_eq!(out.midi.len(), 52 * SR / HOP);
    }

    /// Wait (at most 5 s) until `l` has `secs` of singer separated.
    fn separated(l: &Learner, secs: usize) -> usize {
        let t = Instant::now();
        while l.vocal_so_far.lock().unwrap().len() < secs * SR && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        l.vocal_so_far.lock().unwrap().len()
    }

    #[test]
    fn songs_that_ended_are_learned_one_at_a_time_in_the_order_they_ended() {
        let _one = SINGING_TESTS.lock().unwrap();     // (the queue is the app's one; the quick tests read it)
        let track = |id: &str| crate::source::Track { id: id.into(), name: id.into(), ..Default::default() };
        let jobs: Vec<crate::progress::Job> = (0..3).map(|i| crate::progress::Job::new(&track(&format!("q-{i}")))).collect();
        let stage = |i: usize| crate::progress::snapshot()["songs"].as_array().unwrap().iter()
            .find(|s| s["track_id"] == format!("q-{i}")).map(|s| s["stage"].as_str().unwrap().to_string());
        let order = Arc::new(Mutex::new(Vec::new()));
        let queued: Vec<Queued> = jobs.iter().map(|j| { j.ended(); Queued::join(Some(j.clone())) }).collect();
        assert!(learning_now());
        // the first is learned at once; the others say they wait to be learned
        assert_eq!((stage(0).unwrap(), stage(1).unwrap(), stage(2).unwrap()), ("waiting".into(), "queued".into(), "queued".into()));
        assert_eq!(queued.iter().map(Queued::ahead).collect::<Vec<_>>(), vec![0, 1, 2]);
        // each learned on its own thread (as each take is scored), the last started first
        let hs: Vec<_> = queued.into_iter().enumerate().rev().map(|(i, q)| {
            let order = order.clone();
            std::thread::spawn(move || {
                q.wait();
                order.lock().unwrap().push(i);
                std::thread::sleep(Duration::from_millis(50));     // learning it
                drop(q);
            })
        }).collect();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(stage(2).unwrap(), "queued");
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2]);
        assert!(!learning_now());
        assert_eq!(stage(2).unwrap(), "waiting");     // (its turn came: no longer said to wait for another)
    }

    #[test]
    fn a_first_play_separates_nothing_while_another_song_is_learned() {
        let _one = SINGING_TESTS.lock().unwrap();
        // the song before is being learned: the first play keeps its music, and lines up nothing yet
        let before = Queued::join(None);
        let l = Learner::start_deferred(Box::new(Slow(0.01)), Box::new(NoPitch), 8000.0);
        l.quick(true);
        l.push(vec![0.1; 2 * 8000 * 12]);
        std::thread::sleep(Duration::from_millis(300));
        assert!(l.vocal_so_far.lock().unwrap().is_empty(), "separated while another song was learned");
        // learned: still within its first 90 s, it catches up, 5 s at a time, as the music arrives
        drop(before);
        l.push(vec![0.1; 2 * 800]);
        assert_eq!(separated(&l, 10), 10 * SR);
        let out = l.finish().expect("learned");
        assert_eq!(out.vocal16.len() * 10, 121 * SR);

        // the song before took longer than this one's first 90 s: all of it is learned after it ends
        let before = Queued::join(None);
        let l = Learner::start_deferred(Box::new(Slow(0.01)), Box::new(NoPitch), 8000.0);
        l.quick(true);
        l.push(vec![0.1; 2 * 8000 * 100]);
        drop(before);
        l.push(vec![0.1; 2 * 800]);
        std::thread::sleep(Duration::from_millis(300));
        assert!(l.vocal_so_far.lock().unwrap().is_empty(), "separated past its first 90 s");
        assert_eq!(l.finish().expect("learned").vocal16.len() * 10, 1001 * SR);
    }

    #[test]
    fn songs_are_never_refused_and_a_dropped_take_stops_at_once() {
        let _one = SINGING_TESTS.lock().unwrap();     // (timed: nobody sings meanwhile)
        let before = running();
        // a playlist's songs, each restarted mid-song: every take gets a learner, however many are still finishing
        let mut kept = Vec::new();
        for _ in 0..4 {
            let l = learner(0.2);
            l.push(vec![0.1; 2 * 8000 * 70]);        // 70 s of music: 4 chunks to separate
            kept.push(l);
            let dropped = learner(0.2);
            dropped.push(vec![0.1; 2 * 8000 * 70]);
            drop(dropped);                           // too short to keep: stops after the chunk it's on, at most
        }
        let t = Instant::now();
        for l in kept {
            let out = l.finish().expect("learned");
            assert_eq!(out.vocal16.len(), 70 * SR);
        }
        // 4 songs x 4 chunks, one at a time (a dropped learner separating everything it was given would double it)
        assert!(t.elapsed() < Duration::from_secs_f64(4.0 * 4.0 * 0.2 + 1.5), "{:?}", t.elapsed());
        let t = Instant::now();
        while running() > before && t.elapsed() < Duration::from_secs(2) {     // (other tests may run learners too)
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(running() <= before, "{} learners still running", running());
    }
}
