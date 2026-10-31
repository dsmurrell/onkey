//! Sound out through the sound server (pulse.rs's connection), for the two things OnKey plays itself: your recording
//! with the song (voice.rs) and the demo song (demo.rs). One playback stream on the default output in the player's
//! own format (the server converts it), filled whenever the server asks, and its clock: how much the output has
//! played (what the server has read, less the output's own latency), which is what "where is it" is answered from.
//! The same interface as Windows' render.rs, so demo.rs and voice.rs are Windows' with only the decoder changed.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use pulseaudio::protocol as pa;

use super::pulse::{reply, Conn, Msg};

/// What a player gives the stream.
pub trait Fill: Send + 'static {
    /// Fill `out` (whole frames, interleaved). `first`: how many frames the stream had been given before these;
    /// `ahead`: how long until the first of them is heard (s), the frames queued before them.
    fn fill(&mut self, out: &mut [f32], first: u64, ahead: f64);
}

/// What the stream's thread last saw of its clock.
#[derive(Debug, Clone, Copy)]
struct Clock {
    /// frames played (heard) at `at`
    played: f64,
    /// frames given to the stream by then
    written: u64,
    at: Instant,
}

struct Shared<F> {
    fill: Mutex<F>,
    clock: Mutex<Option<Clock>>,
    stop: AtomicBool,
    /// the output went away (unplugged, or the server stopped)
    dead: AtomicBool,
    /// the output's own latency at the first timing (µs; u64::MAX until then)
    latency_us: std::sync::atomic::AtomicU64,
}

/// A running stream. Dropped, it stops.
pub struct Stream<F: Fill> {
    shared: Arc<Shared<F>>,
    thread: Option<std::thread::JoinHandle<()>>,
    rate: f64,
}

impl<F: Fill> Stream<F> {
    /// Start playing what `fill` gives, `channels` float channels at `rate`, on the default output. None when there's
    /// no output, or it won't open.
    pub fn open(rate: u32, channels: u16, fill: F) -> Option<Stream<F>> {
        let shared = Arc::new(Shared { fill: Mutex::new(fill), clock: Mutex::new(None), stop: AtomicBool::new(false),
                                       dead: AtomicBool::new(false), latency_us: std::sync::atomic::AtomicU64::new(u64::MAX) });
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let s = shared.clone();
        let thread = std::thread::Builder::new().name("onkey-render".into()).spawn(move || {
            match open_stream(rate, channels) {
                Ok((conn, made)) => {
                    let _ = tx.send(true);
                    if let Err(e) = run(&s, conn, &made, rate as f64, channels as usize) {
                        say(&format!("playback (Linux): the stream stopped: {e}"));
                        s.dead.store(true, Ordering::SeqCst);
                    }
                }
                Err(e) => {
                    say(&format!("playback (Linux): couldn't open the output: {e}"));
                    let _ = tx.send(false);
                }
            }
        }).ok()?;
        if rx.recv_timeout(Duration::from_secs(5)).unwrap_or(false) {
            Some(Stream { shared, thread: Some(thread), rate: rate as f64 })
        } else {
            shared.stop.store(true, Ordering::SeqCst);
            None
        }
    }

