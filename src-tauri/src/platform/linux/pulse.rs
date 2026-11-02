//! Audio on Linux through the sound server, spoken to in the PulseAudio protocol (the `pulseaudio` crate: plain
//! Rust over the server's Unix socket, nothing to link). PipeWire, the sound server of most Linux desktops now,
//! answers the same protocol through pipewire-pulse; older desktops run PulseAudio itself. What this gives the
//! engine, as Windows' audio.rs does: the music app's sound together with the microphone ([music L, music R, mic]
//! frames at one rate, given to a callback), the microphones there are, whether an app plays sound, and a level
//! meter for setup's mic check. render.rs plays OnKey's own sound the same way.
//!
//! How the music app is heard: only its own stream ("per-app": a record stream on the output's monitor, tied to the
//! app's playback stream, which is what pavucontrol's per-app meters use). If the server won't do that, or
//! ONKEY_LINUX_CAPTURE=system is set, everything the default output plays is heard instead (its monitor), OnKey's
//! own sounds and other apps' too. The log says which.
//!
//! First version, not yet run with a real music app (CI has no sound): see README.md's Linux status.
use std::collections::VecDeque;
use std::ffi::{c_void, CStr, CString};
use std::io::{BufReader, Cursor, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pulseaudio::protocol as pa;

/// The callback the engine gives (capture.rs `on_audio`): interleaved frames, how many, its context.
pub type Callback = extern "C" fn(*const f32, i32, *mut c_void);

// ---------------------------------------------------------------- talking to the server

/// Where the server's socket is: $PULSE_SERVER (unix:…), $XDG_RUNTIME_DIR/pulse/native, /run/user/<uid>/pulse/native.
fn socket_path() -> Option<std::path::PathBuf> {
    let uid = unsafe { libc::getuid() };
    std::env::var("PULSE_SERVER").ok().and_then(|s| s.strip_prefix("unix:").map(std::path::PathBuf::from)).into_iter()
        .chain(std::env::var("PULSE_RUNTIME_PATH").ok().map(|s| std::path::PathBuf::from(s).join("native")))
        .chain(std::env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty()).map(|s| std::path::PathBuf::from(s).join("pulse/native")))
        .chain(Some(std::path::PathBuf::from(format!("/run/user/{uid}/pulse/native"))))
        .find(|p| p.exists())
}

/// One message from the server.
pub(super) enum Msg {
    /// audio for a stream (its channel)
    Data(u32, Vec<u8>),
    /// a command the server sent by itself (a request for more audio, a stream killed…)
    Command(Box<pa::Command>),
    /// a reply (or an error) to one of ours, by its sequence number: the whole message, to be read as its kind
    Reply(u32, Vec<u8>),
}

/// A connection to the server.
pub(super) struct Conn {
    sock: BufReader<UnixStream>,
    pub ver: u16,
    seq: u32,
}

impl Conn {
    /// Connect and introduce ourselves as `name` ("OnKey").
    pub fn open(name: &str) -> Result<Conn, String> {
        let path = socket_path().ok_or("no sound server socket (neither pipewire-pulse nor PulseAudio is running)")?;
        let s = UnixStream::connect(&path).map_err(|e| format!("couldn't connect to {}: {e}", path.display()))?;
        s.set_write_timeout(Some(Duration::from_secs(2))).ok();
        let mut c = Conn { sock: BufReader::new(s), ver: pa::MAX_VERSION, seq: 0 };
        let home = std::env::var("HOME").unwrap_or_default();
        let cookie = std::env::var("PULSE_COOKIE").ok().into_iter().chain([format!("{home}/.config/pulse/cookie"), format!("{home}/.pulse-cookie")])
            .find_map(|p| std::fs::read(p).ok()).unwrap_or_default();
        let auth: pa::AuthReply = c.request(pa::Command::Auth(pa::AuthParams { version: pa::MAX_VERSION, supports_shm: false, supports_memfd: false, cookie }))?;
        c.ver = pa::MAX_VERSION.min(auth.version);
        let mut props = pa::Props::new();
        props.set(pa::Prop::ApplicationName, CString::new(name).unwrap_or_default());
        props.set(pa::Prop::ApplicationProcessId, CString::new(std::process::id().to_string()).unwrap_or_default());
        let _: pa::SetClientNameReply = c.request(pa::Command::SetClientName(props))?;
        Ok(c)
    }

    pub fn send(&mut self, cmd: &pa::Command) -> Result<u32, String> {
        self.seq += 1;
        pa::write_command_message(self.sock.get_mut(), self.seq, cmd, self.ver).map_err(|e| e.to_string())?;
        Ok(self.seq)
    }

