//! Synced lyrics from lrclib.net (only artist, title, album and duration are sent).
//!
//! Politely: each answer is kept on this Mac (app data, `lrclib/`), per track and per query, so a song played
//! again never asks again (an answer with synced lyrics is kept 30 days, "none" 1 day); one request at a time,
//! app-wide; when LRCLIB says too many requests (429) or unavailable (503), OnKey waits as long as it says
//! (Retry-After) or longer each time (2, 4, 8… s up to a minute, with jitter), meanwhile answering from what's
//! kept. Requests say who's asking (`user_agent`), as LRCLIB asks.
//! For the lab only: ONKEY_LRCLIB_CACHE=<dir> keeps the answers there instead, and ONKEY_LRCLIB_OFFLINE=1
//! answers only from them, so tests never reach lrclib.net (read by the app in devtest builds; the lab's programs
//! read them themselves and pass them to `use_lab_cache`).
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::source::Track;

/// OnKey's public repository, named in each request so LRCLIB knows who's asking (the same address as
/// site/lib/site.ts). Empty, requests would say just "OnKey/<version>".
pub const REPO_URL: &str = "https://github.com/dsmurrell/onkey";

/// Who's asking, as LRCLIB asks apps to say: "OnKey/<version> (+<repository>)".
pub fn user_agent() -> String {
    let v = env!("CARGO_PKG_VERSION");
    if REPO_URL.is_empty() { format!("OnKey/{v}") } else { format!("OnKey/{v} (+{REPO_URL})") }
}

/// An answer with synced lyrics is kept this long; any other answer (not found, plain lyrics only) for a day.
const KEEP_FOUND: u64 = 30 * 24 * 3600;
const KEEP_NONE: u64 = 24 * 3600;

