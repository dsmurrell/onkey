//! One song being sung (a "take"), port of the Python engine's Take + LiveEngine. Two modes:
//!   learn  first time: a blind take. The song is learned while it plays (vocal separation in the background;
//!          afterwards its pitch line and fingerprint are kept) and you're scored after the song.
//!   sing   the song is learned: notes ahead and live scoring (unless you've chosen blind).
//! All timing runs on the audio's own clock (samples received), so a take behaves the same live and when a
//! test feeds it a recording faster than real time.
use std::sync::Arc;

use serde_json::{json, Value};

use crate::dsp::{self, onset_envelope, shape_hits, FRAME_S, TOL};
use crate::notes::Bar;
use crate::pitch::{Crepe, HOP, SR, WIN};
use crate::resample::Resampler;
use crate::source::{Source, Track};
use crate::store::Learned;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Learn,
    Sing,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self { Mode::Learn => "learn", Mode::Sing => "sing" }
    }
}

/// One move of a take's place by the music's fingerprint (`Take::moves`): song offsets before and after (s), the
/// check's confidence, the rule that allowed it, whether two checks 4 s apart agreed, whether the music matched was
/// short, and where the place came from (the music app's own position, listening's settled place).
#[derive(Clone, Debug)]
pub struct PlaceMove {
    pub take_s: f64,
    pub from: f64,
    pub to: f64,
    pub conf: f32,
    pub rule: &'static str,
    pub apart: bool,
    pub short: bool,
    pub exact_place: bool,
    pub place_sure: bool,
}

pub struct Take {
    pub track: Track,
    pub mode: Mode,
    pub blind: bool,
    /// how strictly it's scored (dsp::LEVELS): the level in Settings when the take started
    pub level: dsp::Level,
    pub lyrics: Vec<(f64, String)>,
    /// the lyrics as LRCLIB gave them, before any lining up
    pub lyrics_given: Vec<(f64, String)>,
    pub rate: f64,
    pub heard: i64,
    /// Wired output or speakers (Core Audio's figure under 100 ms; `heard` may be more after a timing check):
    /// the mic may hear the music, so the bleed guard runs.
    pub speakers: bool,
    /// The captured music is too quiet to trust now (capture::MusicLevel::quiet, set by the engine with each
    /// block): what the mic hears can't be told apart from it, so no live pitch is shown, and the time counts
    /// towards `quiet_s`.
    pub music_quiet: bool,
    /// Seconds of this take with the music too quiet: mostly so, and it isn't scored (unsung).
    pub quiet_s: f64,
    pub learned_fraction: f64,
    song: Option<Arc<Learned>>,
    /// what placement matches against: the learned song, or on a first play of a partly learned song, the
    /// part learned so far (so the new part lands in the right place)
    fp: Option<Arc<Learned>>,
    pub bars: Arc<Vec<Bar>>,
    /// one syllable per bar, when the song has syllable bars (otherwise empty)
    pub bar_text: Arc<Vec<String>>,

    /// samples received (capture rate)
    pub n: usize,
    pub peak: f32,
    pub mic: Vec<i16>,
    pub mic16: Vec<f32>,
    /// the music as captured (stereo 16-bit), only when the private development checkout keeps test audio
    /// (devtest::keep_music: always None in a build of the published source)
    pub music_kept: Option<Vec<i16>>,
    /// the mic with the song's bleed (from speakers) taken out: what your pitch is tracked from
    pub clean16: Vec<f32>,
    /// the music's loudness and bands for the Sing page's ribbon (see viz.rs)
    pub viz: crate::viz::Viz,
    canceller: Option<crate::bleed::Canceller>,
    pub ref16: Vec<f32>,
    mic_rs: Resampler,
    ref_rs: Resampler,

    // your pitch, take frames
    pub usr: Vec<f32>,
    /// every live frame before the noise gate: (midi, confidence), for the after-song score
    pub usr_raw: (Vec<f32>, Vec<f32>),
    pub usr_done: usize,
    sent_usr: usize,
    live: LivePitch,
    /// the raw mic's loudness per take frame (dB, as the after-song score measures it), for the live score's
    /// singing check (see usr_scored)
    mic_db: Vec<f32>,
    /// take frames tracked while the music was too quiet to tell from what the mic hears (never scored live)
    quiet_at: Vec<bool>,
    /// over speakers, the song has drowned the singer out (see `check_drowned`): a hint for the Sing page only
    drowned: bool,
    drowned_checked: f64,

    // placement in the song
    pub song_offset: Option<f64>,
    pub aligned: bool,
    pub provisional: bool,
    pub align_conf: f32,
    last_try: Option<(f64, f32, f64)>,
    /// recent fingerprint checks (place, confidence, take s), to find two on different music that agree
    tries: Vec<(f64, f32, f64)>,
    /// the fingerprint's moves of the place since the engine last read them, for the records (records.rs)
    pub moves: Vec<PlaceMove>,
    align_at: f64,
    pub lag: i64,
    /// stretches skipped over (take frames): not sung, so not scored
    skipped: Vec<(i64, i64)>,
    /// the song's stretches (song seconds) marked "Not the melody" (store::not_melody): its learned notes there follow
    /// a backing vocal or a harmony, so the live score leaves them out, as the after-song one does
    left_out: Vec<(f64, f64)>,
    /// the place each stretch before a jump (skip, gap, resume) had when the jump came: (take frame the stretch ends
    /// at, song offset), in order. A later correction of the place (the music app's jump landing a little off) is
    /// for the music after the jump; applied to the whole take it moved what was sung before too (live, a skip
    /// landing 0.2 s off dropped the live score of Stand By Me from 71 to 55 at once)
    before_jumps: Vec<(i64, f64)>,
    /// 16 kHz sample where the music since the last skip starts (the fingerprint only matches real music)
    audio_from: usize,
    /// the place right after the last skip, before allowing for Spotify's usual lateness (to learn it)
    skip_base: Option<f64>,
    /// resumed after a pause: re-check the place from the music once it plays again (a pause's silence can slip
    /// into the take before the pause is noticed, and the player may rebuffer a moment on resuming)
    resume_check: bool,
    lag_locked: bool,
    /// placed by the music app's own position (exact to a few tenths): the music may refine it, not move it far
    pub exact_place: bool,
    /// placed from the music app's position: only a first guess (its lead varies with the output: Bluetooth
    /// headphones put it 0.73 s off), shown at once and moved freely by the music's own fingerprint
    pub app_guess: bool,
    /// where the song is, by the music's own evidence (tracker.rs); None: the older rules in try_align
    pub tracker: Option<crate::tracker::Tracker>,
    /// placed by listening once its place was settled (shown then)
    pub place_sure: bool,
    lag_at: f64,
    /// the app the song played in (Spotify, Apple Music, YouTube; in listening mode the app heard), and the id it
    /// played as there, kept with the song so "open in your music app" goes back to it
    pub app: Source,
    pub played_as: String,
}

