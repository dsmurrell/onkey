//! Listening mode: OnKey follows the song from its sound alone, with no control of (or prompts about) the music
//! app. The music app's audio is captured as always; from it OnKey works out:
//!   - a song started: a few seconds of sound that a recogniser names (recognize.rs); sound it can't name (a
//!     video, a podcast) is ignored: after three tries it isn't asked about again until it changes markedly
//!     (or another app plays)
//!   - where in the song: the recogniser says where its few seconds start; the song's clock then runs with the
//!     audio, and is checked again every few seconds and after every gap (a jump in the song shows as a gap)
//!   - paused: the app plays exact digital silence while paused; the clock stops
//!   - the song changed: the sound stops matching it, and a recogniser names another
//!
//! The `Listener` is the state machine (fed with audio, it asks for recognitions and takes their answers; its
//! clock is the audio's, so tests can run it faster than real time). The app runs it through `feed` and
//! `state`, with a worker thread doing the recognising.
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::Instant;

use crate::messages::Line;
use crate::recognize::{Match, Recognizer, SR};
use crate::source::{Source, State, Track};

/// 10 ms of 16 kHz audio
const PIECE: usize = SR / 100;
/// this long of exact silence and the music is paused (shorter: a gap inside the song, or a jump)
const PAUSE_S: f64 = 0.15;
/// at a song's very start, its own silence can last this long before it counts as a pause
const LEAD_SILENCE_S: f64 = 2.0;
/// a gap this long is enough to suspect a jump: check where we are once the sound is back
const GAP_S: f64 = 0.03;
/// sound needed before asking what it is (Shazam refuses less than ~3 s)
const IDENTIFY_AFTER_S: f64 = 3.0;
/// and how much of it to send (the recognisers need ~2 s; more is surer)
const WINDOW_S: f64 = 5.0;
/// checks send only the latest sound, so that after a jump they soon hear only the new place
const VERIFY_WINDOW_S: f64 = 2.5;
/// while a song plays, check where we are this often: often for songs learned on this Mac (the check is local
/// and instant), rarely for songs only Shazam knows (each check is a request to Apple)
const VERIFY_EVERY_S: f64 = 3.0;
const VERIFY_SHAZAM_S: f64 = 12.0;

fn verify_every(by: &str) -> f64 {
    if by == "shazam" { VERIFY_SHAZAM_S } else { VERIFY_EVERY_S }
}
/// a song only Shazam knows is checked by Shazam, which refuses less than ~3 s of sound
fn verify_window(by: &str) -> f64 {
    if by == "shazam" { SHAZAM_WINDOW_S } else { VERIFY_WINDOW_S }
}
fn verify_needs(by: &str) -> f64 {
    if by == "shazam" { SHAZAM_WINDOW_S } else { 2.0 }
}
const SHAZAM_WINDOW_S: f64 = 3.5;
/// the least Shazam takes (it refuses under 3 s)
const SHAZAM_MIN_S: f64 = 3.05;
/// after a gap, check once this much sound is back
const VERIFY_AFTER_GAP_S: f64 = 2.0;
/// a check that disagrees with the clock by more than this moves the clock (a jump)
const JUMP_S: f64 = 1.0;
/// a jump by Shazam's place alone, with no gap, is believed only from answers over this long (s)
const JUMP_SPAN_S: f64 = 20.0;
/// a jump forward by this much (s) with the sound running on is the learned copy's fault, not the music's: it has
/// time in it the song hasn't (Store::note_slips)
const SLIP_S: std::ops::RangeInclusive<f64> = JUMP_S..=3.0;
/// such jumps seen while songs played, not yet kept with their song: (song id, where, by how much)
static SLIPS: Mutex<Vec<(String, f64, f64)>> = Mutex::new(Vec::new());
/// checks that fit the clock but all sit more than this far to one side of it: the clock drifts, and is slewed
const DRIFT_S: f64 = 0.25;
/// how fast a drift is taken up (s per s of music)
const SLEW_RATE: f64 = 0.02;
/// after OnKey moves the song, the app takes this long to get there (s): sound from then is the old place
const MOVE_SETTLE_S: f64 = 0.5;
/// how late the music app's jump lands after OnKey asks for it, as a rule (take.rs learns each app's own)
const MOVE_LAG_S: f64 = 0.2;
/// this many checks in a row that can't find the song while it sounds: it's over (or changed)
const LOST_AFTER: u32 = 2;
/// after a pause this long, the sound coming back may be something else: check before saying it's playing
const LONG_PAUSE_S: f64 = 8.0;
/// audio kept, to answer with
const HIST_S: f64 = 25.0;
/// a pause this close to a song's end, or its clock this far past it, and the song is over
const END_S: f64 = 4.0;
/// this many tries that name nothing, and the sound is left alone (a podcast, a call: each try sends Shazam a
/// signature) until it's markedly different from every stretch that couldn't be named
const GIVE_UP_AFTER: u32 = 3;
/// how different that is: the sound's spectral shape (`profile`) differs by this much on average (dB)
const CHANGED_DB: f32 = 5.0;
/// while given up, how often the sound is compared
const CHANGE_CHECK_S: f64 = 1.0;

/// a song learned here named by its signature (or fingerprint) this close to its end (s) isn't believed: a song's
/// last seconds (a fade, its silence) match the quiet start of another (live: Viva La Vida, not learned, named Stay
/// With Me at 172.8 s of 172.7 s, which then ended at once)
const NEAR_END_S: f64 = 3.0;

/// another recording of the song playing, this close to the clock (s), is the same song
const ALIAS_S: f64 = 3.0;

/// Two tracks that are the same song (as far as following it goes): the same artist, and titles that share most
/// words once versions and features are left out ("Somewhere Over the Rainbow" and "Over the Rainbow / What a
/// Wonderful World", "Shallow" and "Shallow - Radio Edit").
pub fn same_song(a: &Track, b: &Track) -> bool {
    if a.artist.is_empty() || !crate::store::same_artist(&a.artist, &b.artist) {
        return false;
    }
    let words = |s: &str| -> Vec<String> {
        let s = s.split(" - ").next().unwrap_or(s);
        let s: String = s.chars().scan(0i32, |depth, c| {
            match c { '(' | '[' => *depth += 1, ')' | ']' => *depth -= 1, _ => {} }
            Some(if *depth > 0 || c == ')' || c == ']' { ' ' } else { c })
        }).collect();
        s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
    };
    let (wa, wb) = (words(&a.name), words(&b.name));
    let (short, long) = if wa.len() <= wb.len() { (wa, wb) } else { (wb, wa) };
    if short.is_empty() {
        return false;
    }
    let shared = short.iter().filter(|w| long.contains(w)).count();
    shared as f64 / short.len() as f64 >= 0.6
}

/// Bands of a `profile`, log-spaced from 100 Hz to 8 kHz.
const BANDS: usize = 16;

/// A stretch of sound's spectral shape: its average power in each band (dB), less the mean over the bands so
/// that loudness doesn't count. Speech and music, or two different songs, differ by several dB on average; the
/// same sound a minute later, by much less.
fn profile(x: &[f32]) -> [f32; BANDS] {
    const N: usize = 1024;
    let fft = realfft::RealFftPlanner::<f32>::new().plan_fft_forward(N);
    let (mut buf, mut spec) = (fft.make_input_vec(), fft.make_output_vec());
    let win: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
    let (lo, hi) = (100.0 * N as f64 / SR as f64, 8000.0 * N as f64 / SR as f64);
    let mut edges = [0usize; BANDS + 1];
    for (i, e) in edges.iter_mut().enumerate() {
        *e = (lo * (hi / lo).powf(i as f64 / BANDS as f64)).round() as usize;
    }
    for i in 1..=BANDS {
        edges[i] = edges[i].max(edges[i - 1] + 1);       // at least one bin each
    }
    let mut power = [0f64; BANDS];
    for frame in x.chunks_exact(N) {
        for ((b, s), w) in buf.iter_mut().zip(frame).zip(&win) {
            *b = s * w;
        }
        if fft.process(&mut buf, &mut spec).is_err() {
            continue;
        }
        for (b, p) in power.iter_mut().enumerate() {
            *p += spec[edges[b]..edges[b + 1]].iter().map(|c| c.norm_sqr() as f64).sum::<f64>() / (edges[b + 1] - edges[b]) as f64;
        }
    }
    let db = power.map(|p| (10.0 * (p + 1e-12).log10()) as f32);
    let mean = db.iter().sum::<f32>() / BANDS as f32;
    db.map(|d| d - mean)
}

/// How different two profiles are: the mean difference over the bands (dB).
fn unlike(a: &[f32; BANDS], b: &[f32; BANDS]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / BANDS as f32
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// what is this?
    Identify,
    /// is this still `track`, and where?
    Verify,
}

pub struct Request {
    pub kind: Kind,
    /// mono 16 kHz, ending at `t_end`
    pub audio: Vec<f32>,
    pub t_end: f64,
    pub track: Option<String>,
    /// asked right after a gap in the sound (where a jump would show)
    pub gap: bool,
    /// song seconds at the start of `audio`, by the clock (a check first asks whether the sound fits there)
    pub expect: Option<f64>,
}

pub enum Answer {
    /// a Verify: the sound fits where the clock says (no jump, whatever else it might also match)
    Fits,
    /// a Verify: the sound doesn't fit where the clock says, and was found here (a jump, no second check needed)
    Moved(f64),
    /// a Verify found the song at this offset (song seconds at the start of the request's audio)
    Located(f64),
    /// a recogniser's answer (to an Identify, or to a Verify that couldn't locate the song)
    Identified(Option<Match>),
    /// nothing found, but a recogniser that might have named it is resting after errors (Shazam offline, say)
    /// for this many seconds: no sign that the sound has no name
    Resting(f64),
    /// nothing found yet, but a recogniser that might name it is still waiting for its answer (Shazam slow, as
    /// after a fresh install) or resting after a single error: no sign that the sound has no name either
    Waiting,
    /// nothing found on this Mac, and Shazam wasn't asked: the music app OnKey controls says what plays (no sign
    /// the sound has no name: tried again, and Shazam asked once the app stops saying)
    Held,
}

/// What an answer said that the clock doesn't, waiting for others to agree.
#[derive(Debug, Clone)]
struct Suspect {
    track: Track,
    id: String,
    /// song seconds at audio time `t`
    pos: f64,
    t: f64,
    /// answers so far that agree, each from sound the others weren't given
    count: u32,
    /// audio time the first one's sound started, and the last one's ended
    first_start: f64,
    last_end: f64,
    /// the first came right after a gap in the sound
    gap: bool,
}

#[derive(Debug, Clone)]
pub struct Cur {
    pub track: Track,
    /// song seconds at audio time `Listener::t`
    pub pos: f64,
    pub paused: bool,
    /// back from a long pause, not yet confirmed to be this song (still reported as paused)
    pub unsure: bool,
    /// audio time it paused
    pub paused_at: f64,
    pub apple_music_id: Option<String>,
    pub by: &'static str,
    /// its place is settled: named by an exact signature, by answers that agree on it, by a check that fitted, or
    /// set by OnKey itself (the display waits for this)
    pub sure: bool,
}

/// Something the listener decided, with the audio time it happened (for the log and for tests).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Started { id: String, pos: f64, by: &'static str },
    Paused { pos: f64 },
    Resumed { pos: f64 },
    Jumped { from: f64, to: f64 },
    Changed { id: String, pos: f64 },
    Lost,
    /// the song came to its end (the sound after it is another song)
    Over { id: String },
    NotRecognised,
}

pub struct Listener {
    hist: VecDeque<f32>,
    pending: Vec<f32>,
    /// audio clock: seconds of audio received
    pub t: f64,
    silent_run: f64,
    sound_run: f64,
    /// audio time the sound last came back after a pause-long silence (a song that started then can't be heard
    /// before it)
    sound_since: f64,
    pub cur: Option<Cur>,
    asked: bool,
    next_identify: f64,
    fails: u32,
    /// the profiles of the stretches that couldn't be named (once given up: what the sound must change from)
    unnamed: Vec<[f32; BANDS]>,
    next_change_check: f64,
    /// sound came back after a gap: check where we are once there's enough of it
    after_gap: bool,
    next_verify: f64,
    misses: u32,
    /// an answer said the song is elsewhere, or is another song (a repeated chorus can fool one check); more
    /// answers that agree with it make it so
    suspect: Option<Suspect>,
    pub events: Vec<(f64, Event)>,
    /// decisions and their reasons, for the log
    pub notes: Vec<(f64, String)>,
    /// audio time the last song ended or was lost (the sound since is probably the next song)
    song_ended_at: Option<f64>,
    /// the song that came to its end last (not lost: over)
    over: Option<String>,
    /// the last answer was that Shazam couldn't be asked (resting after errors)
    shazam_resting: bool,
    /// answers that only confirmed the clock (logged every 20)
    quiet_checks: u64,
    /// how long the last gap in the sound was (s), and the audio time the sound came back after it
    last_gap: f64,
    gap_back: f64,
    /// answers naming what plays, while none is followed (two for the same song name it)
    answers: Vec<Suspect>,
    /// how far the last checks that fitted the clock placed the song from it (s), and the drift still to take up
    drift: Vec<f64>,
    slew: f64,
    /// audio time OnKey itself last moved the song (its restart, skip or seek)
    moved_at: f64,
    /// held after OnKey moved the song, until the app has got there (audio time; 0: not held)
    held_until: f64,
    /// the song the music app OnKey controls last said it plays (while it says, Shazam is held back)
    pub app_track: Option<Track>,
}

impl Default for Listener {
    fn default() -> Self {
        Listener::new()
    }
}

impl Listener {
    pub fn new() -> Listener {
        Listener { hist: VecDeque::new(), pending: Vec::new(), t: 0.0, silent_run: 0.0, sound_run: 0.0, sound_since: 0.0, cur: None, asked: false,
                   next_identify: 0.0, fails: 0, unnamed: Vec::new(), next_change_check: 0.0, after_gap: false, next_verify: 0.0, misses: 0, suspect: None, events: Vec::new(),
                   notes: Vec::new(), song_ended_at: None, over: None, shazam_resting: false, quiet_checks: 0, last_gap: 0.0, gap_back: 0.0, answers: Vec::new(),
                   drift: Vec::new(), slew: 0.0, moved_at: 0.0, held_until: 0.0, app_track: None }
    }

    fn event(&mut self, e: Event) {
        self.events.push((self.t, e));
    }

    /// A decision and why, for the log.
    fn note(&mut self, s: String) {
        self.notes.push((self.t, s));
    }

    /// The song playing is over (or lost): the sound is asked about afresh, at once.
    fn song_over(&mut self, id: &str) {
        self.answers.clear();
        self.over = Some(id.to_string());
        self.misses = 0;
        self.suspect = None;
        self.fails = 0;
        self.unnamed.clear();
        self.next_identify = 0.0;
        self.song_ended_at = Some(self.t);
    }