static CACHE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static OFFLINE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where answers are kept: `<app data>/lrclib` (in devtest builds, the lab's folder and mode if set).
pub fn set_cache_dir(data_dir: &Path) {
    if let Some(dir) = crate::devtest::knob!("ONKEY_LRCLIB_CACHE") {
        use_lab_cache(Path::new(&dir), crate::devtest::knob!("ONKEY_LRCLIB_OFFLINE").is_some_and(|v| v == "1"));
        return;
    }
    let dir = data_dir.join("lrclib");
    let _ = std::fs::create_dir_all(&dir);
    *CACHE_DIR.lock().unwrap() = Some(dir);
}

/// The lab's folder of kept answers instead of the app's, and offline: answer only from them.
pub fn use_lab_cache(dir: &Path, offline: bool) {
    let _ = std::fs::create_dir_all(dir);
    *CACHE_DIR.lock().unwrap() = Some(dir.to_path_buf());
    OFFLINE.store(offline, std::sync::atomic::Ordering::Relaxed);
}

fn offline() -> bool {
    OFFLINE.load(std::sync::atomic::Ordering::Relaxed)
}

/// LRCLIB couldn't be asked (busy, unreachable, or offline): not the same as "no lyrics".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unanswered;

/// [(seconds, line)], empty when none are found (or LRCLIB can't be asked and nothing is kept for the song).
pub fn fetch(track: &Track) -> Vec<(f64, String)> {
    fetch_answered(track).unwrap_or_default()
}

/// As `fetch`, but Err when LRCLIB couldn't be asked (busy, unreachable) and nothing is kept for the song: then
/// it's worth asking again later, which "no lyrics" (Ok, empty) isn't.
/// With "Show lyrics" off (settings.rs), always none, and LRCLIB isn't asked.
pub fn fetch_answered(track: &Track) -> Result<Vec<(f64, String)>, Unanswered> {
    if !crate::settings::show_lyrics() {
        return Ok(Vec::new());
    }
    let key = track_key(track);
    if let Some(e) = cache_get(&key) {
        if fresh(&e, now_s()) {
            return Ok(e.body.as_str().map(parse).unwrap_or_default());
        }
    }
    match fetch_from_lrclib(track) {
        Ok(synced) => {
            cache_put(&key, synced.as_deref().map_or(serde_json::Value::Null, |s| serde_json::Value::String(s.to_string())));
            Ok(synced.map(|s| parse(&s)).unwrap_or_default())
        }
        // can't ask now: what's kept for it, however old (not kept as "none")
        Err(Unanswered) => cache_get(&key).and_then(|e| e.body.as_str().map(parse)).ok_or(Unanswered),
    }
}

/// The answer for a track is kept per track: its id and what's asked about it (a video renamed asks again).
fn track_key(track: &Track) -> String {
    format!("track {} | {} | {} | {} | {}", track.id, track.artist, track.name, track.album, track.duration.round())
}

/// Every synced entry for the song already kept on this Mac, however old (its answer, and what searches for it
/// found): to find the entry a learned song's times were made for without asking LRCLIB.
pub fn kept_entries(track: &Track) -> Vec<Vec<(f64, String)>> {
    let title = strip_version(&track.name);
    let mut out: Vec<Vec<(f64, String)>> = cache_get(&track_key(track)).and_then(|e| e.body.as_str().map(parse)).into_iter().collect();
    let urls = [
        format!("https://lrclib.net/api/search?artist_name={}&track_name={}", urlencoding::encode(&track.artist), urlencoding::encode(&title)),
        format!("https://lrclib.net/api/search?q={}", urlencoding::encode(&format!("{} {title}", track.artist))),
        format!("https://lrclib.net/api/search?track_name={}", urlencoding::encode(&title)),
    ];
    for url in urls {
        if let Some(serde_json::Value::Array(hits)) = cache_get(&format!("get {url}")).map(|e| e.body) {
            out.extend(hits.iter().filter(|h| alike(h["trackName"].as_str().unwrap_or(""), &title) >= 0.5)
                .filter_map(|h| h["syncedLyrics"].as_str().map(parse)));
        }
    }
    out.retain(|l| !l.is_empty());
    out
}

/// A learned song's lyrics, kept on this Mac with LRCLIB's other answers but never expiring while the song is
/// in the library (forget_learned removes them), so a learned song never needs the network to show its words.
/// Kept when the song is learned, and when that entry is found again (or another one is adopted: engine.rs learned_lyrics).
pub fn keep_learned(id: &str, lines: &[(f64, String)]) {
    if lines.is_empty() {
        return;
    }
    let lrc: Vec<String> = lines.iter().map(|(t, s)| format!("[{:02}:{:05.2}] {s}", (t / 60.0).floor() as u64, t % 60.0)).collect();
    cache_put(&format!("learned {id}"), serde_json::Value::String(lrc.join("\n")));
}

/// The lyrics kept for a learned song (see keep_learned).
pub fn learned(id: &str) -> Option<Vec<(f64, String)>> {
    cache_get(&format!("learned {id}"))?.body.as_str().map(parse).filter(|l| !l.is_empty())
}

/// A song forgotten from the library: its kept lyrics go too.
pub fn forget_learned(id: &str) {
    if let Some(p) = cache_path(&format!("learned {id}")) {
        let _ = std::fs::remove_file(p);
    }
}

/// Every synced entry LRCLIB has for the song under its artist and title (one search, kept like other answers): to
/// find again the very entry a learned song's times were made for, when the one pinned on this Mac isn't it.
pub fn entries(track: &Track) -> Result<Vec<Vec<(f64, String)>>, Unanswered> {
    if !crate::settings::show_lyrics() {
        return Ok(Vec::new());
    }
    let title = strip_version(&track.name);
    let url = format!("https://lrclib.net/api/search?artist_name={}&track_name={}", urlencoding::encode(&track.artist), urlencoding::encode(&title));
    let hits = match request(&url)? {
        Some(serde_json::Value::Array(h)) => h,
        _ => Vec::new(),
    };
    Ok(hits.iter().filter(|h| alike(h["trackName"].as_str().unwrap_or(""), &title) >= 0.5)
        .filter_map(|h| h["syncedLyrics"].as_str().map(parse)).filter(|l| !l.is_empty()).collect())
}

/// What "Look again" (the window's button under "No lyrics for this song yet") got.
#[derive(Debug, Clone, PartialEq)]
pub enum LookAgain {
    Found(Vec<(f64, String)>),
    /// LRCLIB answered, and still has none that fit
    None,
    /// LRCLIB couldn't be asked (busy, unreachable)
    Unreachable,
}

/// Ask LRCLIB about a song again now, forgetting the "none" answers kept for it (`forget_none`): lyrics someone
/// has added to LRCLIB since show up at once instead of in a day. A kept answer with lyrics is kept (and given
/// back without asking). With "Show lyrics" off: None, and LRCLIB isn't asked.
pub fn look_again(track: &Track) -> LookAgain {
    if !crate::settings::show_lyrics() {
        return LookAgain::None;
    }
    forget_none(track);
    match fetch_answered(track) {
        Ok(l) if !l.is_empty() => LookAgain::Found(l),
        Ok(_) => LookAgain::None,
        Err(Unanswered) => LookAgain::Unreachable,
    }
}

/// Forget the answers kept for a song that found nothing (its own, and those of the searches made for it); an
/// answer with synced lyrics stays. How many were forgotten.
pub fn forget_none(track: &Track) -> usize {
    let mut n = 0;
    for key in none_keys(track) {
        if let Some(e) = cache_get(&key).filter(|e| !found(&e.body)) {
            if let Some(p) = cache_path(&e.key) {
                let _ = std::fs::remove_file(p);
                n += 1;
            }
        }
    }
    n
}

/// Where a song's answers are kept: under the track itself, and under each request made for it (`request` keeps
/// the answer to each URL).
fn none_keys(track: &Track) -> Vec<String> {
    let title = strip_version(&track.name);
    std::iter::once(track_key(track))
        .chain(std::iter::once(get_url(track, &title)).chain(search_urls(track, &title)).map(|u| format!("get {u}")))
        .collect()
}

fn get_url(track: &Track, title: &str) -> String {
    format!("https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}&duration={}",
            urlencoding::encode(&track.artist), urlencoding::encode(title), urlencoding::encode(&track.album),
            track.duration.round() as i64)
}

/// The searches made for a song, narrow to wide (see `search`).
fn search_urls(track: &Track, title: &str) -> Vec<String> {
    let mut urls = vec![
        format!("https://lrclib.net/api/search?artist_name={}&track_name={}", urlencoding::encode(&track.artist), urlencoding::encode(title)),
        format!("https://lrclib.net/api/search?q={}", urlencoding::encode(&format!("{} {title}", track.artist))),
        format!("https://lrclib.net/api/search?track_name={}", urlencoding::encode(title)),
    ];
    if !track.album.trim().is_empty() {
        urls.push(format!("https://lrclib.net/api/search?q={}", urlencoding::encode(&format!("{title} {}", track.album))));
    }
    urls
}

fn fetch_from_lrclib(track: &Track) -> Result<Option<String>, Unanswered> {
    let title = strip_version(&track.name);
    let synced = request(&get_url(track, &title))?.and_then(|v| v["syncedLyrics"].as_str().map(String::from));
    if synced.is_some() {
        return Ok(synced);
    }
    // a music video is often longer than the record (an intro, an outro, a skit); a record's length is
    // known to within seconds
    let max_off = if track.id.starts_with(crate::youtube::ID_PREFIX) { 120.0 } else { 12.0 };
    search(track, &title, max_off)
}

/// No exact match: search by name and take the closest match whose title matches well, allowing `max_off`
/// seconds' difference in length. Searched from narrow to wide (artist and title; both as free text; the title
/// alone, which finds cast recordings and covers filed under another artist; the title with the album), stopping
/// at the first that gives a match. A cover has the same words, and learning the song lines them up with the
/// singing anyway.
fn search(track: &Track, title: &str, max_off: f64) -> Result<Option<String>, Unanswered> {
    let mut hits = Vec::new();
    let urls = search_urls(track, title);
    let album = (!track.id.starts_with(crate::youtube::ID_PREFIX)).then_some(track.album.as_str());
    let mut unanswered = false;
    for url in urls {
        match request(&url) {
            Ok(Some(serde_json::Value::Array(h))) => hits.extend(h),
            Ok(_) => {}
            Err(Unanswered) => unanswered = true,
        }
        if best_match(&hits, &track.artist, title, track.duration, max_off, album).is_some() {
            break;
        }
    }
    match best_match(&hits, &track.artist, title, track.duration, max_off, album) {
        Some(i) => Ok(hits[i]["syncedLyrics"].as_str().map(String::from)),
        // a search that couldn't be asked might have found them: not "none"
        None if unanswered => Err(Unanswered),
        None => Ok(None),
    }
}

/// Lowercase words without brackets, punctuation or a leading "the": how two names are compared.
fn words(s: &str) -> Vec<String> {
    let mut plain = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            c if c.is_alphanumeric() => plain.extend(c.to_lowercase()),
            '\'' | '’' => {}
            _ => plain.push(' '),
        }
    }
    let mut w: Vec<String> = plain.split_whitespace().map(String::from).collect();
    if w.len() > 1 && w[0] == "the" {
        w.remove(0);
    }
    w
}

