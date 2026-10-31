//! Naming the song from its sound, for listening mode (listen.rs): which song a few seconds of music are, and
//! where in it they start.
//!
//! Three recognisers, tried in this order:
//!   - `Catalog`: the songs OnKey has learned, by their signatures (ShazamKit custom catalog on macOS, OnKey's own
//!     landmark one, landmark.rs, elsewhere). Entirely on this computer, no account or network; exact to ~10 ms
//!     from 2 s of music.
//!   - `Fingerprints`: learned songs that have no signature yet (learned before signatures were kept), by OnKey's
//!     own rhythm fingerprint. Slower (it searches every song), so only a fallback. Also the stand-in recogniser
//!     for tests, with a delay and a miss rate.
//!   - `ShazamCatalog`: Shazam's own catalog, for songs OnKey hasn't learned. Needs the ShazamKit service on
//!     OnKey's App ID (Apple Developer account). A slow answer is waited for (the first ones after a fresh
//!     install can take half a minute); when it fails (refused, offline, or no answer for a minute) it rests and
//!     tries again later: 10 s, then 30 s, then every 2 min.
use std::ffi::c_char;
use std::time::{Duration, Instant};

use crate::dsp::{locate, onset_envelope, FP_BANDS, FRAME_S, LOCATE_MIN_FRAMES};
use crate::source::Track;
use crate::store::Store;

use crate::platform::native::*;

pub const SR: usize = 16_000;

/// Which kind of signature learned songs keep (store.rs tags each with it): ShazamKit's on macOS, OnKey's own
/// landmark ones (landmark.rs) where there's no ShazamKit. A library copied from the other system has its
/// signatures passed over, not misread, and replaced by the next play of each song.
pub const SIG_KIND: &[u8; 4] = crate::platform::signature::KIND;

/// A signature of mono 16 kHz audio (a compact fingerprint, not the audio), of this system's kind (`SIG_KIND`).
pub fn signature(x16: &[f32]) -> Option<Vec<u8>> {
    if x16.len() < SR {
        return None;
    }
    crate::platform::signature::of(x16, SR as f64)
}

/// Signatures matched on this computer: ShazamKit's custom catalog on macOS, landmark.rs's elsewhere.
pub struct SigCatalog(crate::platform::signature::Catalog);

impl SigCatalog {
    pub fn new() -> SigCatalog {
        SigCatalog(crate::platform::signature::Catalog::new())
    }

    /// Add a song's signature under `id`. False: it couldn't be read (another kind, say).
    pub fn add(&mut self, id: &str, sig: &[u8]) -> bool {
        self.0.add(id, sig)
    }

    /// The song a signature is from, and where in its signature's audio the query's starts (s).
    pub fn find(&mut self, sig: &[u8]) -> Option<(String, f64)> {
        self.0.find(sig)
    }
}

impl Default for SigCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// What a recogniser heard.
#[derive(Debug, Clone)]
pub struct Match {
    pub track: Track,
    /// song seconds where the audio given starts
    pub offset: f64,
    /// Apple Music's id for the song (Shazam's catalog), for "Open in Apple Music"
    pub apple_music_id: Option<String>,
    /// which recogniser named it
    pub by: &'static str,
}

pub trait Recognizer: Send {
    /// Which song `x16` (mono 16 kHz, a few seconds) is, and where in it it starts. None: not recognised.
    fn identify(&mut self, x16: &[f32]) -> Option<Match>;
    /// Where in a known song `x16` starts (None: not found there, or this recogniser can't tell).
    fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64>;
    /// Does `x16` fit the song at `offset` (song seconds)? A check before believing a jump: a repeated chorus
    /// can match elsewhere too, but the place the clock says still fits. None: this recogniser can't tell.
    fn fits_at(&mut self, _id: &str, _x16: &[f32], _offset: f64) -> Option<bool> {
        None
    }
    /// Seconds until this recogniser, resting after errors, tries again (None: it isn't resting). A "not
    /// recognised" while one rests means it couldn't ask, not that the sound has no name.
    fn resting(&self) -> Option<f64> {
        None
    }
    /// Requests it made are still waiting for their answer (a slow service, not one that failed): "not recognised"
    /// is no sign the sound has no name.
    fn waiting(&self) -> bool {
        false
    }
    /// It keeps failing (not just one error or a slow answer): the status line says it's unavailable.
    fn down(&self) -> bool {
        self.resting().is_some()
    }
    /// It wasn't asked because something else names the song now (the music app OnKey controls: see `Ask`):
    /// "not recognised" is no sign the sound has no name.
    fn held(&self) -> bool {
        false
    }
}

/// May Shazam be asked now (`ShazamCatalog::allowed`)?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Yes,
    /// not now: the music app OnKey controls says what plays (no sign the sound has no name; asked again once
    /// it stops saying)
    Held,
    /// switched off in Settings: songs not learned here go unnamed
    No,
}

