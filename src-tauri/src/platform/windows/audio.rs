//! Audio on Windows through WASAPI: what the music app plays (its process's loopback, else the whole default
//! output's) together with the microphone, as the engine wants it ([music L, music R, mic] frames at one rate,
//! given to a callback), the microphones there are, and a level meter for setup's mic check.
//!
//! Music and mic are two WASAPI clients on two devices with two clocks, asked for the same rate (Windows
//! converts), delivered at different times. They aren't sample-aligned the way macOS's aggregate device makes
//! them, so they're paired by when each packet was captured: WASAPI stamps every packet with the performance
//! counter time of its first frame, and platform/interleave.rs lines the two up on one timeline from those,
//! following each device's clock against the counter (drift), filling gaps and dropping overlaps by time, and
//! says in the log how it went.
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ::windows::core::{Interface, PCWSTR};
use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use ::windows::Win32::Media::Audio::*;
use ::windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

/// The callback the engine gives (capture.rs `on_audio`): interleaved frames, how many, its context.
pub type Callback = extern "C" fn(*const f32, i32, *mut c_void);

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
/// What each client is asked for: 200 ms of buffer (in 100 ns units).
const BUFFER_100NS: i64 = 2_000_000;

pub(super) fn enumerator() -> ::windows::core::Result<IMMDeviceEnumerator> {
    super::com();
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn device_id(d: &IMMDevice) -> String {
    unsafe {
        match d.GetId() {
            Ok(p) => {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const c_void));
                s
            }
            Err(_) => String::new(),
        }
    }
}

fn device_name(d: &IMMDevice) -> String {
    unsafe {
        d.OpenPropertyStore(STGM_READ).and_then(|s| s.GetValue(&PKEY_Device_FriendlyName))
            .map(|v| v.to_string()).unwrap_or_default()
    }
}

/// The default output device: (id, name).
pub fn output() -> Option<(String, String)> {
    let d = unsafe { enumerator().ok()?.GetDefaultAudioEndpoint(eRender, eConsole).ok()? };
    Some((device_id(&d), device_name(&d)))
}

/// Every microphone connected: (id, name).
pub fn mics() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(en) = enumerator() else { return out };
    unsafe {
        let Ok(all) = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return out };
        for i in 0..all.GetCount().unwrap_or(0) {
            if let Ok(d) = all.Item(i) {
                out.push((device_id(&d), device_name(&d)));
            }
        }
    }
    out
}

/// The microphone preferred in Settings (an id from `mics`), "" for the default.
static PREFERENCE: Mutex<String> = Mutex::new(String::new());

pub fn set_preference(uid: &str) {
    *PREFERENCE.lock().unwrap() = uid.to_string();
}

/// The mic OnKey would listen to now: the preferred one while it's connected, else Windows' default input.
pub fn chosen_mic() -> Option<(String, String)> {
    let pref = PREFERENCE.lock().unwrap().clone();
    let all = mics();
    if let Some(m) = all.iter().find(|m| !pref.is_empty() && m.0 == pref) {
        return Some(m.clone());
    }
    let d = unsafe { enumerator().ok()?.GetDefaultAudioEndpoint(eCapture, eConsole).ok()? };
    Some((device_id(&d), device_name(&d))).or_else(|| all.into_iter().next())
}

fn mic_device(uid: &str) -> Option<IMMDevice> {
    let en = enumerator().ok()?;
    let w: Vec<u16> = uid.encode_utf16().chain(Some(0)).collect();
    unsafe { en.GetDevice(PCWSTR(w.as_ptr())).ok() }
}

/// The output's mix rate: what everything captured is given at.
fn mix_rate(d: &IMMDevice) -> ::windows::core::Result<u32> {
    unsafe {
        let c: IAudioClient = d.Activate(CLSCTX_ALL, None)?;
        let f = c.GetMixFormat()?;
        let rate = (*f).nSamplesPerSec;
        CoTaskMemFree(Some(f as *const c_void));
        Ok(rate)
    }
}

/// A device's own format, as Windows mixes it: "48000 Hz, 2 ch, 32-bit".
fn mix_format(d: &IMMDevice) -> String {
    unsafe {
        let Ok(c) = d.Activate::<IAudioClient>(CLSCTX_ALL, None) else { return "format unknown".into() };
        let Ok(f) = c.GetMixFormat() else { return "format unknown".into() };
        let (rate, ch, bits) = ((*f).nSamplesPerSec, (*f).nChannels, (*f).wBitsPerSample);
        let s = format!("{rate} Hz, {ch} ch, {bits}-bit");
        CoTaskMemFree(Some(f as *const c_void));
        s
    }
}