/// How alike two names are, 0..1: the share of words they have in common (1 when one is the other plus extra
/// words, e.g. a channel name "The Lindens Official" for "The Lindens").
fn alike(a: &str, b: &str) -> f64 {
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    let common = short.iter().filter(|w| long.contains(w)).count();
    if common == short.len() {
        return if a.len() == b.len() { 1.0 } else { 0.9 };
    }
    2.0 * common as f64 / (a.len() + b.len()) as f64
}

/// The search hit (with synced lyrics) that best matches the artist, title and length: the title must match
/// well and the length be within `max_off` s. For a video (`album` None) the artist only ranks: a channel can be
/// named anything. For a record, a hit by another artist must at least belong to the record's album (a cast
/// recording filed under the show), else it's another song that shares the title.
pub fn best_match(hits: &[serde_json::Value], artist: &str, title: &str, duration: f64, max_off: f64,
                  album: Option<&str>) -> Option<usize> {
    hits.iter().enumerate().filter(|(_, h)| h["syncedLyrics"].is_string()).filter_map(|(i, h)| {
        let t = alike(h["trackName"].as_str().unwrap_or(""), title);
        let off = (h["duration"].as_f64().unwrap_or(0.0) - duration).abs();
        if t < 0.75 || (duration > 0.0 && off > max_off) {
            return None;
        }
        let by = alike(h["artistName"].as_str().unwrap_or(""), artist);
        // (a track with no artist, e.g. a file in Music, can't be checked this way)
        if let Some(album) = album.filter(|_| !words(artist).is_empty()) {
            let of_album = ["artistName", "albumName"].iter().any(|k| alike(h[*k].as_str().unwrap_or(""), album) >= 0.5);
            if by < 0.5 && !of_album {
                return None;
            }
        }
        let score = 2.0 * t + by - off / (2.0 * max_off);
        Some((i, score))
    }).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
}