impl Take {
    pub fn new(track: Track, song: Option<Learned>, blind: bool, rate: f64, heard: i64) -> Take {
        let learned_fraction = song.as_ref().map_or(0.0, |s| s.fraction(track.duration));
        let ready = song.as_ref().is_some_and(|s| s.ready(track.duration));
        let mode = if ready { Mode::Sing } else { Mode::Learn };
        let partial = song.as_ref().filter(|s| !s.onset.is_empty()).map(|s| Arc::new(s.clone()));
        // the notes you see and are scored on come from the cleaned line (octave slips and stray instrument
        // notes removed); placement still uses the song's fingerprint as learned
        let song = if ready { song.map(|mut s| { s.midi = crate::notes::clean_line(&s.midi); Arc::new(s) }) } else { None };
        let fp = if ready { song.clone() } else { partial };
        let bars = Arc::new(song.as_ref().map(|s| crate::notes::NoteTracker::bars(&s.midi)).unwrap_or_default());
        let (app, played_as) = (Source::of_id(&track.id), track.id.clone());
        Take {
            app, played_as, track, mode, blind: blind || mode == Mode::Learn, level: dsp::Level::current(), lyrics: Vec::new(), lyrics_given: Vec::new(), rate, heard, speakers: heard < 10, music_quiet: false, quiet_s: 0.0, learned_fraction, song, fp, bars, bar_text: Arc::new(Vec::new()),
            n: 0, peak: 0.0, mic: Vec::new(), mic16: Vec::new(),
            music_kept: crate::devtest::keep_music().then(Vec::new), clean16: Vec::new(),
            viz: crate::viz::Viz::new(rate),
            // no bleed guard with Bluetooth headphones (latency over 100 ms): nothing reaches the mic
            canceller: (heard < 10 && crate::devtest::knob!("ONKEY_NO_BLEED_GUARD").is_none()).then(crate::bleed::Canceller::default), ref16: Vec::new(),
            mic_rs: Resampler::new(rate, SR as f64), ref_rs: Resampler::new(rate, SR as f64),
            usr: Vec::new(), usr_raw: (Vec::new(), Vec::new()), usr_done: 0, sent_usr: 0, live: LivePitch::default(), mic_db: Vec::new(), quiet_at: Vec::new(), drowned: false, drowned_checked: 0.0,
            song_offset: None, aligned: false, provisional: false, align_conf: 0.0, last_try: None, tries: Vec::new(), moves: Vec::new(),
            align_at: -1e9, lag: heard.max(5), skipped: Vec::new(), left_out: Vec::new(), before_jumps: Vec::new(), audio_from: 0, skip_base: None, resume_check: false, lag_locked: true, lag_at: 0.0, exact_place: false, app_guess: false, tracker: Some(crate::tracker::Tracker::new()), place_sure: false,
        }
    }

    pub fn seconds(&self) -> f64 {
        self.n as f64 / self.rate
    }

    /// Rough song position from Spotify's: its position now minus the audio this take has already received.
    /// Near the start of a song Spotify's position is good to ~0.3 s, so the notes show at once and the
    /// fingerprint fine-tunes them.
    pub fn place(&mut self, spotify_position: f64) {
        self.song_offset = Some(spotify_position - self.seconds());
        let t = self.seconds();
        if let Some(tr) = self.tracker.as_mut() {
            tr.guess(spotify_position - t, 0.5, t);
        }
        if spotify_position < 10.0 {
            self.aligned = true;
            self.provisional = true;
        }
    }

    /// Echo cancelling on or off from now on (it's only ever on for wired output: see Take::new).
    pub fn set_echo_cancel(&mut self, on: bool) {
        if !on {
            self.canceller = None;
        } else if self.canceller.is_none() && self.speakers && crate::devtest::knob!("ONKEY_NO_BLEED_GUARD").is_none() {
            self.canceller = Some(crate::bleed::Canceller::default());
        }
    }

    /// Whether the output is wired or speakers, by Core Audio's figure (see `speakers`), when `heard` came from
    /// a timing check: no bleed guard for headphones whatever their delay.
    pub fn set_speakers(&mut self, on: bool) {
        self.speakers = on;
        if !on {
            self.canceller = None;
        }
    }

    /// Interleaved [left, right, mic] frames from the capture.
    pub fn push(&mut self, frames: &[f32]) {
        let k = frames.len() / 3;
        let mut music = Vec::with_capacity(k);
        let mut mic = Vec::with_capacity(k);
        for f in frames.chunks_exact(3) {
            music.push(0.5 * (f[0] + f[1]));
            self.peak = self.peak.max(f[0].abs()).max(f[1].abs());
            mic.push(f[2]);
            if let Some(k) = self.music_kept.as_mut() {
                k.push((f[0].clamp(-1.0, 1.0) * 32767.0) as i16);
                k.push((f[1].clamp(-1.0, 1.0) * 32767.0) as i16);
            }
            self.mic.push((f[2].clamp(-1.0, 1.0) * 32767.0) as i16);
        }
        self.viz.push(&music);
        let (r0, m0) = (self.ref16.len(), self.mic16.len());
        self.ref_rs.process(&music, &mut self.ref16);
        // resumed: the music since it really started again is what the fingerprint check matches
        if self.resume_check && self.audio_from == usize::MAX && music.iter().any(|v| *v != 0.0) {
            self.audio_from = r0;
            self.align_at = -1e9;
        }
        self.mic_rs.process(&mic, &mut self.mic16);
        if self.music_quiet {
            self.quiet_s += k as f64 / self.rate;
        }
        match self.canceller.as_mut() {
            Some(c) => c.process(&self.ref16[r0..], &self.mic16[m0..], &mut self.clean16),
            None => self.clean16.extend_from_slice(&self.mic16[m0..]),
        }
        self.n += k;
    }

    /// The capture stopped for `seconds` while the song played on (it was restarted): that time goes in as silence
    /// left out of the score, so the take's clock stays the song's, and the place is checked again from the music
    /// after it. Unlike `skip`, the music didn't jump: no allowance for a jump landing late.
    pub fn gap(&mut self, seconds: f64) {
        let n = (seconds.max(0.0) * self.rate).round() as usize;
        if n == 0 {
            return;
        }
        let f0 = (self.seconds() / FRAME_S) as i64;
        self.end_stretch(f0);
        self.skipped.push((f0, f0 + (seconds / FRAME_S).ceil() as i64));
        let zeros = vec![0f32; 4800 * 3];
        let mut left = n;
        while left > 0 {
            let k = left.min(4800);
            self.push(&zeros[..k * 3]);
            left -= k;
        }
        self.audio_from = self.ref16.len();
        self.skip_base = None;
        self.last_try = None;
        self.tries.clear();
        self.align_at = -1e9;
    }

    /// OnKey's own skip (skip to singing): the music app was asked to jump `seconds` ahead and lands a moment late.
    pub fn skip(&mut self, seconds: f64) {
        self.jump_ahead(seconds, true);
    }

    /// The song was found further on than the take (you dragged the progress bar and the music app says where it is
    /// now, or listening found the song again after losing it a moment): filled the same way, but the place is where
    /// it was heard or reported, so no allowance for a jump landing late. (Live, I Will Always Love You moved ahead
    /// 3 s eleven times in 33 s, and each allowance put the notes another 0.2 s behind: -1.34 s by the end; in the
    /// sync suite a drag of +8 s showed 0.3 s late until the music corrected it.)
    pub fn moved_ahead(&mut self, seconds: f64) {
        self.jump_ahead(seconds, false);
    }

