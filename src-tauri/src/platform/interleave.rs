//! Music and mic into the engine's [music L, music R, mic] frames on Windows (windows/audio.rs `pump`), paired by
//! when each was captured. They come from two WASAPI clients on two devices, each with its own clock, delivered
//! at different times; macOS's aggregate device lines them up (one clock, the mic's, the tap drift-compensated) and
//! Windows has nothing like it. WASAPI stamps every packet with the time of its first frame on the PC's performance
//! counter (`IAudioCaptureClient::GetBuffer`'s QPC position), so each stream (a `Track`) keeps its frames with their
//! times, and every 10 ms the frames due at one output time are taken from both: the output runs on the performance
//! counter at the nominal rate, a fixed delay behind now (so a packet a little late is still there when it's due,
//! and the delay is raised if one isn't), and each stream is read at the index its own clock says is that time.
//!
//! A stream's clock drifting against the counter (tens of ppm, about a frame a second) shows as the index that's
//! due creeping away from the read cursor; when the smoothed difference passes a frame, one frame is skipped or
//! repeated. A packet whose time jumps (a glitch, the app's stream ending and starting again, a device going away
//! and back) has the gap filled with silence or the overlap dropped by time, not by how much is queued; a jump of
//! more than a second starts that stream's clock afresh. A device that stamps nothing (0, or TIMESTAMP_ERROR) has
//! its packets timed by their arrival and how much was queued behind them, with looser thresholds.
//!
//! Plain arithmetic, so tested on macOS too (the Windows side is in windows/audio.rs).
use std::collections::VecDeque;

/// Ticks a second of the times here: 100 ns, as WASAPI's QPC positions.
pub const TICKS: f64 = 1e7;

/// One packet from a capture client.
#[derive(Clone, Copy, Debug)]
pub struct Packet<'a> {
    /// when its first frame was captured (QPC ticks); None when the device gave none (0, or flagged it wrong)
    pub time: Option<i64>,
    /// when it was read (QPC ticks)
    pub read_at: i64,
    /// frames still queued behind it when it was read (the client's padding less this packet): with `time`
    /// missing, its first frame is taken as captured `(frames + behind)` frames before `read_at`
    pub behind: usize,
    /// AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY: frames were lost before this one
    pub discontinuity: bool,
    /// interleaved, the track's channels a frame
    pub samples: &'a [f32],
}

/// What happened to one stream, for the log.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TrackStats {
    pub packets: u64,
    /// the device gave no time for the packet (counted; the stream is timed by arrival when its first had none)
    pub untimed: u64,
    /// time jumped forward: silence filled in (frames)
    pub gaps: u64,
    pub gap_frames: u64,
    /// time jumped back: the overlap dropped (frames)
    pub overlaps: u64,
    pub overlap_frames: u64,
    /// the clock started afresh: a jump of more than `RESET_S`
    pub restarts: u64,
    /// the read cursor jumped to where the clock said (beyond a small correction)
    pub resyncs: u64,
    /// drift corrections: a frame skipped (the stream's clock fast against the counter) or repeated (slow)
    pub skipped: u64,
    pub repeated: u64,
    /// frames that arrived after the output had passed their time (given as silence; the delay is raised)
    pub late_frames: u64,
}

/// One stream's clock: its absolute frame index against the performance counter, through its first packet since
/// the last restart and its latest, the offset smoothed over packets so one packet's jitter doesn't move it.
#[derive(Debug, Clone)]
struct Clock {
    idx0: i64,
    t0: f64,
    idx_last: i64,
    t_last: f64,
    /// smoothed residual of the packets' times from the line (ticks)
    off: f64,
    /// the rate is measured from the stamps (not when they're arrival times: those jitter far too much for it)
    measure: bool,
}

impl Clock {
    fn new(idx: i64, t: f64, measure: bool) -> Clock {
        Clock { idx0: idx, t0: t, idx_last: idx, t_last: t, off: 0.0, measure }
    }

    /// Ticks a frame of this stream, from its own baseline when there's `MIN_BASELINE_S` of it, else nominal.
    fn period(&self, nominal: f64) -> f64 {
        let span = self.t_last - self.t0;
        if self.measure && span >= MIN_BASELINE_S * TICKS && self.idx_last > self.idx0 {
            let p = span / (self.idx_last - self.idx0) as f64;
            // (a baseline saying the device runs more than 0.5% off is a stamp gone wrong, not a clock)
            if (p / nominal - 1.0).abs() < 5e-3 {
                return p;
            }
        }
        nominal
    }

    fn time(&self, idx: f64, nominal: f64) -> f64 {
        self.t0 + self.off + (idx - self.idx0 as f64) * self.period(nominal)
    }

    fn index(&self, t: f64, nominal: f64) -> f64 {
        self.idx0 as f64 + (t - self.t0 - self.off) / self.period(nominal)
    }

    /// The stream's rate against the counter, ppm (positive: it runs fast); None until there's a baseline.
    fn ppm(&self, nominal: f64) -> Option<f64> {
        let p = self.period(nominal);
        (p != nominal).then(|| (nominal / p - 1.0) * 1e6)
    }

