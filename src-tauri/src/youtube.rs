//! YouTube and YouTube Music playing in a browser (Chrome, Brave, Edge or Safari), through the browser's
//! AppleScript interface: the YouTube tab is found by its address, and a few lines of JavaScript read the video's
//! position and the song's title from the page (the browser's "Allow JavaScript from Apple Events" setting must
//! be on). A browser is only asked once it makes sound, so OnKey never pesters one that's just open.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crate::source::{Source, State, Track};

/// Track ids: "youtube:" and the video id.
pub const ID_PREFIX: &str = "youtube:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Brave,
    Edge,
    Safari,
}

impl Browser {
    const ALL: [Browser; 4] = [Browser::Chrome, Browser::Brave, Browser::Edge, Browser::Safari];

    /// The name AppleScript knows the app by (also what people call it).
    pub fn name(self) -> &'static str {
        match self {
            Browser::Chrome => "Google Chrome",
            Browser::Brave => "Brave Browser",
            Browser::Edge => "Microsoft Edge",
            Browser::Safari => "Safari",
        }
    }

    /// The browser app's bundle id.
    pub fn app_bundle(self) -> &'static str {
        match self {
            Browser::Chrome => "com.google.Chrome",
            Browser::Brave => "com.brave.Browser",
            Browser::Edge => "com.microsoft.edgemac",
            Browser::Safari => "com.apple.Safari",
        }
    }

    /// Bundle id prefix of the processes that play its sound: Chromium browsers play from their helpers
    /// (e.g. com.google.Chrome.helper), Safari from WebKit's GPU process (shared with other apps built on WebKit).
    pub fn audio_bundle(self) -> &'static str {
        match self {
            Browser::Safari => "com.apple.WebKit.GPU",
            b => b.app_bundle(),
        }
    }

    /// Where the setting OnKey needs lives, for the message that asks you to turn it on.
    pub fn js_setting(self) -> &'static str {
        match self {
            Browser::Safari => "Safari ▸ Settings ▸ Advanced: tick \"Show features for web developers\", then Develop ▸ Allow JavaScript from Apple Events",
            _ => "View ▸ Developer ▸ Allow JavaScript from Apple Events",
        }
    }

    fn running(self) -> bool {
        crate::source::app_running(self.app_bundle())
    }

    /// AppleScript that runs `js` in tab `t` (inside the browser's tell block).
    fn run_js(self, js: &str) -> String {
        match self {
            Browser::Safari => format!(r#"do JavaScript "{js}" in t"#),
            _ => format!(r#"execute t javascript "{js}""#),
        }
    }
}

/// The browser whose YouTube tab is being followed, the video playing there, and per video the last position
/// and length seen outside adverts (an advert plays in the same video player).
struct Followed {
    browser: Option<Browser>,
    video: Option<String>,
    last: BTreeMap<String, (f64, f64)>,
    /// video id → (played on YouTube Music, its category as last read)
    kind: BTreeMap<String, (bool, String)>,
}

static FOLLOWED: Mutex<Followed> = Mutex::new(Followed { browser: None, video: None, last: BTreeMap::new(), kind: BTreeMap::new() });

/// A video that may not be a song: played on youtube.com (not YouTube Music) and not in YouTube's Music category
/// (or its category not read yet). Its first minute is checked for talk (notmusic.rs); a Music video never is.
pub fn might_not_be_song(track_id: &str) -> bool {
    let Some(vid) = track_id.strip_prefix(ID_PREFIX) else { return false };
    FOLLOWED.lock().unwrap().kind.get(vid).map_or(true, |(music_site, cat)| !music_site && cat != "Music")
}

/// The category YouTube gave the video (empty when not read).
pub fn category(track_id: &str) -> String {
    track_id.strip_prefix(ID_PREFIX).and_then(|v| FOLLOWED.lock().unwrap().kind.get(v).map(|k| k.1.clone())).unwrap_or_default()
}

/// Note what the page said about a video (`music_site`: played on YouTube Music).
fn note_kind(f: &mut Followed, vid: &str, music_site: bool, category: &str) {
    if !music_site && category.is_empty() {
        return;                                     // nothing read yet: whatever was read before stays
    }
    if f.kind.len() > 200 && !f.kind.contains_key(vid) {
        f.kind.clear();
    }
    f.kind.insert(vid.to_string(), (music_site, category.to_string()));
}

/// Video id → (artist, title).
type Names = BTreeMap<String, (String, String)>;

/// Names you gave videos whose artist and title were read wrong, and the file they're kept in.
static NAMES: Mutex<Option<(PathBuf, Names)>> = Mutex::new(None);

/// YouTube is experimental and off until you turn it on: following it means asking your browser about its tabs,
/// which macOS only allows after a prompt to let OnKey control the browser.
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn enabled() -> bool {
    ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Turn YouTube on or off (kept in OnKey's settings file, through settings.rs like every other setting).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
    crate::settings::set("youtube", on);
}

/// Where your names for videos are kept (OnKey's data folder), read once at launch, with whether YouTube is on
/// (after `settings::init`).
pub fn set_data_dir(dir: &Path) {
    ENABLED.store(crate::settings::get("youtube"), std::sync::atomic::Ordering::Relaxed);
    let file = dir.join("youtube-names.json");
    // there but unreadable: renaming is off this run, so the names you gave aren't written over with one
    let Some(names) = crate::store::read_kept(&file) else {
        crate::store::log_unreadable(&file);
        *NAMES.lock().unwrap() = None;
        return;
    };
    *NAMES.lock().unwrap() = Some((file, names));
}

/// Your artist and title for a video, if you gave one.
pub fn named(video: &str) -> Option<(String, String)> {
    NAMES.lock().unwrap().as_ref()?.1.get(video).cloned()
}

/// Give a video (track id "youtube:…") your own artist and title.
pub fn rename(id: &str, artist: &str, name: &str) -> Result<(), String> {
    let video = id.strip_prefix(ID_PREFIX).ok_or("not a YouTube video")?;
    let (artist, name) = (artist.trim(), name.trim());
    if name.is_empty() {
        return Err("a title is needed".into());
    }
    let mut guard = NAMES.lock().unwrap();
    let (file, names) = guard.as_mut().ok_or("OnKey couldn't read the names you gave videos, so it won't change them now")?;
    names.insert(video.to_string(), (artist.to_string(), name.to_string()));
    crate::store::write_atomic(file, &serde_json::to_vec_pretty(names).unwrap()).map_err(|e| e.to_string())
}

/// The browser YouTube is followed in.
pub fn browser() -> Option<Browser> {
    FOLLOWED.lock().unwrap().browser
}

/// The browser whose sound comes from processes with this bundle id prefix (listening mode).
pub fn browser_for_audio(bundle: &str) -> Option<Browser> {
    Browser::ALL.into_iter().find(|b| b.audio_bundle() == bundle)
}

/// A browser playing YouTube that won't let OnKey read the page (its JavaScript setting is off).
static JS_BLOCKED: Mutex<Option<Browser>> = Mutex::new(None);

/// The browser to tell the user about when YouTube is playing but its page can't be read, with where the
/// setting is.
pub fn js_blocked() -> Option<(&'static str, &'static str)> {
    if !enabled() {
        return None;
    }
    JS_BLOCKED.lock().unwrap().map(|b| (b.name(), b.js_setting()))
}

fn osa(script: &str) -> Option<String> {
    let app = Browser::ALL.into_iter().find(|b| script.contains(&format!("\"{}\"", b.app_bundle()))).map_or("your browser", |b| b.name());
    crate::source::osa(app, script)
}

/// What the page tells us: the video's clock, whether an advert is on, and the song's details (YouTube sets
/// them for the system's Now Playing; YouTube Music's are clean, youtube.com's are the video title and channel).
/// Kept tiny (it runs every 0.4 s) and quote-free for AppleScript.
const PAGE_JS: &str = "(function(){var v=document.querySelector('video');if(!v)return '';\
var p=document.getElementById('movie_player');\
var ad=!!(p&&(p.classList.contains('ad-showing')||p.classList.contains('ad-interrupting')));\
var m=navigator.mediaSession&&navigator.mediaSession.metadata;\
var a=m&&m.artwork&&m.artwork.length?m.artwork[m.artwork.length-1].src:'';\
var c='';try{var r=p&&p.getPlayerResponse&&p.getPlayerResponse();\
if(r&&r.videoDetails&&location.href.indexOf(r.videoDetails.videoId)>=0)c=((r.microformat||{}).playerMicroformatRenderer||{}).category||''}catch(e){}\
return JSON.stringify({t:v.currentTime,d:v.duration,p:v.paused,ad:ad,u:location.href,\
mt:m?m.title:'',ma:m?m.artist:'',al:m?m.album:'',art:a,dt:document.title,c:c})})()";

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Page {
    t: f64,
    d: Option<f64>,
    p: bool,
    ad: bool,
    u: String,
    mt: String,
    ma: String,
    al: String,
    art: String,
    dt: String,
    /// YouTube's category for the video ("Music", "Comedy", "Gaming"…); empty when the player hasn't said
    c: String,
}

/// Every YouTube video tab in a browser, read through JavaScript. Err: the browser refuses JavaScript.
///
/// Each tab gets `TAB_WAIT` to answer: a tab the browser has put to sleep can leave JavaScript unanswered for good
/// (live, 29 Oct: one of three YouTube tabs in Chrome never answered, so the whole ask took 10 s and Chrome was
/// called "not answering" every minute). Such a tab is left out for `TAB_REST`, and the others are still read.
fn read_tabs(b: Browser) -> Result<Vec<Page>, ()> {
    let skip = asleep_now();
    let script = format!(r#"tell application id "{bundle}"
        set skip to {{{skip}}}
        set out to ""
        repeat with w in windows
            repeat with t in tabs of w
                set u to URL of t
                if ({watch}) and skip does not contain u then
                    try
                        with timeout of {wait} seconds
                            set out to out & ({js}) & linefeed
                        end timeout
                    on error e number n
                        if n is -1712 then
                            set out to out & "ASLEEP " & u & linefeed
                        else if e contains "JavaScript" then
                            return "NOJS"
                        end if
                    end try
                end if
            end repeat
        end repeat
        return out
    end tell"#, bundle = b.app_bundle(), js = b.run_js(PAGE_JS), wait = TAB_WAIT, watch = WATCH_TAB,
        skip = skip.iter().map(|u| format!("\"{}\"", u.replace('\\', "").replace('"', ""))).collect::<Vec<_>>().join(", "));
    let out = osa(&script).unwrap_or_default();
    if out == "NOJS" {
        return Err(());
    }
    let (pages, asleep) = parse_tabs(&out);
    for u in asleep {
        crate::engine::log(&format!("{}: a YouTube tab didn't answer in {TAB_WAIT} s (asleep?): left out for {} s", b.name(), TAB_REST.as_secs()));
        ASLEEP.lock().unwrap().push((u, std::time::Instant::now() + TAB_REST));
    }
    Ok(pages)
}

/// How long one tab has to answer (in seconds; the page's JavaScript takes milliseconds).
const TAB_WAIT: u32 = 1;
/// How long a tab that didn't answer is left out. Short, so a sleeping tab you wake and play is followed soon.
const TAB_REST: std::time::Duration = std::time::Duration::from_secs(20);

/// Tabs (by address) that didn't answer, and until when they're left out.
static ASLEEP: Mutex<Vec<(String, std::time::Instant)>> = Mutex::new(Vec::new());

fn asleep_now() -> Vec<String> {
    let mut a = ASLEEP.lock().unwrap();
    a.retain(|(_, until)| *until > std::time::Instant::now());
    a.iter().map(|(u, _)| u.clone()).collect()
}

/// The pages read, and the addresses of tabs that didn't answer in time ("ASLEEP <address>" lines).
fn parse_tabs(out: &str) -> (Vec<Page>, Vec<String>) {
    let mut pages = Vec::new();
    let mut asleep = Vec::new();
    for l in out.lines() {
        if let Some(u) = l.strip_prefix("ASLEEP ") {
            asleep.push(u.to_string());
        } else if let Ok(p) = serde_json::from_str::<Page>(l) {
            pages.push(p);
        }
    }
    (pages, asleep)
}

/// YouTube's own watch pages (AppleScript, on a tab's address `u`): a page elsewhere with "youtube.com/watch" in its
/// address isn't one (it could name the song, and its cover image would be loaded in OnKey's window).
const WATCH_TAB: &str = r#"u starts with "https://www.youtube.com/watch?" or u starts with "https://music.youtube.com/watch?" or u starts with "https://m.youtube.com/watch?""#;

/// The video id in a YouTube watch address ("https://www.youtube.com/watch?v=ID&…"); None for any other site's.
fn video_id(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    if !["www.youtube.com", "music.youtube.com", "m.youtube.com"].contains(&host) || !rest[host.len()..].starts_with("/watch?") {
        return None;
    }
    let q = url.split_once('?')?.1;
    let v = q.split('&').find_map(|kv| kv.strip_prefix("v="))?;
    let v: String = v.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    (!v.is_empty()).then_some(v)
}

/// What YouTube is playing and where, in whichever browser is playing it. A browser is only asked when it is
/// making sound or it's the one already followed (which may be paused).
pub fn state() -> Option<State> {
    if !enabled() {
        return None;
    }
    let followed = FOLLOWED.lock().unwrap().browser;
    let mut order: Vec<Browser> = followed.into_iter().collect();
    order.extend(Browser::ALL.into_iter().filter(|b| Some(*b) != followed));
    let mut paused = None;
    for b in order {
        if (Some(b) != followed && !crate::capture::app_sounding(b.audio_bundle())) || !b.running() {
            let mut blocked = JS_BLOCKED.lock().unwrap();
            if *blocked == Some(b) {
                *blocked = None;              // gone quiet: no need to ask about its setting any more
            }
            continue;
        }
        let tabs = match read_tabs(b) {
            Ok(t) => {
                let mut blocked = JS_BLOCKED.lock().unwrap();
                if *blocked == Some(b) {
                    *blocked = None;
                }
                t
            }
            Err(()) => {
                *JS_BLOCKED.lock().unwrap() = Some(b);
                continue;
            }
        };
        // the tab that's playing (not an advert), else one that's paused: the video followed when it's there (a
        // pause of it isn't another video paused, which ended its take a minute later)
        let video = FOLLOWED.lock().unwrap().video.clone();
        let Some(page) = tabs.iter().find(|p| !p.p && !p.ad)
            .or_else(|| tabs.iter().find(|p| video.is_some() && video_id(&p.u) == video))
            .or_else(|| tabs.first()) else { continue };
        let Some(s) = to_state(b, page) else { continue };
        if s.playing {
            let mut f = FOLLOWED.lock().unwrap();
            f.browser = Some(b);
            f.video = s.track.id.strip_prefix(ID_PREFIX).map(String::from);
            return Some(s);
        }
        if Some(b) == followed || paused.is_none() {
            paused = paused.or(Some(s));
        }
    }
    paused
}

fn to_state(_b: Browser, p: &Page) -> Option<State> {
    let vid = video_id(&p.u)?;
    let music = p.u.contains("music.youtube.com");
    let (artist, name) = if music && !p.mt.is_empty() {
        (strip_feat(&p.ma), strip_feat(&p.mt))
    } else {
        parse_title(if p.mt.is_empty() { p.dt.trim_end_matches(" - YouTube") } else { &p.mt }, &p.ma)
    };
    let (artist, name) = named(&vid).unwrap_or((artist, name));
    // an advert plays in the same player: the song is "paused" where it was, with its own length
    let mut f = FOLLOWED.lock().unwrap();
    // (during an advert the player's response is the advert's: the video's category is read while it plays)
    if !p.ad {
        note_kind(&mut f, &vid, music, &p.c);
    }
    let (position, duration) = if p.ad {
        f.last.get(&vid).copied().unwrap_or((0.0, 0.0))
    } else {
        let d = p.d.filter(|d| d.is_finite()).unwrap_or(0.0);
        f.last.insert(vid.clone(), (p.t, d));
        (p.t, d)
    };
    // (only YouTube's own image host, as listed in the README's Privacy: an address a page gives could be anyone's)
    let art = if p.art.starts_with("https://i.ytimg.com/") { p.art.clone() } else { thumbnail(&vid) };
    Some(State {
        playing: !p.p && !p.ad,
        track: Track { id: format!("{ID_PREFIX}{vid}"), name, artist, album: p.al.clone(), duration, art: Some(art), link: None },
        position,
        source: Source::YouTube,
    })
}

/// A video's thumbnail (public, no login).
pub fn thumbnail(video: &str) -> String {
    format!("https://i.ytimg.com/vi/{video}/hqdefault.jpg")
}

/// Run a bit of JavaScript in the tab playing `video`, in the followed browser: what it evaluated to (None: no
/// such tab, or the browser said no).
fn in_tab(video: &str, js: &str) -> Option<String> {
    let b = FOLLOWED.lock().unwrap().browser?;
    let script = format!(r#"tell application id "{bundle}"
        repeat with w in windows
            repeat with t in tabs of w
                set u to URL of t
                if ({watch}) and u contains "v={video}" then
                    set r to ({run})
                    try
                        return r as string
                    on error
                        return ""
                    end try
                end if
            end repeat
        end repeat
        return "NOTAB"
    end tell"#, bundle = b.app_bundle(), run = b.run_js(js), watch = WATCH_TAB);
    osa(&script).filter(|r| r != "NOTAB")
}

fn current_video() -> Option<String> {
    FOLLOWED.lock().unwrap().video.clone()
}

pub fn control(action: &str) {
    let Some(v) = current_video() else { return };
    let js = match action {
        "restart" => "document.querySelector('video').currentTime=0",
        "playpause" => "var v=document.querySelector('video');v.paused?v.play():v.pause()",
        "pause" => "document.querySelector('video').pause()",
        "play" => "document.querySelector('video').play()",
        // YouTube Music's own buttons, else youtube.com's
        "next" => "var b=document.querySelector('.next-button,.ytp-next-button');if(b)b.click()",
        "previous" => "var b=document.querySelector('.previous-button');if(b)b.click();else document.querySelector('video').currentTime=0",
        _ => return,
    };
    let _ = in_tab(&v, js);
}

/// Jump to a position (seconds) in the video playing. A video that has only just been opened isn't ready to
/// seek, so this keeps trying for a few seconds, off the caller's thread.
pub fn seek(seconds: f64) {
    let Some(v) = current_video() else { return };
    let js = format!("var v=document.querySelector('video');if(v&&v.readyState>0){{v.currentTime={:.2};'ok'}}else{{''}}", seconds.max(0.0));
    std::thread::spawn(move || {
        for _ in 0..20 {
            if in_tab(&v, &js).as_deref() == Some("ok") {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    });
}

/// Play a video: in the YouTube tab already open in the followed browser if there is one, else in a new tab of
/// that browser (or Chrome, Safari, or the default browser), opened in the background so it doesn't come to
/// the front over OnKey.
pub fn play_track(id: &str) {
    let Some(vid) = id.strip_prefix(ID_PREFIX).filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')) else { return };
    let url = format!("https://www.youtube.com/watch?v={vid}");
    let followed = FOLLOWED.lock().unwrap().browser.filter(|b| b.running());
    let reused = followed.is_some_and(|b| osa(&format!(r#"tell application id "{}"
        repeat with w in windows
            repeat with t in tabs of w
                set u to URL of t
                if {watch} then
                    set URL of t to "{url}"
                    return "ok"
                end if
            end repeat
        end repeat
        return ""
    end tell"#, b.app_bundle(), watch = WATCH_TAB)).as_deref() == Some("ok"));
    if !reused {
        let b = followed.or_else(|| [Browser::Chrome, Browser::Safari].into_iter().find(|b| b.running()));
        let mut cmd = Command::new("open");
        cmd.arg("-g");
        if let Some(b) = b {
            cmd.args(["-b", b.app_bundle()]);
        }
        let _ = cmd.arg(&url).status();
        if b.is_some() {
            FOLLOWED.lock().unwrap().browser = b;
        }
    }
    FOLLOWED.lock().unwrap().video = Some(vid.to_string());
}

/// "Artist - Title (Official Video)" → (artist, title), with the channel as the artist when the title doesn't
/// name one. YouTube Music's own details don't need this.
pub fn parse_title(title: &str, channel: &str) -> (String, String) {
    let t = strip_noise(title);
    let t = t.split(" | ").next().unwrap_or(&t).trim().to_string();
    for dash in [" - ", " – ", " — ", " -- "] {
        if let Some((a, b)) = t.split_once(dash) {
            if !a.trim().is_empty() && !b.trim().is_empty() {
                return (strip_feat(a), strip_feat(b.trim_matches(|c| c == '"' || c == '\'' || c == ' ')));
            }
        }
    }
    // Artist "Title"
    if let Some((a, rest)) = t.split_once('"') {
        if let Some((b, _)) = rest.split_once('"') {
            if !a.trim().is_empty() && !b.trim().is_empty() {
                return (strip_feat(a), strip_feat(b));
            }
        }
    }
    (clean_channel(channel), strip_feat(&t))
}

/// Brackets that describe the video rather than the song: (Official Video), [HD], (Lyrics), (Remastered 2011)…
fn strip_noise(s: &str) -> String {
    const NOISE: [&str; 16] = ["official", "video", "audio", "lyric", "hd", "hq", "4k", "visuali", "remaster", "mv", "m/v",
                               "clip", "explicit", "color coded", "full song", "with lyrics"];
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(['(', '[']) {
        let close = if rest[i..].starts_with('(') { ')' } else { ']' };
        let Some(j) = rest[i..].find(close) else { break };
        let inner = rest[i + 1..i + j].to_lowercase();
        out.push_str(&rest[..i]);
        if !NOISE.iter().any(|n| inner.contains(n)) {
            out.push_str(&rest[i..i + j + 1]);
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Without "feat. …" / "ft. …" (lyrics sites list songs under the main artist and plain title).
fn strip_feat(s: &str) -> String {
    let mut s = s.to_string();
    for open in ["(feat", "(ft", "[feat", "[ft"] {
        if let Some(i) = s.to_ascii_lowercase().find(open) {
            let close = if open.starts_with('(') { ')' } else { ']' };
            let end = s[i..].find(close).map_or(s.len(), |j| i + j + 1);
            s.replace_range(i..end, "");
        }
    }
    for word in [" feat. ", " feat ", " ft. ", " ft ", " featuring "] {
        if let Some(i) = s.to_ascii_lowercase().find(word) {
            s.truncate(i);
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A channel name as the artist: "Juno Hale - Topic", "JunoHaleVEVO", "The Lindens Official" → the artist's name.
fn clean_channel(c: &str) -> String {
    let c = c.trim().trim_end_matches(" - Topic").trim_end_matches("VEVO").trim_end_matches("Vevo");
    let c = c.strip_suffix(" Official").or_else(|| c.strip_suffix(" official")).unwrap_or(c);
    c.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_that_didnt_answer_is_named_and_the_others_still_read() {
        let out = "{\"t\":12.5,\"p\":false,\"u\":\"https://www.youtube.com/watch?v=abc\"}\nASLEEP https://www.youtube.com/watch?v=old\n";
        let (pages, asleep) = parse_tabs(out);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].t, 12.5);
        assert_eq!(asleep, vec!["https://www.youtube.com/watch?v=old".to_string()]);
    }

    #[test]
    fn titles() {
        let cases = [
            ("Velvet Harbour - Paper Lanterns (Official Music Video)", "VelvetHarbourVEVO", "Velvet Harbour", "Paper Lanterns"),
            ("Moss Arcade - Slow Satellite (Official Video)", "Moss Arcade", "Moss Arcade", "Slow Satellite"),
            ("Juno Hale - Tidewater (Official Music Video)", "JunoHaleVEVO", "Juno Hale", "Tidewater"),
            ("The Lindens – Glass Weather (Official Video Remastered)", "The Lindens Official", "The Lindens", "Glass Weather"),
            ("The Parlour Kings - Neon Orchard (Official Music Video) [4K]", "The Parlour Kings", "The Parlour Kings", "Neon Orchard"),
            ("Otto Vale - Copper Sky (Official Video) ft. Mira Sol", "Otto Vale", "Otto Vale", "Copper Sky"),
            ("Ren Calder - All The Quiet Rooms (Official Video) ft. Ada Wren", "RenCalderVEVO", "Ren Calder", "All The Quiet Rooms"),
            ("Ivy Marsh - Kite Strings (Lyrics)", "lyricvault", "Ivy Marsh", "Kite Strings"),
            ("Slow Satellite", "Moss Arcade - Topic", "Moss Arcade", "Slow Satellite"),
            ("Glass Weather (Remastered 2011)", "The Lindens - Topic", "The Lindens", "Glass Weather"),
            ("Stay A While", "Harbour Lights - Topic", "Harbour Lights", "Stay A While"),
            ("Theo Brandt - Northern Line (Official Video)", "TheoBrandtVEVO", "Theo Brandt", "Northern Line"),
            ("Lena Vey - Static Hearts", "LenaVeyVEVO", "Lena Vey", "Static Hearts"),
            ("Soft Pilot - Midnight Ferry (Official Music Video) [HD]", "Soft Pilot", "Soft Pilot", "Midnight Ferry"),
            ("UB12 - Blue Blue River (Official Video)", "UB12", "UB12", "Blue Blue River"),
            ("o-ma - Turn It Round (Official Video) [Remastered in 4K]", "o-ma", "o-ma", "Turn It Round"),
            ("Greyfield - Summer Static (Official Video)", "Greyfield", "Greyfield", "Summer Static"),
            ("Nina Okoro - Bright (Video)", "NinaOkoroVEVO", "Nina Okoro", "Bright"),
            ("Fox Lantern - Night Drive (Official Audio) ft. Nina Okoro, Theo Brandt", "Fox Lantern", "Fox Lantern", "Night Drive"),
            ("MC Rook - Paper Crown [HD]", "uploads4u", "MC Rook", "Paper Crown"),
            ("Ada Wren - Every Little Lie (Official Music Video) | Ada Wren", "Ada Wren", "Ada Wren", "Every Little Lie"),
            ("Kit Ellery \"Kids of the Coast\" (Official Video)", "Kit Ellery", "Kit Ellery", "Kids of the Coast"),
            ("Someone I Once Knew (feat. Mira Sol)", "Gale - Topic", "Gale", "Someone I Once Knew"),
            ("Lullaby (Live)", "Jesse Bright - Topic", "Jesse Bright", "Lullaby (Live)"),
        ];
        for (title, channel, artist, name) in cases {
            assert_eq!(parse_title(title, channel), (artist.to_string(), name.to_string()), "{title}");
        }
    }

    #[test]
    fn only_videos_outside_youtube_music_and_its_music_category_may_not_be_songs() {
        let mut f = FOLLOWED.lock().unwrap();
        note_kind(&mut f, "t-music", false, "Music");
        note_kind(&mut f, "t-comedy", false, "Comedy");
        note_kind(&mut f, "t-ytm", true, "");
        note_kind(&mut f, "t-ytm", false, "");          // (a read with nothing in it changes nothing)
        drop(f);
        assert!(!might_not_be_song("youtube:t-music"));
        assert!(might_not_be_song("youtube:t-comedy"));
        assert!(!might_not_be_song("youtube:t-ytm"), "played on YouTube Music");
        assert!(might_not_be_song("youtube:t-unread"), "category not read: checked");
        assert!(!might_not_be_song("spotify:track:1"), "not a video");
        assert_eq!(category("youtube:t-comedy"), "Comedy");
    }

    #[test]
    fn ids() {
        assert_eq!(video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10s").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(video_id("https://music.youtube.com/watch?v=abc-_123&list=RD").as_deref(), Some("abc-_123"));
        assert_eq!(video_id("https://www.youtube.com/watch?list=x&v=Q1w2E3").as_deref(), Some("Q1w2E3"));
        assert_eq!(video_id("https://www.youtube.com/"), None);
        // another site with YouTube's address in its own: not a video
        assert_eq!(video_id("https://evil.example/youtube.com/watch?v=abc"), None);
        assert_eq!(video_id("https://www.youtube.com.evil.example/watch?v=abc"), None);
        assert_eq!(video_id("https://user@www.youtube.com/watch?v=abc"), None);
        assert_eq!(video_id("http://www.youtube.com/watch?v=abc"), None);
        assert_eq!(video_id("https://m.youtube.com/watch?v=abc").as_deref(), Some("abc"));
    }
}
