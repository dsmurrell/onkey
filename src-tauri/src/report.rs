//! "Report a problem": OnKey's records (records.rs) put together as one zip you can attach to a GitHub issue. Built
//! in a folder of its own first, so you can see exactly what's in it before saving; nothing is uploaded, ever.
//!
//! In it: the app's log for the last few days, the plays' summaries from those days (and the one take asked
//! about), the builds that ran on this Mac, and about.json (OnKey's version, the Mac's model, memory and macOS, the
//! settings as on/off switches, and what you wrote). Numbers, ids and log text only. Never in it: lyrics (any log
//! line that quotes four words in a row from lyrics kept on this Mac is left out), audio or song files (your
//! recordings, learned songs, the lyrics cache), your user name or home folder (paths start with ~), or your
//! microphones' names.
//!
//! On Windows also summary.txt (digest.rs): the logs' answers to what we'd otherwise ask a tester, section by section.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

/// Where an issue is opened (the "Open a GitHub issue" button).
pub const NEW_ISSUE_URL: &str = "https://github.com/dsmurrell/onkey/issues/new";

/// What the computer is called in the report's wording.
const MACHINE: &str = if cfg!(target_os = "macos") { "Mac" } else { "PC" };

/// The report being prepared: its folder, and OnKey's data folder it was made from.
static STAGED: Mutex<Option<(PathBuf, PathBuf)>> = Mutex::new(None);

/// What a log line or a summary may not carry: the home folder and the user's names.
pub struct Redact {
    home: String,
    names: Vec<String>,
    /// four words in a row from any lyrics kept on this Mac (lower case, letters and digits only)
    lyric_grams: HashSet<[u64; 4]>,
}

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

fn hash(w: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in w.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn grams(ws: &[String]) -> impl Iterator<Item = [u64; 4]> + '_ {
    ws.windows(4).map(|w| [hash(&w[0]), hash(&w[1]), hash(&w[2]), hash(&w[3])])
}

/// Every string inside a JSON value (lyrics cache entries hold LRC text, or search results holding it).
fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// Song titles, artists and albums in an LRCLIB answer (its entries' names, and the key it was kept under).
fn titles_in(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| titles_in(x, out)),
        Value::Object(o) => {
            for (k, x) in o {
                match (k.as_str(), x) {
                    ("trackName" | "artistName" | "albumName" | "name" | "key", Value::String(t)) => out.push(t.clone()),
                    _ => titles_in(x, out),
                }
            }
        }
        _ => {}
    }
}

