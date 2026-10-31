//! Development only (the `devtest` feature; release builds don't contain it): the live mirror. With
//! ONKEY_MIRROR=1 (`./run` sets it), a small server on 127.0.0.1:47321 (ONKEY_MIRROR_PORT) shows the running
//! app's window in a browser tab, live, so a developer, or a coding agent, can watch and screenshot it without Screen Recording:
//!   /…            the window's own pages (the same exported UI the app embeds), marked as the mirror
//!   /events       Server-Sent Events: every event the engine sends the window ("onkey"), the same JSON, sent the
//!                 moment it's emitted (each `id:` is the send time, epoch ms, so the tab can measure its lag);
//!                 on connect, first what's needed to catch up (the song playing so far, the latest status,
//!                 takes, listening line…), then an `event: mirror` marking the end of that catch-up
//!   /api/…        read-only: takes, library, take, take audio, favourite, settings (nothing that changes
//!                 anything, controls a music app or writes files)
//!   /api/ear      the ear's lines and summary (ear.rs: the gap heard between the song and your voice in "Play with
//!                 the song", measured from what the apps output)
//!   /dev/along    development tools only, and the one thing here that does something: POST with `X-OnKey-Dev: 1`
//!                 to press "Play with the song" on a take (or Stop) without a click (`drive` below)
//!   /api/probe    timing check: sends an "onkey-probe" to the window and the tabs; each answers with the song
//!                 time it shows (the app through ui_log, a tab through /api/probe/report), in $TMPDIR/onkey.log
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::store::Store;

pub const DEFAULT_PORT: u16 = 47321;
/// Most events kept for catching up on the song playing (about 25 minutes of updates)
const SONG_KEEP: usize = 40_000;

/// What a newly opened tab needs to catch up, and the tabs connected.
#[derive(Default)]
struct Hub {
    seq: u64,
    /// the song playing: its "song" event and every event for it since (but the ribbon's "viz")
    song: Vec<(u64, Arc<str>)>,
    /// the latest of each other kind (status, takes, listen, problem, models…)
    latest: HashMap<String, (u64, Arc<str>)>,
    clients: Vec<Sender<Msg>>,
}

#[derive(Clone)]
enum Msg {
    Event(Arc<str>, f64),
    Probe(u64),
}

static HUB: Mutex<Option<Hub>> = Mutex::new(None);

fn now_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// An event the engine sent the window: on to the tabs at once, and kept for catching up.
pub fn send(kind: &str, json: &str) {
    let mut g = HUB.lock().unwrap_or_else(|e| e.into_inner());
    let Some(hub) = g.as_mut() else { return };
    let at = now_ms();
    let s: Arc<str> = Arc::from(json);
    hub.clients.retain(|c| c.send(Msg::Event(s.clone(), at)).is_ok());
    hub.seq += 1;
    let seq = hub.seq;
    match kind {
        "song" => { hub.song.clear(); hub.song.push((seq, s)); }
        "viz" => {}
        "update" | "lyrics" | "notes" | "art" | "track" | "phase" => {
            if !hub.song.is_empty() && hub.song.len() < SONG_KEEP { hub.song.push((seq, s)); }
        }
        _ => { hub.latest.insert(kind.to_string(), (seq, s)); }
    }
}

/// Start the mirror if ONKEY_MIRROR=1.
pub fn start(app: AppHandle, store: Store) {
    if std::env::var("ONKEY_MIRROR").as_deref() != Ok("1") {
        return;
    }
    let port = std::env::var("ONKEY_MIRROR_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => return crate::engine::log(&format!("mirror: can't listen on 127.0.0.1:{port}: {e}")),
    };
    *HUB.lock().unwrap_or_else(|e| e.into_inner()) = Some(Hub::default());
    crate::engine::log(&format!("mirror: http://127.0.0.1:{port}/?mirror"));
    let ctx = Arc::new((app, store));
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let ctx = ctx.clone();
            std::thread::spawn(move || { let _ = serve(conn, &ctx.0, &ctx.1); });
        }
    });
}