    /// What listening is doing, for the window's status line, while no song is being followed: its kind and words
    /// (messages.rs `Line`; every kind in src-tauri/messages/KINDS.md).
    /// None: a song is followed (the engine says what it's doing with it), or nothing sounds.
    pub fn status(&self) -> Option<Line> {
        if self.cur.is_some() || self.sound_run < 0.3 {
            return None;
        }
        Some(if self.fails >= GIVE_UP_AFTER {
            Line::new("unnamed", "Couldn’t name this song")
        } else if self.shazam_resting {
            Line::new("no_shazam", NO_SHAZAM)
        } else if self.song_ended_at.is_some_and(|e| self.t - e < 60.0) {
            Line::new("new_song", "New song… finding it")
        } else {
            Line::new("finding", "Hearing music… finding the song")
        })
    }

    /// OnKey itself moved the song to `pos` (its restart, skip or seek): the clock goes there at once. The sound
    /// from before isn't asked about, nor the moment the app takes to get there; checks wait for 3 s of sound
    /// after it, as after a gap (live: OnKey's restart made no gap, the checks heard the old place and lost the
    /// song). `None`: OnKey moved to another song, which is named afresh.
    pub fn moved(&mut self, pos: Option<f64>) {
        self.moved_after(pos, MOVE_LAG_S);
    }

    /// The same, with the app at the new place `lag` s after now (0: the app reported it, it's there already).
    pub fn moved_after(&mut self, pos: Option<f64>, lag: f64) {
        self.moved_at = self.t;
        self.hist.clear();
        self.sound_run = 0.0;
        self.sound_since = self.t + lag;     // (the sound before then is the old place: never a take's lead-in)
        self.suspect = None;
        self.answers.clear();
        self.drift.clear();
        self.slew = 0.0;
        self.misses = 0;
        self.after_gap = true;
        self.last_gap = 0.0;            // (no gap: OnKey moved the same song, no other song starts here)
        self.gap_back = self.t + lag;
        match (pos, self.cur.as_mut()) {
            (Some(p), Some(c)) => {
                let from = c.pos;
                // the app gets there a moment later, the old place playing on meanwhile. Moved back (a new take
                // will start): held, as if paused, so no take takes that sound, then the clock runs from the new
                // place. Moved on (the take carries on, filled to the new place, allowing for the same moment):
                // the clock starts that moment behind. Moved while paused (OnKey's restart, Spotify staying paused):
                // nothing plays meanwhile, and the clock waits at the new place for the sound to come back (not back
                // from a long pause, unsure: the sound is playing)
                // (live: restarted after 110 s paused, the whole pause was taken off the new place: -110.6 s)
                if c.paused && !c.unsure {
                    c.pos = p;
                    c.paused_at = self.t;
                } else if p < from && lag > 0.0 {
                    c.pos = p;
                    c.paused = true;
                    c.paused_at = self.t;
                    self.held_until = self.t + lag;
                } else {
                    c.pos = p - lag;
                    c.paused = false;
                }
                if !c.paused || self.held_until > 0.0 {
                    self.silent_run = 0.0;      // (the silence so far is the old place's, never taken off the new)
                }
                c.unsure = false;
                c.sure = true;
                let name = c.track.name.clone();
                self.note(format!("{name} moved from {from:.1} s to {p:.1} s ({})", if lag > 0.0 { "OnKey moved it" } else { "the music app says" }));
                self.event(Event::Jumped { from, to: p });
            }
            (None, Some(_)) => {
                let c = self.cur.take().unwrap();
                self.note(format!("OnKey moved on from {}: finding the next song", c.track.name));
                self.song_over(&c.track.id);
                self.event(Event::Over { id: c.track.id });
            }
            _ => self.next_identify = 0.0,
        }
    }

    /// Seconds of sound heard since the last pause, as far as it's kept (see the host's `heard_since_pause`).
    pub fn heard_since_pause(&self) -> f64 {
        (self.t - self.sound_since).min(self.hist.len() as f64 / SR as f64).max(0.0)
    }

    /// How many seconds of the audio heard last belong to the song playing now (from its start, if that was heard):
    /// its place in the song, but no further back than the last pause and the audio kept. A take started late
    /// (once the song was named) begins with these, so it covers the song from its start.
    pub fn lead_in(&self) -> f64 {
        match &self.cur {
            Some(c) if !c.paused => c.pos.min(self.t - self.sound_since).min(self.hist.len() as f64 / SR as f64).max(0.0),
            _ => 0.0,
        }
    }

    /// Mono 16 kHz music audio, as it arrives.
    pub fn feed(&mut self, x16: &[f32]) {
        self.pending.extend_from_slice(x16);
        let whole = self.pending.len() / PIECE * PIECE;
        let pieces: Vec<f32> = self.pending.drain(..whole).collect();
        for p in pieces.chunks_exact(PIECE) {
            self.piece(p);
        }
    }

    fn piece(&mut self, p: &[f32]) {
        // OnKey moved the song, and the app has had time to get there: it plays on from the new place
        if self.held_until > 0.0 && self.t >= self.held_until {
            self.held_until = 0.0;
            if let Some(c) = self.cur.as_mut().filter(|c| c.paused) {
                c.paused = false;
                self.silent_run = 0.0;      // (the clock runs from the new place now: silence while held isn't in it)
            }
        }
        let silent = p.iter().all(|v| v.abs() < 1e-7);
        if silent {
            self.silent_run += 0.01;
            self.sound_run = 0.0;
            if let Some(c) = self.cur.as_mut() {
                if !c.paused {
                    c.pos += 0.01;           // a gap inside the song, until it's long enough to be a pause
                    // (a song's first second is often exact silence of its own: there, only 2 s of it is a pause)
                    let pause_after = if c.pos - self.silent_run < 1.0 { LEAD_SILENCE_S } else { PAUSE_S };
                    if self.silent_run >= pause_after {
                        c.paused = true;
                        c.paused_at = self.t;
                        c.pos -= self.silent_run;  // the clock stopped when the sound did
                        let pos = c.pos;
                        self.event(Event::Paused { pos });
                    }
                }
            }
        } else {
            if self.silent_run >= PAUSE_S {
                self.sound_since = self.t;
            }
            if self.silent_run >= GAP_S {
                self.after_gap = true;
                self.last_gap = self.silent_run;
                self.gap_back = self.t;
                let t = self.t;
                // sound back after a pause at the song's very end: the next song (Spotify's autoplay pauses a moment
                // between songs). Asked about at once, not first checked as the old song
                if self.cur.as_ref().is_some_and(|c| c.paused && c.track.duration > 0.0 && c.pos >= c.track.duration - END_S) {
                    let c = self.cur.take().unwrap();
                    self.note(format!("{} is over (sound back after a pause at {:.1} s of {:.1} s): finding the next song", c.track.name, c.pos, c.track.duration));
                    self.song_over(&c.track.id);
                    self.event(Event::Over { id: c.track.id });
                }
                if let Some(c) = self.cur.as_mut() {
                    if c.paused && t - c.paused_at >= LONG_PAUSE_S {
                        c.unsure = true;     // stays paused until a check says it's this song again
                    } else if c.paused {
                        c.paused = false;
                        let pos = c.pos;
                        self.event(Event::Resumed { pos });
                    }
                } else if self.silent_run >= PAUSE_S && self.fails < GIVE_UP_AFTER {
                    // new sound after a silence: worth asking about straight away. The tries so far still count
                    // (a podcast can have silent gaps); once given up, only sound that's different is asked about
                    self.next_identify = 0.0;
                }
            }
            self.silent_run = 0.0;
            self.sound_run += 0.01;
            if let Some(c) = self.cur.as_mut().filter(|c| !c.paused) {
                // (and a drift found is taken up gently, at most 2% faster or slower)
                let step = self.slew.clamp(-SLEW_RATE * 0.01, SLEW_RATE * 0.01);
                c.pos += step;
                self.slew -= step;
                c.pos += 0.01;
            }
            // played on well past the song's end with no pause: another song (or a longer version) is playing
            if self.cur.as_ref().is_some_and(|c| !c.paused && c.track.duration > 0.0 && c.pos > c.track.duration + END_S) {
                let c = self.cur.take().unwrap();
                self.note(format!("{} is over (its clock passed its length, {:.1} s): finding the next song", c.track.name, c.track.duration));
                self.song_over(&c.track.id);
                self.event(Event::Over { id: c.track.id });
            }
        }
        self.t += 0.01;
        self.hist.extend(p.iter().copied());
        let keep = (HIST_S * SR as f64) as usize;
        while self.hist.len() > keep {
            self.hist.pop_front();
        }
    }

    fn tail(&self, secs: f64) -> Vec<f32> {
        let n = ((secs * SR as f64) as usize).min(self.hist.len());
        self.hist.iter().skip(self.hist.len() - n).copied().collect()
    }

    /// A recognition to run now, if one is due (only one at a time: the last must be answered first).
    pub fn want(&mut self) -> Option<Request> {
        if self.asked {
            return None;
        }
        // given up on this sound: ask again only once it's markedly unlike every stretch that couldn't be named
        if self.cur.is_none() && self.fails >= GIVE_UP_AFTER && self.sound_run >= IDENTIFY_AFTER_S && self.t >= self.next_change_check {
            self.next_change_check = self.t + CHANGE_CHECK_S;
            let now = profile(&self.tail(self.sound_run.min(WINDOW_S)));
            if self.unnamed.iter().all(|u| unlike(u, &now) > CHANGED_DB) {
                self.fails = 0;
                self.unnamed.clear();
                self.next_identify = 0.0;
            }
        }
        let req = match &self.cur {
            None if self.sound_run >= IDENTIFY_AFTER_S && self.t >= self.next_identify =>
                Request { kind: Kind::Identify, audio: self.tail(self.sound_run.min(WINDOW_S)), t_end: self.t, track: None, gap: false, expect: None },
            // after a gap, once there's 3 s of sound: less wasn't enough for the songs learned here either (two checks
            // of 2 s named nothing after every restart of a learned song, live, and lost it)
            Some(c) if (!c.paused || c.unsure) && self.sound_run >= if self.after_gap { SHAZAM_MIN_S } else { verify_needs(c.by) }
                && ((self.after_gap && self.sound_run >= VERIFY_AFTER_GAP_S) || self.t >= self.next_verify) => {
                let w = self.sound_run.min(if self.after_gap { SHAZAM_WINDOW_S } else { verify_window(c.by) });
                Request { kind: Kind::Verify, audio: self.tail(w), t_end: self.t,
                          track: Some(c.track.id.clone()), gap: self.after_gap || c.unsure,
                          expect: (!c.paused).then(|| c.pos - w) }
            }
            _ => return None,
        };
        self.asked = true;
        Some(req)
    }

    /// A try that named nothing: again in 3 s, or, after `GIVE_UP_AFTER` of them, not until the sound changes.
    fn unnamed(&mut self, req: &Request) {
        self.fails += 1;
        self.unnamed.push(profile(&req.audio));
        if self.fails < GIVE_UP_AFTER {
            self.next_identify = self.t + 3.0;
        } else {
            self.next_identify = f64::INFINITY;
            self.next_change_check = self.t + CHANGE_CHECK_S;
            self.event(Event::NotRecognised);
        }
    }

