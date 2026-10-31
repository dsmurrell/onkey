//! YouTube and YouTube Music playing in a browser: what OnKey does with them whatever the OS (ids, names read from
//! titles, which videos may not be songs, your own names for videos, the on/off switch). How the browser is asked is
//! the platform's: AppleScript and a few lines of JavaScript in the tab on macOS (platform/mac/youtube.rs), the
//! browser's system media session on Windows (platform/windows/browsers.rs).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::source::State;

/// Track ids: "youtube:" and the video id.
pub const ID_PREFIX: &str = "youtube:";

/// Video id → (played on YouTube Music, its category as last read).
static KINDS: Mutex<BTreeMap<String, (bool, String)>> = Mutex::new(BTreeMap::new());

/// A video that may not be a song: played on youtube.com (not YouTube Music) and not in YouTube's Music category
/// (or its category not read yet). Its first minute is checked for talk (notmusic.rs); a Music video never is.
pub fn might_not_be_song(track_id: &str) -> bool {
    let Some(vid) = track_id.strip_prefix(ID_PREFIX) else { return false };
    KINDS.lock().unwrap().get(vid).map_or(true, |(music_site, cat)| !music_site && cat != "Music")
}

/// The category YouTube gave the video (empty when not read).
pub fn category(track_id: &str) -> String {
    track_id.strip_prefix(ID_PREFIX).and_then(|v| KINDS.lock().unwrap().get(v).map(|k| k.1.clone())).unwrap_or_default()
}

/// Note what the page said about a video (`vid`: its id after "youtube:"; `music_site`: played on YouTube Music;
/// `category`: YouTube's, "" when not read). Windows' media session says no category: on YouTube Music (it said the
/// album) it's a song; else its first minute is checked for talk.
pub fn note_kind(vid: &str, music_site: bool, category: &str) {
    if !music_site && category.is_empty() {
        return;                                     // nothing read yet: whatever was read before stays
    }
    let mut kinds = KINDS.lock().unwrap();
    if kinds.len() > 200 && !kinds.contains_key(vid) {
        kinds.clear();
    }
    kinds.insert(vid.to_string(), (music_site, category.to_string()));
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

/// What YouTube is playing and where (None while YouTube is off).
pub fn state() -> Option<State> {
    if !enabled() {
        return None;
    }
    crate::platform::youtube::state()
}

/// "restart", "next", "previous", "playpause", "pause" or "play" to the video followed.
pub fn control(action: &str) {
    crate::platform::youtube::control(action);
}

/// Jump to a position (seconds) in the video playing.
pub fn seek(seconds: f64) {
    crate::platform::youtube::seek(seconds);
}

/// Play a video by its id ("youtube:…").
pub fn play_track(id: &str) {
    crate::platform::youtube::play_track(id);
}

/// Can OnKey follow YouTube in the browser whose sound comes from processes with this bundle id prefix?
pub fn followable(bundle: &str) -> bool {
    crate::platform::youtube::followable(bundle)
}

/// The bundle id prefix of the processes playing the followed browser's sound.
pub fn audio_bundle() -> &'static str {
    crate::platform::youtube::audio_bundle()
}

/// Is the position followed the browser's own? Always on macOS (the video's clock); on Windows only once the
/// session's timeline is seen current (else OnKey's clock, and listening's alignment places the song: listen.rs).
pub fn exact_position() -> bool {
    crate::platform::youtube::exact_position()
}

/// The browser to tell the user about when YouTube is playing but its page can't be read, with where the
/// setting is (macOS: its "Allow JavaScript from Apple Events").
pub fn js_blocked() -> Option<(&'static str, &'static str)> {
    if !enabled() {
        return None;
    }
    crate::platform::youtube::js_blocked()
}