/// Learned songs, by their signatures (`SigCatalog`), matched on this computer.
pub struct Catalog {
    cat: SigCatalog,
    /// song id → (track, song seconds where its signature starts)
    songs: Vec<(Track, f64)>,
}

impl Catalog {
    pub fn from_store(store: &Store) -> Catalog {
        let mut cat = SigCatalog::new();
        let mut songs = Vec::new();
        for t in store.learned_tracks() {
            if let Some((sig, start, _)) = store.signature(&t.id) {
                if cat.add(&t.id, &sig) {
                    songs.push((t, start));
                }
            }
        }
        Catalog { cat, songs }
    }

    pub fn len(&self) -> usize {
        self.songs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    pub fn has(&self, id: &str) -> bool {
        self.songs.iter().any(|(t, _)| t.id == id)
    }

    fn query(&mut self, x16: &[f32]) -> Option<(Track, f64)> {
        if self.songs.is_empty() {
            return None;
        }
        let sig = signature(x16)?;
        let (id, offset) = self.cat.find(&sig)?;
        let (t, start) = self.songs.iter().find(|(t, _)| t.id == id)?;
        Some((t.clone(), start + offset))
    }
}

impl Recognizer for Catalog {
    fn identify(&mut self, x16: &[f32]) -> Option<Match> {
        self.query(x16).map(|(track, offset)| Match { track, offset, apple_music_id: None, by: "catalog" })
    }
    fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64> {
        self.query(x16).filter(|(t, _)| t.id == id).map(|(_, o)| o)
    }
}

/// Learned songs by OnKey's own rhythm fingerprint (no signature needed). For tests it can be made slow and
/// forgetful: each answer is withheld with probability `miss`, and `delay_s` says how long it "takes".
pub struct Fingerprints {
    /// (track, fingerprint, may name it: only songs the catalog can't, in the app)
    songs: Vec<(Track, Vec<[f32; FP_BANDS]>, bool)>,
    pub miss: f64,
    pub delay_s: f64,
    rng: u64,
}

impl Fingerprints {
    /// Every learned song with a fingerprint (to check places in); with `name_only_without_signature`, naming a
    /// song is left to the catalog for songs that have a signature.
    pub fn from_store(store: &Store, name_only_without_signature: bool) -> Fingerprints {
        let songs = store.learned_tracks().into_iter()
            .filter_map(|t| {
                let l = store.load_song(&t.id)?;
                let name = !name_only_without_signature || store.signature(&t.id).is_none();
                (!l.onset.is_empty()).then_some((t, l.onset, name))
            }).collect();
        Fingerprints { songs, miss: 0.0, delay_s: 0.0, rng: 0x9E37_79B9_7F4A_7C15 }
    }

    pub fn len(&self) -> usize {
        self.songs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    fn cosine(env: &[[f32; FP_BANDS]], seg: &[[f32; FP_BANDS]]) -> f32 {
        let norm = |v: &[[f32; FP_BANDS]]| v.iter().flat_map(|r| r.iter()).map(|x| x * x).sum::<f32>().sqrt();
        let dot: f32 = env.iter().zip(seg).map(|(a, b)| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>()).sum();
        dot / (norm(env) * norm(seg) + 1e-9)
    }

    fn missed(&mut self) -> bool {
        // xorshift: deterministic, so a test run is repeatable
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 10_000) as f64 / 10_000.0 < self.miss
    }