    /// Still on the nominal rate: the smoothing follows faster, so a stream well off nominal isn't placed late
    /// (smoothing lags a steady drift by its time constant).
    fn settling(&self, nominal: f64) -> bool {
        self.measure && self.period(nominal) == nominal
    }
}

/// A jump of more than this starts the clock afresh (everything buffered is older than any delay, so dropped).
const RESET_S: f64 = 1.0;
/// How much of a stream's own timing is needed before its rate is taken from it rather than nominal (stamps a few
/// microseconds off give the rate to a few ppm over this; a wrong rate only tells over the delay, 50 ms or so).
const MIN_BASELINE_S: f64 = 0.5;

/// One stream's frames with their times, and the cursor the output reads it at.
pub struct Track {
    ch: usize,
    /// ticks a frame at the nominal rate
    period: f64,
    buf: VecDeque<f32>,
    /// absolute index of `buf`'s first frame (indices only ever grow, across restarts too)
    head: i64,
    clock: Option<Clock>,
    /// timed by arrival: its first packet (since a restart) had no time from the device
    arrival: bool,
    /// the frame to give next
    cursor: Option<i64>,
    /// smoothed time between a packet's capture and its reading (ticks): how late this stream is delivered
    lag: Option<f64>,
    pub stats: TrackStats,
}

impl Track {
    fn new(ch: usize, rate: f64) -> Track {
        Track { ch, period: TICKS / rate, buf: VecDeque::new(), head: 0, clock: None, arrival: false, cursor: None, lag: None,
                stats: TrackStats::default() }
    }

    fn end(&self) -> i64 {
        self.head + (self.buf.len() / self.ch) as i64
    }

    /// Thresholds for this stream: (a jump in time that's a gap or overlap rather than jitter, in ticks; the smoothed
    /// cursor error that's corrected by a frame, in frames; how fast the clock's smoothing follows). Arrival times jitter by
    /// the poll and the engine's period (10–20 ms), device times by microseconds.
    fn tolerances(&self) -> (f64, f64, f64) {
        if self.arrival {
            (0.06 * TICKS, 0.001 * TICKS / self.period, 0.02)
        } else {
            let settling = self.clock.as_ref().is_some_and(|c| c.settling(self.period));
            (0.002 * TICKS, 0.5, if settling { 0.3 } else { 0.1 })
        }
    }

    /// A packet onto the end of the stream, placed by its time. Returns the time of its first frame given, and
    /// whether the output had already passed it (frames lost, so the caller can hold the output back more); None
    /// when nothing of it was kept.
    fn push(&mut self, p: Packet) -> Option<(f64, bool)> {
        let n = (p.samples.len() / self.ch) as i64;
        if n == 0 {
            return None;
        }
        self.stats.packets += 1;
        self.stats.untimed += p.time.is_none() as u64;
        let nominal = self.period;
        let arrival_time = p.read_at as f64 - (n + p.behind as i64) as f64 * nominal;
        let (jump_tol, _, k) = self.tolerances();
        let end = self.end();
        let Some(mut c) = self.clock.take() else {
            // the stream's first packet: its clock starts here
            self.arrival = p.time.is_none();
            let t = p.time.map_or(arrival_time, |t| t as f64);
            self.clock = Some(Clock::new(end, t, !self.arrival));
            self.lag = Some(p.read_at as f64 - t);
            self.buf.extend(p.samples.iter().copied());
            return Some((t, self.lost(end)));
        };
        let t = match (p.time, self.arrival) {
            (Some(t), _) => t as f64,
            (None, true) => arrival_time,
            // a device that times its packets, missing one: where the clock predicts it, nothing learned from it
            (None, false) => {
                let t = c.time(end as f64, nominal);
                self.clock = Some(c);
                self.buf.extend(p.samples.iter().copied());
                return Some((t, self.lost(end)));
            }
        };
        let lag = p.read_at as f64 - t;
        self.lag = Some(self.lag.map_or(lag, |l| l + k * (lag - l)));
        let p_before = c.period(nominal);
        let residual = t - c.time(end as f64, nominal);
        let fill = (residual / p_before).round() as i64;
        let mut samples = p.samples;
        let (mut first, mut first_t) = (end, t);
        if !p.discontinuity && residual.abs() <= jump_tol {
            // where expected, give or take jitter: the clock follows it a little
            c.off += k * residual;
            c.idx_last = end;
            c.t_last = t;
            // (the rate this packet refines: the line is kept through this packet's place, so a refinement never
            // moves what's due)
            c.off += (end - c.idx0) as f64 * (p_before - c.period(nominal));
        } else if residual.abs() > RESET_S * TICKS {
            // long after (the app played nothing for a while; a device came back) or long before (stamps from
            // another stream): a fresh clock, and what's buffered is older than any delay
            self.stats.restarts += 1;
            self.head = end;
            self.buf.clear();
            c = Clock::new(end, t, !self.arrival);
            self.cursor = None;
        } else if fill >= 0 {
            // frames lost (a glitch, or the device said so): silence in their place
            self.stats.gaps += 1;
            self.stats.gap_frames += fill as u64;
            self.buf.extend(std::iter::repeat(0.0).take(fill as usize * self.ch));
            first = self.end();
            c.idx_last = first;
            c.t_last = t;
            c.off = t - c.t0 - (first - c.idx0) as f64 * c.period(nominal);
        } else {
            // frames for a time already buffered (a stream re-sending as it restarts): those dropped
            let drop = (-fill).min(n);
            self.stats.overlaps += 1;
            self.stats.overlap_frames += drop as u64;
            samples = &samples[drop as usize * self.ch..];
            if drop < n {
                first_t = t + drop as f64 * p_before;
                c.idx_last = end;
                c.t_last = first_t;
                c.off = first_t - c.t0 - (end - c.idx0) as f64 * c.period(nominal);
            }
        }
        self.clock = Some(c);
        if samples.is_empty() {
            return None;
        }
        self.buf.extend(samples.iter().copied());
        Some((first_t, self.lost(first)))
    }