/// When LRCLIB last failed (a server error or no answer, while online) and last answered: unix seconds.
static LAST_FAIL: Mutex<u64> = Mutex::new(0);
static LAST_ANSWER: Mutex<u64> = Mutex::new(0);

/// How long after its last failure LRCLIB still counts as down, with no answer since (a song being learned asks
/// again at least every minute, so it stays down while one is).
const DOWN_FOR_S: u64 = 180;

fn noted(answered: bool) {
    *(if answered { &LAST_ANSWER } else { &LAST_FAIL }).lock().unwrap() = now_s();
}

/// LRCLIB isn't working right now (the window says so: songs learned meanwhile have no words yet).
pub fn service_down() -> bool {
    down(*LAST_FAIL.lock().unwrap(), *LAST_ANSWER.lock().unwrap(), now_s())
}

fn down(last_fail: u64, last_answer: u64, now: u64) -> bool {
    last_fail > 0 && last_fail >= last_answer && now.saturating_sub(last_fail) < DOWN_FOR_S
}

/// LRCLIB's answer to `url` (Ok(None): not found), from what's kept if it's fresh. Asked one at a time app-wide,
/// at most every 0.3 s, never while LRCLIB has asked to wait (then Err after waiting up to 10 s); a request that
/// doesn't get through is tried once more, as a song learned without lyrics never gets syllable bars.
fn request(url: &str) -> Result<Option<serde_json::Value>, Unanswered> {
    let key = format!("get {url}");
    if let Some(e) = cache_get(&key).filter(|e| fresh(e, now_s())) {
        return Ok((!e.body.is_null()).then_some(e.body));
    }
    if offline() {
        return Err(Unanswered);
    }
    // "Show lyrics" off: nothing goes to LRCLIB (the callers above don't ask either)
    if !crate::settings::show_lyrics() {
        return Ok(None);
    }
    static NET: Mutex<Gate> = Mutex::new(Gate { next: None, fails: 0 });
    let mut gate = NET.lock().unwrap();        // one request at a time, app-wide
    for attempt in 0..2 {
        if let Some(next) = gate.next {
            let wait = next.saturating_duration_since(Instant::now());
            if wait > Duration::from_secs(10) {
                return Err(Unanswered);
            }
            std::thread::sleep(wait);
        }
        gate.next = Some(Instant::now() + Duration::from_millis(300));
        match ureq::get(url).set("User-Agent", &user_agent()).timeout(Duration::from_secs(10)).call() {
            Ok(r) => {
                gate.fails = 0;
                noted(true);
                let v: Option<serde_json::Value> = r.into_json().ok();
                cache_put(&key, v.clone().unwrap_or(serde_json::Value::Null));
                return Ok(v);
            }
            // not found; or a question LRCLIB can't take (400: no artist, as some local files have), which
            // asking again won't change (it isn't "can't be asked now")
            Err(ureq::Error::Status(400 | 404, _)) => {
                gate.fails = 0;
                noted(true);
                cache_put(&key, serde_json::Value::Null);
                return Ok(None);
            }
            // too many requests, or unavailable: wait as long as it says, or longer each time
            Err(ureq::Error::Status(code @ (429 | 503), r)) => {
                let wait = back_off(r.header("Retry-After"), gate.fails, jitter());
                gate.fails += 1;
                gate.next = Some(Instant::now() + wait);
                if code == 503 {
                    noted(false);
                }
                crate::engine::log(&format!("lyrics: LRCLIB says {code}: asking again in {:.1} s", wait.as_secs_f64()));
            }
            Err(ureq::Error::Status(code, _)) => {
                crate::engine::log(&format!("lyrics: LRCLIB answered {code}"));
                if code >= 500 {
                    noted(false);
                }
                return Err(Unanswered);
            }
            Err(e) if attempt == 0 => {
                crate::engine::log(&format!("lyrics: LRCLIB didn't answer ({e}); once more"));
                gate.next = Some(Instant::now() + Duration::from_millis(800));
            }
            Err(e) => {
                crate::engine::log(&format!("lyrics: LRCLIB didn't answer ({e})"));
                noted(false);
                return Err(Unanswered);
            }
        }
    }
    Err(Unanswered)
}