    /// The answer to the last request (`self.t` may have moved on while it was worked out).
    pub fn answer(&mut self, req: &Request, ans: Answer) {
        if req.t_end > self.t + 1e-6 {
            return;             // asked before this listener was started afresh (another app, or not listened to)
        }
        self.asked = false;
        if req.t_end - req.audio.len() as f64 / SR as f64 <= self.moved_at + MOVE_SETTLE_S && self.moved_at > 0.0 {
            return;             // about sound from before OnKey moved the song (or while the app was getting there)
        }
        self.shazam_resting = matches!(ans, Answer::Resting(_));
        let len = req.audio.len() as f64 / SR as f64;
        // song seconds now, from where the request's audio started (the music kept playing meanwhile, unless paused)
        let now_from = |offset: f64, paused: bool, t: f64| offset + len + if paused { 0.0 } else { t - req.t_end };
        // a place past the song's end is a wrong answer (live: another song after a long pause, "found" in a
        // learned song's last seconds, which then ended at once, over and over): as good as none
        let length = |id: &str| match &self.cur { Some(c) if c.track.id == id => c.track.duration, _ => 0.0 };
        let past_end = match &ans {
            Answer::Identified(Some(m)) => Some((m.track.name.clone(), m.track.duration, now_from(m.offset, false, self.t))),
            Answer::Located(o) | Answer::Moved(o) => req.track.as_deref().map(|id| (id.to_string(), length(id), now_from(*o, false, self.t))),
            _ => None,
        }.filter(|(_, d, p)| *d > 0.0 && *p > d + 1.0);
        let ans = match past_end {
            Some((name, d, p)) => {
                self.note(format!("answer places {name} at {p:.1} s, past its end ({d:.1} s): taken as nothing named"));
                Answer::Identified(None)
            }
            None => ans,
        };
        // a song learned here named in its last seconds, not the one followed: no answer yet (asked again soon; not
        // a failure to name the sound, which may well be a song)
        let followed = self.cur.as_ref().map(|c| c.track.id.clone());
        let near_end = match &ans {
            Answer::Identified(Some(m)) if m.by != "shazam" && followed.as_deref() != Some(m.track.id.as_str()) && m.track.duration > 0.0 => {
                let p = now_from(m.offset, false, self.t);
                (p > m.track.duration - NEAR_END_S).then(|| (m.track.name.clone(), m.track.duration, p))
            }
            _ => None,
        };
        let ans = match near_end {
            Some((name, d, p)) => {
                self.note(format!("answer places {name} at {p:.1} s, in its last {NEAR_END_S:.0} s ({d:.1} s): not believed"));
                Answer::Waiting
            }
            None => ans,
        };
        // every answer is logged, but those that only confirm the clock are counted, not each said
        let clock = self.cur.as_ref().filter(|c| req.track.as_deref() == Some(c.track.id.as_str())).map(|c| (c.track.name.clone(), c.pos, c.by));
        let fits = |p: f64| clock.as_ref().is_some_and(|c| (p - c.1).abs() <= JUMP_S);
        let quiet = !req.gap && match &ans {
            Answer::Fits => true,
            Answer::Located(o) => fits(now_from(*o, false, self.t)),
            Answer::Identified(Some(m)) => req.track.as_deref() == Some(m.track.id.as_str()) && fits(now_from(m.offset, false, self.t)),
            _ => false,
        };
        if quiet {
            self.quiet_checks += 1;
            if self.quiet_checks % 20 == 0 {
                self.note(format!("checks of {} fit the clock ({} so far)", clock.as_ref().map_or("?", |c| c.0.as_str()), self.quiet_checks));
            }
        } else {
            let what = match &ans {
                Answer::Fits => "fits the clock".to_string(),
                Answer::Moved(o) => format!("doesn't fit the clock; found at {:.1} s", now_from(*o, false, self.t)),
                Answer::Located(o) => format!("found at {:.1} s", now_from(*o, false, self.t)),
                Answer::Identified(Some(m)) => format!("{} names {} ({}) at {:.1} s", m.by, m.track.name, m.track.id, now_from(m.offset, false, self.t)),
                Answer::Identified(None) => "nothing named".to_string(),
                Answer::Resting(s) => format!("nothing named; Shazam resting ({s:.0} s more)"),
                Answer::Waiting => "nothing named yet; waiting for Shazam".to_string(),
                Answer::Held => "not a song learned here (the music app says what plays: Shazam not asked)".to_string(),
            };
            let asked = match &clock {
                Some((name, pos, by)) => format!("check of {name} (clock {pos:.1} s, known by {by})"),
                None => "what is this?".to_string(),
            };
            let gap = if req.gap { format!(" after a {:.2} s gap", self.last_gap) } else { String::new() };
            self.note(format!("answer to {asked}, {len:.1} s of sound{gap}: {what}"));
        }
        match (req.kind, ans) {
            (Kind::Identify, Answer::Identified(Some(m))) => {
                let pos = now_from(m.offset, false, self.t);
                let (id, by) = (m.track.id.clone(), m.by);
                // Shazam (and OnKey's own rough fingerprint) can be wrong about a few seconds of sound: the song is
                // named once a second answer names it too. Where in it is settled separately: Shazam sometimes
                // places a stretch at another repeat of a chorus, or anywhere in a repetitive intro (Dancing On My
                // Own: 14.5, 11.4, 10.9, 19.3 s in 10 s), so the place two answers agree on is taken, else the
                // latest, and the checks that follow settle it (as after a gap). Answers from the signatures of
                // songs learned on this Mac are exact and count at once.
                let t = self.t;
                let mut settled = true;
                let mut pos = pos;
                if by != "catalog" {
                    let same = |s: &Suspect| s.id == id || same_song(&s.track, &m.track);
                    let named_before = self.answers.iter().any(same);
                    let agreeing = self.answers.iter().any(|s| same(s) && (pos - (s.pos + (t - s.t))).abs() <= 0.5);
                    let pair = (!agreeing).then(|| {
                        // two earlier answers for it that agree with each other: their place
                        self.answers.iter().filter(|s| same(s)).find_map(|a| self.answers.iter().filter(|b| same(b) && b.t > a.t)
                            .find(|b| (b.pos - (a.pos + (b.t - a.t))).abs() <= 0.5).map(|b| b.pos + (t - b.t)))
                    }).flatten();
                    // sound after a pause (autoplay's next song, play pressed, OnKey's own change of song) almost
                    // always starts the song at 0:00: one answer that places it there, give or take a second, is
                    // believed at once (the place, and so the song, agree with what was expected)
                    let since = t - self.sound_since;
                    let at_start = !named_before && self.sound_since > 0.0 && since < 20.0 && (pos - since).abs() <= 1.0;
                    if at_start {
                        self.notes.push((t, format!("{} ({id}) named at {pos:.1} s, where a song that started {since:.1} s ago would be: believed at once",
                                                    m.track.name)));
                    } else if !named_before {
                        // asked again soon (the song is being missed meanwhile); a song named only once isn't a
                        // failure to name the sound, unless answer after answer names a different one
                        self.notes.push((t, format!("{} ({id}) named once: waiting for a second answer", m.track.name)));
                        self.answers.push(Suspect { id, track: m.track.clone(), pos, t, count: 1, first_start: req.t_end - len, last_end: req.t_end, gap: false });
                        if self.answers.len() >= 5 {
                            self.answers.clear();
                            self.unnamed(req);
                        } else {
                            self.next_identify = t + 1.0;
                        }
                        return;
                    }
                    if !agreeing && !at_start {
                        match pair {
                            Some(p) => pos = p,
                            None => settled = false,
                        }
                    }
                }
                if !settled {
                    self.notes.push((t, format!("{} named again but placed elsewhere: the latest place taken ({pos:.1} s), checked next", m.track.name)));
                }
                let changed = self.cur.as_ref().is_some_and(|c| c.track.id != id);
                self.cur = Some(Cur { track: m.track, pos, paused: false, unsure: false, paused_at: 0.0, apple_music_id: m.apple_music_id, by, sure: settled });
                self.suspect = None;
                self.answers.clear();
                self.drift.clear();
                self.slew = 0.0;
                self.fails = 0;
                self.unnamed.clear();
                self.misses = 0;
                // a place not yet agreed on is checked straight away, as after a gap (a jump is then believed
                // from two answers that agree)
                self.after_gap = !settled;
                self.next_verify = self.t + if settled { verify_every(by) } else { 1.0 };
                self.event(if changed { Event::Changed { id, pos } } else { Event::Started { id, pos, by } });
            }
            (Kind::Identify, Answer::Resting(_) | Answer::Held) => {
                // Shazam couldn't be asked: not a failure to name it. The songs learned on this Mac are still
                // tried (no network), as often as usual; Shazam joins in again once its rest is over
                self.next_identify = self.t + 3.0;
            }
            (Kind::Identify, Answer::Waiting) => {
                // Shazam hasn't answered yet: asked again in a moment (with newer sound; the earlier requests stay
                // open, and whichever answers first names the song)
                self.next_identify = self.t + 1.0;
            }
            (Kind::Identify, _) => self.unnamed(req),
            (Kind::Verify, ans) => {
                let t = self.t;
                let Some(c) = self.cur.as_mut() else { return };
                if req.track.as_deref() != Some(c.track.id.as_str()) {
                    return;
                }
                // where the check says we are: (song id, song seconds now, and the rest of a new song's details)
                let mut moved = false;
                let (id, pos, other) = match ans {
                    Answer::Moved(o) => { moved = true; (c.track.id.clone(), now_from(o, false, t), None) }
                    Answer::Fits => {
                        self.misses = 0;
                        self.after_gap = false;
                        self.suspect = None;
                        self.next_verify = t + verify_every(c.by);
                        return;
                    }
                    Answer::Located(o) => (c.track.id.clone(), now_from(o, false, t), None),
                    // a song only Shazam knows, and Shazam can't be asked: the clock runs on with the audio. A song
                    // learned here that the local check can't place, while Shazam is still to answer, or held back
                    // because the music app says it plays this song: the same (a partly learned song past its
                    // learned part: live, Scar Tissue learned 15%, lost at 34 s after two such checks in 2 s). Shazam
                    // resting, or the app saying another song: it has stopped playing, a miss
                    Answer::Resting(_) | Answer::Waiting | Answer::Held if c.by == "shazam" => {
                        self.after_gap = false;
                        self.next_verify = t + verify_every(c.by);
                        return;
                    }
                    Answer::Waiting => {
                        self.after_gap = false;
                        self.next_verify = t + verify_every(c.by);
                        return;
                    }
                    Answer::Held if self.app_track.as_ref().map_or(true, |a| a.id == c.track.id || same_song(a, &c.track) || crate::store::same_title(a, &c.track)) => {
                        self.after_gap = false;
                        self.next_verify = t + verify_every(c.by);
                        return;
                    }
                    Answer::Identified(Some(m)) => (m.track.id.clone(), now_from(m.offset, false, t), Some(m)),
                    Answer::Identified(None) | Answer::Resting(_) | Answer::Held => {
                        self.misses += 1;
                        self.next_verify = t + 1.0;
                        if self.misses >= LOST_AFTER {
                            self.cur = None;
                            self.misses = 0;
                            self.next_identify = t;
                            self.suspect = None;
                            self.song_ended_at = Some(t);
                            self.answers.clear();
                            self.event(Event::Lost);
                        }
                        return;
                    }
                };
                self.misses = 0;
                self.after_gap = false;
                self.next_verify = t + verify_every(c.by);
                // another recording of this song (a single and its album version, or a medley of it) that Shazam
                // names by turns: the same song, where its place agrees with the clock
                if let Some(m) = other.as_ref().filter(|m| m.track.id != c.track.id && same_song(&m.track, &c.track) && (pos - c.pos).abs() <= ALIAS_S) {
                    self.notes.push((t, format!("Shazam names {} ({}) at {pos:.1} s: another recording of {} (clock {:.1} s), kept as it is",
                                                m.track.name, m.track.id, c.track.name, c.pos)));
                    self.suspect = None;
                    return;
                }
                let same = id == c.track.id;
                let agrees = same && (pos - c.pos).abs() <= JUMP_S;
                if agrees && !c.unsure {
                    c.sure = true;
                    // the first check after a move, placed exactly here (not Shazam's place): the clock goes there.
                    // A move puts the clock where the app says, less its lead as measured, which can be most of a
                    // second off (offline, alongheard: a drag 0.2-0.8 s off left the recording that far off after it)
                    if req.gap && other.is_none() && (pos - c.pos).abs() > 0.05 {
                        self.notes.push((t, format!("check of {} after a move places it at {pos:.2} s (clock {:.2} s)", c.track.name, c.pos)));
                        c.pos = pos;
                    }
                    if let Some(s) = self.suspect.take() {
                        let what = if s.id == c.track.id { format!("{:.1} s", s.pos + (t - s.t)) } else { s.id };
                        self.notes.push((t, format!("check of {} fits the clock ({:.1} s): {what} not believed", c.track.name, c.pos)));
                    }
                    // checks that fit, but all on one side of the clock: it drifts (Don't Stop Believin', 2024
                    // Remaster: 1.5 s behind Shazam's place over 4 min). Slewed toward them, never jumped
                    self.drift.push(pos - (c.pos + self.slew));
                    if self.drift.len() > 3 {
                        self.drift.remove(0);
                    }
                    let d = &self.drift;
                    let mean = d.iter().sum::<f64>() / d.len() as f64;
                    let spread = d.iter().cloned().fold(f64::MIN, f64::max) - d.iter().cloned().fold(f64::MAX, f64::min);
                    if d.len() >= 2 && d.iter().all(|x| x.signum() == mean.signum() && x.abs() > DRIFT_S) && spread < 0.3 {
                        self.slew += mean;
                        self.drift.clear();
                        self.notes.push((t, format!("{}'s clock drifts: the last checks place it {mean:+.2} s from it, slewed over {:.0} s",
                                                    c.track.name, mean.abs() / SLEW_RATE)));
                    }
                    return;
                }
                // somewhere else (or another song). The music app plays on in order unless there's a gap (a seek, a
                // pause, the next song), so without one the clock is believed over a few answers: Shazam places a
                // stretch at another repeat of a chorus now and then. How many answers that agree with each other,
                // from separate stretches of sound, it takes:
                let shazam = other.as_ref().map_or(c.by == "shazam", |m| m.by == "shazam");
                let tol = if shazam { 0.5 } else { 0.3 };
                let mut same_answer = self.suspect.as_mut().filter(|s| s.id == id && (pos - (s.pos + (t - s.t))).abs() <= tol);
                // the answers after a gap count as after it until they're believed or not
                let gap = req.gap || same_answer.as_ref().is_some_and(|s| s.gap);
                // (a stretch that no longer fits the clock and was found elsewhere is no surer: a partly learned
                // song's fingerprint also finds a repeated chorus at its other repeat)
                // (another song after a gap: only a gap that's a pause, or a song that starts where the sound came
                // back; Spotify's output drops out for 30-80 ms now and then mid-song, under load: live, 18 checks
                // in a day "after a gap" found the song where it was, and each would have let one wrong answer
                // change the song)
                let new_track = self.last_gap >= PAUSE_S || (pos - (t - self.gap_back)).abs() <= 1.5;
                let need = if c.unsure || (gap && !(same && shazam) && (same || new_track)) { 1 }  // after a gap
                           else if same && shazam && !gap { 3 }                         // a jump, by Shazam's place
                           else { 2 };                                                  // another song, no gap
                let start = req.t_end - len;
                let mut count = 1;
                // ...and a jump by Shazam's place with no gap, answers over 20 s at least: a phrase repeated every
                // 9 s fooled three answers in a row (Rather Be, live, against Spotify's own position)
                let mut spanned = true;
                if let Some(s) = same_answer.as_mut() {
                    // the same answer from sound it was already given counts once
                    let separate = if gap { start >= s.first_start + 1.5 } else { start >= s.last_end - 0.1 };
                    if separate {
                        s.count += 1;
                        s.last_end = req.t_end;
                    }
                    count = s.count;
                    spanned = gap || !(same && shazam) || t - s.t >= JUMP_SPAN_S;
                } else if need > 1 {
                    let track = other.as_ref().map_or_else(|| c.track.clone(), |m| m.track.clone());
                    self.suspect = Some(Suspect { id: id.clone(), track, pos, t, count: 1, first_start: start, last_end: req.t_end, gap });
                }
                let what = if same { format!("at {pos:.1} s (clock {:.1} s)", c.pos) } else { format!("{id} at {pos:.1} s") };
                if count < need || !spanned {
                    // asked again on sound not yet heard
                    self.next_verify = if gap { t + 1.0 } else { req.t_end + verify_window(c.by) + 0.05 };
                    self.notes.push((t, format!("check of {} says {what}: {count} of {need} answers{} ({}{})", c.track.name, if spanned { "" } else { ", not yet over 20 s" },
                                                if gap { "after a gap" } else { "no gap" }, if shazam { ", Shazam" } else { "" })));
                    return;
                }
                self.notes.push((t, format!("check of {} says {what}: believed ({count} of {need}{}{})", c.track.name,
                                            if gap { ", after a gap" } else { ", no gap" }, if moved { ", no longer fits the clock" } else { "" })));
                // the sound since the move (the first answer that heard the new place, or the gap it came after)
                // is the new place's; none from before is ever a take's lead-in (a seek back started takes 20 s early)
                let first = self.suspect.as_ref().map_or(start, |s| s.first_start);
                let jump_at = if gap { (t - self.sound_run).min(first) } else { first };
                self.suspect = None;
                self.drift.clear();
                self.slew = 0.0;
                if same {
                    if c.unsure {
                        c.unsure = false;
                        c.sure = true;
                        c.paused = false;
                        c.pos = pos;
                        self.event(Event::Resumed { pos });
                    } else {
                        let from = c.pos;
                        c.sure = true;
                        c.pos = pos;
                        self.sound_since = self.sound_since.max(jump_at);
                        // ahead with the sound running on (no seek, no gap): the learned copy has time the song
                        // hasn't (27 Oct: Take On Me, 1.26 s of silence while the output switched, +1.27 s at 2:06
                        // on every play since). Kept with the song: the same jump on two plays suggests a relearn
                        if !gap && SLIP_S.contains(&(pos - from)) {
                            let mut slips = SLIPS.lock().unwrap();
                            slips.push((c.track.id.clone(), from, pos - from));
                            let n = slips.len();
                            if n > 50 {
                                slips.drain(..n - 50);
                            }
                        }
                        self.event(Event::Jumped { from, to: pos });
                    }
                } else if let Some(m) = other {
                    *c = Cur { track: m.track, pos, paused: false, unsure: false, paused_at: 0.0, apple_music_id: m.apple_music_id, by: m.by, sure: true };
                    self.event(Event::Changed { id, pos });
                }
            }
        }
    }

    /// The song and where it is, at audio time `t` (None: nothing recognised playing).
    pub fn now(&self) -> Option<&Cur> {
        self.cur.as_ref()
    }
}