fn serve(mut conn: TcpStream, app: &AppHandle, store: &Store) -> std::io::Result<()> {
    conn.set_nodelay(true)?;
    conn.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/").to_string());
    let mut drive_header = false;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() { break; }
        if let Some((k, v)) = h.split_once(':') {
            drive_header |= k.trim().eq_ignore_ascii_case(DRIVE_HEADER) && v.trim() == "1";
        }
    }
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let q: HashMap<String, String> = query.split('&').filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), urlencoding::decode(&v.replace('+', " ")).map(|s| s.into_owned()).unwrap_or_default()))
        .collect();
    if path.starts_with("/dev/") {
        let (status, body) = drive(app, method, path, &q, drive_header, crate::dev_on());
        return respond_owned(&mut conn, status, "application/json", body.to_string().as_bytes(), false);
    }
    if method != "GET" && method != "HEAD" {
        return respond(&mut conn, "405 Method Not Allowed", "text/plain", b"the mirror is read-only");
    }
    let arg = |k: &str| q.get(k).cloned().unwrap_or_default();
    let track = || crate::source::Track { id: arg("id"), name: arg("name"), artist: arg("artist"), ..Default::default() };
    let api = |v: Value| -> (String, &'static str, Vec<u8>) { ("200 OK".into(), "application/json", v.to_string().into_bytes()) };
    let (status, mime, body) = match path {
        "/events" => return events(conn),
        "/api/takes" => api(json!(store.takes())),
        "/api/library" => api(json!(store.library())),
        "/api/take" => match store.take_result(&arg("id")) {
            Some(v) => api(v),
            None => ("404 Not Found".into(), "application/json", b"null".to_vec()),
        },
        "/api/favorite" => api(json!(store.is_favorite(&track()))),
        "/api/settings" => api(json!({"listening": crate::settings::listening(), "control_spotify": crate::settings::control("control_spotify"),
            "control_music": crate::settings::control("control_music"), "youtube": crate::youtube::enabled(), "listen_only": crate::settings::listen_only(),
            "shazam": crate::settings::shazam(), "show_lyrics": crate::settings::show_lyrics(),
            "scoring_level": crate::dsp::Level::current().name(),
            "fade_guide": crate::settings::fade_guide()})),
        "/api/audio" => match store.take_dir(&arg("id")).and_then(|d| crate::takeaudio::recording(&d))
            .and_then(|p| Some((crate::takeaudio::mime(&p), std::fs::read(&p).ok()?))) {
            Some((mime, b)) => ("200 OK".into(), mime, b),
            None => ("404 Not Found".into(), "text/plain", b"no audio".to_vec()),
        },
        "/api/ear" => api(crate::ear::latest()),
        "/api/probe" => {
            static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let _ = app.emit("onkey-probe", json!({"seq": seq}));
            if let Some(hub) = HUB.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                hub.clients.retain(|c| c.send(Msg::Probe(seq)).is_ok());
            }
            api(json!({"seq": seq}))
        }
        "/api/probe/report" => {
            crate::engine::log(&format!("ui: probe {} mirror t={} at={} lyric={}", arg("seq"), arg("t"), arg("at"), arg("lyric")));
            api(json!(true))
        }
        p if p.starts_with("/api/") => ("404 Not Found".into(), "text/plain", b"not in the read-only mirror".to_vec()),
        p => match app.asset_resolver().get(p.to_string()) {
            Some(a) => {
                let mime = a.mime_type().to_string();
                let mut bytes = a.bytes().to_vec();
                if mime.starts_with("text/html") {
                    // the page knows it's the mirror on every page, not only one opened with ?mirror
                    let html = String::from_utf8_lossy(&bytes).replacen("<head>", "<head><script>window.__ONKEY_MIRROR__=1</script>", 1);
                    bytes = html.into_bytes();
                }
                return respond_owned(&mut conn, "200 OK", &mime, &bytes, method == "HEAD");
            }
            None => ("404 Not Found".into(), "text/plain", b"not found".to_vec()),
        },
    };
    respond_owned(&mut conn, &status, mime, &body, method == "HEAD")
}

/// The header a drive request must carry (`X-OnKey-Dev: 1`): a web page in a browser can't send it to this port
/// without the browser first asking (a preflight the mirror never answers), so only a local tool like curl can.
const DRIVE_HEADER: &str = "x-onkey-dev";