/// Every output and microphone connected, with its format and which is Windows' default, for the log: outputs as
/// output "…" (an owner's name left out), microphones as mic '…' (left out of a report, as on the Mac).
pub fn describe_devices() -> String {
    let Ok(en) = enumerator() else { return "can't list them".into() };
    let mut out = Vec::new();
    for (flow, kind) in [(eRender, "output"), (eCapture, "mic")] {
        let default = unsafe { en.GetDefaultAudioEndpoint(flow, eConsole) }.ok().map(|d| device_id(&d));
        let Ok(all) = (unsafe { en.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }) else { continue };
        let n = unsafe { all.GetCount() }.unwrap_or(0);
        if n == 0 {
            out.push(format!("no {kind}"));
        }
        for i in 0..n {
            let Ok(d) = (unsafe { all.Item(i) }) else { continue };
            let def = if default.as_deref() == Some(device_id(&d).as_str()) { ", default" } else { "" };
            out.push(match kind {
                "output" => format!("output \"{}\" ({}{def})", crate::digest::device_name(&device_name(&d)), mix_format(&d)),
                _ => format!("mic '{}' ({}{def})", device_name(&d), mix_format(&d)),
            });
        }
    }
    out.join("; ")
}

/// A started WASAPI capture client giving float samples, `channels` per frame at `rate` (Windows converts).
struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    channels: usize,
    /// silence for a packet flagged SILENT (its data is to be taken as zeros)
    zeros: Vec<f32>,
}

/// The performance counter now, in 100 ns ticks: the clock WASAPI's packet positions are on.
pub fn qpc_now() -> i64 {
    use ::windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    static FREQ: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    let f = *FREQ.get_or_init(|| {
        let mut f = 0i64;
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1)
    });
    let mut c = 0i64;
    let _ = unsafe { QueryPerformanceCounter(&mut c) };
    (c / f) * 10_000_000 + (c % f) * 10_000_000 / f
}

/// A packet's stamp as trusted: not 0, not flagged wrong, and within 2 s of now (a device whose stamps are on some
/// other clock is timed by arrival instead).
fn stamp(qpc: u64, flags: u32, now: i64) -> Option<i64> {
    let t = qpc as i64;
    (qpc != 0 && flags & (AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32) == 0 && (now - t).abs() < 20_000_000).then_some(t)
}

impl Stream {
    fn open(d: &IMMDevice, loopback: bool, channels: u16, rate: u32) -> ::windows::core::Result<Stream> {
        let client: IAudioClient = unsafe { d.Activate(CLSCTX_ALL, None)? };
        Stream::start(client, loopback, false, channels, rate)
    }

    /// Only what one program and the programs it started play (Windows 10 2004 and later): the music app's sound
    /// without anything else's, OnKey's own included, as macOS's process tap gives it.
    fn open_process(pid: u32, channels: u16, rate: u32) -> ::windows::core::Result<Stream> {
        Stream::start(process_loopback_client(pid)?, true, true, channels, rate)
    }