/// Run a request against a recogniser (what the app's worker thread and the tests both do).
pub fn work(r: &mut dyn Recognizer, req: &Request) -> Answer {
    // no gap: first, does the sound still fit where the clock says? (then nothing changed)
    if let (Kind::Verify, Some(id), Some(e), false) = (req.kind, &req.track, req.expect, req.gap) {
        match r.fits_at(id, &req.audio, e) {
            Some(true) => return Answer::Fits,
            Some(false) => if let Some(o) = r.locate_in(id, &req.audio) { return Answer::Moved(o) },
            None => {}
        }
    }
    let a = match (req.kind, &req.track) {
        (Kind::Verify, Some(id)) => match r.locate_in(id, &req.audio) {
            Some(o) => Answer::Located(o),
            None => Answer::Identified(r.identify(&req.audio)),
        },
        _ => Answer::Identified(r.identify(&req.audio)),
    };
    match (a, r.resting()) {
        (Answer::Identified(None), Some(s)) if r.down() => Answer::Resting(s),
        (Answer::Identified(None), Some(_)) => Answer::Waiting,
        (Answer::Identified(None), None) if r.waiting() => Answer::Waiting,
        (Answer::Identified(None), None) if r.held() => Answer::Held,
        (a, _) => a,
    }
}

// ---------------------------------------------------------------- the app's side

/// Apps listened to, by the bundle id prefix of the processes that play their sound. Capturing them needs only
/// the audio-recording permission, never control of the app. The music apps always; a browser only while
/// YouTube is in use (see `may_listen`): its sound is anything (calls, podcasts, other videos). Windows and Linux
/// add Opera and Vivaldi, which publish a media session / MPRIS player OnKey follows (platform/*/browsers.rs).
#[cfg(target_os = "macos")]
pub const APPS: [(&str, &str); 8] = [
    ("com.spotify.client", "Spotify"),
    ("com.apple.Music", "Apple Music"),
    ("com.google.Chrome", "Chrome"),
    ("com.apple.WebKit.GPU", "Safari"),
    ("com.brave.Browser", "Brave"),
    ("com.microsoft.edgemac", "Edge"),
    ("company.thebrowser.Browser", "Arc"),
    ("org.mozilla.firefox", "Firefox"),
];
#[cfg(not(target_os = "macos"))]
pub const APPS: [(&str, &str); 10] = [
    ("com.spotify.client", "Spotify"),
    ("com.apple.Music", "Apple Music"),
    ("com.google.Chrome", "Chrome"),
    ("com.apple.WebKit.GPU", "Safari"),
    ("com.brave.Browser", "Brave"),
    ("com.microsoft.edgemac", "Edge"),
    ("company.thebrowser.Browser", "Arc"),
    ("org.mozilla.firefox", "Firefox"),
    ("com.operasoftware.Opera", "Opera"),
    ("com.vivaldi.Vivaldi", "Vivaldi"),
];

/// WebKit's shared sound process: Safari's, and also OnKey's own window's (and Mail's).
const WEBKIT: &str = "com.apple.WebKit.GPU";

/// Whether the sound of `bundle` may be listened to (and so fingerprinted). Spotify and Apple Music, always. A
/// browser, only while `youtube` (YouTube control is on, or a YouTube song is in use). WebKit's, never while
/// OnKey plays a take back (`take_playing`), unless OnKey's own window can be told apart from Safari
/// (`separable`): OnKey's playback must never be heard as music.
pub fn allowed(bundle: &str, youtube: bool, take_playing: bool, separable: bool) -> bool {
    match bundle {
        "com.spotify.client" | "com.apple.Music" => true,
        WEBKIT if take_playing && !separable => false,
        b => youtube && APPS.iter().any(|(a, _)| *a == b),
    }
}

/// OnKey is playing one of your takes back (its window's sound comes from WebKit, like Safari's).
static TAKE_PLAYING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_take_playing(on: bool) {
    TAKE_PLAYING.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// When a YouTube song was last in use: opened from OnKey, listened back to, or followed. A browser's sound is
/// listened to for a while after that, though YouTube control is off.
static YOUTUBE_USED: Mutex<Option<Instant>> = Mutex::new(None);
const YOUTUBE_IN_USE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

pub fn youtube_used() {
    *YOUTUBE_USED.lock().unwrap() = Some(Instant::now());
}

fn youtube_in_use() -> bool {
    crate::youtube::enabled() || YOUTUBE_USED.lock().unwrap().is_some_and(|t| t.elapsed() < YOUTUBE_IN_USE)
}

/// `allowed`, now.
pub fn may_listen(bundle: &str) -> bool {
    let playing = TAKE_PLAYING.load(std::sync::atomic::Ordering::Relaxed);
    allowed(bundle, youtube_in_use(), playing, bundle != WEBKIT || crate::capture::webkit_separable())
}

/// A browser making sound while OnKey isn't listening to browsers (YouTube off and not in use lately): its name,
/// and whether turning YouTube on would let OnKey sing along to it (in any browser when listening; else only those
/// whose YouTube tab it can follow). For the window's hint to turn YouTube on. Core Audio only, no AppleScript.
pub fn browser_unheard() -> Option<(&'static str, bool)> {
    if youtube_in_use() {
        return None;
    }
    // (OnKey's own window plays through WebKit too: not Safari while a take plays back, unless told apart)
    let webkit = !TAKE_PLAYING.load(std::sync::atomic::Ordering::Relaxed) || crate::capture::webkit_separable();
    unheard_browser(|b| (b != WEBKIT || webkit) && crate::capture::app_sounding(b), crate::settings::listening())
}

/// `browser_unheard`'s choice: a browser OnKey can follow first, then the others.
fn unheard_browser(sounding: impl Fn(&str) -> bool, listening: bool) -> Option<(&'static str, bool)> {
    let followable = crate::youtube::followable;
    let browsers = || APPS.iter().filter(|(b, _)| !matches!(*b, "com.spotify.client" | "com.apple.Music"));
    browsers().filter(|(b, _)| followable(b)).chain(browsers().filter(|(b, _)| !followable(b)))
        .find(|(b, _)| sounding(b))
        .map(|(b, n)| (*n, listening || followable(b)))
}

/// Which of OnKey's sources an app's sound counts as (for its controls, when you've allowed them).
pub fn source_of(bundle: &str) -> Source {
    match bundle {
        "com.spotify.client" => Source::Spotify,
        "com.apple.Music" => Source::Music,
        b if crate::youtube::followable(b) => Source::YouTube,
        _ => Source::Other,
    }
}

pub fn app_name(bundle: &str) -> &'static str {
    app_name_known(bundle).unwrap_or("your music app")
}

/// As `app_name`, but None for an app OnKey has no name for: what goes to the window, which says "your music app"
/// in its own language.
pub fn app_name_known(bundle: &str) -> Option<&'static str> {
    APPS.iter().find(|(b, _)| *b == bundle).map(|(_, n)| *n)
}

/// The app playing sound now (the one already followed while it still plays), of those that may be listened
/// to. No AppleScript: only Core Audio.
pub fn sounding(prefer: Option<&str>) -> Option<&'static str> {
    // development builds: the simulated player is what sounds (ONKEY_SIM_LISTEN)
    if let Some(on) = crate::devtest::sim_sounding() {
        let _ = crate::capture::app_sounding(APPS[0].0);      // (what the real look costs)
        return on.then_some(APPS[0].0);
    }
    if let Some(p) = prefer {
        if let Some((b, _)) = APPS.iter().find(|(b, _)| *b == p) {
            if may_listen(b) && crate::capture::app_sounding(b) {
                return Some(b);
            }
        }
    }
    APPS.iter().map(|(b, _)| *b).find(|b| may_listen(b) && crate::capture::app_sounding(b))
}

struct Host {
    l: Listener,
    rs: crate::resample::Resampler,
    rate: f64,
    tx: Sender<Request>,
    rx: Receiver<(Request, Answer)>,
    last_feed: Instant,
    bundle: Option<&'static str>,
    /// timing diagnostics (`timing`): the last block's length (s), and how far the audio fed has fallen behind the
    /// wall clock since `fed_from` (fed without a break of over a second): `fed_s` of audio in that time
    block_s: f64,
    fed_from: Instant,
    fed_s: f64,
}

static HOST: Mutex<Option<Host>> = Mutex::new(None);
/// Recognitions asked for since launch: [identify, verify] (for the sing benchmark's log)
static REQUESTS: [std::sync::atomic::AtomicU64; 2] = [std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0)];

pub fn requests() -> (u64, u64) {
    (REQUESTS[0].load(std::sync::atomic::Ordering::Relaxed), REQUESTS[1].load(std::sync::atomic::Ordering::Relaxed))
}
static STORE: Mutex<Option<crate::store::Store>> = Mutex::new(None);
static REBUILD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where learned songs are (their signatures make the on-this-Mac catalog).
pub fn init(store: crate::store::Store) {
    *STORE.lock().unwrap() = Some(store);
}

/// A song was learned or forgotten: the catalog is rebuilt before the next recognition.
pub fn songs_changed() {
    REBUILD.store(true, std::sync::atomic::Ordering::Relaxed);
    *KNOWN.lock().unwrap() = None;      // a song played under its app's id may be learned now
}

fn start_host(rate: f64) -> Host {
    let (tx, wrx) = channel::<Request>();
    let (wtx, rx) = channel::<(Request, Answer)>();
    std::thread::Builder::new().name("onkey-recognise".into()).spawn(move || {
        let store = STORE.lock().unwrap().clone();
        let build = || -> Box<dyn Recognizer> {
            match &store {
                Some(s) => Box::new(crate::recognize::Chain::for_app(s)),
                None => Box::new(crate::recognize::Chain::of(vec![Box::new(crate::recognize::ShazamCatalog::new())])),
            }
        };
        let mut r = build();
        for req in wrx {
            if REBUILD.swap(false, std::sync::atomic::Ordering::Relaxed) {
                r = build();
            }
            let asked = Instant::now();
            // (a panic in it, reading a reply say, is an answer of nothing, and a recogniser made afresh: the worker
            // carries on, else listening would wait for this answer for good and never name another song)
            let a = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(r.as_mut(), &req))) {
                Ok(a) => a,
                Err(_) => {
                    crate::engine::log("listening: recognising failed (a panic): taken as nothing named");
                    r = build();
                    Answer::Identified(None)
                }
            };
            // the records: each recognition request, how long it took and what it answered (ids and numbers)
            crate::records::note("recognize", serde_json::json!({"kind": format!("{:?}", req.kind), "gap": req.gap,
                "ms": (asked.elapsed().as_secs_f64() * 10_000.0).round() / 10.0, "audio_s": req.audio.len() as f64 / SR as f64,
                "rms_db": crate::records::rms_db(&req.audio), "expect": req.expect, "for": req.track,
                "answer": match &a {
                    Answer::Fits => serde_json::json!("fits"),
                    Answer::Moved(o) => serde_json::json!({"moved": o}),
                    Answer::Located(o) => serde_json::json!({"located": o}),
                    Answer::Identified(Some(m)) => serde_json::json!({"id": m.track.id, "offset": m.offset, "by": m.by}),
                    Answer::Identified(None) => serde_json::json!("not recognised"),
                    Answer::Resting(s) => serde_json::json!({"resting_s": s}),
                    Answer::Waiting => serde_json::json!("waiting for Shazam"),
                    Answer::Held => serde_json::json!("not recognised; Shazam held back"),
                }}));
            if wtx.send((req, a)).is_err() {
                break;
            }
        }
    }).unwrap();
    Host { l: Listener::new(), rs: crate::resample::Resampler::new(rate, SR as f64), rate, tx, rx, last_feed: Instant::now(), bundle: None, block_s: 0.0, fed_from: Instant::now(), fed_s: 0.0 }
}

/// The captured audio (interleaved [music L, music R, mic]) from the app `bundle`, as the engine drains it.
pub fn feed(block: &[f32], rate: f64, bundle: &'static str) {
    // not a source that may be listened to (now): its sound isn't looked at, and what was heard is forgotten
    if !may_listen(bundle) {
        if let Some(h) = HOST.lock().unwrap().as_mut().filter(|h| h.l.t > 0.0) {
            h.l = Listener::new();
        }
        return;
    }
    let mut g = HOST.lock().unwrap();
    if g.as_ref().map_or(true, |h| h.rate != rate) {
        *g = Some(start_host(rate));
    }
    let h = g.as_mut().unwrap();
    if h.bundle != Some(bundle) {
        if h.bundle.is_some() {
            crate::engine::log(&format!("listening to {}", app_name(bundle)));
            h.l = Listener::new();      // another app: its sound is asked about afresh, whatever the last one's was
        }
        h.bundle = Some(bundle);
    }
    let mono: Vec<f32> = block.chunks_exact(crate::capture::CHANNELS).map(|f| 0.5 * (f[0] + f[1])).collect();
    let mut x16 = Vec::with_capacity(mono.len() / 2);
    h.rs.process(&mono, &mut x16);
    let before = h.l.events.len();
    h.l.feed(&x16);
    let now = Instant::now();
    h.block_s = mono.len() as f64 / rate;
    // (the first block after a break covers time before it: counting starts from its end)
    if now.duration_since(h.last_feed).as_secs_f64() > 1.0 {
        h.fed_from = now;
        h.fed_s = 0.0;
    } else {
        h.fed_s += h.block_s;
    }
    h.last_feed = now;
    loop {
        match h.rx.try_recv() {
            Ok((req, a)) => h.l.answer(&req, a),
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            // the recognising thread is gone: a new one, and the request it had is asked again
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                crate::engine::log("listening: the recognising thread stopped: started again");
                let fresh = start_host(rate);
                h.tx = fresh.tx;
                h.rx = fresh.rx;
                h.l.asked = false;
                break;
            }
        }
    }
    if let Some(req) = h.l.want() {
        REQUESTS[(req.kind == Kind::Verify) as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = h.tx.send(req);
    }
    for (t, s) in h.l.notes.drain(..) {
        crate::engine::log(&format!("listening: {s} (at {t:.1} s)"));
    }
    for (t, e) in h.l.events[before..].iter() {
        crate::engine::log(&format!("listening: {e:?} at {t:.1} s"));
        crate::records::note("listen", serde_json::json!({"event": format!("{e:?}"), "at_s": (t * 100.0).round() / 100.0}));
    }
    let n = h.l.events.len();
    if n > 200 {
        h.l.events.drain(..n - 100);
    }
}

/// The jumps that blamed song `id`'s learned copy (see SLIP_S), since last asked: (where, by how much).
pub fn take_slips(id: &str) -> Vec<(f64, f64)> {
    let mut slips = SLIPS.lock().unwrap();
    let mine = slips.iter().filter(|s| s.0 == id).map(|s| (s.1, s.2)).collect();
    slips.retain(|s| s.0 != id);
    mine
}

/// Seconds of sound heard since the last pause (as far as it's kept): a song the music app says started that
/// long ago or less began within it.
pub fn heard_since_pause() -> f64 {
    HOST.lock().unwrap().as_ref().map_or(0.0, |h| h.l.heard_since_pause())
}