    /// Send a command and wait (up to 3 s) for its reply, skipping anything else that comes meanwhile.
    pub fn request<T: pa::CommandReply>(&mut self, cmd: pa::Command) -> Result<T, String> {
        let seq = self.send(&cmd)?;
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() || !self.ready(left) {
                return Err("the sound server didn't answer".into());
            }
            if let Msg::Reply(s, buf) = self.next()? {
                if s == seq {
                    return reply(&buf, self.ver);
                }
            }
        }
    }

    /// A message is there to read now, or comes within `wait`.
    pub fn ready(&self, wait: Duration) -> bool {
        if !self.sock.buffer().is_empty() {
            return true;
        }
        let mut p = libc::pollfd { fd: self.sock.get_ref().as_raw_fd(), events: libc::POLLIN, revents: 0 };
        unsafe { libc::poll(&mut p, 1, wait.as_millis().min(i32::MAX as u128) as i32) > 0 }
    }

    /// The next message (waits for it).
    pub fn next(&mut self) -> Result<Msg, String> {
        let desc = pa::read_descriptor(&mut self.sock).map_err(|e| format!("the sound server went away ({e})"))?;
        let mut payload = vec![0u8; desc.length as usize];
        self.sock.read_exact(&mut payload).map_err(|e| format!("the sound server went away ({e})"))?;
        if desc.channel != u32::MAX {
            return Ok(Msg::Data(desc.channel, payload));
        }
        // a command's payload starts with its tag and sequence number, each a 'L' and 4 bytes
        let be = |i: usize| payload.get(i..i + 4).map_or(u32::MAX, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
        let (tag, seq) = (be(1), be(6));
        if tag == 0 || tag == 2 {
            // Error or Reply: kept whole, to be read as the kind of reply expected
            let mut buf = vec![0u8; pa::DESCRIPTOR_SIZE];
            pa::encode_descriptor((&mut buf[..pa::DESCRIPTOR_SIZE]).try_into().unwrap(), &desc);
            buf.extend_from_slice(&payload);
            return Ok(Msg::Reply(seq, buf));
        }
        match pa::Command::read_tag_prefixed(&mut Cursor::new(&payload), self.ver) {
            Ok((_, c)) => Ok(Msg::Command(Box::new(c))),
            // (one this crate doesn't know: nothing OnKey needs)
            Err(_) => Ok(Msg::Command(Box::new(pa::Command::Exit))),
        }
    }

    /// Audio for a playback stream.
    pub fn write_audio(&mut self, channel: u32, bytes: &[u8]) -> Result<(), String> {
        pa::write_memblock(self.sock.get_mut(), channel, bytes, 0).map_err(|e| e.to_string())?;
        self.sock.get_mut().flush().map_err(|e| e.to_string())
    }
}

/// A reply message read as `T` (an error reply as its error).
pub(super) fn reply<T: pa::CommandReply>(buf: &[u8], ver: u16) -> Result<T, String> {
    pa::read_reply_message::<T>(&mut Cursor::new(buf), ver).map(|(_, t)| t).map_err(|e| format!("the sound server said: {e}"))
}

fn text(c: &CStr) -> String {
    c.to_string_lossy().into_owned()
}

fn prop(p: &pa::Props, which: pa::Prop) -> String {
    p.get(which).map(|b| String::from_utf8_lossy(b).trim_end_matches('\0').to_string()).unwrap_or_default()
}

/// The connection the device questions go through: one, kept open (opened again after an error, but not more than
/// every 2 s while the server is away, so a missing server costs nothing).
struct Query {
    conn: Option<Conn>,
    failed: Option<Instant>,
}

static QUERY: Mutex<Query> = Mutex::new(Query { conn: None, failed: None });

/// What the server said, kept for a second: the engine asks several of these questions every few seconds.
type Kept<T> = Mutex<Option<(Instant, T)>>;
static SERVER: Kept<pa::ServerInfo> = Mutex::new(None);
static SINKS: Kept<Vec<pa::SinkInfo>> = Mutex::new(None);
static SOURCES: Kept<Vec<pa::SourceInfo>> = Mutex::new(None);
static INPUTS: Kept<Vec<pa::SinkInputInfo>> = Mutex::new(None);
const KEEP: Duration = Duration::from_secs(1);

/// Ask the server `cmd` on the query connection (or answer from `kept` if it's under a second old).
fn ask<T: pa::CommandReply + Clone>(kept: &Kept<T>, cmd: pa::Command) -> Result<T, String> {
    if let Some((at, v)) = kept.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if at.elapsed() < KEEP {
            return Ok(v.clone());
        }
    }
    let mut q = QUERY.lock().unwrap_or_else(|e| e.into_inner());
    if q.conn.is_none() {
        if q.failed.is_some_and(|f| f.elapsed() < Duration::from_secs(2)) {
            return Err("no sound server (tried under 2 s ago)".into());
        }
        match Conn::open("OnKey") {
            Ok(c) => {
                q.conn = Some(c);
                q.failed = None;
            }
            Err(e) => {
                q.failed = Some(Instant::now());
                return Err(e);
            }
        }
    }
    let r = q.conn.as_mut().map_or_else(|| Err("no connection".to_string()), |c| c.request::<T>(cmd));
    match &r {
        Ok(v) => *kept.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), v.clone())),
        Err(_) => {
            // (opened afresh next time: the server may have restarted)
            q.conn = None;
            q.failed = Some(Instant::now());
        }
    }
    r
}