struct Gate {
    /// not before then (spacing, or LRCLIB asked to wait)
    next: Option<Instant>,
    /// 429/503 answers in a row
    fails: u32,
}

/// How long to wait after a 429/503: its Retry-After (seconds, up to 10 min) if given, else 2, 4, 8… s (up to a
/// minute) for each one in a row, plus `jitter` (0..1 s) so waiting apps don't all come back at once.
fn back_off(retry_after: Option<&str>, fails: u32, jitter: f64) -> Duration {
    let base = match retry_after.and_then(|h| h.trim().parse::<f64>().ok()) {
        Some(s) if s >= 0.0 => s.min(600.0),
        _ => 2f64.powi(fails.min(5) as i32 + 1).min(60.0),
    };
    Duration::from_secs_f64(base + jitter.clamp(0.0, 1.0))
}

fn jitter() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.5, |d| (d.subsec_nanos() % 1000) as f64 / 1000.0)
}

fn now_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// An answer kept: when, and what (null: none).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Kept {
    key: String,
    at: u64,
    body: serde_json::Value,
}

/// Whether a kept answer is still good: 30 days if it has synced lyrics, else a day.
fn fresh(e: &Kept, now: u64) -> bool {
    if e.key.starts_with("learned ") {
        return true;                    // a learned song's lyrics: kept while it's in the library
    }
    now.saturating_sub(e.at) < if found(&e.body) { KEEP_FOUND } else { KEEP_NONE }
}

/// Whether a kept answer has synced lyrics in it: LRC text, an entry with some, or a search with such an entry.
fn found(body: &serde_json::Value) -> bool {
    let synced = |v: &serde_json::Value| v.is_string() || v["syncedLyrics"].is_string();
    match body {
        serde_json::Value::Array(a) => a.iter().any(synced),
        v => synced(v),
    }
}

fn cache_path(key: &str) -> Option<PathBuf> {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    Some(CACHE_DIR.lock().unwrap().as_ref()?.join(format!("{h:016x}.json")))
}

fn cache_get(key: &str) -> Option<Kept> {
    let e: Kept = serde_json::from_slice(&std::fs::read(cache_path(key)?).ok()?).ok()?;
    (e.key == key).then_some(e)
}

fn cache_put(key: &str, body: serde_json::Value) {
    if let Some(p) = cache_path(key) {
        let e = Kept { key: key.to_string(), at: now_s(), body };
        let _ = crate::store::write_atomic(&p, &serde_json::to_vec(&e).unwrap_or_default());
    }
}