    fn start(client: IAudioClient, loopback: bool, process: bool, channels: u16, rate: u32) -> ::windows::core::Result<Stream> {
        unsafe {
            let fmt = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
                nChannels: channels,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * 4 * channels as u32,
                nBlockAlign: 4 * channels,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            let mut flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            if loopback {
                flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
            }
            // (a process loopback client is set up event-driven, as Microsoft's sample does; it's still read by
            // polling, so the event is only there to be signalled. Its buffer is 200 ms like the others': with 0,
            // Windows gave it about one 10 ms period, and a poll a little late (Windows' timer ticks ~15 ms) lost
            // music)
            let event = if process { Some(::windows::Win32::System::Threading::CreateEventW(None, false, false, None)?) } else { None };
            if event.is_some() {
                flags |= AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            }
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_100NS, 0, &fmt, None)?;
            if let Some(e) = event {
                client.SetEventHandle(e)?;
            }
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Stream { client, capture, channels: channels as usize, zeros: Vec::new() })
        }
    }

    /// Every packet captured since last asked, each with its time (see `stamp`) and flags, to `f`. `now` is the
    /// performance counter at this read (`qpc_now`).
    fn read(&mut self, now: i64, mut f: impl FnMut(crate::platform::interleave::Packet)) -> ::windows::core::Result<()> {
        unsafe {
            // frames queued: those behind a packet say when it was captured if the device doesn't (best effort: a
            // client that won't say leaves the packets timed from the read alone)
            let mut queued = self.client.GetCurrentPadding().unwrap_or(0) as usize;
            while self.capture.GetNextPacketSize()? > 0 {
                let mut data: *mut u8 = std::ptr::null_mut();
                let (mut frames, mut flags, mut pos, mut qpc) = (0u32, 0u32, 0u64, 0u64);
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, Some(&mut pos), Some(&mut qpc))?;
                let n = frames as usize * self.channels;
                let samples: &[f32] = if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                    self.zeros.resize(self.zeros.len().max(n), 0.0);
                    &self.zeros[..n]
                } else {
                    std::slice::from_raw_parts(data as *const f32, n)
                };
                queued = queued.saturating_sub(frames as usize);
                f(crate::platform::interleave::Packet {
                    time: stamp(qpc, flags, now),
                    read_at: now,
                    behind: queued,
                    discontinuity: flags & (AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32) != 0,
                    samples,
                });
                self.capture.ReleaseBuffer(frames)?;
            }
        }
        Ok(())
    }

    /// Everything captured since last asked, onto the end of `out` (the meter: no times needed).
    fn drain(&mut self, out: &mut VecDeque<f32>) -> ::windows::core::Result<()> {
        let now = qpc_now();
        self.read(now, |p| out.extend(p.samples.iter().copied()))
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
    }
}

/// Signals when ActivateAudioInterfaceAsync has finished.
#[::windows::core::implement(IActivateAudioInterfaceCompletionHandler)]
struct Activated(Arc<(Mutex<bool>, std::sync::Condvar)>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(&self, _op: ::windows::core::Ref<IActivateAudioInterfaceAsyncOperation>) -> ::windows::core::Result<()> {
        let (done, cv) = &*self.0;
        *done.lock().unwrap() = true;
        cv.notify_all();
        Ok(())
    }
}

/// An audio client capturing what process `pid` and its children play (Windows 10 2004+).
fn process_loopback_client(pid: u32) -> ::windows::core::Result<IAudioClient> {
    use ::windows::Win32::System::Com::{StructuredStorage::PROPVARIANT, BLOB};
    use ::windows::Win32::System::Variant::VT_BLOB;
    // The parameters on the heap, and the PROPVARIANT pointing at them never dropped: the windows crate's PROPVARIANT
    // runs PropVariantClear when dropped, which frees a VT_BLOB's data with CoTaskMemFree, and this data isn't
    // Windows' to free. (Freeing it was heap corruption, 0xc0000374, the moment a song started in Spotify: xps,
    // issue #1, 1 Nov 2026.)
    let params = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    });
    let mut pv = std::mem::ManuallyDrop::new(PROPVARIANT::default());
    unsafe {
        let inner = &mut *pv.Anonymous.Anonymous;
        inner.vt = VT_BLOB;
        inner.Anonymous.blob = BLOB { cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                                      pBlobData: &*params as *const _ as *mut u8 };
    }
    let signal = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let handler: IActivateAudioInterfaceCompletionHandler = Activated(signal.clone()).into();
    unsafe {
        let op = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*pv), &handler)?;
        let (done, cv) = &*signal;
        let (finished, _) = cv.wait_timeout_while(done.lock().unwrap(), Duration::from_secs(5), |d| !*d).unwrap();
        if !*finished {
            // still running: it may read the parameters later, so they're never freed (a few bytes, rarely)
            std::mem::forget(params);
            return Err(::windows::core::Error::from(::windows::Win32::Foundation::E_FAIL));
        }
        let (mut hr, mut unk) = (::windows::core::HRESULT(0), None);
        op.GetActivateResult(&mut hr, &mut unk)?;
        hr.ok()?;
        unk.ok_or_else(|| ::windows::core::Error::from(::windows::Win32::Foundation::E_FAIL))?.cast()
    }
}

/// How the running capture hears the music app: "process" (only it) or "system" (the whole default output).
pub static MODE: Mutex<&str> = Mutex::new("");

struct Running {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
    mic: (String, String),
}

static CURRENT: Mutex<Option<Running>> = Mutex::new(None);