    /// The best place for `env` in each song, scored by cosine similarity there (which tells the right song
    /// from the others far better than the peak-over-runner-up ratio: 0.93-0.99 against at most 0.34 on the
    /// test songs, see examples/fpcheck.rs). Returns (song index, frame, cosine) of the best song.
    fn best(&self, env: &[[f32; FP_BANDS]], only: Option<&str>) -> Option<(usize, i64, f32)> {
        // `locate` can't place less than 3 s (it says frame 0 for it): no answer. (A check's 2.5 s was "found" at
        // the song's start whenever its intro had a steady beat, and two such checks moved the clock back to 0)
        if env.len() < LOCATE_MIN_FRAMES {
            return None;
        }
        let mut best: Option<(usize, i64, f32)> = None;
        for (i, (t, onset, name)) in self.songs.iter().enumerate() {
            if only.map_or(!name, |id| id != t.id) || onset.len() < env.len() {
                continue;
            }
            let (k, _) = locate(env, onset, 0, None);
            let cos = Fingerprints::cosine(env, &onset[k as usize..k as usize + env.len()]);
            if best.map_or(true, |b| cos > b.2) {
                best = Some((i, k, cos));
            }
        }
        best
    }
}

/// How alike a stretch and its best place in a song must be (cosine of their fingerprints) to count.
const FP_COS: f32 = 0.6;

/// A check fits the clock if the stretch is found this close to where the clock says (frames of FRAME_S): 200 ms.
/// (Was 30 ms: live, a third of the checks that "didn't fit" were found within 150 ms of the clock, most of the
/// rest 150-250 ms early, each costing a full search. Kept under listen.rs's DRIFT_S, 250 ms: a clock off by enough
/// to be slewed still doesn't fit, so the drift is still measured.)
const FIT_FRAMES: i64 = 20;

impl Recognizer for Fingerprints {
    fn identify(&mut self, x16: &[f32]) -> Option<Match> {
        if self.missed() {
            return None;
        }
        let env = onset_envelope(x16);
        let (i, k, conf) = self.best(&env, None)?;
        (conf >= FP_COS).then(|| Match { track: self.songs[i].0.clone(), offset: k as f64 * FRAME_S, apple_music_id: None, by: "fingerprint" })
    }
    fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64> {
        let env = onset_envelope(x16);
        let (_, k, conf) = self.best(&env, Some(id))?;
        (conf >= FP_COS).then_some(k as f64 * FRAME_S)
    }
    fn fits_at(&mut self, id: &str, x16: &[f32], offset: f64) -> Option<bool> {
        let (_, onset, _) = self.songs.iter().find(|(t, _, _)| t.id == id)?;
        let env = onset_envelope(x16);
        let k0 = (offset / FRAME_S).round() as i64;
        // within 200 ms of where the clock says
        let best = (k0 - FIT_FRAMES..=k0 + FIT_FRAMES).filter(|k| *k >= 0 && (*k as usize) + env.len() <= onset.len())
            .map(|k| Fingerprints::cosine(&env, &onset[k as usize..k as usize + env.len()]))
            .fold(f32::NEG_INFINITY, f32::max);
        best.is_finite().then_some(best >= FP_COS)
    }
}

/// After failures in a row, how long to rest before trying again: 10 s, 30 s, then 2 min each time.
const REST: [f64; 3] = [10.0, 30.0, 120.0];

/// Shortest sound sent to Shazam, in seconds.
const SHAZAM_MIN_S: usize = 3;

/// Rests between tries of a service that failed, longer each time it fails again, until it answers. Times are
/// seconds on the service's clock (`ShazamService::now`).
#[derive(Debug, Default)]
pub struct Backoff {
    errors: u32,
    until: Option<f64>,
}

impl Backoff {
    /// May it be tried at `now`?
    pub fn ready(&self, now: f64) -> bool {
        self.until.map_or(true, |u| now >= u)
    }

    /// It failed at `now`: how long it rests (s).
    pub fn failed(&mut self, now: f64) -> f64 {
        let rest = REST[(self.errors as usize).min(REST.len() - 1)];
        self.errors += 1;
        self.until = Some(now + rest);
        rest
    }

    /// It answered (a match or not): no rest.
    pub fn ok(&mut self) {
        self.errors = 0;
        self.until = None;
    }

    /// Failures in a row.
    pub fn errors(&self) -> u32 {
        self.errors
    }

    /// Seconds of rest left at `now` (None: not resting).
    pub fn left(&self, now: f64) -> Option<f64> {
        self.until.filter(|u| *u > now).map(|u| u - now)
    }
}

/// What Shazam's catalog said to one request.
pub enum Reply {
    Named(Match),
    NoMatch,
    /// the signature was unusable (too short, say): this request's fault, not the service's
    Unusable,
    /// offline, refused (no ShazamKit service on OnKey's App ID) or another error: why
    Failed(String),
}

/// Requests to Shazam's catalog, asked without waiting and answered whenever they are (Apple's, through the Swift
/// bridge; tests script their own).
pub trait ShazamService: Send {
    /// Seconds on this service's clock (the time requests are asked and answered in).
    fn now(&self) -> f64;
    /// Ask about `x16` (mono 16 kHz): a ticket, or None if no request could be made of it.
    fn begin(&mut self, x16: &[f32]) -> Option<u64>;
    /// Wait at most `secs` for an answer to any of `tickets`: the first to come (that ticket is then done with).
    fn wait(&mut self, tickets: &[u64], secs: f64) -> Option<(u64, Reply)>;
    /// A ticket whose answer isn't wanted any more.
    fn forget(&mut self, ticket: u64);
}

/// Apple's ShazamKit (native/shazam.swift).
pub struct Apple;

fn epoch() -> Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

impl Apple {
    fn reply(r: i32, out: &[c_char]) -> Reply {
        let s = unsafe { std::ffi::CStr::from_ptr(out.as_ptr()) }.to_string_lossy().into_owned();
        match r {
            1 => Apple::named(&s).map_or(Reply::NoMatch, Reply::Named),
            0 => Reply::NoMatch,
            -3 => Reply::Failed(s),
            _ => Reply::Unusable,
        }
    }