impl Redact {
    /// For this Mac: its user, and the lyrics kept in `data` (the LRCLIB cache, and each take's lyrics). Four words
    /// in a row that are also four words of a learned song's title or artist aren't counted as lyrics (a log line
    /// naming "How to Save a Life" stays).
    pub fn for_data(data: &Path) -> Redact {
        let home = home_dir();
        let mut names: Vec<String> = Vec::new();
        if let Some(user) = std::path::Path::new(&home).file_name().map(|u| u.to_string_lossy().into_owned()) {
            names.push(user);
        }
        if let Ok(u) = std::env::var("USER") {
            names.push(u);
        }
        // (the account's full name; Windows: also its account name)
        names.extend(crate::platform::sys::user_names());
        names.retain(|n| n.chars().count() >= 3);
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        names.dedup();
        let mut texts: Vec<String> = Vec::new();
        // song titles and artists (any that LRCLIB answered about, taken, or learned): never counted as lyrics
        let mut titles: Vec<String> = Vec::new();
        for dir in [data.join("lrclib")] {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                if let Some(v) = std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
                    let mut s = Vec::new();
                    strings(&v, &mut s);
                    texts.extend(s.into_iter().filter(|s| s.contains('\n') || s.len() > 40).map(String::from));
                    titles_in(&v, &mut titles);
                }
            }
        }
        for e in std::fs::read_dir(data.join("takes")).into_iter().flatten().flatten() {
            if let Some(v) = std::fs::read(e.path().join("meta.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
                titles.push(format!("{} {}", v["track"]["artist"].as_str().unwrap_or(""), v["track"]["name"].as_str().unwrap_or("")));
                for l in v["lyrics"].as_array().into_iter().flatten() {
                    if let Some(t) = l[1].as_str() {
                        texts.push(t.to_string());
                    }
                }
            }
        }
        titles.extend(["songs", "records/takes"].iter().flat_map(|d| std::fs::read_dir(data.join(d)).into_iter().flatten().flatten())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json") && !e.path().to_string_lossy().ends_with(".syl.json"))
            .filter_map(|e| std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()))
            .map(|v| format!("{} {}", v["track"]["artist"].as_str().unwrap_or(""), v["track"]["name"].as_str().unwrap_or(""))));
        Redact::new(home, names, &texts, &titles)
    }

    pub fn new(home: String, names: Vec<String>, lyrics: &[String], titles: &[String]) -> Redact {
        let mut lyric_grams = HashSet::new();
        for t in lyrics {
            for line in t.lines() {
                // (an LRC line's time stamp isn't a word of it)
                let line = line.trim_start().strip_prefix('[').and_then(|r| r.split_once(']')).map_or(line, |(_, r)| r);
                lyric_grams.extend(grams(&words(line)));
            }
        }
        for t in titles {
            for g in grams(&words(t)) {
                lyric_grams.remove(&g);
            }
        }
        Redact { home, names, lyric_grams }
    }

    /// Does this text quote lyrics (four words in a row of a line kept on this Mac)?
    pub fn quotes_lyrics(&self, s: &str) -> bool {
        grams(&words(s)).any(|g| self.lyric_grams.contains(&g))
    }

    /// A log line as it goes in the report: None if it may quote lyrics; the home folder as ~, the user's names and
    /// microphone names taken out.
    pub fn line(&self, s: &str) -> Option<String> {
        if self.quotes_lyrics(s) {
            return None;
        }
        Some(self.text(s))
    }

    /// Text with the home folder as ~, and the user's names and microphones' names taken out.
    pub fn text(&self, s: &str) -> String {
        let mut s = if self.home.len() > 1 { s.replace(&self.home, "~") } else { s.to_string() };
        // (Windows: the profile folder also as JSON writes it, with forward slashes, and in lower case)
        #[cfg(windows)]
        if self.home.len() > 3 {
            let fwd = self.home.replace('\\', "/");
            for h in [self.home.replace('\\', "\\\\"), fwd.clone(), self.home.to_lowercase(), fwd.to_lowercase()] {
                s = s.replace(&h, "~");
            }
        }
        for n in &self.names {
            s = s.replace(n.as_str(), "[user]");
        }
        // microphones are often named after their owner ("Daniel's AirPods"): "mic '…'", "microphone now '…' (was '…')"
        for key in ["mic '", "now '", "(was '"] {
            let mut from = 0;
            while let Some(i) = s[from..].find(key).map(|i| i + from + key.len()) {
                let Some(j) = s[i..].find('\'').map(|j| j + i) else { break };
                if &s[i..j] != "[mic]" {
                    s.replace_range(i..j, "[mic]");
                }
                from = i + "[mic]".len() + 1;
            }
        }
        hide_hardware_addresses(&s)
    }
}

/// The home folder (Windows: the profile folder, C:\Users\<name>).
fn home_dir() -> String {
    crate::platform::shell::home()
}