    /// Whether the cursor had already gone past a packet placed at `first` (those frames are lost: counted).
    fn lost(&mut self, first: i64) -> bool {
        let Some(r) = self.cursor else { return false };
        if r <= first {
            return false;
        }
        self.stats.late_frames += (r - first).min(self.end() - first).max(0) as u64;
        true
    }

    /// `n` frames of this stream for output times `t`, `t + period`…, onto `out` (silence where it has none).
    fn read(&mut self, t: f64, n: usize, out: &mut Vec<f32>) {
        out.clear();
        let Some(c) = &self.clock else {
            out.resize(n * self.ch, 0.0);
            return;
        };
        let due = c.index(t, self.period);
        let (jump_tol, step_tol, _) = self.tolerances();
        let jump_frames = jump_tol / self.period;
        let r = match self.cursor {
            Some(r) if (due - r as f64).abs() <= jump_frames => r,
            was => {
                self.stats.resyncs += was.is_some() as u64;
                due.round() as i64
            }
        };
        // the stream's clock against the counter: the cursor stays the nearest frame to the one due (the clock
        // smooths the stamps' jitter, so no smoothing here: it would lag a steady drift); a frame off, one is
        // skipped or repeated
        let e = due - r as f64;
        let skip: i64 = if e > step_tol {
            self.stats.skipped += 1;
            1
        } else if e < -step_tol {
            self.stats.repeated += 1;
            -1
        } else {
            0
        };
        let (head, end) = (self.head, self.end());
        for i in 0..n as i64 {
            // skip: the output misses frame r; repeat: frame r is given twice
            let idx = match skip {
                1 => r + i + 1,
                -1 => r + (i - 1).max(0),
                _ => r + i,
            };
            if idx >= head && idx < end {
                let at = (idx - head) as usize * self.ch;
                out.extend(self.buf.range(at..at + self.ch));
            } else {
                out.extend(std::iter::repeat(0.0).take(self.ch));
            }
        }
        let next = r + n as i64 + skip;
        self.cursor = Some(next);
        let gone = (next - self.head).clamp(0, (self.buf.len() / self.ch) as i64);
        self.buf.drain(..gone as usize * self.ch);
        self.head += gone;
    }

    fn ppm(&self) -> Option<f64> {
        if self.arrival {
            return None;
        }
        self.clock.as_ref().and_then(|c| c.ppm(self.period))
    }

    /// "music: 2 gaps (0.3 s filled), 1 overlap (4 ms dropped), 1 restart, 0 resyncs, 3 ms late; arrives 14 ms
    /// after capture, −12 ppm" (the drift corrections are in the summary's own words).
    fn describe(&self, name: &str) -> String {
        let s = &self.stats;
        if s.packets == 0 {
            return format!("{name}: nothing came");
        }
        let ms = |frames: u64| frames as f64 * self.period / TICKS * 1000.0;
        let ppm = match self.ppm() {
            Some(p) => format!("{p:+.0} ppm"),
            None if self.arrival => "drift unmeasured".into(),
            None => "drift not yet measured".into(),
        };
        format!("{name}: {} gaps ({:.0} ms filled), {} overlaps ({:.0} ms dropped), {} restarts, {} resyncs, {:.0} ms late{}; arrives {:.0} ms after capture, {ppm}{}",
                s.gaps, ms(s.gap_frames), s.overlaps, ms(s.overlap_frames), s.restarts, s.resyncs, ms(s.late_frames),
                if s.untimed > 0 { format!(", {} packets untimed", s.untimed) } else { String::new() },
                self.lag.unwrap_or(0.0) / TICKS * 1000.0,
                if self.arrival { " (timed by arrival: the device gave no times)" } else { "" })
    }
}

/// Music and mic paired by capture time into [music L, music R, mic] frames.
pub struct Aligner {
    /// ticks a frame
    period: f64,
    pub music: Track,
    pub mic: Track,
    /// the output time of the next frame given (ticks); set at the first `take`
    out_t: Option<f64>,
    /// how far behind now the output runs (ticks): a packet is still there when it's due
    delay: f64,
    pub delay_raised: u32,
    /// times the output skipped ahead (`SKIP_S`)
    pub skipped_ahead: u32,
    /// frames given
    pub frames: u64,
    m: Vec<f32>,
    v: Vec<f32>,
}