    /// The song jumped ahead: the skipped time becomes silence in the take, so everything after the jump keeps its
    /// place in the song. Nothing is learned or scored for silence.
    fn jump_ahead(&mut self, seconds: f64, lands_late: bool) {
        let n = (seconds * self.rate).round() as usize;
        let f0 = (self.seconds() / FRAME_S) as i64;
        self.end_stretch(f0);
        self.skipped.push((f0, f0 + (seconds / FRAME_S).ceil() as i64));
        let zeros = vec![0f32; 4800 * 3];
        let mut left = n;
        while left > 0 {
            let k = left.min(4800);
            self.push(&zeros[..k * 3]);
            left -= k;
        }
        // the app's jump lands a little late (the old audio plays on for a moment): allow for how late it
        // usually is on this Mac, learned from earlier skips, then re-check the place from the music after
        // the jump, straight away, as at the start of a song
        self.skip_base = None;
        if let Some(o) = self.song_offset.as_mut().filter(|_| lands_late) {
            self.skip_base = Some(*o);
            *o -= seek_lag(Source::of_id(&self.track.id));
        }
        let (t, guess) = (self.seconds(), self.song_offset.map(|o| (o, 1.5)));
        if let Some(tr) = self.tracker.as_mut() {
            match guess {
                Some(g) if lands_late => tr.skipped(t, g),
                _ => tr.jumped(t, guess),
            }
        }
        self.audio_from = self.ref16.len();
        if self.aligned {
            self.provisional = true;
        }
        self.last_try = None;
        self.tries.clear();
        self.align_at = -1e9;
    }

    /// The music was paused and has started again: check the place afresh from the music after the resume, as
    /// after a skip, allowing a correction of a few seconds (a pause's silence that slipped into the take).
    pub fn resumed(&mut self) {
        if !self.aligned || self.fp.is_none() {
            return;
        }
        self.end_stretch((self.seconds() / FRAME_S) as i64);
        self.resume_check = true;
        self.skip_base = None;
        self.provisional = true;
        self.audio_from = usize::MAX;        // set once the music is heard again (see push)
        let (t, guess) = (self.seconds(), self.song_offset.map(|o| (o, 2.0)));
        if let Some(tr) = self.tracker.as_mut() {
            tr.jumped(t, guess);
        }
        self.last_try = None;
        self.tries.clear();
    }

    /// Where in the song the audio just received is (seconds), once placed.
    pub fn song_time(&self) -> Option<f64> {
        self.song_offset.map(|o| o + self.seconds())
    }

    /// Do whatever the audio received so far allows: your pitch, the song's placement, your latency.
    /// Only the placement part of `step` (no pitch tracking): for replaying captured music offline
    /// (examples/trackreplay.rs).
    pub fn step_place(&mut self) {
        let t = self.seconds();
        if self.fp.is_some() && t - self.align_at >= self.align_wait() {
            self.try_align();
        }
    }

    pub fn step(&mut self, crepe: &mut Crepe) {
        self.step_mic(crepe);
        let t = self.seconds();
        if self.fp.is_some() && t - self.align_at >= self.align_wait() {
            self.try_align();
        }
        if t - self.lag_at >= 0.25 {
            self.lag_at = t;
            self.update_lag();
        }
    }

    fn step_mic(&mut self, crepe: &mut Crepe) {
        let total = self.clean16.len();
        if total < WIN {
            return;
        }
        let newest = (total - WIN / 2) / HOP;    // centre of the newest full frame, 32 ms before the end
        if newest <= self.usr_done {
            return;
        }
        let k = (newest - self.usr_done).min(60);
        let tail = &self.clean16[total.saturating_sub(SR)..];
        let Ok(fs) = crepe.frames(tail, k) else { return };
        self.usr.resize(newest - k, f32::NAN);
        self.usr_raw.0.resize(newest - k, f32::NAN);
        self.usr_raw.1.resize(newest - k, 0.0);
        self.usr_raw.0.extend(fs.iter().map(|f| f.midi));
        self.usr_raw.1.extend(fs.iter().map(|f| f.confidence));
        let from = self.mic_db.len().min(newest - k);
        self.mic_db.truncate(from);
        self.mic_db.extend((from..newest).map(|j| dsp::frame_db(&self.mic16, j)));
        self.quiet_at.resize(newest - k, true);
        self.quiet_at.extend(std::iter::repeat(self.music_quiet).take(k));
        // (music too quiet to tell from what the mic hears: no live line)
        let shown = self.live.push(&fs);
        let music_quiet = self.music_quiet;
        self.usr.extend(shown.into_iter().map(|v| if music_quiet { f32::NAN } else { v }));
        self.usr_done = newest;
    }

    fn align_wait(&self) -> f64 {
        // twice a second until the music has confirmed the place (to snap in as soon as it can), then every 3 s
        if self.tracker.as_ref().is_some_and(|t| t.confirmed()) { 3.0 } else { 0.5 }
    }

    /// Place the song from the music: the tracker's check (tracker.rs).
    fn try_align(&mut self) {
        self.track();
    }

    /// A move of the place by the music's fingerprint, kept for the records (what it was, what it became, why).
    fn note_move(&mut self, from: f64, to: f64, conf: f32, rule: &'static str, apart: bool, short: bool) {
        if self.moves.len() < 64 {
            self.moves.push(PlaceMove { take_s: self.seconds(), from, to, conf, rule, apart, short,
                                        exact_place: self.exact_place, place_sure: self.place_sure });
        }
    }

    /// One check for the tracker: the best few places the last 3-16 s of music matches, near where it's thought to
    /// be (±6 s), or anywhere in the song while nothing has been found there.
    fn track(&mut self) {
        let Some(song) = self.fp.clone() else { return };
        // (searched around the guess, not the latest place shown, so the search can't walk away)
        let Some(guess) = self.tracker.as_ref().and_then(|t| t.guess_offset().or(t.offset())).or(self.song_offset) else { return };
        if self.audio_from == usize::MAX {
            return;
        }
        let total = self.ref16.len();
        let len = (total - self.audio_from.min(total)).min(TRACK_WINDOW_S * SR);
        if len < align_samples(lock_seconds()) {
            return;
        }
        let tail = &self.ref16[total - len..];
        if tail.iter().fold(0f32, |m, v| m.max(v.abs())) < 1e-3 {
            return;
        }
        self.align_at = self.seconds();
        let env = onset_envelope(tail);
        let start = ((total - len) as f64 / HOP as f64).round() as i64;     // take frame of env[0]
        let at = start + (guess / FRAME_S).round() as i64;
        let peaks = dsp::locate_peaks(&env, &song.onset, at - 600, Some(at + 601), 4);
        let peaks: Vec<(f64, f32)> = peaks.into_iter().map(|(k, r)| ((k - start) as f64 * FRAME_S, r)).collect();
        let t = self.seconds();
        // (the song's beat, once per take)
        let beat = if self.tracker.as_ref().is_some_and(|t| !t.has_beat()) {
            self.fp.as_ref().and_then(|f| crate::beats::track(&f.onset)).map(|b| 60.0 / b.bpm as f64)
        } else {
            None
        };
        let Some(tr) = self.tracker.as_mut() else { return };
        if let Some(p) = beat {
            tr.set_beat(p);
        }
        // which candidates the song itself repeats at, seen from the place shown (an alias needs long evidence)
        if let Some(shown) = tr.offset() {
            let at = start + (shown / FRAME_S).round() as i64;
            let n = env.len().min(800);
            let aliases = peaks.iter().map(|p| p.0).filter(|o| (o - shown).abs() > 0.08)
                .filter(|o| dsp::self_similarity(&song.onset, at, n, ((o - shown) / FRAME_S).round() as i64) > ALIAS_SIMILAR)
                .collect();
            #[cfg(feature = "devtest")]
            if crate::devtest::knob!("TRACKER_TRACE").is_some() {
                let sims: Vec<(f64, f32)> = peaks.iter().map(|p| ((p.0 * 100.0).round() / 100.0, dsp::self_similarity(&song.onset, at, n, ((p.0 - shown) / FRAME_S).round() as i64))).collect();
                eprintln!("    check t {t:.1}: shown {shown:+.2} peaks {:?} selfsim {:?}", peaks.iter().map(|p| ((p.0 * 100.0).round() / 100.0, (p.1 * 100.0).round() / 100.0)).collect::<Vec<_>>(), sims);
            }
            tr.set_aliases(aliases);
        }
        let before = tr.offset();
        tr.observe(&peaks, t);
        // (each move of the shown place is kept for the take's record, with the tracker's reason)
        let moved = tr.decisions.last().filter(|d| d.t == t && before != Some(d.to)).map(|d| (d.from, d.to, d.why));
        if let Some(o) = tr.offset() {
            self.song_offset = Some(o);
            self.aligned = true;
            self.provisional = !tr.confirmed();
            self.align_conf = peaks.first().map_or(0.0, |p| p.1);
            self.resume_check = false;
        }
        // after OnKey's skip, once the music confirms where the jump landed: how late it landed, for the next skip
        if let (Some(base), Some(o)) = (self.skip_base, tr.offset().filter(|_| tr.confirmed())) {
            self.skip_base = None;
            learn_seek_lag(Source::of_id(&self.track.id), base - o);
        }
        if let Some((Some(from), to, why)) = moved {
            let conf = self.align_conf;
            self.note_move(from, to, conf, why, false, false);
        }
    }