/// The title lyrics sites list: "Song - 2011 Remaster" -> "Song", "Song (From \"Show\")" -> "Song",
/// "Song (feat. X)" -> "Song".
fn strip_version(name: &str) -> String {
    let mut s = name.to_string();
    if let Some(i) = s.find(" - ") {
        let tail = s[i..].to_lowercase();
        if ["remaster", "version", "edit", "mix", "mono", "stereo", "live", "from "].iter().any(|w| tail.contains(w)) {
            s.truncate(i);
        }
    }
    for open in ['(', '['] {
        if let Some(i) = s.find(open) {
            let tail = s[i + 1..].trim_start().to_lowercase();
            if ["from ", "feat", "ft.", "with "].iter().any(|w| tail.starts_with(w)) {
                s.truncate(i);
            }
        }
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn lyrics_service_down_until_it_answers_or_a_while_passes() {
        use super::{down, DOWN_FOR_S};
        assert!(!down(0, 0, 1000), "never failed");
        assert!(down(1000, 0, 1010), "failed, no answer since");
        assert!(down(1000, 900, 1000 + DOWN_FOR_S - 1), "still down just inside the window");
        assert!(!down(1000, 900, 1000 + DOWN_FOR_S), "no news for a while: not said any more");
        assert!(!down(1000, 1005, 1010), "answered after the failure");
    }

    use super::*;
    use serde_json::json;

    #[test]
    fn says_who_is_asking() {
        let ua = user_agent();
        let v = format!("OnKey/{}", env!("CARGO_PKG_VERSION"));
        if REPO_URL.is_empty() { assert_eq!(ua, v) } else { assert_eq!(ua, format!("{v} (+{REPO_URL})")) }
    }

    #[test]
    fn waits_as_lrclib_says_or_longer_each_time() {
        assert_eq!(back_off(Some("7"), 0, 0.0), Duration::from_secs(7));
        assert_eq!(back_off(Some(" 30 "), 4, 0.25), Duration::from_secs_f64(30.25));
        assert_eq!(back_off(Some("99999"), 0, 0.0), Duration::from_secs(600));
        // no (or an unreadable) Retry-After: 2, 4, 8 … s, at most a minute
        let w: Vec<f64> = (0..8).map(|n| back_off(None, n, 0.0).as_secs_f64()).collect();
        assert_eq!(w, vec![2.0, 4.0, 8.0, 16.0, 32.0, 60.0, 60.0, 60.0]);
        assert_eq!(back_off(Some("Wed, 21 Oct 2026 07:28:00 GMT"), 1, 0.0), Duration::from_secs(4));
        assert!(back_off(None, 0, 7.0) <= Duration::from_secs(3));        // jitter is under a second
    }

    #[test]
    fn answers_are_kept_and_found_again() {
        let day = 24 * 3600;
        let k = |body: serde_json::Value, age: u64| Kept { key: "k".into(), at: 1_000_000_000 - age, body };
        let now = 1_000_000_000;
        // synced lyrics: a month; none, or plain lyrics only: a day
        assert!(fresh(&k(json!("[00:01.00] la"), 29 * day), now));
        assert!(!fresh(&k(json!("[00:01.00] la"), 31 * day), now));
        assert!(fresh(&k(json!({"syncedLyrics": "[00:01.00] la"}), 20 * day), now));
        assert!(fresh(&k(json!([{"plainLyrics": "la"}, {"syncedLyrics": "[00:01.00] la"}]), 20 * day), now));
        assert!(fresh(&k(serde_json::Value::Null, day - 60), now));
        assert!(!fresh(&k(serde_json::Value::Null, day + 60), now));
        assert!(!fresh(&k(json!([{"plainLyrics": "la"}]), 2 * day), now));
        // kept on disk, by key; the lab's offline mode answers from them only
        let dir = std::env::temp_dir().join(format!("onkey-lrclib-{}", std::process::id()));
        set_cache_dir(&dir);
        let track = Track { id: "isrc:TEST00000001".into(), name: "Paper Lanterns".into(), artist: "Velvet Harbour".into(),
                            album: "Harbour Lights".into(), duration: 201.0, art: None, link: None };
        let key = format!("track {} | {} | {} | {} | {}", track.id, track.artist, track.name, track.album, 201);
        cache_put(&key, json!("[00:12.50] made-up words\n[00:15.00] more of them"));
        assert_eq!(fetch(&track), vec![(12.5, "made-up words".to_string()), (15.0, "more of them".to_string())]);
        // what's kept for it, however old, with what a search for it found (other songs' hits left out)
        let mut e = cache_get(&key).unwrap();
        e.at = 0;
        let _ = crate::store::write_atomic(&cache_path(&key).unwrap(), &serde_json::to_vec(&e).unwrap());
        cache_put("get https://lrclib.net/api/search?artist_name=Velvet%20Harbour&track_name=Paper%20Lanterns",
                  json!([{"trackName": "Paper Lanterns (Live)", "syncedLyrics": "[00:13.00] made-up words, live"},
                         {"trackName": "Harbour Song", "syncedLyrics": "[00:01.00] other words"}]));
        assert_eq!(kept_entries(&track), vec![vec![(12.5, "made-up words".to_string()), (15.0, "more of them".to_string())],
                                              vec![(13.0, "made-up words, live".to_string())]]);
        assert!(cache_get("another key").is_none());
        // "Look again" forgets only the answers that found nothing: the track's own (not this one, which has
        // lyrics), and each search's (a search with no synced hit goes; the one with a synced hit stays)
        let found_search = "get https://lrclib.net/api/search?artist_name=Velvet%20Harbour&track_name=Paper%20Lanterns";
        let none_get = format!("get {}", get_url(&track, "Paper Lanterns"));
        let plain_search = "get https://lrclib.net/api/search?q=Velvet%20Harbour%20Paper%20Lanterns";
        cache_put(&none_get, serde_json::Value::Null);
        cache_put(plain_search, json!([{"trackName": "Paper Lanterns", "plainLyrics": "la"}]));
        assert_eq!(forget_none(&track), 2);
        assert!(cache_get(&key).is_some(), "the track's answer has lyrics: kept");
        assert!(cache_get(found_search).is_some(), "a search that found some: kept");
        assert!(cache_get(&none_get).is_none(), "the not-found answer: forgotten");
        assert!(cache_get(plain_search).is_none(), "a search with plain lyrics only: forgotten");
        assert_eq!(forget_none(&track), 0, "nothing left to forget");
        let mut none_track = track.clone();
        none_track.id = "isrc:TEST00000002".into();
        cache_put(&track_key(&none_track), serde_json::Value::Null);
        assert_eq!(forget_none(&none_track), 1, "a track's own not-found answer goes too");
        // "Show lyrics" off: none, and nothing asked of LRCLIB (a song with nothing kept is never looked up)
        crate::settings::set("show_lyrics", false);
        assert_eq!(fetch_answered(&track), Ok(Vec::new()));
        assert_eq!(entries(&track), Ok(Vec::new()));
        assert_eq!(look_again(&none_track), LookAgain::None);
        assert_eq!(request("https://lrclib.net/api/get?track_name=Never%20Asked"), Ok(None));
        crate::settings::set_value("show_lyrics", serde_json::Value::Null);
        // a learned song's lyrics: kept with no expiry (however old), until it's forgotten
        let lines = vec![(12.5, "made-up words".to_string()), (75.25, "more of them".to_string())];
        keep_learned("isrc:TEST00000001", &lines);
        assert_eq!(learned("isrc:TEST00000001"), Some(lines.clone()));
        let mut e = cache_get("learned isrc:TEST00000001").unwrap();
        e.at = 0;
        assert!(fresh(&e, now_s()));
        forget_learned("isrc:TEST00000001");
        assert_eq!(learned("isrc:TEST00000001"), None);
        assert_eq!(learned("isrc:NEVER"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn hit(artist: &str, title: &str, duration: f64) -> serde_json::Value {
        json!({"artistName": artist, "trackName": title, "duration": duration, "syncedLyrics": "[00:01.00] la"})
    }

    #[test]
    fn youtube_matches() {
        // a music video 40 s longer than the record: the record's lyrics, not a cover's or another song's
        let hits = [hit("Some Covers Band", "Tainted Love", 200.0), hit("Soft Cell", "Tainted Love / Where Did Our Love Go", 538.0),
                    hit("Soft Cell", "Tainted Love", 160.0), hit("Soft Cell", "Memorabilia", 300.0)];
        assert_eq!(best_match(&hits, "Soft Cell", "Tainted Love", 200.0, 120.0, None), Some(2));
        // the channel named differently from the artist: the title decides, the closer length wins
        let hits = [hit("The Lindens", "Glass Weather", 355.0), hit("The Lindens", "Glass Weather (Live at the Pier)", 260.0)];
        assert_eq!(best_match(&hits, "The Lindens Official", "Glass Weather", 359.0, 120.0, None), Some(0));
        // "The" and punctuation don't matter
        let hits = [hit("The Parlour Kings", "Neon Orchard", 222.0)];
        assert_eq!(best_match(&hits, "Parlour Kings", "Neon Orchard", 230.0, 120.0, None), Some(0));
        // only a different song with the same artist: nothing
        let hits = [hit("Moss Arcade", "Champagne Rain", 451.0), hit("Moss Arcade", "Don't Look Down", 288.0)];
        assert_eq!(best_match(&hits, "Moss Arcade", "Slow Satellite", 260.0, 120.0, None), None);
        // more than two minutes longer (a long video cut): nothing
        let hits = [hit("Velvet Harbour", "Paper Lanterns", 357.0)];
        assert_eq!(best_match(&hits, "Velvet Harbour", "Paper Lanterns", 820.0, 120.0, None), None);
        // plain lyrics only: not used
        let hits = [json!({"artistName": "Juno Hale", "trackName": "Hello Again", "duration": 295.0, "plainLyrics": "la la"})];
        assert_eq!(best_match(&hits, "Juno Hale", "Hello Again", 367.0, 120.0, None), None);
    }

    #[test]
    fn record_matches() {
        // a cast recording filed under the show, not the singer: found by the title, the length close
        let show = Some("The Show (Original Cast Soundtrack)");
        let hits = [hit("The Show!", "Lantern Song", 173.4)];
        assert_eq!(best_match(&hits, "Ada Wren", "Lantern Song", 172.0, 12.0, show), Some(0));
        // ... or under another singer, on an album of the show
        let mut h = hit("Bo Finch", "Lantern Song", 170.0);
        h["albumName"] = json!("The Show (1994 Revival Cast)");
        assert_eq!(best_match(&[h], "Ada Wren", "Lantern Song", 172.0, 12.0, show), Some(0));
        // the same title but a recording 20 s longer: a record's length is known, so not this one
        let hits = [hit("Ada Wren", "Lantern Song", 134.0)];
        assert_eq!(best_match(&hits, "Ada Wren", "Lantern Song", 114.0, 12.0, show), None);
        // another artist's song with the same title and length, on another album: not this song
        let mut h = hit("Kaito Band", "Lantern Song", 226.0);
        h["albumName"] = json!("Neon Nights");
        assert_eq!(best_match(&[h.clone()], "Ada Wren", "Lantern Song", 228.0, 12.0, Some("The Very Best Of")), None);
        // a track with no artist or album (a file in Music): the title and length decide
        assert_eq!(best_match(&[h], "", "Lantern Song", 228.0, 12.0, Some("")), Some(0));
        // "- From ..." in the hit's title still matches
        let hits = [hit("Writers/Ada Wren", "Lantern Song - From \"The Show\"", 246.0)];
        assert_eq!(best_match(&hits, "Ada Wren", "Lantern Song", 247.0, 12.0, show), Some(0));
    }

    #[test]
    fn look_again_covers_every_question_asked_for_a_song() {
        let track = Track { id: "yt:abc".into(), name: "Night Drive (feat. Mira Sol)".into(), artist: "Juno Hale".into(),
                            album: "Late".into(), duration: 200.4, art: None, link: None };
        let keys = none_keys(&track);
        // the track's own answer, LRCLIB's exact look-up, and the four searches (the album's included)
        assert_eq!(keys.len(), 6);
        assert_eq!(keys[0], track_key(&track));
        assert_eq!(keys[1], "get https://lrclib.net/api/get?artist_name=Juno%20Hale&track_name=Night%20Drive&album_name=Late&duration=200");
        assert!(keys[2..].iter().all(|k| k.starts_with("get https://lrclib.net/api/search?")));
        assert!(keys.iter().any(|k| k.ends_with("q=Night%20Drive%20Late")));
        // no album: three searches
        let no_album = Track { album: String::new(), ..track };
        assert_eq!(none_keys(&no_album).len(), 5);
        // what counts as found: LRC text, an entry with synced lyrics, a search with one; not plain lyrics or nothing
        assert!(found(&json!("[00:01.00] la")));
        assert!(found(&json!({"syncedLyrics": "[00:01.00] la"})));
        assert!(found(&json!([{"plainLyrics": "la"}, {"syncedLyrics": "[00:01.00] la"}])));
        assert!(!found(&json!([{"plainLyrics": "la"}])));
        assert!(!found(&json!({"plainLyrics": "la"})));
        assert!(!found(&serde_json::Value::Null));
        assert!(!found(&json!([])));
    }

    #[test]
    fn plain_titles() {
        assert_eq!(strip_version("Lantern Song (From \"The Show\")"), "Lantern Song");
        assert_eq!(strip_version("Lantern Song (From The Show!)"), "Lantern Song");
        assert_eq!(strip_version("Lantern Song - From \"The Show\""), "Lantern Song");
        assert_eq!(strip_version("Night Drive (feat. Mira Sol)"), "Night Drive");
        assert_eq!(strip_version("Glass Weather - 2011 Remaster"), "Glass Weather");
        assert_eq!(strip_version("Glass Weather (Live at the Pier)"), "Glass Weather (Live at the Pier)");
        assert_eq!(strip_version("Oliver!"), "Oliver!");
    }
}

/// [(seconds, line)] from LRC text (also used by the lab's library rebuild, given lyrics fetched elsewhere).
pub fn parse(lrc: &str) -> Vec<(f64, String)> {
    lrc.lines().filter_map(|line| {
        let rest = line.strip_prefix('[')?;
        let (stamp, text) = rest.split_once(']')?;
        let (m, s) = stamp.split_once(':')?;
        let t = m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?;
        Some((t, text.trim().to_string()))
    }).collect()
}