/// What a drive request asks the window to do, as the event sent to it ("onkey-dev-along"): None if it isn't one.
///   POST /dev/along?take=<take id>&song=<s into the song>   open the take and press "Play with the song" from there
///   POST /dev/along?take=<take id>&t=<s into the take>      the same, from that place on the take's graph
///   POST /dev/along/stop                                     press Stop
fn drive_event(path: &str, q: &HashMap<String, String>) -> Option<Value> {
    let num = |k: &str| q.get(k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
    match path {
        "/dev/along/stop" => Some(json!({"stop": true})),
        "/dev/along" => {
            let take = q.get("take").filter(|t| crate::store::valid_take_id(t))?;
            Some(json!({"take": take, "song": num("song"), "t": num("t"), "n": now_ms()}))
        }
        _ => None,
    }
}

/// Development only: drive the window without clicks, so a coding agent can test "Play with the song" end to end.
/// Only with the development tools on (./run's ONKEY_DEV_TOOLS=1), only a POST carrying `X-OnKey-Dev: 1`, and only
/// from this Mac (the mirror listens on 127.0.0.1). It sends the window what to press; the window does it through
/// the very code the buttons run (ui/components/DevDrive.tsx, ui/app/take/page.tsx).
fn drive(app: &AppHandle, method: &str, path: &str, q: &HashMap<String, String>, header: bool, dev: bool) -> (&'static str, Value) {
    if !dev {
        return ("403 Forbidden", json!({"error": "development tools are off (start OnKey with ./run)"}));
    }
    if method != "POST" || !header {
        return ("403 Forbidden", json!({"error": format!("POST with the header {DRIVE_HEADER}: 1")}));
    }
    match drive_event(path, q) {
        Some(ev) => {
            crate::engine::log(&format!("dev drive: {ev}"));
            let _ = app.emit("onkey-dev-along", ev.clone());
            ("200 OK", json!({"sent": ev}))
        }
        None => ("404 Not Found", json!({"error": "POST /dev/along?take=<id>&song=<s> (or &t=<s into the take>), or POST /dev/along/stop"})),
    }
}

fn respond(conn: &mut TcpStream, status: &str, mime: &str, body: &[u8]) -> std::io::Result<()> {
    respond_owned(conn, status, mime, body, false)
}

fn respond_owned(conn: &mut TcpStream, status: &str, mime: &str, body: &[u8], head: bool) -> std::io::Result<()> {
    write!(conn, "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len())?;
    if !head { conn.write_all(body)?; }
    conn.flush()
}

/// The event stream: catch up, then every event as it's sent.
fn events(mut conn: TcpStream) -> std::io::Result<()> {
    let (tx, rx) = channel();
    let catch_up = {
        let mut g = HUB.lock().unwrap_or_else(|e| e.into_inner());
        let Some(hub) = g.as_mut() else { return Ok(()) };
        let mut all: Vec<(u64, Arc<str>)> = hub.latest.values().cloned().chain(hub.song.iter().cloned()).collect();
        all.sort_by_key(|(seq, _)| *seq);
        hub.clients.push(tx);
        all
    };
    conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\nretry: 1000\n\n")?;
    let mut buf = String::new();
    for (_, s) in &catch_up {
        buf.push_str("id: 0\ndata: ");
        buf.push_str(s);
        buf.push_str("\n\n");
    }
    buf.push_str("event: mirror\ndata: {\"caught_up\":true}\n\n");
    conn.write_all(buf.as_bytes())?;
    // the reading half: a closed tab ends the stream at once
    let mut probe = conn.try_clone()?;
    std::thread::spawn(move || { let _ = probe.read(&mut [0u8; 64]); let _ = probe.shutdown(std::net::Shutdown::Both); });
    loop {
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Msg::Event(s, at)) => conn.write_all(format!("id: {at:.3}\ndata: {s}\n\n").as_bytes())?,
            Ok(Msg::Probe(seq)) => conn.write_all(format!("event: probe\ndata: {{\"seq\":{seq}}}\n\n").as_bytes())?,
            Err(RecvTimeoutError::Timeout) => conn.write_all(b": keep-alive\n\n")?,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drive_request_names_a_real_take_and_a_place() {
        let q = |s: &str| -> HashMap<String, String> { s.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.into(), v.into())).collect() };
        let ev = drive_event("/dev/along", &q("take=20261026-140201-fun-We-Are-Young&song=61.5")).unwrap();
        assert_eq!(ev["take"], "20261026-140201-fun-We-Are-Young");
        assert_eq!(ev["song"], 61.5);
        assert!(ev["t"].is_null());
        assert_eq!(drive_event("/dev/along", &q("take=20261026-140201-x&t=12")).unwrap()["t"], 12.0);
        assert_eq!(drive_event("/dev/along/stop", &q("")).unwrap(), json!({"stop": true}));
        // not a take id (a path out of the takes folder), no take, a bad number, another path
        assert!(drive_event("/dev/along", &q("take=../../etc&song=1")).is_none());
        assert!(drive_event("/dev/along", &q("song=1")).is_none());
        assert!(drive_event("/dev/along", &q("take=20261026-140201-x&song=-3")).unwrap()["song"].is_null());
        assert!(drive_event("/dev/other", &q("take=20261026-140201-x")).is_none());
    }

    #[test]
    fn a_tab_catches_up_on_the_song_playing_and_the_latest_status() {
        *HUB.lock().unwrap() = Some(Hub::default());
        send("status", r#"{"type":"status","n":1}"#);
        send("song", r#"{"type":"song","version":1}"#);
        send("update", r#"{"type":"update","version":1}"#);
        send("song", r#"{"type":"song","version":2}"#);
        send("update", r#"{"type":"update","version":2}"#);
        send("viz", r#"{"type":"viz"}"#);
        send("status", r#"{"type":"status","n":2}"#);
        let g = HUB.lock().unwrap();
        let hub = g.as_ref().unwrap();
        let mut all: Vec<_> = hub.latest.values().cloned().chain(hub.song.iter().cloned()).collect();
        all.sort_by_key(|(s, _)| *s);
        let kinds: Vec<&str> = all.iter().map(|(_, s)| &**s).collect();
        assert_eq!(kinds, [r#"{"type":"song","version":2}"#, r#"{"type":"update","version":2}"#, r#"{"type":"status","n":2}"#]);
    }
}