/// Where the music comes from, to open its stream (again, if it stops).
#[derive(Clone)]
enum Source {
    /// one process and the programs it started (per-process loopback)
    Process(u32),
    /// the whole default output
    Output,
}

fn open_music(source: &Source, rate: u32) -> ::windows::core::Result<Stream> {
    match source {
        Source::Process(pid) => Stream::open_process(*pid, 2, rate),
        Source::Output => Stream::open(&unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)? }, true, 2, rate),
    }
}

fn open_mic(uid: &str, rate: u32) -> ::windows::core::Result<Stream> {
    let d = mic_device(uid).ok_or_else(|| ::windows::core::Error::from(::windows::Win32::Foundation::E_FAIL))?;
    Stream::open(&d, false, 1, rate)
}

/// Start capturing (see the module's notes): 0 with the rate, else 1 (the app isn't running), 3 (a device failed).
pub fn start(bundle: &str, cb: Callback, ctx: *mut c_void, out_rate: &mut f64) -> i32 {
    stop();
    if !bundle.is_empty() && !super::procs::exes_for(bundle).is_empty() && !super::procs::running(bundle) {
        return 1;
    }
    let mic = chosen_mic();
    let stop_flag = Arc::new(AtomicBool::new(false));
    // (the rate, and whether the mic opened)
    let (tx, rx) = std::sync::mpsc::channel::<Result<(f64, bool), i32>>();
    let ctx = ctx as usize;
    let flag = stop_flag.clone();
    let mic_uid = mic.as_ref().map(|m| m.0.clone());
    let root = super::procs::root_pid_for(bundle);
    let bundle = bundle.to_string();
    let thread = std::thread::spawn(move || {
        super::com();
        let opened = (|| -> ::windows::core::Result<(Stream, Source, Option<Stream>, u32)> {
            let out = unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)? };
            let rate = mix_rate(&out)?;
            // the app alone where Windows can; else everything the default output plays
            let process = match root {
                Some(pid) => Stream::open_process(pid, 2, rate).map_err(|e| format!("per-process loopback of pid {pid} failed: {e}")),
                None => Err("no process of it found".into()),
            };
            let (music, source, how) = match process {
                Ok(s) => {
                    *MODE.lock().unwrap() = "process";
                    let exes = super::procs::exes_for(&bundle);
                    let who = if exes.is_empty() { "OnKey itself".to_string() } else { exes.join(" / ") };
                    (s, Source::Process(root.unwrap_or(0)), format!("only {who} (pid {}) and the programs it started (per-process loopback)", root.unwrap_or(0)))
                }
                Err(why) => {
                    *MODE.lock().unwrap() = "system";
                    (Stream::open(&out, true, 2, rate)?, Source::Output, format!("everything the default output plays, other apps and OnKey's own sounds too (whole-output loopback; {why})"))
                }
            };
            // no mic, or one that won't open: the mic channel is silent (as on macOS with no input device)
            let mic_dev = mic_uid.as_deref().and_then(mic_device);
            let mic = mic_dev.as_ref().and_then(|d| Stream::open(d, false, 1, rate).ok());
            let mic_said = match (&mic_dev, &mic) {
                (Some(d), Some(_)) => format!("mic '{}' ({})", device_name(d), mix_format(d)),
                (Some(d), None) => format!("mic '{}' ({}) wouldn't open: its channel is silent", device_name(d), mix_format(d)),
                _ => "no mic: its channel is silent".into(),
            };
            crate::engine::log(&format!("capture (Windows): {how}; output \"{}\" ({}); {mic_said}; all taken at {rate} Hz, 32-bit float, paired by capture time",
                                        crate::digest::device_name(&device_name(&out)), mix_format(&out)));
            Ok((music, source, mic, rate))
        })();
        let (music, source, mic, rate) = match opened {
            Ok(x) => x,
            Err(e) => {
                crate::engine::log(&format!("capture (Windows): couldn't open the output or the mic: {e}"));
                let _ = tx.send(Err(3));
                return;
            }
        };
        let _ = tx.send(Ok((rate as f64, mic.is_some())));
        let mic_uid = mic.is_some().then(|| mic_uid.unwrap_or_default());
        pump(music, source, mic, mic_uid, rate, cb, ctx as *mut c_void, &flag);
    });
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok((rate, mic_open))) => {
            *out_rate = rate;
            // a mic that wouldn't open isn't the one listened to: none, so the window says there's no microphone
            let mic = if mic_open { mic.unwrap_or_default() } else { Default::default() };
            *CURRENT.lock().unwrap() = Some(Running { stop: stop_flag, thread, mic });
            0
        }
        _ => {
            stop_flag.store(true, Ordering::SeqCst);
            3
        }
    }
}