impl Aligner {
    /// The output's delay behind now to begin with (ms): WASAPI delivers a packet up to a period (10 ms) after its
    /// capture, polled every 10 ms on a timer that ticks about every 15 ms.
    pub const DELAY_MS: f64 = 50.0;
    /// The most it's raised to when packets still come late.
    pub const MAX_DELAY_MS: f64 = 300.0;
    /// At most this much output in one `take` (a stalled thread catching up): the rest next time.
    const BURST_S: f64 = 2.0;
    /// Behind now by more than this (the PC asleep), the output skips ahead instead of catching up.
    const SKIP_S: f64 = 5.0;

    pub fn new(rate: u32) -> Aligner {
        let rate = rate as f64;
        Aligner { period: TICKS / rate, music: Track::new(2, rate), mic: Track::new(1, rate), out_t: None,
                  delay: Aligner::DELAY_MS / 1000.0 * TICKS, delay_raised: 0, skipped_ahead: 0, frames: 0, m: Vec::new(), v: Vec::new() }
    }

    pub fn push_music(&mut self, p: Packet) {
        if let Some((t, lost)) = self.music.push(p) {
            self.hold_back(p.read_at, t, lost);
        }
    }

    pub fn push_mic(&mut self, p: Packet) {
        if let Some((t, lost)) = self.mic.push(p) {
            self.hold_back(p.read_at, t, lost);
        }
    }

    /// A packet for time `t` read at `read_at` came after the output had passed it (frames lost): run further
    /// behind now, by how late it was against the delay now (and a 10 ms margin), up to `MAX_DELAY_MS`. The output
    /// pauses for the difference. (Lost frames alone decide: a stalled thread catching up reads old packets too,
    /// but passed nothing while it stalled.)
    fn hold_back(&mut self, read_at: i64, t: f64, lost: bool) {
        if !lost {
            return;
        }
        let late = read_at as f64 - self.delay - t;
        let want = (self.delay + late + 0.01 * TICKS).min(Aligner::MAX_DELAY_MS / 1000.0 * TICKS);
        if want > self.delay {
            self.delay = want;
            self.delay_raised += 1;
        }
    }

    pub fn delay_ms(&self) -> f64 {
        self.delay / TICKS * 1000.0
    }

    /// The frames due by `now` (QPC ticks) onto `out`: how many.
    pub fn take(&mut self, now: i64, out: &mut Vec<f32>) -> usize {
        let release = now as f64 - self.delay;
        let mut t = *self.out_t.get_or_insert(release);
        if release - t > Aligner::SKIP_S * TICKS {
            // the PC slept, or nothing ran for a long while: that time is skipped, not given as a long burst
            // (the engine's clock loses it, as macOS's does over sleep; the engine starts capture afresh on waking)
            self.skipped_ahead += 1;
            t = release;
            self.out_t = Some(t);
        }
        let n = ((release - t) / self.period).floor();
        if n < 1.0 {
            return 0;
        }
        let n = n.min(Aligner::BURST_S * TICKS / self.period) as usize;
        self.music.read(t, n, &mut self.m);
        self.mic.read(t, n, &mut self.v);
        out.reserve(n * 3);
        for i in 0..n {
            out.extend_from_slice(&[self.m[2 * i], self.m[2 * i + 1], self.v[i]]);
        }
        self.out_t = Some(t + n as f64 * self.period);
        self.frames += n as u64;
        n
    }