/// What the server says of itself: its name and version, the default output and mic, the rate it mixes at.
fn server_info() -> Result<pa::ServerInfo, String> {
    ask(&SERVER, pa::Command::GetServerInfo)
}

fn sinks() -> Vec<pa::SinkInfo> {
    ask(&SINKS, pa::Command::GetSinkInfoList).unwrap_or_default()
}

fn sources() -> Vec<pa::SourceInfo> {
    ask(&SOURCES, pa::Command::GetSourceInfoList).unwrap_or_default()
}

fn sink_inputs() -> Vec<pa::SinkInputInfo> {
    ask(&INPUTS, pa::Command::GetSinkInputInfoList).unwrap_or_default()
}

/// "PulseAudio (on PipeWire 1.0.5) 15.0.0, 48000 Hz" for the log, or why there's none.
pub fn describe_server() -> String {
    match server_info() {
        Ok(s) => format!("{} {} at {} Hz; default output \"{}\", default mic '{}'", s.server_name.as_deref().map(text).unwrap_or_default(),
                         s.server_version.as_deref().map(text).unwrap_or_default(), s.sample_spec.sample_rate,
                         s.default_sink_name.as_deref().map(text).unwrap_or_default(), s.default_source_name.as_deref().map(text).unwrap_or_default()),
        Err(e) => format!("none ({e}): OnKey can't hear the music or the mic"),
    }
}

/// Every output and microphone, with its format, for the log: outputs as output "…", microphones as mic '…' (left
/// out of a report, as on the Mac).
pub fn describe_devices() -> String {
    let def = server_info().ok();
    let mut out = Vec::new();
    for s in sinks() {
        let d = def.as_ref().and_then(|d| d.default_sink_name.as_deref()) == Some(s.name.as_c_str());
        out.push(format!("output \"{}\" ({} Hz, {} ch{})", crate::digest::device_name(&s.description.as_deref().map(text).unwrap_or_else(|| text(&s.name))),
                         s.sample_spec.sample_rate, s.sample_spec.channels, if d { ", default" } else { "" }));
    }
    for s in sources().into_iter().filter(|s| s.monitor_of_sink_index.is_none()) {
        let d = def.as_ref().and_then(|d| d.default_source_name.as_deref()) == Some(s.name.as_c_str());
        out.push(format!("mic '{}' ({} Hz, {} ch{})", s.description.as_deref().map(text).unwrap_or_else(|| text(&s.name)),
                         s.sample_spec.sample_rate, s.sample_spec.channels, if d { ", default" } else { "" }));
    }
    if out.is_empty() { "none found".into() } else { out.join("; ") }
}

// ---------------------------------------------------------------- devices

/// The default output: (name, description).
pub fn output() -> Option<(String, String)> {
    let name = server_info().ok()?.default_sink_name?;
    let s = sinks().into_iter().find(|s| s.name == name)?;
    Some((text(&s.name), s.description.as_deref().map(text).unwrap_or_else(|| text(&s.name))))
}

/// Every microphone (input that isn't an output's monitor): (name, description).
pub fn mics() -> Vec<(String, String)> {
    sources().into_iter().filter(|s| s.monitor_of_sink_index.is_none())
        .map(|s| (text(&s.name), s.description.as_deref().map(text).unwrap_or_else(|| text(&s.name)))).collect()
}

/// The microphone preferred in Settings (a name from `mics`), "" for the default.
static PREFERENCE: Mutex<String> = Mutex::new(String::new());

pub fn set_preference(uid: &str) {
    *PREFERENCE.lock().unwrap_or_else(|e| e.into_inner()) = uid.to_string();
}

/// The mic OnKey would listen to now: the preferred one while it's there, else the server's default input (if that
/// isn't a monitor), else the first.
pub fn chosen_mic() -> Option<(String, String)> {
    let pref = PREFERENCE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let all = mics();
    if let Some(m) = all.iter().find(|m| !pref.is_empty() && m.0 == pref) {
        return Some(m.clone());
    }
    let def = server_info().ok().and_then(|s| s.default_source_name).map(|n| text(&n));
    all.iter().find(|m| Some(&m.0) == def.as_ref()).cloned().or_else(|| all.into_iter().next())
}

/// How long after an app plays a sound it's heard on the default output: the output's own latency, in ms.
pub fn output_latency_ms() -> i32 {
    let Some(name) = server_info().ok().and_then(|s| s.default_sink_name) else { return 0 };
    sinks().into_iter().find(|s| s.name == name).map_or(0, |s| (s.actual_latency.max(s.configured_latency) / 1000) as i32)
}