    /// Listening's place from the sound (a broad guess for the tracker).
    pub fn track_guess(&mut self, song_pos: f64, sigma: f64) {
        let t = self.seconds();
        if let Some(tr) = self.tracker.as_mut() {
            tr.guess(song_pos - t, sigma, t);
            if self.song_offset.is_none() {
                self.song_offset = tr.offset();
            }
        }
    }

    fn k0(&self) -> i64 {
        (self.song_offset.unwrap_or(0.0) / FRAME_S).round() as i64
    }

    /// The stretch before a jump keeps the place it had (see `before_jumps`).
    fn end_stretch(&mut self, f: i64) {
        if let Some(o) = self.song_offset {
            if self.before_jumps.last().map_or(true, |l| l.0 < f) {
                self.before_jumps.push((f, o));
            }
        }
    }

    /// The take's stretches between jumps: [from, to) take frames and each one's song offset in frames (the last
    /// stretch, to the end, the place now). What a first play learns goes in at these places too (analysis.rs).
    pub fn stretches(&self) -> Vec<(i64, i64, i64)> {
        let mut out = Vec::with_capacity(self.before_jumps.len() + 1);
        let mut from = 0;
        for &(to, o) in &self.before_jumps {
            out.push((from, to, (o / FRAME_S).round() as i64));
            from = to;
        }
        out.push((from, i64::MAX, self.k0()));
        out
    }

    /// `x` (take time, `per_frame` values a frame) laid out as if the whole take had the place it has now: each
    /// stretch before a jump moved by how far its own place differs from that (`fill` where nothing lands), so
    /// what's learned from it (the singer separated for the syllable bars) lands in the song where it was heard. In
    /// the sync suite, a first play dragged +30 s at 40 s: the place after the seek allows 0.2 s for the jump landing
    /// late (seek_lag), and with it the 40 s before the seek were learned 0.2 s late as well (0% within 0.3 s of the
    /// truth, the music app's position being 0.13 s off already); at their own place, 100%.
    pub fn one_place<T: Copy>(&self, x: &[T], per_frame: usize, fill: T) -> Vec<T> {
        let parts = self.stretches();
        if parts.len() == 1 {
            return x.to_vec();
        }
        let now = self.k0();
        let at = |f: i64| (f.max(0) as i128 * per_frame as i128).min(x.len() as i128) as usize;
        let mut out = vec![fill; x.len()];
        for (lo, hi, k) in parts {
            let d = (k - now) * per_frame as i64;
            let (a, b) = (at(lo), at(hi));
            for (i, v) in x.iter().enumerate().take(b).skip(a) {
                let j = i as i64 + d;
                if j >= 0 && (j as usize) < out.len() {
                    out[j as usize] = *v;
                }
            }
        }
        out
    }

    /// The music this take captured (16 kHz), laid out at the place it has now (`one_place`): what the song's Shazam
    /// signature is made from, starting at `song_offset`. The take's own `ref16` is in take time, and a stretch
    /// before a jump had another place: a signature made from it as it is put that stretch where the last one is
    /// (The A Team, 22 Oct: a play skipped to singing at 177 s, its place moving from +0.06 to -0.87 s, made the
    /// signature that put the first 177 s of the song 0.93 s early, so listening's clock ran over a second behind).
    pub fn music_placed(&self) -> Vec<f32> {
        self.one_place(&self.ref16, HOP, 0.0)
    }

    /// The singer's pitch for take frames [a, b) (NaN where silent or unknown), once placed.
    /// One bar per sung syllable (made when the song was learned): replaces the bars from the pitch line.
    pub fn use_syllables(&mut self, syl: Vec<(i64, i64, i32, String)>) {
        self.bars = Arc::new(syl.iter().map(|b| (b.0, b.1, b.2)).collect());
        self.bar_text = Arc::new(syl.into_iter().map(|b| b.3).collect());
    }

    /// Show and score notes only where the lyrics say someone is singing (from just before each line to its
    /// end, at most 10 s, with a second of slack): separation lets instruments through in intros, solos and
    /// drops, and those notes aren't the singer's. Needs at least 4 lyric lines; returns true if it applied.
    pub fn gate_by_lyrics(&mut self) -> bool {
        let Some(song) = &self.song else { return false };
        let lines: Vec<usize> = (0..self.lyrics.len()).filter(|&i| self.lyrics[i].1.chars().any(|c| c.is_alphanumeric())).collect();
        if lines.len() < 4 {
            return false;
        }
        let n = song.midi.len();
        let mut keep = vec![false; n];
        for &i in &lines {
            let t = self.lyrics[i].0;
            let next = self.lyrics.get(i + 1).map_or(t + 10.0, |l| l.0);
            let (a, b) = (t - 1.0, next.min(t + 10.0) + 1.0);
            for k in ((a / FRAME_S).max(0.0) as usize)..((b / FRAME_S).max(0.0) as usize).min(n) {
                keep[k] = true;
            }
        }
        let mut gated = (**song).clone();
        for (k, m) in gated.midi.iter_mut().enumerate() {
            if !keep[k] {
                *m = f32::NAN;
            }
        }
        if self.bar_text.is_empty() {                     // syllable bars already follow the lyrics
            self.bars = Arc::new(crate::notes::NoteTracker::bars(&gated.midi));
        }
        self.song = Some(Arc::new(gated));
        true
    }

    /// The notes changed (gated by the lyrics): the window updates them without restarting the take.
    pub fn notes_event(&self, version: u64) -> Value {
        let s = self.song.as_ref();
        json!({"type": "notes", "version": version,
            "cache": s.map(|s| s.midi.iter().map(|v| if v.is_nan() { None } else { Some((v * 100.0).round() / 100.0) }).collect::<Vec<_>>()),
            "cache_bars": self.bars.iter().map(|b| [b.0, b.1, b.2 as i64]).collect::<Vec<_>>(), "bar_text": *self.bar_text})
    }

    /// Has this take been placed by its fingerprint (not just by Spotify's position)?
    pub fn fingerprint_placed(&self) -> bool {
        self.aligned && !self.provisional && self.align_conf > 0.0
    }