/// A video's thumbnail (public, no login).
pub fn thumbnail(video: &str) -> String {
    format!("https://i.ytimg.com/vi/{video}/hqdefault.jpg")
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
pub(crate) fn strip_feat(s: &str) -> String {
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

/// A browser's media session (Windows) names a song by what the page told it: YouTube Music its artist, title and
/// album; youtube.com the video's title and channel, no album. → (artist, title), as `to_state` reads the page.
pub fn session_names(title: &str, artist: &str, album: &str) -> (String, String) {
    if !album.trim().is_empty() {
        return (strip_feat(artist), strip_feat(title));
    }
    let (a, t) = parse_title(title, artist);
    // a channel taken as the artist: "JunoHaleVEVO" is Juno Hale (lyrics are found by the name)
    let vevo = artist.trim().ends_with("VEVO") || artist.trim().ends_with("Vevo");
    let a = if vevo && a == clean_channel(artist) && !a.contains(' ') { spaced(&a) } else { a };
    (a, t)
}

/// "JunoHale" → "Juno Hale", "MCRook" → "MC Rook": a space before a capital that starts a word.
fn spaced(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &ch) in c.iter().enumerate() {
        let (prev, next) = (i.checked_sub(1).map(|j| c[j]), c.get(i + 1));
        if ch.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Why what a browser's media session (Windows) names isn't taken for a song, if it isn't: no artist (a page that
/// doesn't say what it plays: the browser shows the page's title), a length no song has (`duration` s, 0 unknown),
/// or a title or channel that says it's talk, a game or a film. A video that gets through and isn't on YouTube
/// Music is still checked for talk by its first minute (`might_not_be_song`, notmusic.rs).
pub fn not_a_song(title: &str, artist: &str, album: &str, duration: f64) -> Option<&'static str> {
    if artist.trim().is_empty() {
        return Some("no artist: the page doesn't say what it plays");
    }
    if duration > 20.0 * 60.0 {
        return Some("over 20 minutes: a mix, a show or a stream");
    }
    if duration > 0.0 && duration < 45.0 {
        return Some("under 45 seconds: a short or an advert");
    }
    if !album.trim().is_empty() {
        return None;                                // YouTube Music, which says the album: a song
    }
    // (whole words only, and none a song is often called: "How to Save a Life", "Chain Reaction", "Good News")
    const TALK: [&str; 22] = ["podcast", "episode", "interview", "gameplay", "walkthrough", "let's play", "lets play", "let s play", "trailer",
                              "reacts", "tutorial", "livestream", "live stream", "#shorts", "unboxing", "vlog", "documentary",
                              "lecture", "full album", "full movie", "explained", "asmr"];
    let words = |s: &str, keep: &str| {
        let s = s.to_lowercase().replace(|c: char| !c.is_alphanumeric() && !keep.contains(c), " ");
        format!(" {} ", s.split_whitespace().collect::<Vec<_>>().join(" "))
    };
    let (title, channel) = (words(title, "#'"), words(artist, ""));
    let said = |w: &str, text: &str| text.contains(&format!(" {w} "));
    if TALK.iter().any(|w| said(w, &title)) || ["podcast", "news"].iter().any(|w| said(w, &channel)) {
        return Some("its title or channel says it's not music");
    }
    None
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
    fn a_browsers_media_session_is_named_as_the_page_is() {
        // youtube.com: the video's title and channel, no album
        let cases = [
            ("Juno Hale - Tidewater (Official Music Video)", "JunoHaleVEVO", "", "Juno Hale", "Tidewater"),
            ("Tidewater [Lyrics]", "JunoHaleVEVO", "", "Juno Hale", "Tidewater"),
            ("Ivy Marsh - Kite Strings (Lyrics) ft. Otto Vale", "lyricvault", "", "Ivy Marsh", "Kite Strings"),
            ("Slow Satellite", "Moss Arcade - Topic", "", "Moss Arcade", "Slow Satellite"),
            ("UB12 - Blue Blue River", "UB12VEVO", "", "UB12", "Blue Blue River"),
            ("Paper Crown", "MCRookVEVO", "", "MC Rook", "Paper Crown"),
            // YouTube Music: its own artist and title, with the album
            ("Copper Sky (feat. Mira Sol)", "Otto Vale", "Copper Sky - Single", "Otto Vale", "Copper Sky"),
            ("Someone - Else", "Gale", "Rooms", "Gale", "Someone - Else"),
        ];
        for (title, artist, album, a, t) in cases {
            assert_eq!(session_names(title, artist, album), (a.to_string(), t.to_string()), "{title}");
        }
    }

    #[test]
    fn only_what_looks_like_a_song_in_a_browser_is_followed() {
        assert_eq!(not_a_song("Juno Hale - Tidewater (Official Video)", "JunoHaleVEVO", "", 214.0), None);
        assert_eq!(not_a_song("Tidewater", "Juno Hale", "Harbour", 0.0), None, "no timeline: length unknown");
        // songs called what a talk video might be
        for t in ["How to Save a Life", "Chain Reaction", "Good News", "Highlights", "Review My Kisses"] {
            assert_eq!(not_a_song(t, "Some Band", "", 230.0), None, "{t}");
        }
        assert!(not_a_song("Netflix", "", "", 0.0).is_some(), "a page that names no artist");
        assert!(not_a_song("Lofi beats to study to", "Lofi Girl", "", 3.0 * 3600.0).is_some(), "hours long");
        assert!(not_a_song("New phone!", "Someone", "", 20.0).is_some(), "a short");
        for (t, c) in [("The Joe Show Podcast #212 - Guest", "The Joe Show"), ("Elden Ring Gameplay Walkthrough Part 1", "Gamer"),
                       ("Dune: Part Three | Official Trailer", "Warner Bros"), ("Singer Interview 2026", "Talk FM"),
                       ("cute cat #shorts", "cats"), ("Let's Play Minecraft", "Steve"), ("Evening bulletin", "Sky News")] {
            assert!(not_a_song(t, c, "", 300.0).is_some(), "{t}");
        }
        // YouTube Music says the album: a song, whatever it's called
        assert_eq!(not_a_song("Interview", "Some Band", "Talk Is Cheap", 200.0), None);
    }

    #[test]
    fn only_videos_outside_youtube_music_and_its_music_category_may_not_be_songs() {
        note_kind("t-music", false, "Music");
        note_kind("t-comedy", false, "Comedy");
        note_kind("t-ytm", true, "");
        note_kind("t-ytm", false, "");          // (a read with nothing in it changes nothing)
        assert!(!might_not_be_song("youtube:t-music"));
        assert!(might_not_be_song("youtube:t-comedy"));
        assert!(!might_not_be_song("youtube:t-ytm"), "played on YouTube Music");
        assert!(might_not_be_song("youtube:t-unread"), "category not read: checked");
        assert!(!might_not_be_song("spotify:track:1"), "not a video");
        assert_eq!(category("youtube:t-comedy"), "Comedy");
    }

}