/// The app a bundle id stands for, as the server knows its streams: by program name or process id (OnKey's own:
/// this process).
fn is_app(i: &pa::SinkInputInfo, bundle: &str, pids: &dyn Fn() -> Vec<u32>) -> bool {
    let pid = prop(&i.props, pa::Prop::ApplicationProcessId).parse::<u32>().ok();
    if super::procs::own(bundle) {
        return pid == Some(std::process::id());
    }
    let exe = prop(&i.props, pa::Prop::ApplicationProcessBinary).to_lowercase();
    let exes = super::procs::exes_for(bundle);
    (!exe.is_empty() && exes.contains(&exe.as_str())) || pid.is_some_and(|p| pids().contains(&p))
        || (bundle == "com.spotify.client" && prop(&i.props, pa::Prop::ApplicationName).eq_ignore_ascii_case("spotify"))
}

/// Which of an app's streams (index, paused, name) to hear: a playing one named after the song (a browser names
/// each tab's stream after its title, which names the song), else the first (playing ones first, the newest first).
/// romulka, issue #1 (2 Nov 2026): another tab's stream, open but silent, was heard while YouTube played the song.
fn choose(streams: &[(u32, bool, String)], song: &str) -> Option<u32> {
    streams.iter().find(|s| !s.1 && names(s, song)).or(streams.first()).map(|s| s.0)
}

/// Whether a stream's name names the song (a name of 3 letters or more).
fn names(stream: &(u32, bool, String), song: &str) -> bool {
    let song = song.trim().to_lowercase();
    song.chars().count() >= 3 && stream.2.to_lowercase().contains(&song)
}

/// Whether to leave the stream heard (`mine`) for the one `choose` picks: when that one plays, and `mine` is paused
/// or gone, or it's named after the song and `mine` isn't.
fn switch(streams: &[(u32, bool, String)], song: &str, mine: u32) -> bool {
    let find = |id: u32| streams.iter().find(|s| s.0 == id);
    let Some(c) = choose(streams, song).filter(|c| *c != mine).and_then(find).filter(|c| !c.1) else { return false };
    match find(mine) {
        None => true,
        Some(m) => m.1 || (names(c, song) && !names(m, song)),
    }
}

/// The song a browser plays, by name, when `bundle` is the browser followed ("" otherwise).
fn song_for(bundle: &str) -> String {
    if super::browsers::audio_bundle() == bundle { super::browsers::song() } else { String::new() }
}

fn streams(inputs: &[pa::SinkInputInfo]) -> Vec<(u32, bool, String)> {
    inputs.iter().map(|i| (i.index, i.corked, i.name.to_string_lossy().into_owned())).collect()
}

/// The app's playback streams now: playing ones first, the newest first.
fn app_inputs(bundle: &str) -> Vec<pa::SinkInputInfo> {
    // (the process list only when a stream doesn't say its program, read once)
    let pids = std::cell::OnceCell::new();
    let pids = || pids.get_or_init(|| super::procs::pids_for(bundle)).clone();
    let mut v: Vec<_> = sink_inputs().into_iter().filter(|i| is_app(i, bundle, &pids)).collect();
    v.sort_by_key(|i| (i.corked, std::cmp::Reverse(i.index)));
    v
}

/// Whether the app a bundle id stands for has a playback stream playing now (no capture needed).
pub fn app_sounding(bundle: &str) -> bool {
    app_inputs(bundle).iter().any(|i| !i.corked)
}

// ---------------------------------------------------------------- recording

/// What to record.
#[derive(Debug, Clone)]
enum Target {
    /// a source by name (a mic, or an output's monitor)
    Source(String),
    /// one app's playback stream (sink input), through the monitor of the output it plays on
    App { input: u32, monitor: u32 },
}

/// Why recording ended.
enum Ended {
    Stopped,
    /// the server ended the stream (the app's stream went, or the device did)
    Killed,
    /// the stream couldn't be made (`String`: the server's word)
    Refused(String),
    Lost(String),
}