    pub fn target(&self, a: i64, b: i64) -> Vec<f32> {
        let mut out = vec![f32::NAN; (b - a).max(0) as usize];
        if let (Some(song), Some(_), true) = (&self.song, self.song_offset, self.aligned) {
            let parts = self.stretches();
            for (i, o) in out.iter_mut().enumerate() {
                if self.skipped.iter().any(|&(s, e)| (s..e).contains(&(a + i as i64))) {
                    continue;
                }
                let f = a + i as i64;
                let k0 = parts.iter().find(|p| f >= p.0 && f < p.1).map_or(0, |p| p.2);
                let k = f + k0;
                if k >= 0 && (k as usize) < song.midi.len() {
                    *o = song.midi[k as usize];
                }
            }
        }
        out
    }

    /// The bars' notes for take frames [a, b) (NaN where there's no bar): you score as well holding a bar's
    /// note as following the singer's own line through it.
    pub fn bar_target(&self, a: i64, b: i64) -> Vec<f32> {
        let mut out = vec![f32::NAN; (b - a).max(0) as usize];
        if self.song.is_none() || !self.aligned || self.song_offset.is_none() {
            return out;
        }
        for (lo, hi, k0) in self.stretches() {
            for &(s, e, m) in self.bars.iter() {
                let (s, e) = ((s - k0).max(a).max(lo), (e - k0).min(b).min(hi));
                for f in s..e {
                    if !self.skipped.iter().any(|&(x, y)| (x..y).contains(&f)) {
                        out[(f - a) as usize] = m as f32;
                    }
                }
            }
        }
        out
    }

    fn ref_known_until(&self) -> i64 {
        match (&self.song, self.song_offset) {
            (Some(s), Some(_)) => s.midi.len() as i64 - self.k0(),
            _ => 0,
        }
    }

    fn update_lag(&mut self) {
        let end = (self.usr_done as i64).min(self.ref_known_until() + self.lag);
        if end < 500 {
            return;
        }
        let a = (end - 2000).max(0);
        let usr = &self.usr[a as usize..end as usize];
        let mut best = (0usize, -1i64);
        let mut current = 0usize;
        for l in 5..=60i64 {
            if a - l < 0 {
                continue;
            }
            let r = self.target(a - l, end - l);
            let (mut u2, mut r2) = (Vec::new(), Vec::new());
            for (u, v) in usr.iter().zip(&r) {
                if !u.is_nan() && !v.is_nan() {
                    u2.push(*u);
                    r2.push(*v);
                }
            }
            let h = shape_hits(&u2, &r2, 50.0);
            if h > best.0 || best.1 < 0 {
                best = (h, l);
            }
            if l == self.lag {
                current = h;
            }
        }
        if best.1 < 0 {
            return;
        }
        // switch only on clear evidence; more is needed to move away from the starting estimate
        let need = if self.lag_locked { 1.25 } else { 1.05 };
        if best.0 >= 300 && best.0 as f64 > need * current as f64 {
            self.lag = best.1;
            self.lag_locked = true;
        }
    }

    /// Leave the song's "Not the melody" stretches (song seconds) out of this take's score.
    pub fn leave_out(&mut self, stretches: Vec<(f64, f64)>) {
        self.left_out = stretches;
    }

    /// The song's "Not the melody" stretches in this take (take frames [from, to), each part of it at its place).
    pub fn left_out(&self) -> Vec<(i64, i64)> {
        if self.left_out.is_empty() || self.song_offset.is_none() {
            return Vec::new();
        }
        crate::analysis::take_frames(&self.left_out, &self.stretches())
    }

    /// Running score over the take so far (None until there's something to score).
    pub fn running(&self) -> Option<f32> {
        let end = (self.usr_done as i64 - self.lag).min(self.ref_known_until());
        if end <= 0 || !self.aligned {
            return None;
        }
        let r = self.target(0, end);
        let usr = self.usr_scored(end);
        let mut ext = vec![f32::NAN; TOL];
        ext.extend(self.target(0, end + TOL as i64));
        let err_line = self.level.error(&ext, &usr);
        let mut ext_bar = vec![f32::NAN; TOL];
        ext_bar.extend(self.bar_target(0, end + TOL as i64));
        let err_bar = self.level.error(&ext_bar, &usr);
        let err: Vec<f32> = err_line.iter().zip(&err_bar)
            .map(|(l, b)| if !b.is_nan() && (l.is_nan() || b.abs() < l.abs()) { *b } else { *l }).collect();
        let (mut voiced, mut both, mut cr) = (0usize, 0usize, 0f64);
        let out = self.left_out();
        for i in 0..end as usize {
            if !r[i].is_nan() && !out.iter().any(|&(a, b)| (a..b).contains(&(i as i64))) {
                voiced += 1;
                if !usr[i].is_nan() {
                    both += 1;
                    cr += self.level.credit(err[i]) as f64;
                }
            }
        }
        if both == 0 {
            return None;
        }
        let acc = (cr / both as f64) as f32;
        let cov = both as f32 / voiced as f32;
        Some(dsp::score(acc, cov))
    }

    /// Your pitch for take frames [0, end) as the live score counts it (`lag` frames later in the take: you hear
    /// the song late), by the after-song score's singing check (analysis.rs) on the take so far: a confident pitch,
    /// smoothed as after the song, with the raw mic over `dsp::singing_gate`. Over speakers the song leaks into the
    /// mic and its pitch is heard too; the loudness bar is what tells you from it (live, without the check, a take
    /// with nobody singing scored 30, its final 0). Applied afresh to the whole take each time, so the live score
    /// ends where the final one does. Early on the take so far hasn't shown its quiet and loud parts (an intro is
    /// all leak), so the bar starts LEAK_MARGIN_DB higher and eases down over the first LEAK_EASE_S of sound.
    fn usr_scored(&self, end: i64) -> Vec<f32> {
        let (midi, conf) = self.usr_track();
        let (margin, ease) = leak_margin();
        let heard_s = self.mic_db.iter().filter(|d| **d > dsp::SILENT_DB).count() as f64 * FRAME_S;
        let gate = dsp::singing_gate(&self.mic_db) + margin * (1.0 - heard_s / ease).max(0.0) as f32;
        (0..end.max(0) as usize).map(|i| {
            let j = i + self.lag as usize;
            let sung = conf.get(j).is_some_and(|c| *c > 0.45) && self.mic_db.get(j).is_some_and(|d| *d > gate)
                && self.quiet_at.get(j).is_some_and(|q| !q);
            if sung { midi[j] } else { f32::NAN }
        }).collect()
    }

    pub fn learned(&self) -> Option<&Learned> {
        self.song.as_deref()
    }

    /// The event announcing this take to the window.
    pub fn song_event(&self, version: u64) -> Value {
        let mut ev = json!({"type": "song", "version": version, "track": self.track, "lyrics": self.lyrics,
            "fps": 100, "heard": self.heard, "cached": self.song.is_some(), "mode": self.mode.name(),
            "learned": self.learned_fraction, "song_offset": self.song_offset, "blind": self.blind,
            "level": self.level.name()});
        if let Some(s) = &self.song {
            ev["cache"] = json!(s.midi.iter().map(|v| if v.is_nan() { None } else { Some((v * 100.0).round() / 100.0) }).collect::<Vec<_>>());
            ev["cache_bars"] = json!(self.bars.iter().map(|b| [b.0, b.1, b.2 as i64]).collect::<Vec<_>>());
            ev["bar_text"] = json!(*self.bar_text);
        }
        // the song's beat, for the dancer (from the music fingerprint: songs learned before this have it too)
        if let Some(b) = self.fp.as_ref().and_then(|f| crate::beats::track(&f.onset)) {
            ev["beats"] = json!({"at": b.at, "downbeat": b.downbeat, "bpm": (b.bpm * 10.0).round() / 10.0,
                                 "energy": b.energy.iter().map(|e| (e * 100.0).round() / 100.0).collect::<Vec<_>>()});
        }
        ev
    }