/// A capture stream read every tick, opened again (every 2 s, as long as the capture runs) when a read fails: the
/// app's stream ending and starting again, a device going away and back. The other stream carries on meanwhile
/// (its frames get silence for this one), and the engine's frames never stop.
struct Fed {
    name: &'static str,
    stream: Option<Stream>,
    retry_at: Option<Instant>,
    /// the failure logged (once), so the recovery is too
    failed: bool,
}

impl Fed {
    fn new(name: &'static str, stream: Option<Stream>) -> Fed {
        Fed { name, stream, retry_at: None, failed: false }
    }

    fn read(&mut self, now: i64, open: impl Fn() -> ::windows::core::Result<Stream>, f: impl FnMut(crate::platform::interleave::Packet)) {
        match &mut self.stream {
            Some(s) => {
                if let Err(e) = s.read(now, f) {
                    crate::engine::log(&format!("capture (Windows): the {} stream stopped ({e}); opening it again", self.name));
                    self.stream = None;
                    self.failed = true;
                    self.retry_at = Some(Instant::now() + Duration::from_secs(2));
                }
            }
            None => {
                if self.retry_at.is_some_and(|at| Instant::now() >= at) {
                    match open() {
                        Ok(s) => {
                            crate::engine::log(&format!("capture (Windows): the {} stream is open again", self.name));
                            self.stream = Some(s);
                            self.retry_at = None;
                        }
                        Err(_) => self.retry_at = Some(Instant::now() + Duration::from_secs(2)),
                    }
                }
            }
        }
    }
}

/// Hands the engine [music L, music R, mic] frames every 10 ms, paired by capture time (platform/interleave.rs):
/// the output runs on the performance counter at the rate, a little behind now, so the engine's clock never stops
/// whether or not anything plays or a mic is there. Every 10 minutes and at the end, one line says what was
/// captured and how the two were lined up (offset at delivery, drift, gaps, the delay), for a tester's report.
#[allow(clippy::too_many_arguments)]
fn pump(music: Stream, source: Source, mic: Option<Stream>, mic_uid: Option<String>, rate: u32, cb: Callback, ctx: *mut c_void, stop: &AtomicBool) {
    let mut al = crate::platform::interleave::Aligner::new(rate);
    let mut frame = Vec::<f32>::new();
    let mut seen = crate::digest::Captured::default();
    let mut said_at = Instant::now();
    let mut music = Fed::new("music", Some(music));
    let mut mic = Fed::new("mic", mic);
    while !stop.load(Ordering::SeqCst) {
        if said_at.elapsed() > Duration::from_secs(600) {
            said_at = Instant::now();
            crate::engine::log(&format!("capture (Windows) so far: {}; {}", seen.line(0.01), al.summary()));
        }
        std::thread::sleep(Duration::from_millis(10));
        let now = qpc_now();
        music.read(now, || open_music(&source, rate), |p| al.push_music(p));
        if let Some(uid) = &mic_uid {
            mic.read(now, || open_mic(uid, rate), |p| al.push_mic(p));
        }
        frame.clear();
        let n = al.take(now, &mut frame);
        if n > 0 {
            cb(frame.as_ptr(), n as i32, ctx);
            seen.add(&frame);
        }
    }
    crate::engine::log(&format!("capture (Windows) stopped: {}; {}", seen.line(0.01), al.summary()));
}

pub fn stop() {
    if let Some(r) = CURRENT.lock().unwrap().take() {
        r.stop.store(true, Ordering::SeqCst);
        let _ = r.thread.join();
    }
}

/// The mic the running capture listens to: (id, name), "" for both when none (or it wouldn't open).
pub fn capture_mic() -> (String, String) {
    CURRENT.lock().unwrap().as_ref().map(|r| r.mic.clone()).unwrap_or_default()
}