/// Record `target` as `channels` float channels at `rate` onto the end of `out` until `stop` (or the stream ends).
/// `check`: asked about once a second; true ends it (to follow a newer stream of the app).
fn record(target: &Target, channels: u8, rate: u32, out: &Mutex<VecDeque<f32>>, stop: &AtomicBool,
          mut check: impl FnMut(&Target) -> bool) -> Ended {
    let mut c = match Conn::open("OnKey") {
        Ok(c) => c,
        Err(e) => return Ended::Lost(e),
    };
    let map = if channels == 1 { pa::ChannelMap::mono() } else { pa::ChannelMap::stereo() };
    let mut params = pa::RecordStreamParams {
        sample_spec: pa::SampleSpec { format: pa::SampleFormat::Float32Le, channels, sample_rate: rate },
        channel_map: map,
        // fragments of 20 ms
        buffer_attr: pa::stream::BufferAttr { fragment_size: rate / 50 * channels as u32 * 4, ..Default::default() },
        flags: pa::stream::StreamFlags { adjust_latency: true, ..Default::default() },
        ..Default::default()
    };
    params.props.set(pa::Prop::MediaName, c"OnKey listening");
    params.props.set_bytes(c"media.role", c"production".to_bytes_with_nul());
    match target {
        Target::Source(name) => params.source_name = CString::new(name.as_str()).ok(),
        Target::App { input, monitor } => {
            params.source_index = Some(*monitor);
            params.direct_on_input_index = Some(*input);
            params.flags.no_move = true;
        }
    }
    let made: pa::CreateRecordStreamReply = match c.request(pa::Command::CreateRecordStream(params)) {
        Ok(r) => r,
        Err(e) => return Ended::Refused(e),
    };
    let spec = made.sample_spec;
    let mut checked = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        if checked.elapsed() > Duration::from_secs(1) {
            checked = Instant::now();
            if check(target) {
                return Ended::Killed;
            }
        }
        if !c.ready(Duration::from_millis(100)) {
            continue;
        }
        match c.next() {
            Ok(Msg::Data(ch, bytes)) if ch == made.channel => {
                let mut q = out.lock().unwrap_or_else(|e| e.into_inner());
                match spec.format {
                    pa::SampleFormat::S16Le => q.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)),
                    _ => q.extend(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
                }
                // (never more than 10 s kept: nobody's reading)
                let cap = rate as usize * channels as usize * 10;
                if q.len() > cap {
                    let extra = q.len() - cap;
                    q.drain(..extra);
                }
            }
            Ok(Msg::Command(c)) if matches!(*c, pa::Command::RecordStreamKilled(ch) if ch == made.channel) => return Ended::Killed,
            Ok(_) => {}
            Err(e) => return Ended::Lost(e),
        }
    }
    let _ = c.send(&pa::Command::DeleteRecordStream(made.channel));
    Ended::Stopped
}

/// How the running capture hears the music app: "app" (only its stream) or "system" (everything the output plays).
pub static MODE: Mutex<&str> = Mutex::new("");

struct Running {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
    mic: (String, String),
}

static CURRENT: Mutex<Option<Running>> = Mutex::new(None);

/// The default output's monitor (the source recording everything it plays), by name.
fn default_monitor() -> Option<String> {
    let name = server_info().ok()?.default_sink_name?;
    sinks().into_iter().find(|s| s.name == name)?.monitor_source_name.map(|m| text(&m))
}

/// The monitor source of the output a playback stream plays on.
fn monitor_of(input: &pa::SinkInputInfo) -> Option<u32> {
    sinks().into_iter().find(|s| s.index == input.sink_index)?.monitor_source_index
}

