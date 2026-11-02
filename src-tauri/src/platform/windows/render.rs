//! Sound out through WASAPI, for the two things OnKey plays itself: your recording with the song (voice.rs) and the
//! demo song (demo.rs). One shared-mode stream on the default output, in the player's own format (Windows converts
//! it to the output's), kept topped up from a thread by the player's `Fill`, and its clock: how much the output has
//! played, which is what "where is it" is answered from.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ::windows::Win32::Media::Audio::*;
use ::windows::Win32::System::Com::CLSCTX_ALL;

/// What a player gives the stream.
pub trait Fill: Send + 'static {
    /// Fill `out` (whole frames, interleaved). `first`: how many frames the stream had been given before these;
    /// `ahead`: how long until the first of them is heard (s), the frames queued before them.
    fn fill(&mut self, out: &mut [f32], first: u64, ahead: f64);
}

/// The buffer asked for (100 ns units): 100 ms, topped up every 10 ms.
const BUFFER_100NS: i64 = 1_000_000;
const WAKE: Duration = Duration::from_millis(10);
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

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
    /// the output went away (unplugged, or changed under the stream)
    dead: AtomicBool,
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
                                       dead: AtomicBool::new(false) });
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let s = shared.clone();
        let thread = std::thread::Builder::new().name("onkey-render".into()).spawn(move || {
            super::com();
            match open_client(rate, channels) {
                Ok((client, render, clock)) => {
                    let _ = tx.send(true);
                    run(&s, &client, &render, &clock, rate as f64, channels as usize);
                    let _ = unsafe { client.Stop() };
                }
                Err(_) => {
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
        f(&mut self.shared.fill.lock().unwrap())
    }

    /// Frames heard so far (the stream's clock, carried on from its last reading by the wall clock, never past
    /// what it's been given). None until it's started, or once its output has gone.
    pub fn played(&self) -> Option<f64> {
        if self.shared.dead.load(Ordering::SeqCst) {
            return None;
        }
        let c = (*self.shared.clock.lock().unwrap())?;
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

fn open_client(rate: u32, channels: u16) -> ::windows::core::Result<(IAudioClient, IAudioRenderClient, IAudioClock)> {
    unsafe {
        let device = super::audio::enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let fmt = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
            nChannels: channels,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * 4 * channels as u32,
            nBlockAlign: 4 * channels,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                          BUFFER_100NS, 0, &fmt, None)?;
        let render: IAudioRenderClient = client.GetService()?;
        let clock: IAudioClock = client.GetService()?;
        Ok((client, render, clock))
    }
}

/// Keep the stream topped up until told to stop (or the output goes), reading its clock each time.
fn run<F: Fill>(s: &Shared<F>, client: &IAudioClient, render: &IAudioRenderClient, clock: &IAudioClock, rate: f64, channels: usize) {
    let result = (|| -> ::windows::core::Result<()> {
        unsafe {
            let size = client.GetBufferSize()?;
            let freq = clock.GetFrequency()? as f64;
            let mut written: u64 = 0;
            let mut started = false;
            while !s.stop.load(Ordering::SeqCst) {
                let queued = client.GetCurrentPadding()?;
                let room = size.saturating_sub(queued);
                if room > 0 {
                    let p = render.GetBuffer(room)?;
                    let out = std::slice::from_raw_parts_mut(p as *mut f32, room as usize * channels);
                    s.fill.lock().unwrap().fill(out, written, queued as f64 / rate);
                    render.ReleaseBuffer(room, 0)?;
                    written += room as u64;
                }
                if !started {
                    client.Start()?;
                    started = true;
                }
                // (the position is in the clock's own units: `freq` of them a second)
                let mut pos = 0u64;
                clock.GetPosition(&mut pos, None)?;
                *s.clock.lock().unwrap() = Some(Clock { played: pos as f64 / freq * rate, written, at: Instant::now() });
                std::thread::sleep(WAKE);
            }
            Ok(())
        }
    })();
    if result.is_err() {
        s.dead.store(true, Ordering::SeqCst);
    }
}

/// Is there an output to play to (for tests on machines without one).
#[cfg(test)]
pub fn has_output() -> bool {
    super::audio::output().is_some()
}