/// Bluetooth hardware addresses (a device's Core Audio id is one: "AC-DE-48-00-11-22:output"; Linux's sound server
/// names one "bluez_sink.AC_DE_48_00_11_22.a2dp_sink") as [device]: a persistent identifier of someone's headphones.
fn hide_hardware_addresses(s: &str) -> String {
    let b = s.as_bytes();
    let hex2 = |i: usize| i + 2 <= b.len() && b[i].is_ascii_hexdigit() && b[i + 1].is_ascii_hexdigit();
    let address_at = |i: usize| {
        let sep = *b.get(i + 2)?;
        ((sep == b'-' || sep == b':' || sep == b'_') && (0..6).all(|k| hex2(i + 3 * k) && (k == 5 || b.get(i + 3 * k + 2) == Some(&sep))))
            .then_some(17)
    };
    let (mut out, mut i) = (String::with_capacity(s.len()), 0);
    while i < b.len() {
        // (only where an address starts: not in the middle of a longer run of hex)
        let starts = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        match address_at(i).filter(|n| starts && b.get(i + n).is_none_or(|c| !c.is_ascii_alphanumeric())) {
            Some(n) => {
                out.push_str("[device]");
                i += n;
            }
            None => {
                let c = s[i..].chars().next().unwrap();
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out
}

/// The settings as they go in a report: on/off switches, and whether a microphone was chosen (not which).
pub fn settings_for_report(v: &Value) -> Value {
    let mut out = json!({});
    for (k, x) in v.as_object().into_iter().flatten() {
        out[k] = match x {
            Value::Bool(_) | Value::Number(_) | Value::Null => x.clone(),
            _ => json!("(set)"),
        };
    }
    out
}

/// Days (YYYYMMDD) from `days` days ago to today.
fn recent_days(days: u32) -> Vec<String> {
    let now = std::time::SystemTime::now();
    (0..days.max(1)).map(|d| crate::store::format_time(now - std::time::Duration::from_secs(d as u64 * 86_400), "%Y%m%d")).collect()
}

/// Put a report together in a folder of its own (the last one prepared is replaced): logs from the last `days` days,
/// the plays' summaries from those days and the record of `take` (a take id from History), the builds seen, and
/// about.json. What's in it, for the window to list: {"folder", "files": [{name, bytes, what, what_kind, what_params}],
/// "bytes"}: `what` is English; `what_kind` (log, take, take_reported, builds, summary, about) and `what_params`
/// (artist, name) let the window say it in its language (src-tauri/messages/KINDS.md).
pub fn prepare(data: &Path, take: Option<&str>, days: u32) -> Result<Value, String> {
    let dir = std::env::temp_dir().join(format!("onkey-report-{}", crate::store::format_time(std::time::SystemTime::now(), "%Y%m%d-%H%M%S")));
    if let Some((old, _)) = STAGED.lock().unwrap().take() {
        let _ = std::fs::remove_dir_all(old);
    }
    let r = Redact::for_data(data);
    let files = stage(data, &dir, take, days, &r, crate::settings::all())?;
    *STAGED.lock().unwrap() = Some((dir.clone(), data.to_path_buf()));
    let bytes: u64 = files.iter().map(|f| f["bytes"].as_u64().unwrap_or(0)).sum();
    Ok(json!({"folder": r.text(&dir.to_string_lossy()), "files": files, "bytes": bytes}))
}

/// The report's files in `dir` (see `prepare`).
pub fn stage(data: &Path, dir: &Path, take: Option<&str>, days: u32, r: &Redact, settings: Value) -> Result<Vec<Value>, String> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir.join("logs")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("takes")).map_err(|e| e.to_string())?;
    let rec = data.join("records");
    let mut files: Vec<Value> = Vec::new();
    let mut add = |name: String, body: &str, what: String, kind: &str, params: Value| -> Result<(), String> {
        std::fs::write(dir.join(&name), body).map_err(|e| e.to_string())?;
        files.push(json!({"name": name, "bytes": body.len(), "what": what, "what_kind": kind, "what_params": params}));
        Ok(())
    };
    // the log, day by day (lines that may quote lyrics left out)
    let wanted = recent_days(days);
    let mut logs: Vec<PathBuf> = std::fs::read_dir(rec.join("logs")).into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.strip_prefix("onkey-")).is_some_and(|d| wanted.iter().any(|w| w == d)))
        .collect();
    logs.sort();
    // (no day files yet, from a build before the records: the end of the live log)
    if logs.is_empty() {
        logs.push(std::env::temp_dir().join("onkey.log"));
    }
    let mut left_out = 0;
    let mut all_logs = String::new();
    for p in &logs {
        let Ok(text) = std::fs::read_to_string(p) else { continue };
        let text = if text.len() > 8 << 20 { text[text.len() - (8 << 20)..].to_string() } else { text };
        let mut out = String::with_capacity(text.len());
        for line in text.lines() {
            match r.line(line) {
                Some(l) => { out.push_str(&l); out.push('\n'); }
                None => { left_out += 1; out.push_str("[a line that may quote lyrics was left out]\n"); }
            }
        }
        let name = format!("logs/{}", p.file_name().unwrap_or_default().to_string_lossy());
        add(name, &out, "OnKey's log for that day".into(), "log", json!({}))?;
        all_logs.push_str(&out);
    }
    // the plays' summaries: the take asked about, and every play in those days
    let mut takes: Vec<PathBuf> = std::fs::read_dir(rec.join("takes")).into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    takes.sort();
    for p in takes {
        let name = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let Some(v) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { continue };
        let asked = take.is_some_and(|t| v["outcome"]["take_id"].as_str() == Some(t));
        if !asked && !wanted.iter().any(|d| name.starts_with(d.as_str())) {
            continue;
        }
        let body = r.text(&serde_json::to_string_pretty(&v).unwrap_or_default());
        if r.quotes_lyrics(&body) {
            continue;       // (a summary never holds lyrics; one that seems to isn't sent)
        }
        let (artist, song) = (v["track"]["artist"].as_str().unwrap_or("?"), v["track"]["name"].as_str().unwrap_or("?"));
        let what = format!("{}a play's summary: {artist} - {song}", if asked { "the take reported: " } else { "" });
        add(format!("takes/{name}.json"), &body, r.text(&what), if asked { "take_reported" } else { "take" },
            json!({"artist": r.text(artist), "name": r.text(song)}))?;
    }
    if let Ok(b) = std::fs::read_to_string(rec.join("builds.json")) {
        add("builds.json".into(), &r.text(&b), format!("the OnKey builds that ran on this {MACHINE}"), "builds", json!({}))?;
    }
    // (Windows) summary.txt: the logs' answers to what we'd otherwise ask a tester (digest.rs)
    for (name, text, what) in crate::platform::diag::report_files(&all_logs) {
        add(name, &text, what, "summary", json!({}))?;
    }
    let mut about = json!({"report": 1, "made": crate::store::now_iso(), "build": crate::records::build(), "mac": crate::records::mac(),
        "settings": settings_for_report(&settings), "days": days, "take": take, "log_lines_left_out": left_out,
        "description": "", "contains": files.iter().map(|f| f["name"].clone()).collect::<Vec<_>>(),
        "never_contains": "lyrics, audio, song files, your user name or home folder, microphone names"});
    // (Windows: "mac" holds this PC, its "macos" the Windows version)
    crate::platform::diag::report_about(&mut about);
    let body = serde_json::to_string_pretty(&about).unwrap_or_default();
    std::fs::write(dir.join("about.json"), &body).map_err(|e| e.to_string())?;
    files.push(json!({"name": "about.json", "bytes": body.len(), "what": format!("OnKey's version, this {MACHINE}, the settings, and what you wrote"),
                      "what_kind": "about", "what_params": {}}));
    Ok(files)
}

