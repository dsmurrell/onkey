//! YouTube and YouTube Music in a browser (Chrome, Brave, Edge or Safari), through the browser's AppleScript
//! interface: the YouTube tab is found by its address, and a few lines of JavaScript read the video's position and
//! the song's title from the page (the browser's "Allow JavaScript from Apple Events" setting must be on). A browser
//! is only asked once it makes sound, so OnKey never pesters one that's just open.
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

use super::applescript;
use crate::source::{Source, State, Track};
use crate::youtube::ID_PREFIX;

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

    /// Where the setting OnKey needs lives, for the message that asks you to turn it on (English; the window says it
    /// in its language by `js_setting_kind`: src-tauri/messages/KINDS.md).
    pub fn js_setting(self) -> &'static str {
        match self {
            Browser::Safari => "Safari ▸ Settings ▸ Advanced: tick \"Show features for web developers\", then Develop ▸ Allow JavaScript from Apple Events",
            _ => "View ▸ Developer ▸ Allow JavaScript from Apple Events",
        }
    }

    /// Which of the two settings texts applies: "safari", or "chromium" (Chrome, Brave, Edge: the same menu).
    pub fn js_setting_kind(self) -> &'static str {
        match self {
            Browser::Safari => "safari",
            _ => "chromium",
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
}

static FOLLOWED: Mutex<Followed> = Mutex::new(Followed { browser: None, video: None, last: BTreeMap::new() });

/// The browser YouTube is followed in.
pub fn browser() -> Option<Browser> {
    FOLLOWED.lock().unwrap().browser
}

/// The browser whose sound comes from processes with this bundle id prefix (listening mode).
pub fn browser_for_audio(bundle: &str) -> Option<Browser> {
    Browser::ALL.into_iter().find(|b| b.audio_bundle() == bundle)
}

/// Can OnKey follow YouTube in the browser whose sound comes from processes with this bundle id prefix?
pub fn followable(bundle: &str) -> bool {
    browser_for_audio(bundle).is_some()
}

/// The bundle id prefix of the processes playing the followed browser's sound.
pub fn audio_bundle() -> &'static str {
    browser().map_or("com.google.Chrome", |b| b.audio_bundle())
}

/// Is the position followed the browser's own? Always (the video's clock).
pub fn exact_position() -> bool {
    true
}

/// A browser playing YouTube that won't let OnKey read the page (its JavaScript setting is off).
static JS_BLOCKED: Mutex<Option<Browser>> = Mutex::new(None);

/// The browser to tell the user about when YouTube is playing but its page can't be read, with where the
/// setting is.
pub fn js_blocked() -> Option<(&'static str, &'static str, &'static str)> {
    JS_BLOCKED.lock().unwrap().map(|b| (b.name(), b.js_setting(), b.js_setting_kind()))
}

fn osa(script: &str) -> Option<String> {
    let app = Browser::ALL.into_iter().find(|b| script.contains(&format!("\"{}\"", b.app_bundle()))).map_or("your browser", |b| b.name());
    applescript::osa(app, script)
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
        (crate::youtube::strip_feat(&p.ma), crate::youtube::strip_feat(&p.mt))
    } else {
        // (the tab's title without YouTube's count of notifications and its name: "(3) Artist - Title - YouTube")
        crate::youtube::parse_title(if p.mt.is_empty() { crate::youtube::tab_text(&p.dt).0 } else { &p.mt }, &p.ma)
    };
    let (artist, name) = crate::youtube::named(&vid).unwrap_or((artist, name));
    // an advert plays in the same player: the song is "paused" where it was, with its own length
    let mut f = FOLLOWED.lock().unwrap();
    // (during an advert the player's response is the advert's: the video's category is read while it plays)
    if !p.ad {
        crate::youtube::note_kind(&vid, music, &p.c);
    }
    let (position, duration) = if p.ad {
        f.last.get(&vid).copied().unwrap_or((0.0, 0.0))
    } else {
        let d = p.d.filter(|d| d.is_finite()).unwrap_or(0.0);
        f.last.insert(vid.clone(), (p.t, d));
        (p.t, d)
    };
    // (only YouTube's own image host, as listed in the README's Privacy: an address a page gives could be anyone's)
    let art = if p.art.starts_with("https://i.ytimg.com/") { p.art.clone() } else { crate::youtube::thumbnail(&vid) };
    Some(State {
        playing: !p.p && !p.ad,
        track: Track { id: format!("{ID_PREFIX}{vid}"), name, artist, album: p.al.clone(), duration, art: Some(art), link: None },
        position,
        source: Source::YouTube,
    })
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