/// Hear the music app (by bundle id) onto `out`, stereo at `rate`, until `stop`: its own stream when the server
/// allows (following it as it comes and goes), else the default output's monitor.
fn hear_music(bundle: &str, rate: u32, out: &Mutex<VecDeque<f32>>, stop: &AtomicBool) {
    let forced = std::env::var("ONKEY_LINUX_CAPTURE").is_ok_and(|v| v == "system");
    let mut system = forced;
    let mut said_waiting = false;
    let name = super::procs::exes_for(bundle).first().copied().unwrap_or(if super::procs::own(bundle) { "OnKey itself" } else { "the app" });
    while !stop.load(Ordering::SeqCst) {
        if system {
            *MODE.lock().unwrap_or_else(|e| e.into_inner()) = "system";
            let Some(monitor) = default_monitor() else {
                crate::engine::log("capture (Linux): no default output to hear");
                std::thread::sleep(Duration::from_secs(1));
                continue;
            };
            crate::engine::log(&format!("capture (Linux): hearing everything the default output plays ({monitor}), other apps and OnKey's own sounds too{}",
                                        if forced { " (ONKEY_LINUX_CAPTURE=system)" } else { "" }));
            match record(&Target::Source(monitor), 2, rate, out, stop, |_| false) {
                Ended::Stopped => return,
                Ended::Refused(e) | Ended::Lost(e) => crate::engine::log(&format!("capture (Linux): the output's monitor stopped: {e}")),
                Ended::Killed => {}
            }
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        *MODE.lock().unwrap_or_else(|e| e.into_inner()) = "app";
        let inputs = app_inputs(bundle);
        let chosen = choose(&streams(&inputs), &song_for(bundle));
        let Some((first, monitor)) = inputs.iter().find(|i| Some(i.index) == chosen).and_then(|i| Some((i.clone(), monitor_of(i)?))) else {
            if !said_waiting {
                said_waiting = true;
                crate::engine::log(&format!("capture (Linux): {name} has no sound stream yet; waiting for it (silence meanwhile)"));
            }
            std::thread::sleep(Duration::from_millis(500));
            continue;
        };
        said_waiting = false;
        crate::engine::log(&format!("capture (Linux): hearing only {name}'s stream #{} (\"{}\", {} Hz, {} ch{}), per-app", first.index,
                                    first.name.to_string_lossy(), first.sample_spec.sample_rate, first.sample_spec.channels,
                                    if first.corked { ", paused" } else { "" }));
        let target = Target::App { input: first.index, monitor };
        // follow another stream of the app that plays while this one is paused (a new tab, say), or the one playing
        // the song
        let check = |t: &Target| {
            let Target::App { input, .. } = t else { return false };
            switch(&streams(&app_inputs(bundle)), &song_for(bundle), *input)
        };
        match record(&target, 2, rate, out, stop, check) {
            Ended::Stopped => return,
            Ended::Killed => crate::engine::log(&format!("capture (Linux): {name}'s stream #{} ended; looking for its next", first.index)),
            Ended::Lost(e) => {
                crate::engine::log(&format!("capture (Linux): lost {name}'s stream: {e}"));
                std::thread::sleep(Duration::from_millis(500));
            }
            Ended::Refused(e) => {
                crate::engine::log(&format!("capture (Linux): the sound server won't record one app's stream ({e}): hearing the whole output instead"));
                system = true;
            }
        }
    }
}

/// Start capturing (see the module's notes): 0 with the rate, else 1 (the app isn't running), 3 (no sound server).
pub fn start(bundle: &str, cb: Callback, ctx: *mut c_void, out_rate: &mut f64) -> i32 {
    stop();
    if !bundle.is_empty() && !super::procs::exes_for(bundle).is_empty() && !super::procs::running(bundle) {
        return 1;
    }
    let server = match server_info() {
        Ok(s) => s,
        Err(e) => {
            crate::engine::log(&format!("capture (Linux): no sound server: {e}"));
            return 3;
        }
    };
    let rate = match server.sample_spec.sample_rate {
        r @ 8_000..=192_000 => r,
        _ => 48_000,
    };
    let mic = chosen_mic();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let music = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let voice = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let mut threads = Vec::new();
    {
        let (bundle, music, stop) = (bundle.to_string(), music.clone(), stop_flag.clone());
        threads.push(std::thread::Builder::new().name("onkey-capture-music".into()).spawn(move || hear_music(&bundle, rate, &music, &stop)).ok());
    }
    let has_mic = mic.is_some();
    if let Some((uid, name)) = mic.clone() {
        let (voice, stop) = (voice.clone(), stop_flag.clone());
        threads.push(std::thread::Builder::new().name("onkey-capture-mic".into()).spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match record(&Target::Source(uid.clone()), 1, rate, &voice, &stop, |_| false) {
                    Ended::Stopped => return,
                    Ended::Killed => crate::engine::log(&format!("capture (Linux): mic '{name}' went away")),
                    Ended::Refused(e) | Ended::Lost(e) => crate::engine::log(&format!("capture (Linux): mic '{name}' failed: {e}")),
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }).ok());
    }
    crate::engine::log(&format!("capture (Linux): {}; {}; all taken at {rate} Hz, 32-bit float; echo cancelling off",
                                describe_server_short(&server), match &mic { Some((_, n)) => format!("mic '{n}'"), None => "no mic: its channel is silent".into() }));
    let ctx = ctx as usize;
    {
        let (stop, music, voice) = (stop_flag.clone(), music.clone(), voice.clone());
        threads.push(std::thread::Builder::new().name("onkey-capture".into())
            .spawn(move || pump(&music, has_mic.then_some(&*voice), rate, cb, ctx as *mut c_void, &stop)).ok());
    }
    let threads: Vec<_> = threads.into_iter().flatten().collect();
    *out_rate = rate as f64;
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running { stop: stop_flag, threads, mic: mic.unwrap_or_default() });
    0
}

fn describe_server_short(s: &pa::ServerInfo) -> String {
    format!("{} {}", s.server_name.as_deref().map(text).unwrap_or_default(), s.server_version.as_deref().map(text).unwrap_or_default())
}