    /// One line for the log: how the two were lined up and what it took.
    pub fn summary(&self) -> String {
        let ms = |ticks: f64| ticks / TICKS * 1000.0;
        let apart = match (self.music.ppm(), self.mic.ppm()) {
            (Some(a), Some(b)) => format!("{:.0} ppm apart", a - b),
            _ if self.music.arrival || self.mic.arrival => "drift apart unmeasured".into(),
            _ => "drift apart not yet measured".into(),
        };
        let offset = match (self.music.lag, self.mic.lag) {
            (Some(a), Some(b)) => format!("for the same moment the music is delivered {:.0} ms {} the mic", ms(a - b).abs(), if a < b { "before" } else { "after" }),
            _ => "offset at delivery unmeasured (one stream gave nothing)".into(),
        };
        let moved = self.music.stats.skipped + self.music.stats.repeated + self.mic.stats.skipped + self.mic.stats.repeated;
        let secs = self.frames as f64 * self.period / TICKS;
        format!("paired by capture time: {offset}; {apart}, {moved} frames skipped or repeated ({:.1} a second); held back {:.0} ms{}{}; {}; {}",
                if secs > 0.0 { moved as f64 / secs } else { 0.0 }, self.delay_ms(),
                if self.delay_raised > 0 { format!(" (raised {} times)", self.delay_raised) } else { String::new() },
                if self.skipped_ahead > 0 { format!("; skipped ahead {} times (asleep, or nothing ran for over 5 s)", self.skipped_ahead) } else { String::new() },
                self.music.describe("music"), self.mic.describe("mic"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;
    const PERIOD: f64 = TICKS / RATE;
    const TICK: f64 = 0.01 * TICKS;
    /// Sample values are the frame's true capture time in nominal frames from 0, offset so 0 means silence.
    const BASE: f32 = 1000.0;

    /// A capture device: packets of `frames` at its own rate (`ppm` off nominal), each stamped with the true
    /// performance-counter time of its first frame (plus `jitter`), delivered `delay` after its last frame.
    struct Device {
        ch: usize,
        ppm: f64,
        frames: usize,
        delay: f64,
        jitter: f64,
        /// device times are given (false: the packets come untimed, as a device flagging TIMESTAMP_ERROR)
        timed: bool,
        /// true time of the next frame to produce
        next_t: f64,
        /// produce nothing before this time (the app paused; the mic not started)
        from: f64,
        /// packets made and not yet read: (ready at, stamp, samples, discontinuity)
        queue: VecDeque<(f64, f64, Vec<f32>, bool)>,
        seed: u64,
        /// mark the next packet a discontinuity
        glitch: bool,
    }

    impl Device {
        fn new(ch: usize, ppm: f64, delay_ms: f64) -> Device {
            Device { ch, ppm, frames: 480, delay: delay_ms / 1000.0 * TICKS, jitter: 0.0, timed: true, next_t: 0.0, from: 0.0,
                     queue: VecDeque::new(), seed: 7, glitch: false }
        }

        fn period(&self) -> f64 {
            PERIOD / (1.0 + self.ppm * 1e-6)
        }

        fn rand(&mut self) -> f64 {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            (self.seed % 10_000) as f64 / 10_000.0 - 0.5
        }

        /// Everything the device has captured up to true time `now`.
        fn run_to(&mut self, now: f64) {
            let p = self.period();
            while self.next_t + self.frames as f64 * p <= now {
                let start = self.next_t;
                self.next_t += self.frames as f64 * p;
                if start < self.from {
                    continue;
                }
                let j = self.jitter * self.rand();
                let samples: Vec<f32> = (0..self.frames).flat_map(|i| {
                    let v = BASE + ((start + i as f64 * p) / PERIOD) as f32;
                    std::iter::repeat(v).take(self.ch)
                }).collect();
                self.queue.push_back((self.next_t + self.delay, start + j, samples, self.glitch));
                self.glitch = false;
            }
        }

        /// Read what's been delivered by `now`, into `push`.
        fn read(&mut self, now: f64, mut push: impl FnMut(Packet)) {
            let ready = self.queue.iter().take_while(|q| q.0 <= now).count();
            let mut behind: usize = self.queue.iter().take(ready).map(|q| q.2.len() / self.ch).sum();
            for _ in 0..ready {
                let (_, stamp, samples, disc) = self.queue.pop_front().unwrap();
                behind -= samples.len() / self.ch;
                push(Packet { time: self.timed.then_some(stamp.round() as i64), read_at: now as i64, behind, discontinuity: disc, samples: &samples });
            }
        }

        /// Drop the frames made between two times: data lost, flagged on the packet after them.
        fn lose(&mut self, from: f64, to: f64) {
            self.queue.retain(|q| q.1 < from || q.1 >= to);
            match self.queue.iter_mut().find(|q| q.1 >= to) {
                Some(q) => q.3 = true,
                None => self.glitch = true,
            }
        }
    }

    /// The pump: both devices read and the aligner asked every 10 ms (± `wobble` of the timer) of true time, from
    /// `start` to `end`. Returns every output frame with its output time: (t, L, mic).
    fn pump(al: &mut Aligner, music: &mut Device, mic: Option<&mut Device>, start: f64, end: f64, wobble: f64) -> Vec<(f64, f32, f32)> {
        let mut out = Vec::new();
        let mut frames = Vec::new();
        let mut now = start;
        let mut seed = 99u64;
        let mut mic = mic;
        while now < end {
            music.run_to(now);
            music.read(now, |p| al.push_music(p));
            if let Some(m) = mic.as_deref_mut() {
                m.run_to(now);
                m.read(now, |p| al.push_mic(p));
            }
            frames.clear();
            let t0 = al.out_t;
            let n = al.take(now as i64, &mut frames);
            if let Some(t0) = t0.filter(|_| n > 0) {
                for (i, f) in frames.chunks_exact(3).enumerate() {
                    out.push((t0 + i as f64 * PERIOD, f[0], f[2]));
                }
            }
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            now += TICK + wobble * ((seed % 1000) as f64 / 1000.0 - 0.5);
        }
        out
    }

    /// A sample's error from where it should be, in frames: its value against the output time it was given at.
    fn err(t: f64, v: f32) -> f64 {
        (v - BASE) as f64 - t / PERIOD
    }

    fn sounding(v: f32) -> bool {
        v != 0.0
    }

    /// The largest music−mic misalignment (frames) and the largest absolute error of each, over frames where both sound.
    fn worst(out: &[(f64, f32, f32)], from: f64) -> (f64, f64, f64) {
        let mut w = (0f64, 0f64, 0f64);
        for &(t, l, m) in out.iter().filter(|o| o.0 >= from && sounding(o.1) && sounding(o.2)) {
            let (el, em) = (err(t, l), err(t, m));
            w = (w.0.max((el - em).abs()), w.1.max(el.abs()), w.2.max(em.abs()));
        }
        w
    }

    #[test]
    fn two_streams_delivered_at_different_times_are_paired_to_the_frame() {
        let mut al = Aligner::new(48_000);
        // the loopback's packets come 12 ms after capture, the mic's 28 ms: the old pairing was 16 ms off
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 0.0, 28.0));
        let out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 10.0 * TICKS, 0.0);
        let (apart, m, v) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.0, "music and mic {apart} frames apart");
        assert!(m <= 1.0 && v <= 1.0, "music {m}, mic {v} frames from their time");
        assert_eq!((al.music.stats.gaps, al.music.stats.resyncs, al.mic.stats.gaps, al.delay_raised), (0, 0, 0, 0));
        assert_eq!(al.music.stats.skipped + al.music.stats.repeated + al.mic.stats.skipped + al.mic.stats.repeated, 0);
        // the mic's packets come 16 ms after the music's (read on a 10 ms grid: 10 to 20 ms)
        let lead = (al.mic.lag.unwrap() - al.music.lag.unwrap()) / TICKS * 1000.0;
        assert!((8.0..=22.0).contains(&lead), "{lead} ms");
        let s = al.summary();
        assert!(s.contains("the music is delivered 1") && s.contains("ms before the mic"), "{s}");
    }

    #[test]
    fn clocks_drifting_200_ppm_apart_stay_paired_and_are_measured() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 200.0, 12.0), Device::new(1, -200.0, 25.0));
        let out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 60.0 * TICKS, 0.0);
        let (apart, m, v) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.5, "music and mic {apart} frames apart");
        assert!(m <= 1.0 && v <= 1.0, "music {m}, mic {v} frames from their time");
        // 400 ppm apart at 48 kHz over 60 s: about 1150 frames moved between them
        let moved = al.music.stats.skipped + al.mic.stats.repeated;
        assert!((1100..=1200).contains(&moved), "moved {moved}: {:?} {:?}", al.music.stats, al.mic.stats);
        // (a correction the wrong way or two while a clock settles on the nominal rate, none after)
        assert!(al.music.stats.repeated <= 2 && al.mic.stats.skipped <= 2, "{:?} {:?}", al.music.stats, al.mic.stats);
        assert_eq!((al.music.stats.resyncs, al.mic.stats.resyncs), (0, 0));
        let (pm, pv) = (al.music.ppm().unwrap(), al.mic.ppm().unwrap());
        assert!((pm - 200.0).abs() < 5.0 && (pv + 200.0).abs() < 5.0, "measured {pm} and {pv} ppm");
        let s = al.summary();
        assert!(s.contains("400 ppm apart") && s.contains("music: 0 gaps") && s.contains("+200 ppm"), "{s}");
    }

    #[test]
    fn stamp_jitter_and_an_uneven_timer_move_nothing() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 30.0, 10.0), Device::new(1, -40.0, 20.0));
        music.jitter = 0.0002 * TICKS;       // ±100 µs, about 5 frames
        mic.jitter = 0.0002 * TICKS;
        // the pump's timer wobbling ±8 ms (Windows' ticks about every 15 ms)
        let out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 30.0 * TICKS, 0.016 * TICKS);
        let (apart, m, v) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 3.0, "music and mic {apart} frames apart");
        assert!(m <= 3.0 && v <= 3.0, "music {m}, mic {v} frames from their time");
        assert_eq!((al.music.stats.gaps, al.music.stats.overlaps, al.music.stats.resyncs, al.mic.stats.gaps, al.mic.stats.resyncs), (0, 0, 0, 0, 0));
        assert_eq!(al.delay_raised, 0, "{}", al.summary());
    }

    #[test]
    fn lost_frames_are_a_gap_of_silence_and_nothing_moves() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 50.0, 12.0), Device::new(1, 0.0, 25.0));
        let mut out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 5.0 * TICKS, 0.0);
        // the loopback loses 30 ms (three packets) at 5 s, the next packet flagged
        music.run_to(5.5 * TICKS);
        music.lose(5.0 * TICKS, 5.03 * TICKS);
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 5.5 * TICKS, 10.0 * TICKS, 0.0));
        let (apart, _, _) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.5, "music and mic {apart} frames apart");
        // silence exactly over the lost 30 ms (the samples' own times tell)
        let quiet: Vec<f64> = out.iter().filter(|o| o.0 > 4.0 * TICKS && o.0 < 6.0 * TICKS && !sounding(o.1)).map(|o| o.0).collect();
        assert!((1400..=1500).contains(&quiet.len()), "{} silent frames", quiet.len());
        assert!(quiet[0] >= 4.999 * TICKS && *quiet.last().unwrap() <= 5.041 * TICKS, "silence {:?}..{:?}", quiet[0] / TICKS, quiet.last().unwrap() / TICKS);
        assert_eq!((al.music.stats.gaps, al.music.stats.resyncs), (1, 0), "{:?}", al.music.stats);
        assert!((1438..=1442).contains(&al.music.stats.gap_frames), "{:?}", al.music.stats);
    }

    #[test]
    fn a_discontinuity_flag_with_no_time_jump_is_harmless() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 0.0, 25.0));
        let mut out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 2.0 * TICKS, 0.0);
        music.glitch = true;
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 2.0 * TICKS, 4.0 * TICKS, 0.0));
        let (apart, _, _) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.0);
        assert_eq!((al.music.stats.gaps, al.music.stats.gap_frames, al.music.stats.overlaps), (1, 0, 0));
        assert!(out.iter().filter(|o| o.0 > 1.0 * TICKS).all(|o| sounding(o.1)));
    }

    #[test]
    fn a_mic_starting_late_joins_in_time() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 100.0, 25.0));
        mic.from = 3.0 * TICKS;
        let out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 8.0 * TICKS, 0.0);
        // the music was given from the start, on time, with a silent mic
        let early: Vec<_> = out.iter().filter(|o| o.0 > 0.5 * TICKS && o.0 < 2.9 * TICKS).collect();
        assert!(!early.is_empty() && early.iter().all(|o| sounding(o.1) && !sounding(o.2)));
        assert!(early.iter().all(|o| err(o.0, o.1).abs() <= 1.0));
        let (apart, _, v) = worst(&out, 3.5 * TICKS);
        assert!(apart <= 1.0 && v <= 1.0, "apart {apart}, mic {v}");
        assert_eq!(al.mic.stats.resyncs, 0);
    }

    #[test]
    fn music_that_stops_and_starts_again_is_paired_again() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 100.0, 12.0), Device::new(1, 0.0, 25.0));
        let mut out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 4.0 * TICKS, 0.0);
        // the app plays nothing for 5 s: the per-process loopback sends nothing, then goes on with the real time
        music.from = 9.0 * TICKS;
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 4.0 * TICKS, 14.0 * TICKS, 0.0));
        let (apart, m, _) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.0 && m <= 1.0, "apart {apart}, music {m}");
        // 5 s of silence (the pause starts and ends on the device's packets: up to two more), and the music back
        // within a packet of 9 s
        let silent = out.iter().filter(|o| o.0 >= 0.0 && !sounding(o.1)).count();
        assert!((5.0 * RATE - 100.0..=5.0 * RATE + 1000.0).contains(&(silent as f64)), "{silent} silent frames");
        let back = out.iter().find(|o| o.0 >= 9.0 * TICKS && sounding(o.1)).unwrap().0;
        assert!(back < 9.011 * TICKS, "music back at {}", back / TICKS);
        assert_eq!(al.music.stats.restarts, 1);
        // the mic ran on untouched
        assert_eq!((al.mic.stats.gaps, al.mic.stats.resyncs), (0, 0));
    }

    #[test]
    fn a_short_pause_is_filled_and_a_stream_restarting_with_earlier_time_is_cut() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 0.0, 25.0));
        let mut out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 3.0 * TICKS, 0.0);
        music.from = 3.5 * TICKS;       // 0.5 s pause: filled with silence, the clock kept
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 3.0 * TICKS, 6.0 * TICKS, 0.0));
        assert_eq!((al.music.stats.gaps, al.music.stats.restarts), (1, 0));
        assert!((23_000..=25_000).contains(&al.music.stats.gap_frames), "{}", al.music.stats.gap_frames);
        // a stream whose stamps overlap the last packet by 5 ms (a restart re-sending): the overlap dropped
        music.run_to(6.0 * TICKS);
        music.next_t -= 0.005 * TICKS;
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 6.0 * TICKS, 9.0 * TICKS, 0.0));
        assert_eq!(al.music.stats.overlaps, 1);
        assert!((235..=245).contains(&al.music.stats.overlap_frames), "{}", al.music.stats.overlap_frames);
        let (apart, _, _) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.0, "{apart}");
        assert_eq!(al.music.stats.resyncs, 0);
    }

    #[test]
    fn with_no_mic_the_music_runs_on_the_counter_at_the_rate() {
        let mut al = Aligner::new(48_000);
        let mut music = Device::new(2, -150.0, 12.0);
        let out = pump(&mut al, &mut music, None, 0.0, 20.0 * TICKS, 0.004 * TICKS);
        assert!(out.iter().all(|o| !sounding(o.2)));
        let late: Vec<_> = out.iter().filter(|o| o.0 > 1.0 * TICKS).collect();
        assert!(late.iter().all(|o| sounding(o.1) && err(o.0, o.1).abs() <= 1.5), "{:?}", late.iter().map(|o| err(o.0, o.1)).fold(0f64, f64::max));
        // frames given a second: the rate (the output runs on the counter, 50 ms behind)
        let span = out.last().unwrap().0 - out[0].0;
        assert!(((out.len() - 1) as f64 / (span / TICKS) - RATE).abs() < 1.0);
        assert!(out.last().unwrap().0 > 19.9 * TICKS && out.last().unwrap().0 < 20.0 * TICKS - 0.04 * TICKS);
        let s = al.summary();
        assert!(s.contains("mic: nothing came") && s.contains("-150 ppm"), "{s}");
    }

    #[test]
    fn packets_coming_too_late_raise_the_delay_and_are_then_in_time() {
        let mut al = Aligner::new(48_000);
        // the mic's packets come 70 ms after capture: more than the 50 ms the output waits
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 0.0, 70.0));
        let out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 10.0 * TICKS, 0.0);
        assert!(al.delay_raised >= 1 && al.delay_ms() >= 80.0 && al.delay_ms() <= 110.0, "{}", al.summary());
        let late = al.mic.stats.late_frames;
        assert!(late > 0 && late < 4000, "{late}");
        // from 1 s on everything is there and paired
        let (apart, _, _) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.0);
        assert!(out.iter().filter(|o| o.0 > 1.0 * TICKS).all(|o| sounding(o.2)));
        let raised = al.delay_raised;
        pump(&mut al, &mut music, Some(&mut mic), 10.0 * TICKS, 20.0 * TICKS, 0.008 * TICKS);
        assert_eq!((al.mic.stats.late_frames, al.delay_raised), (late, raised), "late again after the delay was raised: {}", al.summary());
    }

    #[test]
    fn a_device_giving_no_times_is_paired_by_arrival_within_a_few_ms() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 100.0, 5.0), Device::new(1, 0.0, 25.0));
        music.timed = false;
        let out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 30.0 * TICKS, 0.012 * TICKS);
        let (apart, _, v) = worst(&out, 2.0 * TICKS);
        // the arrival estimate is late by the delivery delay (5 ms) and the poll (up to 16 ms): within 20 ms
        assert!(apart <= 0.02 * RATE, "{apart} frames apart");
        assert!(v <= 2.0);
        assert_eq!((al.music.stats.gaps, al.music.stats.overlaps, al.music.stats.restarts), (0, 0, 0), "{}", al.summary());
        assert!(al.music.stats.resyncs <= 2, "{}", al.summary());
        let s = al.summary();
        assert!(s.contains("timed by arrival") && s.contains("drift unmeasured"), "{s}");
    }

    #[test]
    fn one_untimed_packet_from_a_device_that_times_them_is_taken_as_predicted() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 0.0, 25.0));
        let mut out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 2.0 * TICKS, 0.0);
        // the one packet ready just after the last tick comes with no time
        let ready = music.queue[0].0;
        assert!(ready > 1.99 * TICKS && ready <= 2.0 * TICKS && music.queue[1].0 > ready + 0.005 * TICKS);
        music.timed = false;
        music.read(ready, |p| al.push_music(p));
        music.timed = true;
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 2.0 * TICKS, 4.0 * TICKS, 0.0));
        assert_eq!(al.music.stats.untimed, 1);
        assert!(!al.music.arrival);
        let (apart, _, _) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.0);
    }

    #[test]
    fn a_stalled_pump_catches_up_in_bursts_and_nothing_is_lost() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 0.0, 25.0));
        let mut out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 2.0 * TICKS, 0.0);
        // nothing runs for 300 ms (the devices keep capturing)
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 2.3 * TICKS, 4.0 * TICKS, 0.0));
        let (apart, _, _) = worst(&out, 1.0 * TICKS);
        assert!(apart <= 1.0);
        assert!(out.iter().filter(|o| o.0 > 1.0 * TICKS && o.0 < 3.9 * TICKS).all(|o| sounding(o.1) && sounding(o.2)));
        assert_eq!(al.delay_raised, 0);
    }
    #[test]
    fn a_long_sleep_is_skipped_not_caught_up() {
        let mut al = Aligner::new(48_000);
        let (mut music, mut mic) = (Device::new(2, 0.0, 12.0), Device::new(1, 0.0, 25.0));
        let mut out = pump(&mut al, &mut music, Some(&mut mic), 0.0, 2.0 * TICKS, 0.0);
        // the devices' queues empty while the PC sleeps 60 s (Windows restarts the streams; here they just go on)
        music.run_to(62.0 * TICKS);
        mic.run_to(62.0 * TICKS);
        music.queue.clear();
        mic.queue.clear();
        out.extend(pump(&mut al, &mut music, Some(&mut mic), 62.0 * TICKS, 64.0 * TICKS, 0.0));
        assert_eq!(al.skipped_ahead, 1);
        // the engine got about 4 s of frames, not 64
        assert!((3.8 * RATE..=4.1 * RATE).contains(&(out.len() as f64)), "{} frames", out.len());
        let (apart, _, _) = worst(&out, 62.5 * TICKS);
        assert!(apart <= 1.0, "{apart}");
        assert!(out.iter().filter(|o| o.0 > 62.5 * TICKS).all(|o| sounding(o.1) && sounding(o.2)));
        assert!(al.summary().contains("skipped ahead 1 times"), "{}", al.summary());
    }

}