/// Seconds of the audio just captured that belong to song `id` from its start (see `Listener::lead_in`): a take
/// started now begins with them. 0 if `id` isn't the song being followed.
pub fn lead_in(id: &str) -> f64 {
    HOST.lock().unwrap().as_ref().and_then(|h| h.l.now().filter(|c| c.track.id == id).map(|_| h.l.lead_in())).unwrap_or(0.0)
}

/// The music app OnKey controls says it plays, but none of its sound is on this Mac (see `follow`).
static AWAY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What listening is doing while no song is followed, for the status line (see `Listener::status`).
pub fn status_line() -> Option<Line> {
    if AWAY.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(b) = want_bundle() {
            let app = app_name(b);
            return Some(Line::new("elsewhere", format!("{app} is playing, but not on this Mac")).with("app", app));
        }
    }
    let g = HOST.lock().unwrap();
    let st = g.as_ref()?.l.status()?;
    if std::env::var("ONKEY_NO_SHAZAM").is_ok() && st.kind != "unnamed" {
        return Some(Line::new("no_shazam", NO_SHAZAM));
    }
    // switched off in Settings: a song not learned here is never named, and that's why
    if !crate::settings::shazam() && matches!(st.kind, "unnamed" | "no_shazam") {
        return Some(Line::new("shazam_off", "Not a song learned here: Shazam is off in Settings"));
    }
    Some(st)
}

const NO_SHAZAM: &str = "Shazam unavailable: only songs learned here are recognised";

/// Song `id` came to its end (rather than being lost for a moment), and nothing's been named since.
pub fn is_over(id: &str) -> bool {
    is_over_now(id)
}

/// OnKey moved the song itself (see `Listener::moved`).
pub fn moved(pos: Option<f64>) {
    if let Some(h) = HOST.lock().unwrap().as_mut() {
        h.l.moved(pos);
    }
}

/// The music app itself reports the song moved to `pos` (you sought there; it's there already): listening's
/// clock goes there too, at once (live: after Spotify seeks, the checks went on hearing the old place for 4 s).
pub fn app_moved(pos: f64) {
    if let Some(h) = HOST.lock().unwrap().as_mut() {
        h.l.moved_after(Some(pos), 0.0);
    }
}

fn is_over_now(id: &str) -> bool {
    HOST.lock().unwrap().as_ref().is_some_and(|h| h.l.cur.is_none() && h.l.over.as_deref() == Some(id))
}

/// Where song `id` is at the end of the audio captured so far (song seconds), if it's the song being followed.
pub fn pos_of(id: &str) -> Option<f64> {
    pos_now(id)
}

/// Where song `id` is at the end of the audio captured so far, as the engine reads it: the one answer.
/// The music app's own position when it says (`app_pos`, exact), else listening's clock.
pub struct Position {
    /// song seconds
    pub pos: f64,
    /// seconds of the audio just captured that are this song's, from where it started playing this time
    pub lead: f64,
    /// from the music app itself (a take placed by it lets the music refine its place only a little)
    pub exact: bool,
    /// its place is settled (exact, or confirmed: see `Cur::sure`)
    pub sure: bool,
    /// from listening's clock (it follows this song)
    pub heard: bool,
}

pub fn position(id: &str, app_pos: f64) -> Position {
    let own = |exact| Position { pos: app_pos, lead: app_pos.min(heard_since_pause()).max(0.0), exact, sure: exact, heard: false };
    if from_app() {
        return own(true);
    }
    match pos_now(id) {
        Some(p) => Position { pos: p, lead: lead_in(id), exact: false, sure: sure_of(id), heard: true },
        None => own(false),
    }
}

/// For the log of "Play with the song" (diagnostics only): how old listening's last feed is (s), how long that block
/// of audio was (s), and how far the audio fed has fallen behind the wall clock while fed without a break (s:
/// growing means the capture delivers late, or loses audio).
pub fn timing() -> Option<(f64, f64, f64)> {
    let g = HOST.lock().unwrap();
    let h = g.as_ref()?;
    let behind = h.last_feed.duration_since(h.fed_from).as_secs_f64() - h.fed_s;
    Some((h.last_feed.elapsed().as_secs_f64(), h.block_s, behind))
}

fn sure_of(id: &str) -> bool {
    HOST.lock().unwrap().as_ref().and_then(|h| h.l.now().filter(|c| c.track.id == id).map(|c| c.sure)).unwrap_or(false)
}

fn pos_now(id: &str) -> Option<f64> {
    HOST.lock().unwrap().as_ref().and_then(|h| h.l.now().filter(|c| c.track.id == id && !c.paused).map(|c| c.pos))
}

/// What's playing, as far as listening can tell, in the shape the engine follows. The position runs on between
/// audio deliveries (up to a quarter second).
pub fn state() -> Option<State> {
    let g = HOST.lock().unwrap();
    let h = g.as_ref()?;
    let c = h.l.now()?;
    let bundle = h.bundle?;
    let ahead = if c.paused { 0.0 } else { h.last_feed.elapsed().as_secs_f64().min(0.25) };
    Some(State { playing: !c.paused, track: c.track.clone(), position: c.pos + ahead, source: source_of(bundle) })
}

/// The app the engine should capture (listening mode): the one playing sound now, else the last one.
static WANT: Mutex<Option<&'static str>> = Mutex::new(None);

pub fn want_bundle() -> Option<&'static str> {
    *WANT.lock().unwrap()
}

/// Whether an app that may be listened to was playing sound when `follow` last looked.
static SOUNDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn sounding_now() -> bool {
    SOUNDING.load(std::sync::atomic::Ordering::Relaxed)
}