/// How long after an app plays a sound it's heard on the default output: the stream's latency plus one device
/// period, in ms (WASAPI's own figures; the engine measures the rest).
pub fn output_latency_ms() -> i32 {
    (|| -> ::windows::core::Result<i32> {
        unsafe {
            let d = enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let c: IAudioClient = d.Activate(CLSCTX_ALL, None)?;
            let f = c.GetMixFormat()?;
            let r = c.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, BUFFER_100NS, 0, f, None);
            CoTaskMemFree(Some(f as *const c_void));
            r?;
            let (mut period, mut min) = (0i64, 0i64);
            c.GetDevicePeriod(Some(&mut period), Some(&mut min))?;
            let lat = c.GetStreamLatency()?;
            Ok(((lat + period) / 10_000) as i32)
        }
    })().unwrap_or(0)
}

/// Whether a process of the app a bundle id stands for has an audio session playing now (no capture needed).
/// Listening asks this of every app in turn: one look at the sessions (and, only when one plays, one process list)
/// answers them all for `FRESH`.
pub fn app_sounding(bundle: &str) -> bool {
    const FRESH: Duration = Duration::from_millis(250);
    static LOOKED: Mutex<Option<(Instant, Vec<u32>, Vec<(u32, String)>)>> = Mutex::new(None);
    let mut looked = LOOKED.lock().unwrap_or_else(|e| e.into_inner());
    if looked.as_ref().is_none_or(|(at, _, _)| at.elapsed() > FRESH) {
        let active = active_session_pids();
        let procs = if active.is_empty() { Vec::new() } else { super::procs::list() };
        *looked = Some((Instant::now(), active, procs));
    }
    let (_, active, procs) = looked.as_ref().unwrap();
    !active.is_empty() && super::procs::pids_in(procs, bundle).iter().any(|p| active.contains(p))
}

/// The processes with an audio session playing on the default output now.
fn active_session_pids() -> Vec<u32> {
    (|| -> ::windows::core::Result<Vec<u32>> {
        let mut out = Vec::new();
        unsafe {
            let d = enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let mgr: IAudioSessionManager2 = d.Activate(CLSCTX_ALL, None)?;
            let all = mgr.GetSessionEnumerator()?;
            for i in 0..all.GetCount()? {
                let s = all.GetSession(i)?;
                if s.GetState()? != AudioSessionStateActive {
                    continue;
                }
                let s2: IAudioSessionControl2 = s.cast()?;
                out.push(s2.GetProcessId()?);
            }
        }
        Ok(out)
    })().unwrap_or_default()
}

// ---------------------------------------------------------------- the mic meter (setup's mic check)

static METER_STOP: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
static METER_LEVEL: AtomicU32 = AtomicU32::new(0xC2F0_0000);   // -120.0
static METER_BLOCKS: AtomicI32 = AtomicI32::new(0);

/// Start (true) or stop the meter: 0 if it's running afterwards (or stopped when asked), 1 no input device, 2 it
/// wouldn't open.
pub fn meter(on: bool) -> i32 {
    if let Some(s) = METER_STOP.lock().unwrap().take() {
        s.store(true, Ordering::SeqCst);
    }
    METER_LEVEL.store((-120f32).to_bits(), Ordering::SeqCst);
    METER_BLOCKS.store(0, Ordering::SeqCst);
    if !on {
        return 0;
    }
    let Some((uid, _)) = chosen_mic() else { return 1 };
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    let flag = stop.clone();
    std::thread::spawn(move || {
        super::com();
        let Some(mut s) = mic_device(&uid).and_then(|d| Stream::open(&d, false, 1, 48_000).ok()) else {
            let _ = tx.send(false);
            return;
        };
        let _ = tx.send(true);
        let mut buf = VecDeque::new();
        while !flag.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(20));
            if s.drain(&mut buf).is_err() {
                break;
            }
            if buf.is_empty() {
                continue;
            }
            let ms = buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32;
            let db = if ms > 0.0 { (10.0 * ms.log10()).max(-120.0) } else { -120.0 };
            METER_LEVEL.store(db.to_bits(), Ordering::SeqCst);
            METER_BLOCKS.fetch_add(1, Ordering::SeqCst);
            buf.clear();
        }
    });
    if rx.recv_timeout(Duration::from_secs(3)).unwrap_or(false) {
        *METER_STOP.lock().unwrap() = Some(stop);
        0
    } else {
        stop.store(true, Ordering::SeqCst);
        2
    }
}

pub fn meter_level() -> f32 {
    f32::from_bits(METER_LEVEL.load(Ordering::SeqCst))
}