/// Hands the engine [music L, music R, mic] frames every 10 ms. The mic's clock leads when there's a mic; with
/// none, the wall clock does. Music that runs ahead of the mic by over 100 ms (clock drift) is dropped. (As
/// Windows' audio.rs `pump`.)
fn pump(music: &Mutex<VecDeque<f32>>, mic: Option<&Mutex<VecDeque<f32>>>, rate: u32, cb: Callback, ctx: *mut c_void, stop: &AtomicBool) {
    let (mut m, mut v) = (VecDeque::<f32>::new(), VecDeque::<f32>::new());
    let mut frame = Vec::<f32>::new();
    let started = Instant::now();
    let mut given: u64 = 0;
    let mut seen = crate::digest::Captured::default();
    let mut said_at = Instant::now();
    // (the mic's first audio can take a moment: until it comes, the wall clock leads)
    let mut mic_started = false;
    while !stop.load(Ordering::SeqCst) {
        if said_at.elapsed() > Duration::from_secs(600) {
            said_at = Instant::now();
            crate::engine::log(&format!("capture (Linux) so far: {}", seen.line(0.01)));
        }
        std::thread::sleep(Duration::from_millis(10));
        m.extend(music.lock().unwrap_or_else(|e| e.into_inner()).drain(..));
        if let Some(q) = mic {
            v.extend(q.lock().unwrap_or_else(|e| e.into_inner()).drain(..));
            mic_started |= !v.is_empty();
        }
        let n = if mic_started {
            v.len()
        } else {
            let due = (started.elapsed().as_secs_f64() * rate as f64) as u64;
            let n = due.saturating_sub(given) as usize;
            v.extend(std::iter::repeat(0.0).take(n));
            n
        };
        let ahead = (m.len() / 2).saturating_sub(n + rate as usize / 10);
        m.drain(..ahead * 2);
        seen.dropped += (ahead > 0) as u64;
        frame.clear();
        for _ in 0..n {
            let (l, r) = (m.pop_front().unwrap_or(0.0), m.pop_front().unwrap_or(0.0));
            frame.extend_from_slice(&[l, r, v.pop_front().unwrap_or(0.0)]);
        }
        if n > 0 {
            cb(frame.as_ptr(), n as i32, ctx);
            given += n as u64;
            seen.add(&frame);
        }
    }
    crate::engine::log(&format!("capture (Linux) stopped: {}", seen.line(0.01)));
}

pub fn stop() {
    let r = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(r) = r {
        r.stop.store(true, Ordering::SeqCst);
        for t in r.threads {
            let _ = t.join();
        }
    }
}

/// The mic the running capture listens to: (name, description), "" for both when none.
pub fn capture_mic() -> (String, String) {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|r| r.mic.clone()).unwrap_or_default()
}

// ---------------------------------------------------------------- the mic meter (setup's mic check)

static METER_STOP: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
static METER_LEVEL: AtomicU32 = AtomicU32::new(0xC2F0_0000);   // -120.0
static METER_BLOCKS: AtomicI32 = AtomicI32::new(0);

/// Start (true) or stop the meter: 0 if it's running afterwards (or stopped when asked), 1 no input device, 2 it
/// wouldn't open.
pub fn meter(on: bool) -> i32 {
    if let Some(s) = METER_STOP.lock().unwrap_or_else(|e| e.into_inner()).take() {
        s.store(true, Ordering::SeqCst);
    }
    METER_LEVEL.store((-120f32).to_bits(), Ordering::SeqCst);
    METER_BLOCKS.store(0, Ordering::SeqCst);
    if !on {
        return 0;
    }
    let Some((uid, _)) = chosen_mic() else { return 1 };
    meter_reader(uid)
}