    /// The player, to change what it plays.
    pub fn with<R>(&self, f: impl FnOnce(&mut F) -> R) -> R {
        f(&mut self.shared.fill.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Frames heard so far (the stream's clock, carried on from its last reading by the wall clock, never past
    /// what it's been given). None until it's started, or once its output has gone.
    pub fn played(&self) -> Option<f64> {
        if self.shared.dead.load(Ordering::SeqCst) {
            return None;
        }
        let c = (*self.shared.clock.lock().unwrap_or_else(|e| e.into_inner()))?;
        Some((c.played + c.at.elapsed().as_secs_f64() * self.rate).min(c.written as f64))
    }
}

impl<F: Fill> Drop for Stream<F> {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Into the log (and, in tests, onto stderr, which a failing test shows).
fn say(msg: &str) {
    crate::engine::log(msg);
    #[cfg(test)]
    eprintln!("{msg}");
}

fn open_stream(rate: u32, channels: u16) -> Result<(Conn, pa::CreatePlaybackStreamReply), String> {
    let mut c = Conn::open("OnKey")?;
    let bytes_per_s = rate * channels as u32 * 4;
    let mut params = pa::PlaybackStreamParams {
        sample_spec: pa::SampleSpec { format: pa::SampleFormat::Float32Le, channels: channels as u8, sample_rate: rate },
        channel_map: if channels == 1 { pa::ChannelMap::mono() } else { pa::ChannelMap::stereo() },
        sink_name: Some(pa::DEFAULT_SINK.to_owned()),
        // 100 ms queued, asked for 10 ms at a time
        buffer_attr: pa::stream::BufferAttr { target_length: bytes_per_s / 10, minimum_request_length: bytes_per_s / 100, ..Default::default() },
        // (no volume of its own: it plays at the output's, as Windows' does; one at 100 % would turn the output up
        // to full where PulseAudio's flat volumes are on)
        cvolume: None,
        flags: pa::stream::StreamFlags { adjust_latency: true, ..Default::default() },
        ..Default::default()
    };
    params.props.set(pa::Prop::MediaName, c"OnKey");
    params.props.set_bytes(c"media.role", c"music".to_bytes_with_nul());
    let made: pa::CreatePlaybackStreamReply = c.request(pa::Command::CreatePlaybackStream(params))?;
    Ok((c, made))
}

/// Fill the stream whenever the server asks, asking its latency every 50 ms for the clock, until told to stop (or
/// the output goes).
fn run<F: Fill>(s: &Shared<F>, mut c: Conn, made: &pa::CreatePlaybackStreamReply, rate: f64, channels: usize) -> Result<(), String> {
    let frame_bytes = channels * 4;
    let mut written: u64 = 0;
    let mut buf: Vec<f32> = Vec::new();
    let mut give = |c: &mut Conn, bytes: u32, written: &mut u64, played: f64| -> Result<(), String> {
        let frames = bytes as usize / frame_bytes;
        if frames == 0 {
            return Ok(());
        }
        buf.clear();
        buf.resize(frames * channels, 0.0);
        let ahead = (*written as f64 - played).max(0.0) / rate;
        s.fill.lock().unwrap_or_else(|e| e.into_inner()).fill(&mut buf, *written, ahead);
        let raw: Vec<u8> = buf.iter().flat_map(|v| v.to_le_bytes()).collect();
        c.write_audio(made.channel, &raw)?;
        *written += frames as u64;
        Ok(())
    };
    give(&mut c, made.requested_bytes, &mut written, 0.0)?;
    let mut asked: Option<u32> = None;
    let mut last_ask = Instant::now() - Duration::from_secs(1);
    let mut played = 0.0;
    let mut said_latency = false;
    let began = Instant::now();
    while !s.stop.load(Ordering::SeqCst) {
        if asked.is_none() && last_ask.elapsed() > Duration::from_millis(50) {
            last_ask = Instant::now();
            asked = Some(c.send(&pa::Command::GetPlaybackLatency(pa::LatencyParams { channel: made.channel, now: SystemTime::now() }))?);
        }
        if !c.ready(Duration::from_millis(10)) {
            continue;
        }
        match c.next()? {
            Msg::Command(cmd) => match *cmd {
                pa::Command::Request(r) if r.channel == made.channel => give(&mut c, r.length, &mut written, played)?,
                pa::Command::PlaybackStreamKilled(ch) if ch == made.channel => return Err("the output went away".into()),
                _ => {}
            },
            Msg::Reply(seq, b) if Some(seq) == asked => {
                asked = None;
                match reply::<pa::PlaybackLatency>(&b, c.ver) {
                    Ok(l) => {
                        // heard: what the server has read from the stream, less what the output still holds
                        played = (l.read_offset.max(0) as f64 / frame_bytes as f64 - l.sink_usec as f64 * rate / 1e6).max(0.0);
                        if !said_latency {
                            said_latency = true;
                            s.latency_us.store(l.sink_usec, Ordering::SeqCst);
                            say(&format!("playback (Linux): the output's first timing: read {} B, written {} B, output latency {} us, playing {}",
                                         l.read_offset, l.write_offset, l.sink_usec, l.playing));
                        }
                    }
                    Err(e) => {
                        // no timing from the server: the wall clock since the first audio, less what's queued
                        if !said_latency {
                            said_latency = true;
                            say(&format!("playback (Linux): no timing from the server ({e}); going by the wall clock"));
                        }
                        let queued = made.buffer_attr.target_length as f64 / frame_bytes as f64;
                        played = (began.elapsed().as_secs_f64() * rate - queued).clamp(0.0, written as f64);
                    }
                }
                *s.clock.lock().unwrap_or_else(|e| e.into_inner()) = Some(Clock { played, written, at: Instant::now() });
            }
            _ => {}
        }
    }
    let _ = c.send(&pa::Command::DeletePlaybackStream(made.channel));
    Ok(())
}

/// Is there an output to play to, like a real one (for tests on machines without one)? CI's PulseAudio null output
/// says it holds up to 2 s of sound, so nothing is heard within a test's second: that counts as none.
#[cfg(test)]
pub fn has_output() -> bool {
    struct Silence;
    impl Fill for Silence {
        fn fill(&mut self, out: &mut [f32], _first: u64, _ahead: f64) {
            out.fill(0.0);
        }
    }
    if super::pulse::output().is_none() {
        return false;
    }
    let Some(probe) = Stream::open(48_000, 1, Silence) else { return false };
    let until = Instant::now() + Duration::from_secs(2);
    while probe.shared.latency_us.load(Ordering::SeqCst) == u64::MAX && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    let us = probe.shared.latency_us.load(Ordering::SeqCst);
    if us > 500_000 {
        eprintln!("the output holds {:.2} s of sound (a stand-in, not speakers)", us as f64 / 1e6);
        return false;
    }
    true
}