    /// Clock, placement and your newest pitch frames.
    /// Frames of your pitch already sent to the window.
    pub fn sent_usr(&self) -> usize {
        self.sent_usr
    }

    /// A learned song whose place is still the first guess, not yet confirmed by the music (the window dims the
    /// notes and says it's lining up): from the start of a take and after each skip, restart or resume.
    pub fn locking(&self) -> bool {
        self.fp.is_some() && self.tracker.as_ref().is_some_and(|t| !t.confirmed())
    }

    /// Over speakers, whether the song is drowning the singer out: for the last DROWNED_WINDOW_S of the singer's part
    /// the mic heard a confident voice in most of it, but the singing check (the after-song score's, on the take so
    /// far) passed under a tenth of that, and under a fifth of all the take's so far. The Sing page then suggests
    /// turning the music down or headphones; nothing is scored differently. Checked once a second; once true, true
    /// for the rest of the take. Measured on 36 recorded speaker plays: Tainted Love sung softly
    /// (final 1) at 72 s, a barely sung take (17) and a music-only one at 31-32 s, 3 of the 33 others (finals 32, 41,
    /// 61) at 31-67 s; none of the canceller sims' singers.
    pub fn drowned(&mut self) -> bool {
        let t = self.seconds();
        if self.drowned || t - self.drowned_checked < 1.0 || !self.speakers || self.mode != Mode::Sing {
            return self.drowned;
        }
        self.drowned_checked = t;
        let lag = self.lag.max(0) as usize;
        let n = self.mic_db.len().min(self.usr_raw.1.len()).min(self.quiet_at.len());
        let heard_s = self.mic_db[..n].iter().filter(|d| **d > dsp::SILENT_DB).count() as f64 * FRAME_S;
        let win = (DROWNED_WINDOW_S / FRAME_S) as usize;
        if heard_s < DROWNED_AFTER_S || n < lag + win {
            return false;
        }
        let gate = dsp::singing_gate(&self.mic_db[..n]);
        let conf = dsp::nan_median_filter(&self.usr_raw.1[..n], 3);
        // the singer's part (the learned line, give or take 0.3 s), in take frames; heard `lag` frames later
        let end = n - lag;
        let near = 30usize;
        let line = self.target(0, (end + near) as i64);
        let mut edge = vec![0i32; end + 1];
        for (i, _) in line.iter().enumerate().filter(|(_, v)| !v.is_nan()) {
            edge[i.saturating_sub(near).min(end)] += 1;
            edge[(i + near + 1).min(end)] -= 1;
        }
        let part: Vec<bool> = edge[..end].iter().scan(0i32, |k, e| { *k += e; Some(*k > 0) }).collect();
        let count = |a: usize| {
            let (mut heard, mut sung_part, mut voiced, mut passed) = (0usize, 0usize, 0usize, 0usize);
            for (i, in_part) in part.iter().enumerate().skip(a) {
                let j = i + lag;
                if self.mic_db[j] <= dsp::SILENT_DB {
                    continue;
                }
                heard += 1;
                if !in_part || self.quiet_at[j] {
                    continue;
                }
                sung_part += 1;
                if conf[j] > 0.45 {
                    voiced += 1;
                    if self.mic_db[j] > gate {
                        passed += 1;
                    }
                }
            }
            (heard, sung_part, voiced, passed)
        };
        let (heard, sung_part, voiced, passed) = count(end - win);
        let (_, _, all_voiced, all_passed) = count(0);
        self.drowned = heard as f64 >= 0.8 * win as f64 && sung_part as f64 >= 0.5 * win as f64
            && voiced as f64 >= 0.4 * sung_part as f64 && (passed as f64) < 0.1 * voiced as f64
            && (all_passed as f64) < 0.2 * all_voiced as f64;
        self.drowned
    }

    pub fn update_event(&mut self, version: u64, score: Option<f32>) -> Value {
        let mut ev = json!({"type": "update", "version": version, "tap_now": self.seconds() / FRAME_S, "heard": self.heard,
            "song_offset": self.song_offset, "lag": self.lag, "score": score.map(|s| s.round()), "blind": self.blind,
            "aligned": self.aligned, "locking": self.locking()});
        // how loud you are now, in dB over the room's background (only while you're heard singing)
        if self.usr.last().is_some_and(|v| !v.is_nan()) {
            if let Some(&r) = self.live.rms_hist.last() {
                let floor = self.live.floor();
                ev["level"] = json!(((20.0 * (r / floor).max(1e-6).log10()) * 10.0).round() / 10.0);
            }
        }
        if self.usr_done > self.sent_usr {
            let vals: Vec<Option<f32>> = self.usr[self.sent_usr..self.usr_done].iter()
                .map(|v| if v.is_nan() { None } else { Some((v * 100.0).round() / 100.0) }).collect();
            ev["usr"] = json!([self.sent_usr, vals]);
            self.sent_usr = self.usr_done;
        }
        if self.drowned() {
            ev["drowned"] = json!(true);
        }
        ev
    }

    /// Your pitch for the whole take as the after-song score wants it (smoothed like a full re-track), from
    /// the frames already tracked live, so a take can be scored the moment it ends.
    /// When you were singing, as judged live (confident pitch above the background on the cleaned microphone).
    pub fn usr_voiced(&self) -> Vec<bool> {
        self.usr.iter().map(|v| !v.is_nan()).collect()
    }

    pub fn usr_track(&self) -> (Vec<f32>, Vec<f32>) {
        (dsp::nan_median_filter(&self.usr_raw.0, 5), dsp::nan_median_filter(&self.usr_raw.1, 3))
    }

    /// The bars in take frames, for scoring notes after the song.
    pub fn bars_in_take(&self) -> Vec<Bar> {
        let mut out = Vec::with_capacity(self.bars.len());
        for (lo, hi, k0) in self.stretches() {
            out.extend(self.bars.iter().map(|b| ((b.0 - k0).max(lo), (b.1 - k0).min(hi), b.2)).filter(|b| b.0 < b.1));
        }
        out
    }

    /// The singer's line for the whole take (take frames), for scoring a learned song after it.
    pub fn reference_line(&self) -> Vec<f32> {
        let n = (self.seconds() / FRAME_S) as i64;
        self.target(0, n)
    }

    /// First play: the take's rhythm fingerprint and which frames had Spotify audio.
    pub fn fingerprint(&self) -> (Vec<[f32; dsp::FP_BANDS]>, Vec<bool>) {
        let env = onset_envelope(&self.ref16);
        let n = 1 + self.ref16.len() / HOP;
        let audible = (0..n).map(|i| {
            let a = i * HOP;
            let b = (a + 2 * HOP).min(self.ref16.len());
            a < b && self.ref16[a..b].iter().any(|v| v.abs() > 1e-3)
        }).collect();
        (env, audible)
    }

    /// What lining up the lyrics needs, to run on another thread: the lyrics, placement and the music so far.
    pub fn lyrics_probe(&self) -> LyricsProbe {
        LyricsProbe { lyrics: self.lyrics.clone(), song_offset: self.song_offset, ref16: self.ref16.clone() }
    }

    /// Line the lyrics up with the singer heard so far (`vocal16`: the separated lead vocal from the take's
    /// start). Some(lyrics) when they clearly fit better shifted.
    pub fn aligned_lyrics(&self, vocal16: &[f32]) -> Option<crate::lyrics_align::Aligned> {
        self.lyrics_probe().aligned_lyrics(vocal16)
    }
}