/// Your description into the report being prepared (its about.json), then the whole of it zipped to `to`.
pub fn save(description: &str, to: &Path) -> Result<(), String> {
    let (dir, data) = STAGED.lock().unwrap().clone().ok_or("no report prepared")?;
    finish(&dir, description, to, &Redact::for_data(&data))
}

pub fn finish(dir: &Path, description: &str, to: &Path, r: &Redact) -> Result<(), String> {
    let about = dir.join("about.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&about).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // (a line of it quoting lyrics is left out too)
    let said: String = description.chars().take(4000).collect();
    v["description"] = json!(said.lines().map(|l| r.line(l).unwrap_or_else(|| "[lyrics left out]".into())).collect::<Vec<_>>().join("\n"));
    std::fs::write(&about, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())?;
    zip_dir(dir, to).map_err(|e| e.to_string())
}

/// The report's folder (to show it in Finder).
pub fn staged() -> Option<PathBuf> {
    STAGED.lock().unwrap().as_ref().map(|s| s.0.clone())
}

/// A zip of every file under `dir` (deflated), written atomically.
pub fn zip_dir(dir: &Path, to: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            let p = e.path();
            if p.is_dir() { stack.push(p) } else { paths.push(p) }
        }
    }
    paths.sort();
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let (time, date) = dos_time();
    for p in &paths {
        let name = p.strip_prefix(dir).unwrap_or(p).to_string_lossy().replace('\\', "/");
        let data = std::fs::read(p)?;
        let crc = crc32fast::hash(&data);
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&data)?;
        let comp = z.finish()?;
        let offset = out.len() as u32;
        let header = |sig: u32, central: bool| {
            let mut h: Vec<u8> = Vec::new();
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes());          // made by
            }
            h.extend_from_slice(&20u16.to_le_bytes());              // version needed
            h.extend_from_slice(&0x0800u16.to_le_bytes());          // flags: UTF-8 names
            h.extend_from_slice(&8u16.to_le_bytes());               // deflate
            h.extend_from_slice(&time.to_le_bytes());
            h.extend_from_slice(&date.to_le_bytes());
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());               // extra
            if central {
                h.extend_from_slice(&0u16.to_le_bytes());           // comment
                h.extend_from_slice(&0u16.to_le_bytes());           // disk
                h.extend_from_slice(&0u16.to_le_bytes());           // internal attributes
                h.extend_from_slice(&((0o100644u32) << 16).to_le_bytes());     // external: a plain file
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        out.extend(header(0x04034b50, false));
        out.extend_from_slice(&comp);
        central.extend(header(0x02014b50, true));
    }
    let (cd_at, cd_len) = (out.len() as u32, central.len() as u32);
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(paths.len() as u16).to_le_bytes());
    out.extend_from_slice(&(paths.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    crate::store::write_atomic(to, &out)
}

/// Now, as a zip file's DOS time and date (local time).
fn dos_time() -> (u16, u16) {
    let f = |fmt: &str| crate::store::format_time(std::time::SystemTime::now(), fmt).parse::<u16>().unwrap_or(0);
    let (y, mo, d, h, mi, s) = (f("%Y"), f("%m"), f("%d"), f("%H"), f("%M"), f("%S"));
    ((h << 11) | (mi << 5) | (s / 2), ((y.saturating_sub(1980)) << 9) | (mo << 5) | d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LYRICS: [&str; 4] = ["Where is the moon tonight my darling", "Under the silver bridge we wander slowly",
                               "Paper lanterns drifting down the river", "Short line"];

    fn data_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("onkey-report-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for sub in ["records/logs", "records/takes", "lrclib", "songs", "takes/20261016-120000-Test-Moon-Song"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        let lrc: String = LYRICS.iter().enumerate().map(|(i, l)| format!("[00:{:02}.00] {l}\n", 10 + i * 5)).collect();
        std::fs::write(d.join("lrclib/abc.json"), json!({"key": "get Test", "at": 0, "body": {"syncedLyrics": lrc}}).to_string()).unwrap();
        std::fs::write(d.join("songs/isrc-TEST.json"), json!({"track": {"id": "isrc:TEST", "name": "Under the Silver Bridge", "artist": "Test"}}).to_string()).unwrap();
        // a take: your recording, its lyrics and scores (none of it goes in a report)
        let t = d.join("takes/20261016-120000-Test-Moon-Song");
        std::fs::write(t.join("mic.wav"), b"RIFF....WAVEfmt ").unwrap();
        std::fs::write(t.join("meta.json"), json!({"lyrics": [[10.0, LYRICS[0]], [15.0, LYRICS[1]]]}).to_string()).unwrap();
        std::fs::write(t.join("result.json"), json!({"lines": [{"text": LYRICS[2]}]}).to_string()).unwrap();
        std::fs::write(d.join("songs/isrc-TEST.bin"), [0u8; 64]).unwrap();
        let home = home_dir();
        let today = crate::store::format_time(std::time::SystemTime::now(), "%Y%m%d");
        std::fs::write(d.join(format!("records/logs/onkey-{today}.log")), format!(
            "2026-10-16T12:00:00 new take (learn): Test - Under the Silver Bridge from 0.0s\n\
             2026-10-16T12:00:01 ui: error: where is the moon tonight, my darling? at {home}/Library/x.js\n\
             2026-10-16T12:00:02 capture started: Spotify, 48000 Hz, mic 'Someone's AirPods'\n\
             2026-10-16T12:00:03 kept audio in {home}/Music/test.wav\n")).unwrap();
        std::fs::write(d.join(format!("records/takes/{today}-120000-Under-the-Silver-Bridge.json")),
                       json!({"track": {"id": "isrc:TEST", "name": "Under the Silver Bridge", "artist": "Test"}, "outcome": {"take_id": "20261016-120000-Test-Moon-Song"},
                              "lyrics": {"check": "0123456789abcdef", "times": [10.0, 15.0]}}).to_string()).unwrap();
        d
    }

    #[test]
    fn a_report_never_holds_lyrics_audio_song_files_or_the_users_home() {
        let data = data_dir();
        let staged = data.join("staged");
        let r = Redact::for_data(&data);
        let files = stage(&data, &staged, Some("20261016-120000-Test-Moon-Song"), 3, &r, json!({"listening": true, "mic": "BuiltInMic-uid"})).unwrap();
        let zip = data.join("report.zip");
        finish(&staged, "It froze. My lyrics: Paper lanterns drifting down the river", &zip, &r).unwrap();
        let names: Vec<String> = files.iter().map(|f| f["name"].as_str().unwrap().to_string()).collect();
        assert!(names.iter().any(|n| n.starts_with("logs/")) && names.iter().any(|n| n.starts_with("takes/")) && names.contains(&"about.json".to_string()), "{names:?}");
        // the zip, read back with macOS's unzip: every file, and its whole text
        let list = std::process::Command::new("unzip").arg("-Z1").arg(&zip).output().unwrap();
        assert!(list.status.success(), "{}", String::from_utf8_lossy(&list.stderr));
        let entries = String::from_utf8_lossy(&list.stdout).to_string();
        for ext in [".wav", ".mp3", ".m4a", ".f32", ".bin", ".shsig", ".syl.json", ".words", "lrclib", "meta.json", "result.json"] {
            assert!(!entries.contains(ext), "{ext} in {entries}");
        }
        let all = std::process::Command::new("unzip").arg("-p").arg(&zip).output().unwrap();
        assert!(all.status.success());
        let text = String::from_utf8_lossy(&all.stdout).to_lowercase();
        for l in &LYRICS[..3] {
            let w = words(l).join(" ");
            assert!(!words(&text).join(" ").contains(&w), "lyrics found: {l}");
        }
        assert!(!text.contains("where is the moon tonight"), "a log line quoting lyrics is left out");
        assert!(!text.contains("hold me close until"), "the description's quoted lyrics are left out too");
        let home = home_dir().to_lowercase();
        assert!(!text.contains(&home), "home folder in the report");
        assert!(text.contains("~/music/test.wav"), "paths start with ~");
        assert!(!text.contains("someone's airpods"), "microphone names are taken out");
        assert!(text.contains("under the silver bridge from 0.0s"), "a log line naming the song (its title is words of its lyrics) stays");
        assert!(text.contains("\"mic\": \"(set)\""), "settings: whether a mic was chosen, not which");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_report_has_a_summary_and_no_profile_folder_in_any_form() {
        let r = Redact::new(r"C:\Users\bob".into(), vec!["bob".into()], &[], &[]);
        assert_eq!(r.text(r"at C:\Users\bob\AppData\x"), r"at ~\AppData\x");
        assert_eq!(r.text(r#"{"p": "C:\\Users\\bob\\x"}"#), r#"{"p": "~\\x"}"#);
        assert_eq!(r.text("file:///C:/Users/bob/x and c:\\users\\bob"), "file:///~/x and ~");
        let data = std::env::temp_dir().join(format!("onkey-report-win-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(data.join("records/logs")).unwrap();
        let home = home_dir();
        let today = crate::store::format_time(std::time::SystemTime::now(), "%Y%m%d");
        std::fs::write(data.join(format!("records/logs/onkey-{today}.log")), format!(
            "2026-10-28T10:00:00 ==== OnKey 0.1.0 on a PC\n\
             2026-10-28T10:00:01 capture (Windows): only spotify.exe (pid 4) and the programs it started (per-process loopback); output \"Speakers\" (48000 Hz, 2 ch, 32-bit); mic 'Bob Mic' (48000 Hz, 1 ch, 32-bit); all taken at 48000 Hz, 32-bit float\n\
             2026-10-28T10:00:02 ui: error: boom at {home}\\AppData\\x.js\n")).unwrap();
        let staged = data.join("staged");
        let r = Redact::for_data(&data);
        let files = stage(&data, &staged, None, 3, &r, json!({})).unwrap();
        assert!(files.iter().any(|f| f["name"] == "summary.txt"));
        let summary = std::fs::read_to_string(staged.join("summary.txt")).unwrap();
        assert!(summary.contains("## Audio") && summary.contains("per-process loopback") && summary.contains("mic '[mic]'"), "{summary}");
        assert!(summary.contains(r"ui: error: boom at ~\AppData\x.js"), "{summary}");
        assert!(!summary.to_lowercase().contains(&home.to_lowercase()), "{summary}");
        let about = std::fs::read_to_string(staged.join("about.json")).unwrap();
        assert!(about.contains("\"windows\""), "{about}");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn the_description_is_kept_without_its_lyrics() {
        let r = Redact::new("/Users/someone".into(), vec!["Some One".into()], &["Paper lanterns drifting down the river".into()], &[]);
        assert!(r.quotes_lyrics("he sang paper lanterns drifting down the end"));
        assert!(!r.quotes_lyrics("paper lanterns"));
        assert_eq!(r.text("/Users/someone/x by Some One"), "~/x by [user]");
        assert_eq!(r.text("microphone now 'A' (was 'B')"), "microphone now '[mic]' (was '[mic]')");
        // a Bluetooth device's id is its hardware address
        assert_eq!(r.text("output switched: 'AC-DE-48-00-11-22:output' -> 'BuiltInSpeakerDevice'"),
                   "output switched: '[device]:output' -> 'BuiltInSpeakerDevice'");
        assert_eq!(r.text("uid ac:de:48:00:11:22, été"), "uid [device], été");
        assert_eq!(r.text("hearing (bluez_sink.AC_DE_48_00_11_22.a2dp_sink.monitor)"), "hearing (bluez_sink.[device].a2dp_sink.monitor)");
        assert_eq!(r.text("at 12:34:56 took 0.5 s; id 1AC-DE-48-00-11-22"), "at 12:34:56 took 0.5 s; id 1AC-DE-48-00-11-22");
    }
}