    fn named(s: &str) -> Option<Match> {
        let v: serde_json::Value = serde_json::from_str(s).ok()?;
        let id = v["isrc"].as_str().map(|i| format!("isrc:{i}"))
            .or_else(|| v["shazamID"].as_str().map(|i| format!("shazam:{i}")))?;
        // ShazamKit doesn't say how long the song is (that needs MusicKit): Apple's public iTunes lookup does,
        // by the Apple Music id it gives (the length finds the right lyrics and measures how much is learned)
        let (duration, album) = v["appleMusicID"].as_str().and_then(itunes_lookup).unwrap_or((0.0, String::new()));
        Some(Match {
            track: Track { id, name: v["title"].as_str()?.to_string(), artist: v["artist"].as_str().unwrap_or("").to_string(),
                           album, duration, art: v["artwork"].as_str().map(String::from),
                           // ShazamKit asks apps to link a match to Apple Music
                           link: v["appleMusicURL"].as_str().and_then(crate::music::apple_music_link)
                               .or_else(|| v["appleMusicID"].as_str().map(|i| format!("https://music.apple.com/song/{i}"))) },
            offset: v["offset"].as_f64().unwrap_or(0.0),
            apple_music_id: v["appleMusicID"].as_str().map(String::from),
            by: "shazam",
        })
    }
}

impl ShazamService for Apple {
    fn now(&self) -> f64 {
        epoch().elapsed().as_secs_f64()
    }
    fn begin(&mut self, x16: &[f32]) -> Option<u64> {
        let sig = signature(x16)?;
        let t = unsafe { onkey_shz_shazam_begin(sig.as_ptr(), sig.len() as i32) };
        (t > 0).then_some(t as u64)
    }
    fn wait(&mut self, tickets: &[u64], secs: f64) -> Option<(u64, Reply)> {
        let until = self.now() + secs;
        let mut out = vec![0 as c_char; 4096];
        loop {
            for &t in tickets {
                let r = unsafe { onkey_shz_shazam_poll(t as i32, out.as_mut_ptr(), out.len() as i32) };
                if r != -6 {
                    return Some((t, Apple::reply(r, &out)));
                }
            }
            if self.now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn forget(&mut self, ticket: u64) {
        unsafe { onkey_shz_shazam_forget(ticket as i32) };
    }
}

/// Each try waits this long (s) for an answer, then hands back "not yet" (the listener asks again a moment later).
const WAIT_S: f64 = 4.0;
/// Requests kept waiting for at most (the oldest is let go of, once it's waited `REPLACE_AFTER_S`, to ask afresh).
const IN_FLIGHT: usize = 3;
const REPLACE_AFTER_S: f64 = 15.0;
/// No answer at all for this long (s): Shazam is failing (a hung connection), and rests like after an error. The
/// first answers after a fresh install can take half a minute (ShazamKit setting itself up: live, 2 requests given
/// up on at 10 s, then a 30 s rest, and the song named 67 s in), so that's no failure.
const NO_ANSWER_S: f64 = 60.0;
/// Failures in a row before the status line says Shazam is unavailable.
const DOWN_AFTER: u32 = 2;

/// Shazam's own catalog. Off with ONKEY_NO_SHAZAM, and whenever `allowed` says no (Settings' "Use Shazam to name
/// new songs" off, or the music app OnKey controls says what's playing: listen.rs `shazam_wanted`). A request isn't given up on while it may still be answered:
/// each try waits a few seconds, and the next asks again with newer sound while the earlier requests stay open, so
/// the first answer to come names the song, however slow the first ones are. After an error (offline, refused), or
/// a minute with no answer at all, it rests (see `Backoff`).
pub struct ShazamCatalog<S: ShazamService = Apple> {
    pub available: bool,
    backoff: Backoff,
    svc: S,
    /// requests not yet answered: (ticket, when asked (its sound's end), its sound's length (s)), oldest first
    open: Vec<(u64, f64, f64)>,
    /// when the first request still unanswered was asked (none answered since)
    unanswered_since: Option<f64>,
    /// how many answers came: the first is logged with how long it took
    answers: u64,
    /// where what it does is said (the app's log; tests keep their own)
    pub say: Box<dyn Fn(&str) + Send>,
    /// may Shazam be asked now? Asked before every request: anything but `Ask::Yes` means nothing is sent to Apple
    /// (the requests still open are let go of), and nothing waited for
    pub allowed: Box<dyn Fn() -> Ask + Send>,
}

impl ShazamCatalog<Apple> {
    pub fn new() -> ShazamCatalog<Apple> {
        ShazamCatalog::with(Apple)
    }
}

impl Default for ShazamCatalog<Apple> {
    fn default() -> Self {
        ShazamCatalog::new()
    }
}

impl<S: ShazamService> ShazamCatalog<S> {
    pub fn with(svc: S) -> ShazamCatalog<S> {
        ShazamCatalog { available: std::env::var("ONKEY_NO_SHAZAM").is_err(), backoff: Backoff::default(), svc,
                        open: Vec::new(), unanswered_since: None, answers: 0, say: Box::new(crate::engine::log),
                        allowed: Box::new(|| Ask::Yes) }
    }

    /// May it be asked now (see `allowed`)?
    fn on(&self) -> bool {
        self.available && (self.allowed)() == Ask::Yes
    }

    fn forget_all(&mut self) {
        for (t, ..) in std::mem::take(&mut self.open) {
            self.svc.forget(t);
        }
        self.unanswered_since = None;
    }

    fn failed(&mut self, why: &str) {
        self.forget_all();
        let rest = self.backoff.failed(self.svc.now());
        (self.say)(&format!("Shazam {why}: trying again in {rest:.0} s; learned songs are still recognised"));
    }

    fn answered(&mut self, asked: f64) {
        if self.answers == 0 {
            let waited = self.svc.now() - self.unanswered_since.unwrap_or(asked);
            (self.say)(&format!("Shazam's first answer: {waited:.1} s after it was first asked"));
        }
        self.answers += 1;
        self.backoff.ok();
        self.unanswered_since = None;
    }
}

impl<S: ShazamService> Recognizer for ShazamCatalog<S> {
    fn identify(&mut self, x16: &[f32]) -> Option<Match> {
        let now = self.svc.now();
        if !self.on() {
            self.forget_all();
            return None;
        }
        if !self.backoff.ready(now) {
            return None;
        }
        // Shazam refuses signatures under ~3 s (a check just after a resume can be shorter), and digital silence
        // names nothing: local catalog only
        if x16.len() < SHAZAM_MIN_S * SR || x16.iter().all(|v| v.abs() < 1e-6) {
            return None;
        }
        let len = x16.len() as f64 / SR as f64;
        // a new request with this sound, the older ones kept open (the oldest let go of to make room)
        if self.open.len() >= IN_FLIGHT && now - self.open[0].1 >= REPLACE_AFTER_S {
            let (t, ..) = self.open.remove(0);
            self.svc.forget(t);
        }
        let mine = if self.open.len() < IN_FLIGHT {
            let t = self.svc.begin(x16)?;
            self.open.push((t, now, len));
            // the records: each request, whether the sound sent was silent, and (below) how long its answer took
            crate::records::note("shazam", serde_json::json!({"what": "asked", "ticket": t, "audio_s": len, "rms_db": crate::records::rms_db(x16)}));
            self.unanswered_since.get_or_insert(now);
            Some(t)
        } else {
            None
        };
        let until = now + WAIT_S;
        loop {
            let left = until - self.svc.now();
            if left <= 0.0 {
                break;
            }
            let tickets: Vec<u64> = self.open.iter().map(|o| o.0).collect();
            let Some((t, reply)) = self.svc.wait(&tickets, left) else { break };
            let Some(i) = self.open.iter().position(|o| o.0 == t) else { continue };
            let (_, asked, asked_len) = self.open.remove(i);
            let code = match &reply { Reply::Named(_) => 1, Reply::NoMatch => 0, Reply::Unusable => -5, Reply::Failed(_) => -3 };
            crate::records::note("shazam", serde_json::json!({"what": "answer", "ticket": t, "code": code,
                "ms": ((self.svc.now() - asked) * 10_000.0).round() / 10.0, "audio_s": asked_len, "open": self.open.len()}));
            match reply {
                Reply::Named(mut m) => {
                    self.answered(asked);
                    self.forget_all();
                    // an earlier request's answer: its place, moved on to where this sound starts
                    m.offset += asked_len + (now - asked) - len;
                    return Some(m);
                }
                Reply::NoMatch => self.answered(asked),
                Reply::Unusable => {}
                Reply::Failed(why) => {
                    self.failed(&format!("couldn't be reached or refused ({why})"));
                    return None;
                }
            }
            if Some(t) == mine || self.open.is_empty() {
                return None;        // this sound's answer (or the last open one's): nothing named
            }
        }
        // no answer yet: the requests stay open, unless none has been answered for a long time
        let since = *self.unanswered_since.get_or_insert(now);
        let waited = self.svc.now() - since;
        crate::records::note("shazam", serde_json::json!({"what": "no answer yet", "waited_s": (waited * 10.0).round() / 10.0, "open": self.open.len()}));
        if waited >= NO_ANSWER_S {
            self.failed(&format!("didn't answer in {waited:.0} s"));
        } else {
            (self.say)(&format!("Shazam hasn't answered yet ({waited:.0} s, {} request{} open): still waiting",
                                        self.open.len(), if self.open.len() == 1 { "" } else { "s" }));
        }
        None
    }
    fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64> {
        self.identify(x16).filter(|m| m.track.id == id).map(|m| m.offset)
    }
    fn resting(&self) -> Option<f64> {
        self.backoff.left(self.svc.now()).filter(|_| self.on())
    }
    fn waiting(&self) -> bool {
        self.on() && !self.open.is_empty()
    }
    fn down(&self) -> bool {
        self.resting().is_some() && self.backoff.errors() >= DOWN_AFTER
    }
    fn held(&self) -> bool {
        self.available && (self.allowed)() == Ask::Held
    }
}

/// A song's length (s) by its Apple Music id, or the id in its Apple Music link (a lookup that failed when the song
/// was named, tried again).
pub fn song_length(link: &str) -> Option<f64> {
    let id = link.split("i=").nth(1).or_else(|| link.split("/song/").nth(1))?;
    let id: String = id.chars().take_while(|c| c.is_ascii_digit()).collect();
    itunes_lookup(&id).map(|(d, _)| d).filter(|d| *d > 0.0)
}

/// A song's length (s) and album from Apple's public iTunes lookup, by its Apple Music id (cached).
fn itunes_lookup(apple_music_id: &str) -> Option<(f64, String)> {
    static CACHE: std::sync::Mutex<Vec<(String, (f64, String))>> = std::sync::Mutex::new(Vec::new());
    if let Some((_, v)) = CACHE.lock().unwrap().iter().find(|(k, _)| k == apple_music_id) {
        return Some(v.clone());
    }
    let url = format!("https://itunes.apple.com/lookup?id={}", urlencoding::encode(apple_music_id));
    let v: serde_json::Value = ureq::get(&url).timeout(std::time::Duration::from_secs(5)).call().ok()?.into_json().ok()?;
    let r = &v["results"][0];
    let found = (r["trackTimeMillis"].as_f64()? / 1000.0, r["collectionName"].as_str().unwrap_or("").to_string());
    CACHE.lock().unwrap().push((apple_music_id.to_string(), found.clone()));
    Some(found)
}

/// The recognisers OnKey uses, in order: learned songs by signature, then by fingerprint, then Shazam.
pub struct Chain {
    pub parts: Vec<Box<dyn Recognizer>>,
    store: Option<Store>,
}

impl Chain {
    pub fn for_app(store: &Store) -> Chain {
        let parts: Vec<Box<dyn Recognizer>> = vec![Box::new(Catalog::from_store(store)),
                                                   Box::new(Fingerprints::from_store(store, true)),
                                                   Box::new(ShazamCatalog { allowed: Box::new(crate::listen::shazam_now), ..ShazamCatalog::new() })];
        Chain { parts, store: Some(store.clone()) }
    }

    pub fn of(parts: Vec<Box<dyn Recognizer>>) -> Chain {
        Chain { parts, store: None }
    }

    /// A song Shazam named that OnKey already learned under another id (from Spotify, say): use that one, so
    /// its notes and your history carry over.
    fn known_as(&self, m: Match) -> Match {
        let Some(store) = &self.store else { return m };
        if m.by != "shazam" {
            return m;
        }
        let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
        let (name, artist) = (norm(&m.track.name), norm(&m.track.artist));
        match store.learned_tracks().into_iter().find(|t| norm(&t.name) == name && norm(&t.artist) == artist) {
            Some(t) => Match { track: Track { art: t.art.clone().or(m.track.art.clone()), link: t.link.clone().or(m.track.link.clone()), ..t }, ..m },
            None => m,
        }
    }
}

impl Recognizer for Chain {
    fn identify(&mut self, x16: &[f32]) -> Option<Match> {
        let m = self.parts.iter_mut().find_map(|r| r.identify(x16))?;
        Some(self.known_as(m))
    }
    fn locate_in(&mut self, id: &str, x16: &[f32]) -> Option<f64> {
        self.parts.iter_mut().find_map(|r| r.locate_in(id, x16))
    }
    fn fits_at(&mut self, id: &str, x16: &[f32], offset: f64) -> Option<bool> {
        self.parts.iter_mut().find_map(|r| r.fits_at(id, x16, offset))
    }
    fn resting(&self) -> Option<f64> {
        self.parts.iter().find_map(|r| r.resting())
    }
    fn waiting(&self) -> bool {
        self.parts.iter().any(|r| r.waiting())
    }
    fn down(&self) -> bool {
        self.parts.iter().any(|r| r.down())
    }
    fn held(&self) -> bool {
        self.parts.iter().any(|r| r.held())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minute of noise bursts at irregular times (a song with a rhythm of its own everywhere).
    fn bursts() -> Vec<f32> {
        let mut rng = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; rng };
        let mut x = vec![0f32; 60 * SR];
        let mut at = 0usize;
        while at < x.len() {
            let len = 800 + (next() % 1600) as usize;
            for v in x[at..(at + len).min(60 * SR)].iter_mut() {
                *v = ((next() % 20_001) as f32 / 10_000.0 - 1.0) * 0.3;
            }
            at += len + 1600 + (next() % 6400) as usize;
        }
        x
    }

    #[test]
    fn a_stretch_too_short_to_place_is_not_placed_at_the_start() {
        // the song's first 2.5 s sound like its 20-22.5 s (a steady intro): a 2.5 s check from 20 s was "found" at 0
        let mut x = bursts();
        let (a, n) = (20 * SR, (2.5 * SR as f64) as usize);
        let bit = x[a..a + n].to_vec();
        x[..n].copy_from_slice(&bit);
        let t = Track { id: "fp:S".into(), name: "S".into(), ..Default::default() };
        let mut f = Fingerprints { songs: vec![(t, onset_envelope(&x), true)], miss: 0.0, delay_s: 0.0, rng: 1 };
        assert_eq!(f.locate_in("fp:S", &bit), None);
        assert!(f.identify(&bit).is_none());
        // 3.5 s (a check after a gap): placed where it is
        let o = f.locate_in("fp:S", &x[30 * SR..(33.5 * SR as f64) as usize]);
        assert!(o.is_some_and(|o| (o - 30.0).abs() < 0.05), "{o:?}");
    }

    #[test]
    fn a_stretch_fits_the_clock_within_200_ms_and_not_elsewhere() {
        // a song of noise bursts at irregular times, and its fingerprint
        let x = bursts();
        let t = Track { id: "fp:S".into(), name: "S".into(), ..Default::default() };
        let mut f = Fingerprints { songs: vec![(t, onset_envelope(&x), true)], miss: 0.0, delay_s: 0.0, rng: 1 };
        let bit = &x[20 * SR..23 * SR];
        assert_eq!(f.fits_at("fp:S", bit, 20.0), Some(true));
        // the clock 120 ms or 180 ms off (live: most checks that didn't fit were found this close): it fits
        for off in [-0.18, -0.12, 0.12, 0.18] {
            assert_eq!(f.fits_at("fp:S", bit, 20.0 + off), Some(true), "{off}");
        }
        // somewhere else: it doesn't
        for at in [21.0, 30.0, 45.0] {
            assert_eq!(f.fits_at("fp:S", bit, at), Some(false), "{at}");
        }
        assert_eq!(f.fits_at("fp:other", bit, 20.0), None);
    }

    #[test]
    fn errors_rest_longer_each_time_and_an_answer_ends_it() {
        let mut b = Backoff::default();
        assert!(b.ready(0.0) && b.left(0.0).is_none());
        assert_eq!(b.failed(0.0), 10.0);
        assert!(!b.ready(9.0));
        assert!(b.ready(10.0));
        assert_eq!(b.failed(10.0), 30.0);
        assert_eq!(b.failed(40.0), 120.0);
        assert_eq!(b.failed(160.0), 120.0);     // stays at 2 min
        assert!((b.left(220.0).unwrap() - 60.0).abs() < 1e-6);
        assert_eq!(b.errors(), 4);
        b.ok();
        assert!(b.ready(0.0) && b.left(0.0).is_none());
        assert_eq!(b.failed(0.0), 10.0);
    }

    /// A scripted Shazam: each request is answered (named "Song", at its sound's start + 100 s) after `delay` s,
    /// except those listed in `hang` (by order asked, from 0), never answered; or it fails, offline.
    struct Script {
        t: f64,
        delay: f64,
        hang: Vec<usize>,
        offline: bool,
        /// (ticket, when it's answered, its reply's offset)
        open: Vec<(u64, f64, f64)>,
        asked: usize,
    }

    impl Script {
        fn new(delay: f64, hang: Vec<usize>) -> Script {
            Script { t: 0.0, delay, hang, offline: false, open: Vec::new(), asked: 0 }
        }
    }

    impl ShazamService for Script {
        fn now(&self) -> f64 {
            self.t
        }
        fn begin(&mut self, x16: &[f32]) -> Option<u64> {
            let n = self.asked;
            self.asked += 1;
            let at = if self.hang.contains(&n) { f64::INFINITY } else { self.t + self.delay };
            // the sound asked about ends now
            self.open.push((n as u64, at, 100.0 + self.t - x16.len() as f64 / SR as f64));
            Some(n as u64)
        }
        fn wait(&mut self, tickets: &[u64], secs: f64) -> Option<(u64, Reply)> {
            if self.offline {
                self.t += 0.1;
                return tickets.first().map(|t| (*t, Reply::Failed("offline".into())));
            }
            let next = self.open.iter().filter(|o| tickets.contains(&o.0)).min_by(|a, b| a.1.total_cmp(&b.1)).copied();
            match next {
                Some((t, at, off)) if at <= self.t + secs => {
                    self.t = self.t.max(at);
                    self.open.retain(|o| o.0 != t);
                    let track = Track { id: "isrc:X".into(), name: "Song".into(), artist: String::new(), album: String::new(),
                                        duration: 200.0, art: None, link: None };
                    Some((t, Reply::Named(Match { track, offset: off, apple_music_id: None, by: "shazam" })))
                }
                _ => {
                    self.t += secs;
                    None
                }
            }
        }
        fn forget(&mut self, ticket: u64) {
            self.open.retain(|o| o.0 != ticket);
        }
    }

    fn quiet(svc: Script) -> ShazamCatalog<Script> {
        let mut s = ShazamCatalog::with(svc);
        s.say = Box::new(|_| {});
        s
    }

    fn music() -> Vec<f32> {
        (0..5 * SR).map(|i| (i as f32 * 0.07).sin() * 0.1).collect()
    }

    #[test]
    fn not_allowed_nothing_is_sent_and_nothing_waited_for() {
        // a request open (hung), then Shazam is switched off: it's let go of, and no other is sent
        let on = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut s = quiet(Script::new(1.0, vec![0]));
        s.available = true;
        let o = on.clone();
        s.allowed = Box::new(move || if o.load(std::sync::atomic::Ordering::Relaxed) { Ask::Yes } else { Ask::No });
        let x = music();
        assert!(s.identify(&x).is_none() && s.waiting());
        on.store(false, std::sync::atomic::Ordering::Relaxed);
        for _ in 0..5 {
            assert!(s.identify(&x).is_none());
            s.svc.t += 3.0;
        }
        assert_eq!(s.svc.asked, 1, "only the one asked while allowed");
        assert!(!s.waiting() && s.resting().is_none() && !s.down() && !s.held() && s.svc.open.is_empty());
        s.allowed = Box::new(|| Ask::Held);
        assert!(s.identify(&x).is_none() && s.held() && s.svc.asked == 1, "held back: not asked either");
    }

    #[test]
    fn a_slow_first_answer_is_waited_for_and_never_called_a_failure() {
        // the first two requests hang (a fresh install's ShazamKit setting itself up); the third is answered
        let mut s = quiet(Script::new(1.0, vec![0, 1]));
        s.available = true;
        let x = music();
        let mut named = None;
        for _ in 0..10 {
            if let Some(m) = s.identify(&x) {
                named = Some((s.svc.t, m));
                break;
            }
            assert!(s.waiting() && s.resting().is_none() && !s.down(), "still waiting, not resting");
            s.svc.t += 1.0;      // the listener asks again a moment later
        }
        let (t, m) = named.expect("named");
        assert!(t < 12.0, "named at {t}");
        // the place is where this sound starts (100 s on from the scripted song's clock)
        assert!((m.offset - (100.0 + t - 1.0 - 5.0)).abs() < 1e-6, "{}", m.offset);
        assert!(!s.waiting());
    }

    #[test]
    fn a_hung_request_answered_late_still_names_the_song_at_the_right_place() {
        // only the first request is ever answered, 12 s after it was asked: by then the sound has moved on
        let mut s = quiet(Script::new(12.0, (1..100).collect()));
        s.available = true;
        let x = music();
        let mut t_named = None;
        for _ in 0..20 {
            let asked = s.svc.t;
            if let Some(m) = s.identify(&x) {
                // song seconds where this call's sound starts: the first request's place, 5 s of sound, on to now
                assert!((m.offset + 5.0 - (100.0 + asked)).abs() < 1e-6, "{} at {asked}", m.offset);
                t_named = Some(s.svc.t);
                break;
            }
            s.svc.t += 1.0;
        }
        let t = t_named.expect("named");
        assert!((12.0..12.5).contains(&t), "named at {t}");
    }

    #[test]
    fn a_minute_without_any_answer_or_an_error_rests() {
        let mut s = quiet(Script::new(1.0, (0..1000).collect()));
        s.available = true;
        let x = music();
        while s.resting().is_none() {
            assert!(s.identify(&x).is_none());
            s.svc.t += 1.0;
        }
        assert!(s.svc.t >= NO_ANSWER_S && s.svc.t < NO_ANSWER_S + WAIT_S + 2.0, "{}", s.svc.t);
        assert!(!s.waiting() && !s.down(), "one failure: resting, but not called unavailable");
        // offline: errors rest 10 s, then 30 s; after the second it's unavailable
        let mut s = quiet(Script::new(1.0, vec![]));
        s.available = true;
        s.svc.offline = true;
        assert!(s.identify(&x).is_none());
        assert!((s.resting().unwrap() - 10.0).abs() < 0.5 && !s.down());
        s.svc.t += 10.0;
        assert!(s.identify(&x).is_none());
        assert!((s.resting().unwrap() - 30.0).abs() < 0.5 && s.down());
        // back online: answered, no rest
        s.svc.t += 30.0;
        s.svc.offline = false;
        assert!(s.identify(&x).is_some());
        assert!(s.resting().is_none() && !s.down());
    }

    #[test]
    fn silence_is_never_sent() {
        let mut s = quiet(Script::new(1.0, vec![]));
        s.available = true;
        assert!(s.identify(&vec![0.0; 5 * SR]).is_none());
        assert_eq!(s.svc.asked, 0);
    }
}