/// Runs the meter on its own thread (a record stream whose blocks are measured as they come): 0 when it's running.
fn meter_reader(uid: String) -> i32 {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    std::thread::spawn(move || {
        let mut c = match Conn::open("OnKey") {
            Ok(c) => c,
            Err(_) => {
                let _ = tx.send(false);
                return;
            }
        };
        let mut params = pa::RecordStreamParams {
            sample_spec: pa::SampleSpec { format: pa::SampleFormat::Float32Le, channels: 1, sample_rate: 48_000 },
            channel_map: pa::ChannelMap::mono(),
            source_name: CString::new(uid).ok(),
            buffer_attr: pa::stream::BufferAttr { fragment_size: 48_000 / 50 * 4, ..Default::default() },
            flags: pa::stream::StreamFlags { adjust_latency: true, ..Default::default() },
            ..Default::default()
        };
        params.props.set(pa::Prop::MediaName, c"OnKey mic check");
        let made: pa::CreateRecordStreamReply = match c.request(pa::Command::CreateRecordStream(params)) {
            Ok(r) => r,
            Err(_) => {
                let _ = tx.send(false);
                return;
            }
        };
        let _ = tx.send(true);
        while !flag.load(Ordering::SeqCst) {
            if !c.ready(Duration::from_millis(100)) {
                continue;
            }
            match c.next() {
                Ok(Msg::Data(ch, bytes)) if ch == made.channel && bytes.len() >= 4 => {
                    let x: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                    let ms = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
                    let db = if ms > 0.0 { (10.0 * ms.log10()).max(-120.0) } else { -120.0 };
                    METER_LEVEL.store(db.to_bits(), Ordering::SeqCst);
                    METER_BLOCKS.fetch_add(1, Ordering::SeqCst);
                }
                Ok(Msg::Command(c)) if matches!(*c, pa::Command::RecordStreamKilled(_)) => break,
                Err(_) => break,
                Ok(_) => {}
            }
        }
        let _ = c.send(&pa::Command::DeleteRecordStream(made.channel));
    });
    if rx.recv_timeout(Duration::from_secs(3)).unwrap_or(false) {
        *METER_STOP.lock().unwrap_or_else(|e| e.into_inner()) = Some(stop);
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
    use super::{choose, switch};

    fn st(v: &[(u32, bool, &str)]) -> Vec<(u32, bool, String)> {
        v.iter().map(|(i, p, n)| (*i, *p, n.to_string())).collect()
    }

    // romulka, issue #1 (2 Nov 2026): Firefox had a messenger tab's stream open (silent, not paused) and then YouTube's
    #[test]
    fn the_tab_playing_the_song_is_heard_not_another_open_one() {
        let both = st(&[(128, false, "(3463) TOOL - Right In Two (Official Audio) - YouTube"), (80, false, "MAX")]);
        assert_eq!(choose(&both, "Right In Two"), Some(128));
        assert!(switch(&both, "Right In Two", 80), "leaves the messenger's stream for the song's");
        assert!(!switch(&both, "Right In Two", 128), "stays on the song's");
        // newest first when the order differs, and the song's still wins
        let other = st(&[(200, false, "MAX"), (128, false, "TOOL - Right In Two - YouTube")]);
        assert_eq!(choose(&other, "right in two"), Some(128));
    }

    #[test]
    fn with_no_song_named_the_old_rules_hold() {
        let s = st(&[(9, false, "Spotify"), (3, true, "Spotify")]);
        assert_eq!(choose(&s, ""), Some(9));
        assert!(!switch(&s, "", 9));
        assert!(switch(&s, "", 3), "a paused stream is left for one that plays");
        assert!(!switch(&st(&[(9, false, "a"), (3, false, "b")]), "", 3), "two playing, no song: stays put");
        assert!(!switch(&st(&[(9, true, "Right In Two")]), "Right In Two", 3), "never to a paused one");
        assert!(switch(&st(&[(9, false, "x")]), "", 3), "its stream gone: the one that plays");
        assert_eq!(choose(&st(&[(9, false, "Up")]), "Up"), Some(9), "a name under 3 letters matches nothing, so the first");
    }

    use std::sync::Mutex;

    static HEARD: Mutex<Vec<f32>> = Mutex::new(Vec::new());

    extern "C" fn heard(data: *const f32, frames: i32, _ctx: *mut std::ffi::c_void) {
        let x = unsafe { std::slice::from_raw_parts(data, frames as usize * 3) };
        HEARD.lock().unwrap().extend_from_slice(x);
    }

    /// With a sound server (CI's PulseAudio), OnKey's own tone is heard by capturing only OnKey's stream (per-app,
    /// as a music app's is), in the music channels.
    #[test]
    fn onkey_hears_its_own_stream_per_app() {
        if super::server_info().is_err() {
            eprintln!("no sound server: skipped");
            return;
        }
        struct Tone(f64);
        impl super::super::render::Fill for Tone {
            fn fill(&mut self, out: &mut [f32], _first: u64, _ahead: f64) {
                for o in out.iter_mut() {
                    *o = (self.0 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() as f32 * 0.5;
                    self.0 += 1.0;
                }
            }
        }
        let Some(tone) = super::super::render::Stream::open(48_000, 1, Tone(0.0)) else {
            eprintln!("no output to play to: skipped");
            return;
        };
        let mut rate = 0.0;
        assert_eq!(super::start(crate::demo::bundle(), heard, std::ptr::null_mut(), &mut rate), 0);
        assert!(rate > 0.0);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let loud = || HEARD.lock().unwrap().chunks_exact(3).filter(|f| f[0].abs() > 0.1).count();
        while loud() < 4_000 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let mode = *super::MODE.lock().unwrap();
        super::stop();
        drop(tone);
        let n = HEARD.lock().unwrap().len() / 3;
        assert!(n > 0, "frames come");
        assert!(loud() >= 4_000, "the tone is heard in the music channels ({} loud frames of {n}, mode {mode})", loud());
        assert_eq!(mode, "app", "heard per-app, not the whole output");
    }

    /// With no sound server (CI), every question has an answer and nothing hangs or panics.
    #[test]
    fn with_no_sound_server_it_answers_and_fails_cleanly() {
        let t = std::time::Instant::now();
        let _ = (super::output(), super::mics(), super::chosen_mic(), super::output_latency_ms(), super::app_sounding("com.spotify.client"));
        assert!(!super::describe_server().is_empty() && !super::describe_devices().is_empty());
        assert!(t.elapsed() < std::time::Duration::from_secs(20), "it didn't hang");
    }
}