pub fn meter_blocks() -> i32 {
    METER_BLOCKS.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::interleave::{Aligner, TICKS};

    /// What one stream's stamps look like over a second of reading.
    #[derive(Default)]
    struct Stamps {
        packets: u64,
        untimed: u64,
        discontinuities: u64,
        frames: u64,
        /// ticks between a packet's stamp and the read (min, max)
        lag: Option<(i64, i64)>,
        /// a packet's stamp against the previous one's plus its frames, in ticks (min, max): the stamps' jitter
        step: Option<(i64, i64)>,
        last: Option<(i64, u64)>,
        backwards: u64,
    }

    impl Stamps {
        fn add(&mut self, p: &crate::platform::interleave::Packet, ch: usize, period: f64) {
            let n = (p.samples.len() / ch) as u64;
            self.packets += 1;
            self.frames += n;
            self.discontinuities += p.discontinuity as u64;
            let Some(t) = p.time else {
                self.untimed += 1;
                return;
            };
            let lag = p.read_at - t;
            self.lag = Some(self.lag.map_or((lag, lag), |(a, b)| (a.min(lag), b.max(lag))));
            if let Some((pt, pn)) = self.last {
                let step = t - (pt + (pn as f64 * period).round() as i64);
                self.step = Some(self.step.map_or((step, step), |(a, b)| (a.min(step), b.max(step))));
                self.backwards += (t < pt) as u64;
            }
            self.last = Some((t, n));
        }

        fn line(&self, name: &str) -> String {
            let us = |(a, b): (i64, i64)| format!("{:+.0}..{:+.0} µs", a as f64 / 10.0, b as f64 / 10.0);
            format!("{name}: {} packets ({} frames) in a second, {} untimed, {} discontinuities, {} stamps going backwards; stamp to read {}; stamp against the previous packet's end {}",
                    self.packets, self.frames, self.untimed, self.discontinuities, self.backwards,
                    self.lag.map_or("n/a".into(), us), self.step.map_or("n/a".into(), us))
        }
    }

    /// What this PC's WASAPI stamps its packets with, for the CI log and a developer's PC: the default output's
    /// loopback and the default mic read for a second, how their stamps behave (given at all, in order, how far
    /// from the counter at the read, the step between packets against their frames), and what the aligner makes
    /// of them. A PC with no audio device (GitHub's runners) says so and passes. The lines go to
    /// %TEMP%\onkey-wasapi-timing.txt, which the Windows workflow prints.
    #[test]
    fn wasapi_packet_times_on_this_pc() {
        super::super::com();
        const RATE: u32 = 48_000;
        let period = TICKS / RATE as f64;
        let mut lines = vec![format!("WASAPI packet times (QPC) on this PC; devices: {}", describe_devices())];
        let open = |flow, loopback, ch| -> Result<Stream, String> {
            let d = unsafe { enumerator().and_then(|e| e.GetDefaultAudioEndpoint(flow, eConsole)) }.map_err(|e| format!("no default device: {e}"))?;
            Stream::open(&d, loopback, ch, RATE).map_err(|e| format!("wouldn't open: {e}"))
        };
        let mut music = open(eRender, true, 2);
        let mut mic = open(eCapture, false, 1);
        if let Err(e) = &music {
            lines.push(format!("output loopback: {e}"));
        }
        if let Err(e) = &mic {
            lines.push(format!("mic: {e}"));
        }
        let (mut sm, mut sv) = (Stamps::default(), Stamps::default());
        let mut al = Aligner::new(RATE);
        let mut frames = Vec::new();
        let mut given = 0usize;
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(1) {
            std::thread::sleep(Duration::from_millis(10));
            let now = qpc_now();
            if let Ok(s) = music.as_mut() {
                let _ = s.read(now, |p| {
                    sm.add(&p, 2, period);
                    al.push_music(p);
                });
            }
            if let Ok(s) = mic.as_mut() {
                let _ = s.read(now, |p| {
                    sv.add(&p, 1, period);
                    al.push_mic(p);
                });
            }
            frames.clear();
            given += al.take(now, &mut frames);
        }
        if music.is_ok() {
            lines.push(sm.line("output loopback"));
        }
        if mic.is_ok() {
            lines.push(sv.line("mic"));
        }
        lines.push(format!("aligner over that second: {given} frames given; {}", al.summary()));
        let text = lines.join("\n");
        println!("{text}");
        let _ = std::fs::write(std::env::temp_dir().join("onkey-wasapi-timing.txt"), &text);
        assert_eq!((sm.backwards, sv.backwards), (0, 0), "stamps went backwards: {text}");
    }
}