pub struct LyricsProbe {
    lyrics: Vec<(f64, String)>,
    song_offset: Option<f64>,
    ref16: Vec<f32>,
}

impl LyricsProbe {
    pub fn aligned_lyrics(&self, vocal16: &[f32]) -> Option<crate::lyrics_align::Aligned> {
        let offset = self.song_offset?;
        let sing = crate::lyrics_align::singing(vocal16, &self.ref16);
        // take frames to song frames
        let k0 = (offset / FRAME_S).round() as i64;
        let voiced: Vec<bool> = if k0 >= 0 {
            std::iter::repeat(false).take(k0 as usize).chain(sing).collect()
        } else {
            sing.into_iter().skip((-k0) as usize).collect()
        };
        crate::lyrics_align::align(&self.lyrics, &voiced)
    }

}

/// Your pitch as shown live and scored: each CREPE frame kept when it's confident, above the room's background
/// and in a voice's range (else NaN); and a steady pure tone (a whistle), which CREPE is often unsure of.
#[derive(Default)]
pub struct LivePitch {
    /// recent frames' loudness (the last 30 s), for the noise gate
    rms_hist: Vec<f32>,
    n: usize,
    /// the last frame's pure tone (MIDI or NaN)
    tone: f32,
}

impl LivePitch {
    /// Frames judged so far.
    pub fn done(&self) -> usize {
        self.n
    }

    /// The noise gate: 3x the quietest moments of the last 30 s, capped so steady singing can't raise it.
    pub fn floor(&self) -> f32 {
        (3.0 * dsp::percentile(&self.rms_hist, 5.0)).clamp(0.002, 0.015)
    }

    /// The next frames, in order: the pitch shown for each (NaN: none).
    pub fn push(&mut self, fs: &[crate::pitch::Frame]) -> Vec<f32> {
        self.rms_hist.extend(fs.iter().map(|f| f.rms));
        if self.rms_hist.len() > 3000 {
            let cut = self.rms_hist.len() - 3000;
            self.rms_hist.drain(..cut);
        }
        let floor = self.floor();
        self.n += fs.len();
        fs.iter().map(|f| {
            let t1 = std::mem::replace(&mut self.tone, f.tone);
            // 28-96: E1 to C7, any voice
            if f.confidence >= 0.45 && f.rms >= floor && (28.0..=96.0).contains(&f.midi) {
                return f.midi;
            }
            // a pure tone held steady (two frames in a row within 30 cents): the room's background spreads over the
            // whole spectrum, so a tone holding half the energy is well above it even when quieter than the gate
            let steady = (f.tone - t1).abs() <= 0.3;
            if steady && f.rms >= 0.002 && f.tone <= 96.0 { f.tone } else { f32::NAN }
        }).collect()
    }
}

/// The live singing check's extra bar at the start of a take (dB), easing to none over the first seconds of sound
/// (see Take::usr_scored); ONKEY_LEAK=<dB>,<seconds> for experiments.
const LEAK_MARGIN_DB: f32 = 6.0;
const LEAK_EASE_S: f64 = 30.0;

fn leak_margin() -> (f32, f64) {
    let Some(v) = crate::devtest::knob!("ONKEY_LEAK") else { return (LEAK_MARGIN_DB, LEAK_EASE_S) };
    let mut it = v.split(',').map(|x| x.parse::<f64>().ok());
    (it.next().flatten().map_or(LEAK_MARGIN_DB, |m| m as f32), it.next().flatten().map_or(LEAK_EASE_S, |e| e.max(1e-3)))
}

/// The speaker hint (`Take::drowned`): the stretch of the singer's part looked at, and the sound heard before it's
/// first looked at (s).
const DROWNED_WINDOW_S: f64 = 20.0;
const DROWNED_AFTER_S: f64 = 30.0;

/// Seconds of music the first exact placement may use (ONKEY_LOCK_S overrides it, for testing).
/// The 16 kHz music a placement from `need_s` seconds waits for: at least what `locate` works from (2.94 s, the
/// 0.98 of 3 s this once allowed, gave it 295 frames of its 300, so every first try was wasted).
pub fn align_samples(need_s: f64) -> usize {
    ((need_s * SR as f64).ceil() as usize).max(dsp::LOCATE_MIN_SAMPLES)
}

/// The most music one check matches (s): long enough to tell places apart, short enough that a version playing
/// ~0.5% fast or slow doesn't smear the match (16 s smeared a 0.6% drift to correlations of 0.2).
const TRACK_WINDOW_S: usize = 8;

/// How alike the song must be to itself at a candidate's distance for the candidate to count as an alias.
const ALIAS_SIMILAR: f32 = 0.5;

/// The music a first placement waits for (3 s; ONKEY_LOCK_S for experiments).
pub fn lock_seconds() -> f64 {
    crate::devtest::knob!("ONKEY_LOCK_S").and_then(|v| v.parse().ok()).unwrap_or(3.0)
}

/// How late each app's jump usually lands on this Mac (ms): learned from each of OnKey's skips once the music has
/// confirmed where it landed (a running average, starting at 0.2 s), separately for each app; the tracker finds the
/// exact landing from the music either way. (Live, 17 Oct, Spotify's landings needed a further 0.09-0.22 s after
/// the 0.2 s allowance on 6 of 9 skips, 0.67-0.95 s on the others; skiptest's landing 0.25 s late was shown 0.05 s
/// off for the 3 s until the first check, every time.)
static SEEK_LAG_MS: crate::source::PerSource = crate::source::PerSource::new(200);

fn seek_lag(source: Source) -> f64 {
    if crate::devtest::knob!("ONKEY_NO_SEEK_LAG").is_some() {
        return 0.0;
    }
    SEEK_LAG_MS.get(source) as f64 / 1000.0
}

fn learn_seek_lag(source: Source, measured: f64) {
    // (a landing over 1 s off is a skip that landed somewhere else, not late)
    if !(0.0..1.0).contains(&measured) || cfg!(test) {
        return;
    }
    let old = SEEK_LAG_MS.get(source) as f64;
    SEEK_LAG_MS.set(source, (0.5 * old + 0.5 * measured * 1000.0).round() as i64);
}


#[cfg(test)]
mod tests {
    use super::*;

    fn take_of(frames: usize) -> Take {
        // a learned song whose line climbs by one (cent-free) step a frame, so the place matters exactly
        let song = Learned { midi: (0..frames).map(|k| (k % 1000) as f32).collect(), covered: vec![true; frames],
                             onset: vec![[0.0; dsp::FP_BANDS]; frames] };
        let track = Track { id: "t".into(), name: "t".into(), artist: String::new(), album: String::new(),
                            duration: frames as f64 * FRAME_S, art: None, link: None };
        let mut take = Take::new(track, Some(song), false, 48000.0, 3);
        take.song_offset = Some(0.0);
        take.aligned = true;
        take
    }

    #[test]
    fn a_correction_after_a_jump_leaves_the_stretch_before_it() {
        // live: a skip landed 0.2 s off; the tracker's correction moved the whole take, and the singing before the
        // skip was suddenly scored 0.2 s off (Stand By Me's live score 71 -> 55 at once)
        let (mut at0, mut at2) = (take_of(5000), take_of(5000));
        at2.song_offset = Some(0.2);
        let mut take = take_of(5000);
        take.end_stretch(1000);
        take.song_offset = Some(0.2);
        let (line, l0, l2) = (take.target(0, 2000), at0.target(0, 2000), at2.target(0, 2000));
        assert_eq!(line[..1000], l0[..1000], "before the jump: the place it had");
        assert_eq!(line[1000..], l2[1000..], "after the jump: the corrected place");
        for t in [&mut take, &mut at0, &mut at2] {
            t.bars = Arc::new(vec![(400, 450, 60), (1400, 1450, 62)]);
        }
        assert_eq!(take.bars_in_take(), vec![(400, 450, 60), (1380, 1430, 62)]);
        let bars = take.bar_target(0, 2000);
        assert!(bars[420] == 60.0 && bars[1385] == 62.0 && bars[1440].is_nan());
    }

