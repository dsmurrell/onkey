//! Audio on Windows through WASAPI: what the default output plays (loopback) together with the microphone, as the
//! engine wants it ([music L, music R, mic] frames at one rate, given to a callback), the microphones there are,
//! and a level meter for setup's mic check.
//!
//! First version, not yet run on a Windows machine:
//! - loopback is of the whole default output, not just the music app's (per-process loopback, Windows 10 2004+,
//!   through ActivateAudioInterfaceAsync with AUDIOCLIENT_ACTIVATION_PARAMS, is the next step); so anything else
//!   playing, OnKey's own sounds included, is heard too;
//! - music and mic are two WASAPI clients asked for the same rate (Windows converts), lined up by the mic's clock;
//!   they aren't sample-aligned the way macOS's aggregate device makes them, so a small, steady offset between them
//!   is likely and has to be measured (the engine already learns an output's latency).
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
            Ok(Stream { client, capture, channels: channels as usize })
        }
    }

    /// Everything captured since last asked, onto the end of `out`.
    fn read(&self, out: &mut VecDeque<f32>) -> ::windows::core::Result<()> {
        unsafe {
            while self.capture.GetNextPacketSize()? > 0 {
                let mut data: *mut u8 = std::ptr::null_mut();
                let (mut frames, mut flags) = (0u32, 0u32);
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let n = frames as usize * self.channels;
                if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                    out.extend(std::iter::repeat(0.0).take(n));
                } else {
                    out.extend(std::slice::from_raw_parts(data as *const f32, n).iter().copied());
                }
                self.capture.ReleaseBuffer(frames)?;
            }
        }
        Ok(())
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
        let opened = (|| -> ::windows::core::Result<(Stream, Option<Stream>, u32)> {
            let out = unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)? };
            let rate = mix_rate(&out)?;
            // the app alone where Windows can; else everything the default output plays
            let process = match root {
                Some(pid) => Stream::open_process(pid, 2, rate).map_err(|e| format!("per-process loopback of pid {pid} failed: {e}")),
                None => Err("no process of it found".into()),
            };
            let (music, how) = match process {
                Ok(s) => {
                    *MODE.lock().unwrap() = "process";
                    let exes = super::procs::exes_for(&bundle);
                    let who = if exes.is_empty() { "OnKey itself".to_string() } else { exes.join(" / ") };
                    (s, format!("only {who} (pid {}) and the programs it started (per-process loopback)", root.unwrap_or(0)))
                }
                Err(why) => {
                    *MODE.lock().unwrap() = "system";
                    (Stream::open(&out, true, 2, rate)?, format!("everything the default output plays, other apps and OnKey's own sounds too (whole-output loopback; {why})"))
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
            crate::engine::log(&format!("capture (Windows): {how}; output \"{}\" ({}); {mic_said}; all taken at {rate} Hz, 32-bit float",
                                        crate::digest::device_name(&device_name(&out)), mix_format(&out)));
            Ok((music, mic, rate))
        })();
        let (music, mic, rate) = match opened {
            Ok(x) => x,
            Err(e) => {
                crate::engine::log(&format!("capture (Windows): couldn't open the output or the mic: {e}"));
                let _ = tx.send(Err(3));
                return;
            }
        };
        let _ = tx.send(Ok((rate as f64, mic.is_some())));
        pump(&music, mic.as_ref(), rate, cb, ctx as *mut c_void, &flag);
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

/// Hands the engine [music L, music R, mic] frames every 10 ms. The mic's clock leads when there's a mic (the
/// loopback client sends nothing at all while nothing plays); with none, the wall clock does, so the engine's
/// clock never stops either way. The music is held back by a fixed cushion and cut back to it when it runs ahead
/// (clock drift): platform/interleave.rs.
fn pump(music: &Stream, mic: Option<&Stream>, rate: u32, cb: Callback, ctx: *mut c_void, stop: &AtomicBool) {
    let mut m = crate::platform::interleave::Interleave::new(rate);
    let mut v = VecDeque::<f32>::new();
    let mut frame = Vec::<f32>::new();
    let started = Instant::now();
    let mut given: u64 = 0;
    // what was captured: logged when it stops, and every 10 minutes while it runs
    let mut seen = crate::digest::Captured::default();
    let mut said_at = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        if said_at.elapsed() > Duration::from_secs(600) {
            said_at = Instant::now();
            crate::engine::log(&format!("capture (Windows) so far: {}", seen.line(0.01)));
        }
        std::thread::sleep(Duration::from_millis(10));
        if music.read(&mut m.music).is_err() {
            break;
        }
        let n = match mic {
            Some(s) => {
                if s.read(&mut v).is_err() {
                    break;
                }
                v.len()
            }
            None => {
                let due = (started.elapsed().as_secs_f64() * rate as f64) as u64;
                let n = due.saturating_sub(given) as usize;
                v.extend(std::iter::repeat(0.0).take(n));
                n
            }
        };
        frame.clear();
        let took = m.take(n, &mut v, &mut frame);
        seen.dropped += took.cut as u64;
        seen.short += took.short as u64;
        if n > 0 {
            cb(frame.as_ptr(), n as i32, ctx);
            given += n as u64;
            seen.add(&frame);
        }
    }
    crate::engine::log(&format!("capture (Windows) stopped: {}", seen.line(0.01)));
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
        let Some(s) = mic_device(&uid).and_then(|d| Stream::open(&d, false, 1, 48_000).ok()) else {
            let _ = tx.send(false);
            return;
        };
        let _ = tx.send(true);
        let mut buf = VecDeque::new();
        while !flag.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(20));
            if s.read(&mut buf).is_err() {
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