/// What to follow in listening mode, polled like `source::poll`: the app playing sound is found from Core Audio
/// (no prompts). If you've allowed OnKey to control that app, the app is asked as well (an exact track and
/// position, as outside listening mode); otherwise the song is the one recognised by its sound.
pub fn follow() -> Option<State> {
    let prev = *WANT.lock().unwrap();
    let now = sounding(prev);
    SOUNDING.store(now.is_some(), std::sync::atomic::Ordering::Relaxed);
    match now {
        Some(b) => *WANT.lock().unwrap() = Some(b),
        // the last app may no longer be listened to (a browser once YouTube is over): nothing to capture
        None if prev.is_some_and(|b| !may_listen(b)) => *WANT.lock().unwrap() = None,
        None => {}
    }
    let src = want_bundle().map(source_of);
    let app = src.filter(|s| *s != Source::Other && crate::source::controllable(*s)).and_then(|s| s.state());
    // Spotify or Apple Music, controlled, says what it plays (a song, or a podcast): Shazam isn't asked about it
    let music_app = matches!(src, Some(Source::Spotify | Source::Music));
    if let Some(s) = app.as_ref().filter(|_| music_app) {
        *APP_SAID.lock().unwrap() = Some((s.clone(), Instant::now()));
        if let Some(h) = HOST.lock().unwrap().as_mut() {
            h.l.app_track = Some(s.track.clone());
        }
    }
    // a song OnKey just asked the app to play: what it says before it really plays it from the start (the song
    // before, or the new one at 0 while it loads) isn't believed (see `starting_says`)
    // (an answer not believed isn't run on below either: that would follow the same stale answer as the app's)
    let mut ignored = false;
    let app = match app {
        Some(s) => {
            let now = start_clock();
            match starting_says(&mut STARTING.lock().unwrap(), &s, now) {
                Start::Ignore => {
                    ignored = true;
                    None
                }
                Start::Waiting => Some(State { playing: false, ..s }),
                Start::Playing => Some(s),
            }
        }
        None => None,
    };
    // (on Windows, a browser whose timeline isn't seen current yet, or an app whose timeline can't be trusted:
    // listening's alignment places the song)
    FROM_APP.store(app.as_ref().is_some_and(|s| crate::source::exact_position(s.source)), std::sync::atomic::Ordering::Relaxed);
    // a podcast episode, an audiobook or an ad is never a song: nothing followed (no take, nothing learned)
    let not_song = app.as_ref().is_some_and(|s| !crate::source::is_song(&s.track.id));
    NOT_SONG.store(not_song && app.as_ref().is_some_and(|s| s.playing), std::sync::atomic::Ordering::Relaxed);
    if not_song {
        return None;
    }
    // the music app says it plays, but sends none of its sound to this Mac (Spotify Connect playing on another
    // device, the Mac asleep: the capture's watchdog gave up restarting a silent tap): nothing is followed, so no
    // take of silence and no learner starts, and the status line says why (live, 18 Oct: four tracks advanced with
    // nothing heard, a learner started and dropped for each)
    let away = app.as_ref().is_some_and(|s| s.playing) && want_bundle().is_some_and(crate::capture::elsewhere);
    AWAY.store(away, std::sync::atomic::Ordering::Relaxed);
    if away {
        return None;
    }
    let st = match app {
        Some(st) => Some(known_here(st)),
        // the music app didn't answer: listening's word, else (for a few seconds, while it still sounds) the app's
        // last, run on
        None => state().or_else(|| {
            let held = APP_SAID.lock().unwrap().as_ref().filter(|_| !ignored && music_app && now.is_some() && crate::source::controllable(src.unwrap()))
                .and_then(|(s, t)| app_held(s, t.elapsed().as_secs_f64()));
            if held.is_some() {
                FROM_APP.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            held.map(known_here)
        }),
    };
    // the same song from the other source (the app didn't answer once, and listening said what plays, by its
    // ISRC; or back): it stays the song it was, its id, name and cover, so its take carries on and the window
    // shows no other release; only where its place comes from changes
    static LAST: Mutex<Option<Track>> = Mutex::new(None);
    let st = st.map(|mut s| {
        s.track = as_followed(&mut LAST.lock().unwrap(), s.track);
        s
    });
    // a YouTube song being followed keeps its browser listened to
    if st.as_ref().is_some_and(|s| s.track.id.starts_with(crate::youtube::ID_PREFIX)) {
        youtube_used();
    }
    st
}

/// What the music app OnKey controls (Spotify, Apple Music) last said it plays, and when.
static APP_SAID: Mutex<Option<(State, Instant)>> = Mutex::new(None);

/// For this long after the music app last said what it plays, Shazam isn't asked (a poll the app doesn't answer
/// once, osascript failing a moment, isn't a reason to send its sound to Apple).
pub const APP_SAID_S: f64 = 3.0;

/// For this long after the music app last said what it plays, what it said runs on while listening names nothing
/// (osascript failing a few times; Automation revoked): the song isn't dropped while Shazam, held back until now,
/// names it (a few seconds: 3 s of sound, and two answers that agree).
pub const APP_HOLD_S: f64 = 12.0;

/// May Shazam be asked now? Only with "Use Shazam to name new songs" on (`on`, Settings), and only when no music
/// app OnKey controls says what's playing (`app_said`: seconds since it last did). Spotify and Apple Music name
/// their own songs, and a song learned here is checked by the catalog on this Mac (no network); Shazam is for the
/// rest: YouTube and other apps, Listen only, or the music app not answering (live: Shazam was asked every 12 s
/// through a first play Spotify itself named, and again on every check of a learned song the catalog missed).
pub fn shazam_wanted(on: bool, app_said: Option<f64>) -> crate::recognize::Ask {
    use crate::recognize::Ask;
    if !on {
        Ask::No
    } else if app_said.is_some_and(|ago| ago <= APP_SAID_S) {
        Ask::Held
    } else {
        Ask::Yes
    }
}

/// `shazam_wanted`, now (the app's chain asks before every request to Shazam: recognize.rs `Chain::for_app`).
pub fn shazam_now() -> crate::recognize::Ask {
    shazam_wanted(crate::settings::shazam(), APP_SAID.lock().unwrap().as_ref().map(|(_, t)| t.elapsed().as_secs_f64()))
}

/// What the music app last said, run on to now (`ago` s later), if that's under `APP_HOLD_S` ago.
pub fn app_held(said: &State, ago: f64) -> Option<State> {
    (ago <= APP_HOLD_S).then(|| State { position: said.position + if said.playing { ago } else { 0.0 }, ..said.clone() })
}

/// `t` as the song followed so far, when it's that song under another id (the music app's, or listening's ISRC):
/// the same artist and title. Otherwise `t`, followed from now on.
pub fn as_followed(last: &mut Option<Track>, t: Track) -> Track {
    match last.as_ref() {
        Some(l) if l.id != t.id && crate::store::same_title(l, &t) => l.clone(),
        _ => {
            *last = Some(t.clone());
            t
        }
    }
}

/// OnKey asked the music app to play a song (its own id there), at this time (s): until the app really plays it
/// from its start, what it says isn't taken as where the song is.
#[derive(Debug, Clone)]
pub struct Starting {
    want: String,
    asked: f64,
    /// the position it first said the new song was at, while loading
    first: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Start {
    /// the song before, or the new one at an old place: not believed
    Ignore,
    /// the new song, not playing yet (loading: its position stands still)
    Waiting,
    /// playing it (or nothing was asked)
    Playing,
}

static STARTING: Mutex<Option<Starting>> = Mutex::new(None);
static START_EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Seconds since OnKey first asked the music app to play something or followed it (for `Starting`).
fn start_clock() -> f64 {
    START_EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// OnKey is asking the music app to play `id` (the Library's Sing, listening back): the song before ends now,
/// and the new one's take starts only once the app really plays it (live: Spotify said "playing, 0:00" a second
/// before any sound, after a moment of the song before; the take started then, the words ran a second early,
/// and the countdown went back from 12 s to 13 s when the place was corrected).
pub fn onkey_plays(id: &str) {
    *STARTING.lock().unwrap() = Some(Starting::new(id, start_clock()));
    moved(None);
}

impl Starting {
    pub fn new(want: &str, asked: f64) -> Starting {
        Starting { want: want.to_string(), asked, first: None }
    }
}

/// What the music app's state `st` means at time `now` (s) while OnKey's request to play a song is pending.
pub fn starting_says(pending: &mut Option<Starting>, st: &State, now: f64) -> Start {
    let Some(p) = pending.as_mut() else { return Start::Playing };
    if now - p.asked > 5.0 {
        *pending = None;        // the app never said so: believe it again
        return Start::Playing;
    }
    if st.track.id != p.want || st.position >= 2.0 {
        return Start::Ignore;
    }
    if !st.playing {
        return Start::Waiting;
    }
    let first = *p.first.get_or_insert(st.position);
    if st.position > first + 0.15 {
        *pending = None;
        Start::Playing
    } else {
        Start::Waiting
    }
}

/// The music app is playing something that isn't a song (a podcast episode, an audiobook, an ad).
static NOT_SONG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn not_song() -> bool {
    NOT_SONG.load(std::sync::atomic::Ordering::Relaxed)
}

/// What's followed comes from the music app itself (you've allowed OnKey to ask it): its position is exact, and is
/// believed over the listener's clock (which a restart of the song leaves behind for a few seconds).
static FROM_APP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn from_app() -> bool {
    FROM_APP.load(std::sync::atomic::Ordering::Relaxed)
}

/// The music app's own word for what's playing, as the song learned here under another id if it is one (Spotify's
/// track id for a song learned from Shazam's ISRC: live, Castle on the Hill, learned, started a Learn take). Looked
/// up once per track.
static KNOWN: Mutex<Option<(String, Option<Track>)>> = Mutex::new(None);

fn known_here(mut st: State) -> State {
    let mut k = KNOWN.lock().unwrap();
    if k.as_ref().map_or(true, |(id, _)| *id != st.track.id) {
        let store = STORE.lock().unwrap();
        let found = store.as_ref().and_then(|s| s.known_as(&st.track));
        if let Some(t) = &found {
            crate::engine::log(&format!("{} - {} ({}) is the song learned here as {}", st.track.artist, st.track.name, st.track.id, t.id));
            // kept with the song: its id in this app, to open it there again
            if let Some(s) = store.as_ref() {
                s.note_alias(&t.id, &st.track.id);
            }
        }
        drop(store);
        *k = Some((st.track.id.clone(), found));
    }
    if let Some(t) = k.as_ref().and_then(|(_, t)| t.clone()) {
        let own = std::mem::take(&mut st.track);
        st.track = Track { duration: if own.duration > 0.0 { own.duration } else { t.duration }, art: t.art.clone().or(own.art),
                           link: t.link.clone().or(own.link), ..t };
    }
    st
}

/// The app being listened to (its bundle id prefix), for the engine's capture.
pub fn bundle() -> Option<&'static str> {
    HOST.lock().unwrap().as_ref().and_then(|h| h.bundle)
}

/// Apple Music's id for the song playing (from Shazam), for "Open in Apple Music".
pub fn apple_music_id() -> Option<String> {
    HOST.lock().unwrap().as_ref().and_then(|h| h.l.now().and_then(|c| c.apple_music_id.clone()))
}

/// Forget the song being followed (listening mode switched off, or capture moved to another app).
pub fn reset() {
    if let Some(h) = HOST.lock().unwrap().as_mut() {
        h.l = Listener::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recogniser that never names anything (and may be resting after errors), counting what it's asked.
    struct Nameless {
        resting: Option<f64>,
        asked: usize,
    }

    impl Recognizer for Nameless {
        fn identify(&mut self, _x16: &[f32]) -> Option<Match> {
            self.asked += 1;
            None
        }
        fn locate_in(&mut self, _id: &str, _x16: &[f32]) -> Option<f64> {
            None
        }
        fn resting(&self) -> Option<f64> {
            self.resting
        }
    }

    /// `secs` of sound: white noise if `tones` is empty, else those tones (Hz), at `level`.
    fn sound(secs: f64, tones: &[f64], level: f32, seed: u64) -> Vec<f32> {
        let mut rng = seed | 1;
        (0..(secs * SR as f64) as usize).map(|i| {
            if tones.is_empty() {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                level * ((rng % 20_001) as f32 / 10_000.0 - 1.0)
            } else {
                let t = i as f64 / SR as f64;
                level * tones.iter().map(|f| (std::f64::consts::TAU * f * t).sin() as f32).sum::<f32>() / tones.len() as f32
            }
        }).collect()
    }

    /// Feed `x` in 0.1 s blocks, answering each request at once.
    fn play(l: &mut Listener, r: &mut dyn Recognizer, x: &[f32]) {
        for block in x.chunks(SR / 10) {
            l.feed(block);
            if let Some(req) = l.want() {
                let a = work(r, &req);
                l.answer(&req, a);
            }
        }
    }

    /// Shazam's catalog: names one song, but only from 3 s of sound or more (it refuses less)
    struct ShazamLike {
        t: f64,
    }

    impl Recognizer for ShazamLike {
        fn identify(&mut self, x16: &[f32]) -> Option<Match> {
            (x16.len() >= 3 * SR).then(|| Match {
                track: Track { id: "isrc:X".into(), name: "X".into(), ..Default::default() },
                offset: self.t, apple_music_id: None, by: "shazam",
            })
        }
        fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64> {
            self.identify(x16).filter(|m| m.track.id == id).map(|m| m.offset)
        }
    }

    #[test]
    fn a_song_only_shazam_knows_is_kept() {
        let mut l = Listener::new();
        let mut r = ShazamLike { t: 0.0 };
        let x = sound(90.0, &[220.0, 330.0], 0.3, 1);
        for block in x.chunks(SR / 10) {
            l.feed(block);
            r.t += 0.1;
            if let Some(req) = l.want() {
                let a = work(&mut r, &req);
                l.answer(&req, a);
            }
        }
        assert!(!l.events.iter().any(|(_, e)| *e == Event::Lost), "{:?}", l.events);
    }

    #[test]
    fn profiles_tell_different_sound_not_loudness() {
        let noise = profile(&sound(5.0, &[], 0.3, 1));
        assert!(unlike(&noise, &profile(&sound(5.0, &[], 0.05, 7))) < 1.0);
        assert!(unlike(&noise, &profile(&sound(5.0, &[220.0, 330.0, 440.0, 660.0], 0.3, 1))) > CHANGED_DB);
    }

    #[test]
    fn unnamed_sound_is_left_alone_until_it_changes() {
        let mut l = Listener::new();
        let mut r = Nameless { resting: None, asked: 0 };
        // two minutes of sound nothing can name (a podcast): three tries, then none
        play(&mut l, &mut r, &sound(120.0, &[], 0.3, 1));
        assert_eq!(r.asked, 3);
        assert_eq!(l.events.iter().filter(|(_, e)| *e == Event::NotRecognised).count(), 1);
        // paused and resumed, a little louder: still the same sound, still not asked about
        play(&mut l, &mut r, &vec![0.0; SR]);
        play(&mut l, &mut r, &sound(60.0, &[], 0.5, 9));
        assert_eq!(r.asked, 3);
        // something else entirely (music): asked about again, three more times at most
        play(&mut l, &mut r, &sound(60.0, &[220.0, 330.0, 440.0, 660.0], 0.3, 1));
        assert_eq!(r.asked, 6);
    }

    #[test]
    fn shazam_resting_is_not_a_failure_to_name() {
        let mut l = Listener::new();
        let mut r = Nameless { resting: Some(30.0), asked: 0 };
        play(&mut l, &mut r, &sound(60.0, &[], 0.3, 1));
        // asked every 3 s (only the songs on this Mac, while Shazam rests), never given up on
        assert!(r.asked >= 15, "{}", r.asked);
        assert!(!l.events.iter().any(|(_, e)| *e == Event::NotRecognised));
        assert_eq!(l.status().unwrap().kind, "no_shazam");
    }

    /// A recogniser still waiting for its answer (Shazam slow after a fresh install), never naming anything.
    struct Slow {
        asked: usize,
    }

    impl Recognizer for Slow {
        fn identify(&mut self, _x16: &[f32]) -> Option<Match> {
            self.asked += 1;
            None
        }
        fn locate_in(&mut self, _id: &str, _x16: &[f32]) -> Option<f64> {
            None
        }
        fn waiting(&self) -> bool {
            true
        }
    }

    #[test]
    fn shazam_slow_to_answer_is_not_a_failure_to_name_nor_unavailable() {
        let mut l = Listener::new();
        let mut r = Slow { asked: 0 };
        play(&mut l, &mut r, &sound(30.0, &[], 0.3, 1));
        // asked again every second, never given up on, and the status line still says it's finding the song
        assert!(r.asked >= 20, "{}", r.asked);
        assert!(!l.events.iter().any(|(_, e)| *e == Event::NotRecognised));
        assert_eq!(l.status().unwrap().kind, "finding");
    }

    #[test]
    fn shazam_only_when_no_music_app_says_and_only_when_on() {
        use crate::recognize::Ask;
        assert_eq!(shazam_wanted(true, None), Ask::Yes, "no app says what plays (Listen only, YouTube)");
        assert_eq!(shazam_wanted(true, Some(0.2)), Ask::Held, "Spotify says what plays");
        assert_eq!(shazam_wanted(true, Some(APP_SAID_S - 0.5)), Ask::Held, "a poll it didn't answer once");
        assert_eq!(shazam_wanted(true, Some(APP_SAID_S + 0.5)), Ask::Yes, "it stopped answering");
        assert!(shazam_wanted(false, None) == Ask::No && shazam_wanted(false, Some(0.2)) == Ask::No, "switched off: never");
        // what it said runs on a while, then not
        let s = State { playing: true, track: Track::default(), position: 30.0, source: Source::Spotify };
        assert_eq!(app_held(&s, 2.0).map(|h| h.position), Some(32.0));
        assert!(app_held(&s, APP_HOLD_S + 0.1).is_none());
    }

    #[test]
    fn only_music_sources_are_listened_to() {
        // the music apps, always
        assert!(allowed("com.spotify.client", false, false, false));
        assert!(allowed("com.apple.Music", false, true, false));
        // a browser only while YouTube is in use
        for b in ["com.google.Chrome", "com.apple.WebKit.GPU", "org.mozilla.firefox"] {
            assert!(!allowed(b, false, false, true), "{b}");
            assert!(allowed(b, true, false, true), "{b}");
        }
        // apps that aren't a music source, never
        for b in ["org.videolan.vlc", "com.tidal.desktop", "us.zoom.xos", "com.apple.mail"] {
            assert!(!allowed(b, true, false, true), "{b}");
        }
        // OnKey playing a take back: WebKit's sound isn't listened to unless OnKey's own can be told apart
        assert!(!allowed("com.apple.WebKit.GPU", true, true, false));
        assert!(allowed("com.apple.WebKit.GPU", true, true, true));
        assert!(allowed("com.google.Chrome", true, true, false));
    }

    #[test]
    fn a_browser_playing_unheard_is_named_followable_ones_first() {
        assert_eq!(unheard_browser(|_| false, true), None);
        // Spotify playing isn't a browser
        assert_eq!(unheard_browser(|b| b == "com.spotify.client", true), None);
        // (no Safari on Windows or Linux: nothing follows its sound there)
        assert_eq!(unheard_browser(|b| b == "com.apple.WebKit.GPU", false), Some(("Safari", cfg!(target_os = "macos"))));
        // Firefox: heard once YouTube is on when listening, but its tab can't be followed otherwise
        assert_eq!(unheard_browser(|b| b == "org.mozilla.firefox", true), Some(("Firefox", true)));
        // (on Windows and Linux Firefox's media session / MPRIS player is followed, as every browser's)
        assert_eq!(unheard_browser(|b| b == "org.mozilla.firefox", false), Some(("Firefox", cfg!(not(target_os = "macos")))));
        // Arc and Chrome both sounding: Chrome, which works either way
        assert_eq!(unheard_browser(|b| b == "company.thebrowser.Browser" || b == "com.google.Chrome", false), Some(("Chrome", true)));
    }

    // ------------------------------------------------------------ a music app playing songs, and Shazam naming them

    /// A song on the radio: (audio time it starts at, id, title, artist, length, song seconds it starts from).
    #[derive(Clone)]
    struct Play {
        at: f64,
        id: &'static str,
        name: &'static str,
        artist: &'static str,
        len: f64,
        from: f64,
    }

    fn song(at: f64, id: &'static str, name: &'static str, len: f64) -> Play {
        Play { at, id, name, artist: "A", len, from: 0.0 }
    }

    /// Shazam's catalog over a script of what plays: names the song playing where the asked audio starts, and where
    /// in it, from 3 s of sound (it refuses less). `quirk` can change an answer (`n`: how many answers so far):
    /// another repeat of a chorus, another recording of the same song.
    struct Radio {
        plays: Vec<Play>,
        /// the request being answered: its audio's end (audio time) and length
        t_end: f64,
        len: f64,
        n: usize,
        quirk: fn(&Play, f64, usize) -> Option<(&'static str, &'static str, f64)>,
        /// a partly learned song's own fingerprint as well, which never quite fits the clock (it was placed a
        /// little off), so every check is answered by where the song is found
        fingerprint: bool,
    }

    fn honest(p: &Play, pos: f64, _n: usize) -> Option<(&'static str, &'static str, f64)> {
        Some((p.id, p.name, pos))
    }

    impl Radio {
        fn new(plays: Vec<Play>) -> Radio {
            Radio { plays, t_end: 0.0, len: 0.0, n: 0, quirk: honest, fingerprint: false }
        }

        /// The song playing at audio time `t`, and where in it.
        fn at(&self, t: f64) -> Option<(&Play, f64)> {
            self.plays.iter().rev().find(|p| p.at <= t).map(|p| (p, p.from + t - p.at)).filter(|(p, pos)| *pos < p.len)
        }

        /// The audio for the whole script, `secs` long: each song a different chord, exact silence between.
        fn audio(&self, secs: f64) -> Vec<f32> {
            let mut x = vec![0f32; (secs * SR as f64) as usize];
            for (k, p) in self.plays.iter().enumerate() {
                let a = (p.at * SR as f64) as usize;
                let b = (((p.at + p.len - p.from) * SR as f64) as usize).min(x.len());
                let end = self.plays.get(k + 1).map_or(b, |q| b.min((q.at * SR as f64) as usize));
                if a < end {
                    let s = sound((end - a) as f64 / SR as f64, &[220.0 + 30.0 * k as f64, 330.0 + 20.0 * k as f64], 0.3, k as u64 + 1);
                    x[a..a + s.len()].copy_from_slice(&s);
                }
            }
            x
        }
    }

    impl Recognizer for Radio {
        fn identify(&mut self, x16: &[f32]) -> Option<Match> {
            // (Shazam refuses under 3 s; the songs learned here match from about 2.4 s)
            let least = if self.plays.iter().any(|p| p.id.starts_with("cat:")) { 2.4 } else { 3.0 };
            if (x16.len() as f64) < least * SR as f64 {
                return None;
            }
            let (p, pos) = self.at(self.t_end - self.len).map(|(p, pos)| (p.clone(), pos))?;
            self.n += 1;
            let (id, name, offset) = (self.quirk)(&p, pos, self.n)?;
            Some(Match { track: Track { id: id.into(), name: name.into(), artist: p.artist.into(), duration: p.len, ..Default::default() },
                         offset, apple_music_id: None, by: if id.starts_with("fp:") { "fingerprint" } else if id.starts_with("cat:") { "catalog" } else { "shazam" } })
        }
        fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64> {
            self.identify(x16).filter(|m| m.track.id == id).map(|m| m.offset)
        }
        fn fits_at(&mut self, _id: &str, _x16: &[f32], _offset: f64) -> Option<bool> {
            self.fingerprint.then_some(false)
        }
    }

    /// Feed `x` in 0.1 s blocks to a listener asking `r`, each answer taking `delay` s to come back (the audio plays
    /// on meanwhile). `at_event` sees the listener right after each event.
    fn listen_to(l: &mut Listener, r: &mut Radio, x: &[f32], delay: f64, mut at_event: impl FnMut(&Listener, &Event)) {
        let mut waiting: Option<(Request, Answer, f64)> = None;
        for block in x.chunks(SR / 10) {
            let before = l.events.len();
            l.feed(block);
            if waiting.as_ref().is_some_and(|w| l.t >= w.2 - 1e-6) {
                let (req, a, _) = waiting.take().unwrap();
                l.answer(&req, a);
            }
            if waiting.is_none() {
                if let Some(req) = l.want() {
                    r.t_end = req.t_end;
                    r.len = req.audio.len() as f64 / SR as f64;
                    let a = work(r, &req);
                    waiting = Some((req, a, l.t + delay));
                }
            }
            for i in before..l.events.len() {
                let e = l.events[i].1.clone();
                at_event(l, &e);
            }
        }
    }

    /// Where the take would start (song seconds) when a song is named: its place now less the sound kept for it.
    fn take_starts(l: &Listener) -> f64 {
        l.cur.as_ref().unwrap().pos - l.lead_in()
    }

    #[test]
    fn a_song_named_late_is_taken_from_its_start() {
        // a second of nothing, then a song Shazam names only after two answers that agree
        let mut r = Radio::new(vec![song(1.0, "isrc:A", "A", 200.0)]);
        let x = r.audio(30.0);
        let mut l = Listener::new();
        let mut starts = Vec::new();
        listen_to(&mut l, &mut r, &x, 1.0, |l, e| if matches!(e, Event::Started { .. }) { starts.push((l.cur.as_ref().unwrap().pos, take_starts(l))) });
        assert_eq!(starts.len(), 1, "{:?}", l.events);
        let (named_at, from) = starts[0];
        assert!(named_at > 4.0, "named at {named_at:.1} s");
        assert!(from.abs() <= 1.0, "the take would start at {from:.2} s");
    }

    #[test]
    fn the_next_song_is_taken_from_its_start() {
        // autoplay: one song to its end, Spotify's short pause, the next
        let mut r = Radio::new(vec![song(0.0, "isrc:A", "A", 40.0), song(40.4, "isrc:B", "B", 200.0)]);
        let x = r.audio(70.0);
        let mut l = Listener::new();
        let mut named = Vec::new();
        listen_to(&mut l, &mut r, &x, 1.0, |l, e| match e {
            Event::Started { id, .. } | Event::Changed { id, .. } if id == "isrc:B" => named.push((l.t, take_starts(l))),
            _ => {}
        });
        assert!(l.events.iter().any(|(_, e)| *e == Event::Over { id: "isrc:A".into() }), "{:?}", l.events);
        assert_eq!(named.len(), 1, "{:?}", l.events);
        let (t, from) = named[0];
        // named on Shazam's first answer (3 s of sound, 1 s to answer): it places B where a song just begun would be
        assert!(t - 40.4 < 4.6, "B named {:.1} s in", t - 40.4);
        assert!(from.abs() <= 1.0, "B's take would start at {from:.2} s");
    }

    #[test]
    fn the_status_line_says_what_listening_is_doing() {
        let mut r = Radio::new(vec![song(0.0, "isrc:A", "A", 40.0), song(40.4, "isrc:B", "B", 200.0)]);
        let x = r.audio(50.0);
        let mut l = Listener::new();
        let kind = |l: &Listener| l.status().map(|s| s.kind);
        listen_to(&mut l, &mut r, &x[..2 * SR], 1.0, |_, _| {});
        assert_eq!(kind(&l), Some("finding"));
        listen_to(&mut l, &mut r, &x[2 * SR..40 * SR], 1.0, |_, _| {});
        assert_eq!(kind(&l), None, "a song is followed");
        listen_to(&mut l, &mut r, &x[40 * SR..42 * SR], 1.0, |_, _| {});
        assert_eq!(kind(&l), Some("new_song"));
        // sound nothing can name, asked about three times
        let mut l = Listener::new();
        play(&mut l, &mut Nameless { resting: None, asked: 0 }, &sound(15.0, &[], 0.3, 1));
        assert_eq!(kind(&l), Some("unnamed"));
        let mut l = Listener::new();
        play(&mut l, &mut Nameless { resting: Some(30.0), asked: 0 }, &sound(5.0, &[], 0.3, 1));
        assert_eq!(kind(&l), Some("no_shazam"));
    }

    #[test]
    fn a_song_started_midway_takes_only_its_own_sound() {
        // you start a song at 1:30 after another app's sound and a pause: nothing from before the pause is kept
        let mut r = Radio::new(vec![Play { at: 5.0, id: "isrc:A", name: "A", artist: "A", len: 200.0, from: 90.0 }]);
        let mut x = sound(4.0, &[], 0.2, 3);
        x.extend(vec![0.0; SR]);
        x.extend(sound(25.0, &[440.0], 0.3, 1));
        let mut l = Listener::new();
        let mut at = None;
        listen_to(&mut l, &mut r, &x, 1.0, |l, e| if matches!(e, Event::Started { .. }) { at = Some((l.t, l.lead_in(), take_starts(l))) });
        let (t, lead, from) = at.expect("named");
        assert!((lead - (t - 5.0)).abs() < 0.05, "kept {lead:.2} s, {:.2} s since the pause", t - 5.0);
        assert!((from - 90.0).abs() < 0.5, "from {from:.2}");
    }

    /// A chorus sung three times, 60 s apart (from 40, 100 and 160 s, 30 s each), that Shazam places at another of
    /// its repeats two answers in three (The Scientist: 25-40 s off).
    fn chorus(p: &Play, pos: f64, n: usize) -> Option<(&'static str, &'static str, f64)> {
        let k = ((pos - 40.0) / 60.0).floor();
        let inside = (0.0..3.0).contains(&k) && pos - 40.0 - 60.0 * k < 30.0;
        let other = if k == 0.0 { 60.0 } else { -60.0 };
        Some((p.id, p.name, if inside && n % 3 != 0 { pos + other } else { pos }))
    }

    fn jumps(l: &Listener) -> Vec<(f64, f64, f64)> {
        l.events.iter().filter_map(|(t, e)| match e { Event::Jumped { from, to } => Some((*t, *from, *to)), _ => None }).collect()
    }

    /// A phrase repeated every 9 s that Shazam places one repeat early three answers in four (Rather Be, live).
    fn phrase(p: &Play, pos: f64, n: usize) -> Option<(&'static str, &'static str, f64)> {
        Some((p.id, p.name, if n % 4 != 0 && pos > 20.0 { pos - 9.0 } else { pos }))
    }

    /// The music runs 0.6% faster than its sound says (Don't Stop Believin', live: 1.5 s over 4 min).
    fn fast(p: &Play, pos: f64, _n: usize) -> Option<(&'static str, &'static str, f64)> {
        Some((p.id, p.name, pos * 1.006))
    }

    #[test]
    fn a_drifting_clock_is_slewed_not_jumped() {
        let mut r = Radio::new(vec![song(0.0, "isrc:D", "Don't Stop Believin'", 300.0)]);
        r.quirk = fast;
        let x = r.audio(240.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
        assert!(jumps(&l).is_empty(), "{:?}", jumps(&l));
        let c = l.cur.as_ref().unwrap();
        // Shazam's place at the end is 240 x 1.006 = 241.44 s; uncorrected the clock would say 240
        assert!((c.pos - 241.44).abs() < 0.4, "clock {:.2}", c.pos);
    }

    #[test]
    fn a_short_repeated_phrase_does_not_move_the_clock() {
        let mut r = Radio::new(vec![song(0.0, "isrc:R", "Rather Be", 300.0)]);
        r.quirk = phrase;
        let x = r.audio(200.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
        assert!(jumps(&l).is_empty(), "false jumps {:?}", jumps(&l));
    }

    #[test]
    fn a_repeated_chorus_does_not_move_the_clock() {
        // Shazam's answers alone, and with a partly learned fingerprint that never quite fits (The Scientist, live)
        for fingerprint in [false, true] {
            let mut r = Radio::new(vec![song(0.0, "isrc:S", "The Scientist", 300.0)]);
            r.quirk = chorus;
            r.fingerprint = fingerprint;
            let x = r.audio(250.0);
            let mut l = Listener::new();
            listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
            assert!(jumps(&l).is_empty(), "fingerprint {fingerprint}: false jumps {:?}", jumps(&l));
            assert!(l.events.iter().filter(|(_, e)| matches!(e, Event::Started { .. })).count() == 1, "{:?}", l.events);
            let c = l.cur.as_ref().unwrap();
            assert!((c.pos - 250.0).abs() < 0.2, "clock {:.2}", c.pos);
        }
    }

    /// The song played from `from`, then sought to `to` at audio time 60 s, with `gap` s of silence there.
    fn seek(to: f64, gap: f64) -> (Radio, Vec<f32>) {
        let r = Radio::new(vec![song(0.0, "isrc:S", "S", 300.0), Play { at: 60.0, id: "isrc:S", name: "S", artist: "A", len: 300.0, from: to }]);
        let mut x = r.audio(100.0);
        x[60 * SR..60 * SR + (gap * SR as f64) as usize].fill(0.0);
        (r, x)
    }

    #[test]
    fn a_seek_moves_the_clock_quickly() {
        for (to, gap) in [(90.0, 0.05), (30.0, 0.05), (90.0, 0.3)] {
            let (mut r, x) = seek(to, gap);
            let mut l = Listener::new();
            listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
            let j = jumps(&l);
            assert_eq!(j.len(), 1, "seek to {to} (gap {gap}): {:?}", l.events);
            let (t, _, at) = j[0];
            assert!(t - 60.0 < 8.0, "seek to {to} followed {:.1} s later", t - 60.0);
            assert!((at - (to + t - 60.0 - gap)).abs() < 0.5, "seek to {to}: clock at {at:.2}");
        }
    }

    /// Shazam naming one recording by two ISRCs by turns (Over the Rainbow / What a Wonderful World, and Somewhere
    /// Over the Rainbow), at the same place.
    fn two_isrcs(p: &Play, pos: f64, n: usize) -> Option<(&'static str, &'static str, f64)> {
        Some(if n % 2 == 0 { ("isrc:USMAC0100119", "Somewhere Over the Rainbow", pos) } else { (p.id, p.name, pos) })
    }

    #[test]
    fn two_recordings_of_one_song_are_one_song() {
        let mut r = Radio::new(vec![Play { at: 0.0, id: "isrc:US84W0500395", name: "Over the Rainbow / What a Wonderful World",
                                           artist: "Israel Kamakawiwo'ole", len: 241.0, from: 0.0 }]);
        r.quirk = two_isrcs;
        let x = r.audio(200.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
        let names: Vec<_> = l.events.iter().filter(|(_, e)| matches!(e, Event::Started { .. } | Event::Changed { .. } | Event::Lost)).collect();
        assert_eq!(names.len(), 1, "{names:?}");
        assert!(jumps(&l).is_empty(), "{:?}", jumps(&l));
    }

    /// A repetitive intro: Shazam names the song every time but places it anywhere in the first 20 s, except every
    /// third answer (Dancing On My Own, live: 14.5, 11.4, 10.9, 19.3 s for 9, 11, 15, 19 s).
    fn intro(p: &Play, pos: f64, n: usize) -> Option<(&'static str, &'static str, f64)> {
        let off = [0.0, 5.3, -4.3][n % 3];
        Some((p.id, p.name, if pos < 30.0 { (pos + off).max(0.0) } else { pos }))
    }

    #[test]
    fn a_song_named_twice_is_named_wherever_it_was_placed() {
        let mut r = Radio::new(vec![song(1.0, "isrc:UK6KW1500205", "Dancing On My Own", 260.0)]);
        r.quirk = intro;
        let x = r.audio(60.0);
        let mut l = Listener::new();
        let mut named = None;
        let mut sure_at_first = None;
        listen_to(&mut l, &mut r, &x, 1.0, |l, e| if matches!(e, Event::Started { .. }) && named.is_none() { named = Some(l.t); sure_at_first = Some(l.cur.as_ref().unwrap().sure) });
        assert_eq!(sure_at_first, Some(false), "named on places that disagree: not settled yet (the lyrics wait)");
        let t = named.unwrap_or_else(|| panic!("never named: {:?}", l.events));
        assert!(t - 1.0 < 8.0, "named {:.1} s in", t - 1.0);
        assert!(!l.events.iter().any(|(_, e)| *e == Event::NotRecognised), "{:?}", l.events);
        assert_eq!(l.events.iter().filter(|(_, e)| matches!(e, Event::Started { .. } | Event::Lost)).count(), 1, "{:?}", l.events);
        // the place is settled by the checks that follow
        let c = l.cur.as_ref().unwrap();
        assert!((c.pos - 59.0).abs() < 0.5, "clock {:.2} at 59 s", c.pos);
        assert!(c.sure, "the place settles once a check fits it");
    }

    /// OnKey's rough fingerprint names a learned song from the first seconds of another (live: "Clocks" at a song
    /// boundary), then Shazam names the right one.
    fn rough_first(p: &Play, pos: f64, n: usize) -> Option<(&'static str, &'static str, f64)> {
        Some(if n == 1 { ("fp:clocks", "Clocks", 134.4) } else { (p.id, p.name, pos) })
    }

    #[test]
    fn a_rough_first_match_is_not_taken() {
        let mut r = Radio::new(vec![song(0.5, "isrc:B", "B", 200.0)]);
        r.quirk = rough_first;
        let x = r.audio(20.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
        let started: Vec<_> = l.events.iter().filter_map(|(_, e)| match e { Event::Started { id, .. } => Some(id.clone()), _ => None }).collect();
        assert_eq!(started, vec!["isrc:B".to_string()], "{:?}", l.events);
    }

    /// A recogniser that places the sound in the song's last seconds, past its end (live: a learned song "found"
    /// in another song after a long pause).
    fn past_end(p: &Play, _pos: f64, _n: usize) -> Option<(&'static str, &'static str, f64)> {
        Some((p.id, p.name, p.len + 3.0))
    }

    #[test]
    fn a_place_past_the_end_is_no_answer() {
        let mut r = Radio::new(vec![song(0.0, "isrc:A", "A", 200.0)]);
        r.quirk = past_end;
        let x = r.audio(40.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
        assert!(!l.events.iter().any(|(_, e)| matches!(e, Event::Started { .. } | Event::Over { .. })), "{:?}", l.events);
    }

    /// The catalog names another song in its last seconds (its sound starting 6 s before the end: 3-5 s of it), twice,
    /// before the right answer comes.
    fn catalog_end(p: &Play, pos: f64, n: usize) -> Option<(&'static str, &'static str, f64)> {
        if n <= 2 { Some(("cat:S", "Stay With Me", p.len - 6.0)) } else { Some((p.id, p.name, pos)) }
    }

    /// A song learned only in part: the catalog knows its first 30 s; past them it names nothing, and Shazam is held
    /// back (the music app says what plays).
    fn learned_part(p: &Play, pos: f64, _n: usize) -> Option<(&'static str, &'static str, f64)> {
        (pos < 30.0).then_some((p.id, p.name, pos))
    }

    struct AppSays(Radio);

    impl Recognizer for AppSays {
        fn identify(&mut self, x16: &[f32]) -> Option<Match> {
            self.0.identify(x16)
        }
        fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64> {
            self.0.locate_in(id, x16)
        }
        fn held(&self) -> bool {
            true
        }
    }

    fn listen_held(app: Option<Track>) -> Listener {
        let mut r = Radio::new(vec![song(0.0, "cat:T", "Scar Tissue", 217.0)]);
        r.quirk = learned_part;
        let x = r.audio(80.0);
        let mut l = Listener::new();
        l.app_track = app;
        let mut a = AppSays(r);
        for block in x.chunks(SR / 10) {
            l.feed(block);
            if let Some(req) = l.want() {
                a.0.t_end = req.t_end;
                a.0.len = req.audio.len() as f64 / SR as f64;
                let ans = work(&mut a, &req);
                l.answer(&req, ans);
            }
        }
        l
    }

    #[test]
    fn a_partly_learned_song_runs_on_while_the_music_app_says_it_plays() {
        // live: Scar Tissue, learned 15%, lost at 34 s after two checks the catalog couldn't place (Shazam held back)
        let l = listen_held(Some(Track { id: "spotify:track:st".into(), name: "Scar Tissue".into(), artist: "A".into(), ..Default::default() }));
        assert!(!l.events.iter().any(|(_, e)| *e == Event::Lost), "{:?}", l.events);
        assert!((l.cur.as_ref().unwrap().pos - 80.0).abs() < 0.5, "clock {:.1}", l.cur.as_ref().unwrap().pos);
        // the music app says another song now: it has stopped playing
        let l = listen_held(Some(Track { id: "spotify:track:other".into(), name: "Other".into(), artist: "B".into(), ..Default::default() }));
        assert!(l.events.iter().any(|(_, e)| *e == Event::Lost), "{:?}", l.events);
    }

    #[test]
    fn a_learned_song_named_in_its_last_seconds_is_not_believed() {
        let mut r = Radio::new(vec![song(0.0, "isrc:V", "Viva La Vida", 240.0)]);
        r.quirk = catalog_end;
        let x = r.audio(40.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 0.2, |_, _| {});
        assert!(!l.events.iter().any(|(_, e)| matches!(e, Event::Started { id, .. } | Event::Changed { id, .. } if id == "cat:S")), "{:?}", l.events);
        assert!(!l.events.iter().any(|(_, e)| matches!(e, Event::Over { .. } | Event::NotRecognised)), "{:?}", l.events);
        assert_eq!(l.cur.as_ref().map(|c| c.track.id.as_str()), Some("isrc:V"), "{:?}", l.events);
    }

    #[test]
    fn a_song_onkey_starts_counts_from_when_it_really_plays() {
        // the Library's Sing: Spotify says the song before for a moment, then the new one at 0:00 while it loads
        // (a second, no sound), then plays it
        let st = |id: &str, playing: bool, position: f64| State { playing, track: Track { id: id.into(), ..Default::default() }, position, source: Source::Spotify };
        let mut p = Some(Starting::new("spotify:track:dancing", 0.0));
        assert_eq!(starting_says(&mut p, &st("spotify:track:before", true, 130.2), 0.2), Start::Ignore);
        assert_eq!(starting_says(&mut p, &st("spotify:track:dancing", true, 0.0), 0.6), Start::Waiting);
        assert_eq!(starting_says(&mut p, &st("spotify:track:dancing", true, 0.05), 1.0), Start::Waiting);
        assert_eq!(starting_says(&mut p, &st("spotify:track:dancing", true, 0.3), 1.6), Start::Playing);
        assert!(p.is_none());
        assert_eq!(starting_says(&mut p, &st("spotify:track:other", true, 50.0), 2.0), Start::Playing, "nothing asked any more");
        // the same song restarted: its old place is ignored; and an app that never says believed again after 5 s
        let mut p = Some(Starting::new("spotify:track:dancing", 10.0));
        assert_eq!(starting_says(&mut p, &st("spotify:track:dancing", true, 95.0), 10.3), Start::Ignore);
        assert_eq!(starting_says(&mut p, &st("spotify:track:before", true, 140.0), 15.5), Start::Playing);
    }

    #[test]
    fn a_song_from_the_other_source_stays_the_song_followed() {
        // Spotify didn't answer once mid-song (a lone -1743): listening said what plays, by ISRC, then Spotify again.
        // One song throughout (its take carries on, no other release's cover); a new song is taken as it comes
        let t = |id: &str, name: &str, art: &str| Track { id: id.into(), name: name.into(), artist: "ABBA".into(), art: Some(art.into()), ..Default::default() };
        let spotify = t("spotify:track:6zk4", "When I Kissed The Teacher", "spotify-cover");
        let mut last = None;
        assert_eq!(as_followed(&mut last, spotify.clone()).id, spotify.id);
        let heard = as_followed(&mut last, t("isrc:SEAYD7601010", "When I Kissed the Teacher", "compilation-cover"));
        assert_eq!((heard.id.as_str(), heard.art.as_deref()), (spotify.id.as_str(), Some("spotify-cover")));
        assert_eq!(as_followed(&mut last, spotify.clone()).id, spotify.id);
        assert_eq!(as_followed(&mut last, t("spotify:track:dq", "Dancing Queen", "dq")).id, "spotify:track:dq");
    }

    #[test]
    fn same_song_is_the_same_artist_and_title_words() {
        let t = |artist: &str, name: &str| Track { name: name.into(), artist: artist.into(), ..Default::default() };
        let iz = "Israel Kamakawiwo'ole";
        assert!(same_song(&t(iz, "Over the Rainbow / What a Wonderful World"), &t(iz, "Somewhere Over the Rainbow")));
        assert!(same_song(&t("Lady Gaga", "Shallow - Radio Edit"), &t("Lady Gaga", "Shallow (feat. B)")));
        assert!(!same_song(&t("Coldplay", "Yellow"), &t("Coldplay", "The Scientist")));
        assert!(!same_song(&t("Coldplay", "Yellow"), &t("The Beatles", "Yellow Submarine")));
        assert!(!same_song(&t("", "Yellow"), &t("", "Yellow")));
    }

    #[test]
    fn another_song_without_a_gap_takes_two_answers() {
        // one song straight into another (a crossfade): followed, once two answers from separate sound agree
        let mut r = Radio::new(vec![song(0.0, "isrc:A", "A", 300.0), song(60.0, "isrc:B", "B", 300.0)]);
        let x = r.audio(100.0);
        let mut l = Listener::new();
        let mut at = None;
        listen_to(&mut l, &mut r, &x, 1.0, |l, e| if matches!(e, Event::Changed { .. } | Event::Started { .. }) && l.cur.as_ref().unwrap().track.id == "isrc:B" { at = Some((l.t, take_starts(l))) });
        let (t, from) = at.unwrap_or_else(|| panic!("{:?}", l.events));
        assert!(t - 60.0 < 20.0, "B followed {:.1} s in", t - 60.0);
        assert!(from.abs() <= 1.0 || from > 0.0, "from {from:.2}");
    }

    /// Shazam can't name a stretch of the song (a quiet bridge, say).
    fn bridge(p: &Play, pos: f64, _n: usize) -> Option<(&'static str, &'static str, f64)> {
        (!(50.0..60.0).contains(&pos)).then_some((p.id, p.name, pos))
    }

    #[test]
    fn a_song_lost_a_moment_is_found_again_with_what_was_heard() {
        // the take carries on through it (engine.rs holds it): the sound kept reaches back to where it was lost
        let mut r = Radio::new(vec![song(0.0, "isrc:S", "S", 300.0)]);
        r.quirk = bridge;
        let x = r.audio(120.0);
        let mut l = Listener::new();
        let (mut lost_at, mut found) = (None, None);
        listen_to(&mut l, &mut r, &x, 1.0, |l, e| match e {
            Event::Lost => lost_at = Some(l.t),
            Event::Started { .. } if lost_at.is_some() => found = Some((l.cur.as_ref().unwrap().pos, l.lead_in())),
            _ => {}
        });
        let lost_at = lost_at.unwrap_or_else(|| panic!("never lost: {:?}", l.events));
        let (pos, lead) = found.unwrap_or_else(|| panic!("not found again: {:?}", l.events));
        assert!(pos - lead <= lost_at, "found at {pos:.1} s with {lead:.1} s kept, lost at {lost_at:.1} s");
        assert_eq!(l.cur.as_ref().unwrap().track.id, "isrc:S");
    }

    #[test]
    fn onkeys_own_restart_moves_the_clock_at_once() {
        // Castle on the Hill, learned: OnKey's restart button at 18.3 s; Spotify gets there 0.3 s later, with no gap
        // to hear. The clock is right within 1 s from then on, and the song is never lost
        let r0 = Radio::new(vec![song(0.0, "cat:C", "Castle on the Hill", 261.0),
                                 Play { at: 18.6, id: "cat:C", name: "Castle on the Hill", artist: "A", len: 261.0, from: 0.0 }]);
        let x = r0.audio(45.0);
        let mut r = r0;
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x[..(18.3 * SR as f64) as usize], 0.2, |_, _| {});
        assert!(l.cur.is_some(), "{:?}", l.events);
        l.moved(Some(0.0));
        let mut worst: f64 = 0.0;
        for chunk in x[(18.3 * SR as f64) as usize..].chunks(SR / 2) {
            listen_to(&mut l, &mut r, chunk, 0.2, |_, _| {});
            let truth = (l.t - 18.6).max(0.0);
            let c = l.cur.as_ref().unwrap_or_else(|| panic!("lost: {:?}", l.events));
            worst = worst.max((c.pos - truth).abs());
        }
        assert!(worst < 1.0, "clock off by up to {worst:.2} s");
        assert!(!l.events.iter().any(|(_, e)| *e == Event::Lost), "{:?}", l.events);
    }

    /// One answer, from the sound just after 60 s, names another song (B, at 100 s).
    fn wrong_after_dropout(p: &Play, pos: f64, _n: usize) -> Option<(&'static str, &'static str, f64)> {
        if p.id == "isrc:A" && (59.5..61.0).contains(&pos) { Some(("isrc:B", "B", 100.0)) } else { Some((p.id, p.name, pos)) }
    }

    #[test]
    fn a_dropout_mid_song_is_no_reason_to_believe_one_answer_for_another_song() {
        // Spotify's output drops out for 50 ms mid-song (heavy CPU): the check after it hears one wrong answer
        let mut r = Radio::new(vec![song(0.0, "isrc:A", "A", 300.0)]);
        r.quirk = wrong_after_dropout;
        let mut x = r.audio(90.0);
        x[60 * SR..60 * SR + (0.05 * SR as f64) as usize].fill(0.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
        assert!(!l.events.iter().any(|(_, e)| matches!(e, Event::Changed { .. })), "{:?}", l.events);
        assert_eq!(l.cur.as_ref().unwrap().track.id, "isrc:A");
        assert!(l.notes.iter().any(|(_, n)| n.contains("isrc:B at") && n.contains("1 of 2")), "{:?}", l.notes);
    }

    #[test]
    fn another_song_from_its_start_after_a_short_gap_is_believed_at_once() {
        // a click on another song (a 0.1 s gap, shorter than a pause): it starts at 0:00, where the sound came back,
        // and the first answer that names it there is believed (as at a song's end: The Kill into Scar Tissue)
        let mut r = Radio::new(vec![song(0.0, "isrc:A", "A", 300.0), song(60.1, "isrc:B", "B", 300.0)]);
        let mut x = r.audio(80.0);
        x[60 * SR..(60.1 * SR as f64) as usize].fill(0.0);
        let mut l = Listener::new();
        let mut at = None;
        listen_to(&mut l, &mut r, &x, 1.0, |l, e| if matches!(e, Event::Changed { .. }) { at.get_or_insert(l.t); });
        let at = at.unwrap_or_else(|| panic!("{:?}", l.events));
        assert!(at - 60.1 < 5.0, "changed {:.1} s in", at - 60.1);
        assert_eq!(l.cur.as_ref().unwrap().track.id, "isrc:B");
    }

    #[test]
    fn a_song_moved_while_paused_never_goes_below_zero() {
        // Shake It Off, live: paused for 110 s, then OnKey's restart pressed (Spotify stays paused): the clock went to
        // -110.6 s, and further below with each press. Also the music app saying it moved (lag 0) while paused
        for lag in [MOVE_LAG_S, 0.0] {
            let mut r = Radio::new(vec![song(0.0, "cat:S", "Shake It Off", 219.0)]);
            let mut x = r.audio(30.0);
            x.extend(vec![0.0; 40 * SR]);      // paused for 40 s
            let mut l = Listener::new();
            listen_to(&mut l, &mut r, &x, 0.2, |_, _| {});
            assert!(l.cur.as_ref().is_some_and(|c| c.paused), "{:?}", l.events);
            for _ in 0..3 {
                l.moved_after(Some(0.0), lag);
                listen_to(&mut l, &mut r, &vec![0.0; 5 * SR], 0.2, |_, _| {});
                let c = l.cur.as_ref().unwrap();
                assert!(c.paused && c.pos.abs() < 0.05, "lag {lag}: clock {:.2} s, {:?}", c.pos, l.events);
            }
            assert!(!l.events.iter().any(|(_, e)| matches!(e, Event::Paused { pos } if *pos < -0.05)), "{:?}", l.events);
        }
    }

    #[test]
    fn a_learned_song_restarted_is_followed_back_to_its_start() {
        // We Are Young, learned, restarted in Spotify at 27 s (a 0.17 s gap): the checks after the gap wait for 3 s of
        // sound (2 s named nothing and lost it, every time), then follow it back to the start at once
        let mut r = Radio::new(vec![song(0.0, "cat:W", "We Are Young", 250.0),
                                    Play { at: 27.0, id: "cat:W", name: "We Are Young", artist: "A", len: 250.0, from: 0.4 }]);
        let mut x = r.audio(45.0);
        x[27 * SR..27 * SR + (0.17 * SR as f64) as usize].fill(0.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 0.2, |_, _| {});
        assert!(!l.events.iter().any(|(_, e)| *e == Event::Lost), "{:?}", l.events);
        let j = jumps(&l);
        assert_eq!(j.len(), 1, "{:?}", l.events);
        assert!(j[0].0 - 27.0 < 5.0, "followed {:.1} s later", j[0].0 - 27.0);
        assert!((l.cur.as_ref().unwrap().pos - (0.4 + 45.0 - 27.17)).abs() < 0.3, "clock {:.2}", l.cur.as_ref().unwrap().pos);
    }

    #[test]
    fn a_seek_without_a_gap_is_followed_once_answers_agree() {
        let (mut r, x) = seek(150.0, 0.0);
        let mut l = Listener::new();
        listen_to(&mut l, &mut r, &x, 1.0, |_, _| {});
        let j = jumps(&l);
        assert_eq!(j.len(), 1, "{:?}", l.events);
        // (slow on purpose: without a gap the clock is believed over Shazam for 20 s; a seek makes a gap)
        assert!(j[0].0 - 60.0 < 40.0, "followed {:.1} s later", j[0].0 - 60.0);
    }
}