    #[test]
    fn what_is_learned_before_a_jump_keeps_its_place() {
        // the place after a jump moved by 0.2 s (its allowance for landing late); the 10 s before it were heard at
        // the place they had, and are learned there
        let mut take = take_of(5000);
        take.end_stretch(1000);
        take.song_offset = Some(0.2);
        let x: Vec<f32> = (0..2000).map(|i| i as f32).collect();
        let out = take.one_place(&x, 1, f32::NAN);
        assert_eq!(out[..980], x[20..1000], "before the jump: moved to the place now");
        assert!(out[980..1000].iter().all(|v| v.is_nan()));
        assert_eq!(out[1000..], x[1000..], "after it: as it was");
        let vocal: Vec<f32> = (0..2000 * 160).map(|i| i as f32).collect();
        assert_eq!(take.one_place(&vocal, 160, 0.0)[..980 * 160], vocal[20 * 160..1000 * 160]);
        let plain = take_of(3000);
        assert_eq!(plain.one_place(&x, 1, f32::NAN), x, "no jump: unchanged");
    }

    #[test]
    fn the_music_a_signature_is_made_from_is_at_the_take_s_place_now() {
        // skipped at 10 s, the place then 0.93 s earlier: the music before the jump lands where it was heard
        let mut take = take_of(5000);
        take.ref16 = (0..2000 * HOP).map(|i| i as f32).collect();
        take.song_offset = Some(0.06);
        take.end_stretch(1000);
        take.song_offset = Some(-0.87);
        let m = take.music_placed();
        assert_eq!(m.len(), take.ref16.len());
        // song time s is at sample (s - song_offset) * SR in the signature's audio, before the jump and after it
        let at = |s: f64| ((s + 0.87) * SR as f64).round() as usize;
        let heard = |s: f64| take.ref16[((s - 0.06) * SR as f64).round() as usize];
        assert_eq!(m[at(5.0)], heard(5.0), "before the jump: at the place it was heard");
        assert_eq!(m[at(15.0)], take.ref16[((15.0 + 0.87) * SR as f64).round() as usize], "after it: as it was");
        let plain = take_of(3000);
        assert!(plain.music_placed().is_empty() && plain.ref16.is_empty());
    }

    #[test]
    fn a_singer_drowned_by_the_speakers_is_told() {
        // over speakers, 60 s of the singer's part with a confident voice in the mic all through: the song's leak
        // (-30 dB) with you under it, never louder (drowned), or you over it half the time (heard)
        let n = 6000;
        let drowned = |sung_db: f32| {
            let mut take = take_of(n + 100);
            take.lag = 0;
            take.n = (n as f64 * FRAME_S * take.rate) as usize;
            take.usr_raw = ((0..n).map(|k| (k % 1000) as f32).collect(), vec![0.9; n]);
            take.quiet_at = vec![false; n];
            take.mic_db = (0..n).map(|k| if k % 2 == 0 { sung_db } else { -30.0 }).collect();
            take.drowned()
        };
        assert!(drowned(-31.0), "nothing passes the singing check");
        assert!(!drowned(-15.0), "half of it passes");
        // headphones: never
        let mut take = take_of(n + 100);
        take.set_speakers(false);
        take.n = (n as f64 * FRAME_S * take.rate) as usize;
        take.usr_raw = ((0..n).map(|k| (k % 1000) as f32).collect(), vec![0.9; n]);
        take.quiet_at = vec![false; n];
        take.mic_db = vec![-30.0; n];
        assert!(!take.drowned());
    }

    #[test]
    fn the_song_leaking_into_the_mic_isnt_scored_live() {
        // over speakers the mic hears the singer's own pitch from the song (the leak, at -40 dB all through); you
        // sing over it (-20 dB) in the second half only: the live score counts what the after-song score would
        let n = 8000;
        let mut take = take_of(n + 100);
        take.lag = 0;
        take.usr_raw = ((0..n).map(|k| (k % 1000) as f32).collect(), vec![0.9; n]);
        take.usr_done = n;
        take.quiet_at = vec![false; n];
        take.mic_db = (0..n).map(|k| if k >= n / 2 && k % 2 == 0 { -20.0 } else { -40.0 }).collect();
        let usr = take.usr_scored(n as i64);
        assert!(usr[..n / 2].iter().all(|v| v.is_nan()), "the leak alone");
        let sung = usr[n / 2..].iter().filter(|v| !v.is_nan()).count();
        assert!((sung as i64 - n as i64 / 4).abs() < 10, "{sung}");
        // the start of a take: the bar is higher until 30 s of sound are heard (an intro is all leak)
        take.mic_db = (0..n).map(|k| if k % 2 == 0 { -28.0 } else { -40.0 }).collect();
        take.mic_db.truncate(1000);
        assert!(take.usr_scored(1000).iter().all(|v| v.is_nan()), "10 s in, 12 dB over the leak isn't enough yet");
        take.mic_db = (0..n).map(|k| if k % 2 == 0 { -28.0 } else { -40.0 }).collect();
        assert!(take.usr_scored(n as i64).iter().filter(|v| !v.is_nan()).count() > n / 3, "after 30 s it is");
    }

    #[test]
    fn a_stretch_left_out_isnt_scored_live() {
        // sung right for 30 s, then 30 s on other notes (the learned notes there a backing vocal's)
        let n = 6000;
        let song = Learned { midi: vec![60.0; n + 100], covered: vec![true; n + 100], onset: vec![[0.0; dsp::FP_BANDS]; n + 100] };
        let track = Track { id: "t".into(), name: "t".into(), duration: (n + 100) as f64 * FRAME_S, ..Default::default() };
        let mut take = Take::new(track, Some(song), false, 48000.0, 3);
        take.song_offset = Some(0.0);
        take.aligned = true;
        take.set_speakers(false);
        take.lag = 0;
        take.usr_raw = ((0..n).map(|k| if k < n / 2 { 60.0 } else { 63.0 }).collect(), vec![0.9; n]);
        take.usr_done = n;
        take.quiet_at = vec![false; n];
        take.mic_db = (0..n).map(|k| if k % 2 == 0 { -20.0 } else { -40.0 }).collect();
        let all = take.running().unwrap();
        assert!(all < 60.0, "{all}");
        // the second half left out: as if it were all the first
        take.leave_out(vec![(30.0, 61.0)]);
        assert_eq!(take.left_out(), vec![(3000, 6100)]);
        let kept = take.running().unwrap();
        assert!(kept > 95.0, "{kept}");
        // the take 10 s into the song: the stretch lands 10 s earlier in it
        take.song_offset = Some(10.0);
        assert_eq!(take.left_out(), vec![(2000, 5100)]);
    }

    #[test]
    fn without_a_jump_the_whole_take_moves_with_the_place() {
        let mut take = take_of(3000);
        take.song_offset = Some(0.3);
        take.bars = Arc::new(vec![(400, 450, 60)]);
        assert_eq!(take.bars_in_take(), vec![(370, 420, 60)]);
    }
}
