//! A Windows report's summary.txt: what a tester's PC and plays were like, read off the log lines that answer the
//! questions we'd otherwise have to ask (this PC, its audio, the music apps followed, learning, takes, errors),
//! one section each. Also two measurements for that log: how far the music app's position was from where OnKey
//! found the song by its sound (`AppGap`, per song), and whether OnKey closed normally the last time (`unclean`).
//! Plain Rust with no Windows calls, so it's tested anywhere; only Windows builds use it.
use serde_json::{json, Value};

// ---------------------------------------------------------------- the app's position against the sound

/// One song's checks of the music app's position against the take's place found from the music's own sound (its
/// fingerprint): once a second while both are known. Positive gaps: the app said a later place than the sound.
#[derive(Debug, Default, Clone)]
pub struct AppGap {
    pub gaps: Vec<f64>,
    /// the app's position against the wall clock: times it moved by more than JUMP_S beyond the time that passed
    pub jumps: u32,
    pub biggest_jump: f64,
    /// (app position, wall clock s) at the last check
    last: Option<(f64, f64)>,
}

/// Checked at most this often (s).
const EVERY_S: f64 = 1.0;
/// A position that moves this much more (or less) than the time that passed has jumped (s).
const JUMP_S: f64 = 0.25;

impl AppGap {
    /// The app said `app` (s into the song) when the sound placed it at `sound`, at `wall` (any steady clock, s).
    pub fn sample(&mut self, app: f64, sound: f64, wall: f64) {
        if let Some((a, w)) = self.last {
            if wall - w < EVERY_S {
                return;
            }
            let off = (app - a) - (wall - w);
            if off.abs() > JUMP_S {
                self.jumps += 1;
                if off.abs() > self.biggest_jump.abs() {
                    self.biggest_jump = off;
                }
            }
        }
        self.last = Some((app, wall));
        self.gaps.push(app - sound);
    }

    /// Forget the last position (the song paused, or a check was missed): no jump is counted across the gap.
    pub fn pause(&mut self) {
        self.last = None;
    }

    /// The median gap, the one furthest from zero, and how many were within 0.25 s of the median.
    pub fn summary(&self) -> Option<Value> {
        if self.gaps.is_empty() {
            return None;
        }
        let mut g = self.gaps.clone();
        g.sort_by(|a, b| a.total_cmp(b));
        let median = g[g.len() / 2];
        let worst = *g.iter().max_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap();
        let near = g.iter().filter(|x| (*x - median).abs() <= 0.25).count();
        let r2 = |x: f64| (x * 100.0).round() / 100.0;
        Some(json!({"checks": g.len(), "median_s": r2(median), "worst_s": r2(worst), "min_s": r2(g[0]), "max_s": r2(g[g.len() - 1]),
                    "within_quarter_s_of_median": r2(near as f64 / g.len() as f64), "jumps": self.jumps, "biggest_jump_s": r2(self.biggest_jump)}))
    }

    /// The log line for it ("app vs sound: …"), None with no checks.
    pub fn line(&self, song: &str, app: &str) -> Option<String> {
        let s = self.summary()?;
        Some(format!("app vs sound: {song}: {app}'s position minus where the sound placed it: median {:+.2} s, worst {:+.2} s (range {:+.2} to {:+.2} s, {:.0}% within 0.25 s of the median, {} checks); its position jumped {} time{}{}",
            s["median_s"].as_f64().unwrap_or(0.0), s["worst_s"].as_f64().unwrap_or(0.0), s["min_s"].as_f64().unwrap_or(0.0),
            s["max_s"].as_f64().unwrap_or(0.0), s["within_quarter_s_of_median"].as_f64().unwrap_or(0.0) * 100.0, s["checks"],
            self.jumps, if self.jumps == 1 { "" } else { "s" },
            if self.jumps > 0 { format!(" (largest {:+.2} s)", self.biggest_jump) } else { String::new() }))
    }
}

/// The take being measured: its version, the app's name and its checks (one take at a time).
static TAKE_GAP: std::sync::Mutex<Option<(u64, &'static str, AppGap)>> = std::sync::Mutex::new(None);

/// A check for take `version` (a new take starts afresh).
pub fn gap_sample(version: u64, app_name: &'static str, app: f64, sound: f64, wall: f64) {
    let mut g = TAKE_GAP.lock().unwrap_or_else(|e| e.into_inner());
    if g.as_ref().map_or(true, |x| x.0 != version) {
        *g = Some((version, app_name, AppGap::default()));
    }
    if let Some(x) = g.as_mut() {
        x.2.sample(app, sound, wall);
    }
}

/// Take `version`'s app paused or played something else for a moment: no jump is counted across it.
pub fn gap_pause(version: u64) {
    if let Some(x) = TAKE_GAP.lock().unwrap_or_else(|e| e.into_inner()).as_mut().filter(|x| x.0 == version) {
        x.2.pause();
    }
}

/// Take `version` ended: its app's name and checks, if any were made.
pub fn gap_finish(version: u64) -> Option<(&'static str, AppGap)> {
    let mut g = TAKE_GAP.lock().unwrap_or_else(|e| e.into_inner());
    if g.as_ref().is_some_and(|x| x.0 == version) {
        return g.take().map(|x| (x.1, x.2));
    }
    None
}

// ---------------------------------------------------------------- the sound captured

/// What a capture gave, for the log: how much of the music and the mic was silent or clipped, and the mic's peak.
/// Fed [music L, music R, mic] frames, about 10 ms at a time.
#[derive(Debug, Default, Clone)]
pub struct Captured {
    pub blocks: u64,
    pub music_silent: u64,
    pub mic_silent: u64,
    /// the longest run of silent music blocks, and the one now
    longest_quiet: u64,
    quiet: u64,
    pub music_clipped: u64,
    pub mic_clipped: u64,
    pub mic_peak: f32,
    /// music dropped as it ran ahead of the mic (clock drift), times
    pub dropped: u64,
    /// music that ran out while it played (later than the cushion, or stopped): a gap, times
    pub short: u64,
}

/// Below this a block is silent (-80 dBFS); at or over this a sample is clipped.
const SILENT: f32 = 1e-4;
const CLIPPED: f32 = 0.999;

impl Captured {
    pub fn add(&mut self, frames: &[f32]) {
        if frames.len() < 3 {
            return;
        }
        let (mut mp, mut vp) = (0f32, 0f32);
        for f in frames.chunks_exact(3) {
            let m = f[0].abs().max(f[1].abs());
            mp = mp.max(m);
            vp = vp.max(f[2].abs());
            self.music_clipped += (m >= CLIPPED) as u64;
            self.mic_clipped += (f[2].abs() >= CLIPPED) as u64;
        }
        self.blocks += 1;
        self.mic_peak = self.mic_peak.max(vp);
        self.mic_silent += (vp < SILENT) as u64;
        if mp < SILENT {
            self.music_silent += 1;
            self.quiet += 1;
            self.longest_quiet = self.longest_quiet.max(self.quiet);
        } else {
            self.quiet = 0;
        }
    }

    /// "music silent 12% of the time (longest 40.0 s), 0 samples clipped; mic silent 3%, peak -8 dBFS, 0 samples
    /// clipped" over `block_s` (s) a block.
    pub fn line(&self, block_s: f64) -> String {
        let pct = |n: u64| if self.blocks == 0 { 0.0 } else { n as f64 * 100.0 / self.blocks as f64 };
        let peak = if self.mic_peak > 0.0 { format!("{:.0} dBFS", 20.0 * self.mic_peak.log10()) } else { "silence".into() };
        format!("{:.0} s captured: music silent {:.0}% of the time (longest {:.1} s), {} samples clipped; mic silent {:.0}%, peak {peak}, {} samples clipped{}{}",
                self.blocks as f64 * block_s, pct(self.music_silent), self.longest_quiet as f64 * block_s, self.music_clipped,
                pct(self.mic_silent), self.mic_clipped,
                if self.dropped > 0 { format!("; music ran ahead of the mic and was cut {} times", self.dropped) } else { String::new() },
                if self.short > 0 { format!("; music ran short {} times (late, or it stopped)", self.short) } else { String::new() })
    }
}

// ---------------------------------------------------------------- device names

/// An output's name as logged: a word naming its owner taken out ("Headphones (Bob's AirPods)" is
/// "Headphones ([name]'s AirPods)"); microphones' names are taken out of a report altogether (report.rs).
pub fn device_name(name: &str) -> String {
    name.split(' ').map(|w| {
        match w.find("'s").or_else(|| w.find("\u{2019}s")) {
            Some(i) if i > 0 => {
                let lead = w[..i].chars().take_while(|c| !c.is_alphanumeric()).collect::<String>();
                format!("{lead}[name]{}", &w[i..])
            }
            _ => w.to_string(),
        }
    }).collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------- did it close normally last time?

/// The log before this launch (its end): None if the last session in it quit normally (a "quitting:" line after its
/// "==== OnKey" header) or there's none; else the last line's time, as a sentence for the log.
pub fn unclean(log: &str) -> Option<String> {
    let start = log.rfind("==== OnKey")?;
    let session = &log[start..];
    if session.lines().any(|l| l.contains(" quitting:") || l.contains(" told to stop")) {
        return None;
    }
    let last = session.lines().rev().find(|l| !l.trim().is_empty())?;
    let when = last.split(' ').next().unwrap_or("?");
    Some(format!("OnKey didn't close normally last time: no quit was logged; its last log line was at {when}"))
}

// ---------------------------------------------------------------- summary.txt

/// What a section takes from the log: lines whose message (after the time) starts with or contains one of these.
struct Section {
    title: &'static str,
    starts: &'static [&'static str],
    contains: &'static [&'static str],
    /// the most recent this many (distinct) lines
    keep: usize,
}

const SECTIONS: &[Section] = &[
    Section { title: "This PC (graphics, and where each model runs)", starts: &["gpu", "models:", "webview2:", "windows:", "linux:"], contains: &[], keep: 20 },
    Section { title: "Audio (devices, how the music app was heard, mic access)", starts: &["audio devices:", "capture (windows)", "capture (linux)", "audio server:", "playback (linux)", "capture started:", "mic access", "capture:"], contains: &["without capture"], keep: 30 },
    Section { title: "Music apps (which app was followed, what its media session said)", starts: &["media session:", "youtube on windows:", "mpris:", "youtube on linux:", "now following", "stopped listening"], contains: &[" plays \""], keep: 30 },
    Section { title: "The app's position against the sound (per song)", starts: &["app vs sound:"], contains: &[], keep: 40 },
    Section { title: "Learning (times per song, and any model that failed)", starts: &["learn times:", "learner:", "learning models:"], contains: &["couldn't be loaded", "learned in"], keep: 30 },
    Section { title: "Takes (recordings saved and played back)", starts: &["take audio:", "voice playback:"], contains: &[], keep: 30 },
    Section { title: "Errors, panics and crashes", starts: &["ui: error", "ui: unhandled", "panic", "crash:", "onkey didn't close normally", "warning"], contains: &[" panicked", "error"], keep: 40 },
];

fn message(line: &str) -> (&str, &str) {
    match line.split_once(' ') {
        Some((t, m)) if t.len() >= 19 && t.as_bytes().get(4) == Some(&b'-') => (t, m),
        _ => ("", line),
    }
}

fn wanted(s: &Section, msg: &str) -> bool {
    let m = msg.to_lowercase();
    s.starts.iter().any(|p| m.starts_with(p)) || s.contains.iter().any(|p| m.contains(p))
}

/// summary.txt: `facts` (about this PC: {"pc": records::mac(), "webview2", "build"}) and, from `log` (the report's
/// log text, already redacted), each section's lines, the same line once with how often it came.
pub fn summary(log: &str, facts: &Value) -> String {
    let mut out = String::new();
    out.push_str("OnKey report summary\n====================\n");
    out.push_str("What the logs say about the questions we'd otherwise ask. The full logs are in logs/, each play's numbers in takes/.\n\n");
    let pc = &facts["pc"];
    let s = |v: &Value| v.as_str().map(String::from).unwrap_or_else(|| if v.is_null() { "?".into() } else { v.to_string() });
    out.push_str(&format!("OnKey {} ({})\n", s(&facts["build"]["version"]), s(&facts["build"]["sha"])));
    out.push_str(&format!("{} | {} | {} | {} GB RAM, {} cores\n", s(&pc["macos"]), s(&pc["model"]), s(&pc["chip"]), s(&pc["ram_gb"]), s(&pc["cores"])));
    out.push_str(&format!("WebView2 {}\n", s(&facts["webview2"])));
    let lines: Vec<&str> = log.lines().collect();
    let launches = lines.iter().filter(|l| l.contains("==== OnKey")).count();
    let quits = lines.iter().filter(|l| l.contains(" quitting: done") || l.contains(" quitting: gave up")).count();
    out.push_str(&format!("{launches} launch{} in these logs, {quits} normal quit{}\n", if launches == 1 { "" } else { "es" }, if quits == 1 { "" } else { "s" }));
    for sec in SECTIONS {
        // each distinct message: its first and last time, and how often
        let mut seen: Vec<(String, String, String, usize)> = Vec::new();
        for l in &lines {
            let (t, m) = message(l);
            if m == "ui: window loaded" || !wanted(sec, m) {
                continue;
            }
            match seen.iter_mut().find(|x| x.0 == m) {
                Some(x) => {
                    x.2 = t.to_string();
                    x.3 += 1;
                }
                None => seen.push((m.to_string(), t.to_string(), t.to_string(), 1)),
            }
        }
        out.push_str(&format!("\n## {}\n", sec.title));
        if seen.is_empty() {
            out.push_str("(nothing in these logs)\n");
            continue;
        }
        let left = seen.len().saturating_sub(sec.keep);
        if left > 0 {
            out.push_str(&format!("({left} older line{} left out here; they're in logs/)\n", if left == 1 { "" } else { "s" }));
        }
        for (m, first, last, n) in &seen[left..] {
            if *n == 1 {
                out.push_str(&format!("{first} {m}\n"));
            } else {
                out.push_str(&format!("{first} {m}  [x{n}, last {last}]\n"));
            }
        }
        if sec.starts == ["app vs sound:"] {
            out.push_str(&gaps_overall(seen.iter().map(|x| x.0.as_str())));
        }
    }
    out
}

/// Across songs: the median of the songs' medians and the worst of their worst gaps, and the jumps in all.
fn gaps_overall<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    let num = |m: &str, key: &str| m.split(key).nth(1).and_then(|r| r.split(' ').next()).and_then(|x| x.trim_start_matches('+').parse::<f64>().ok());
    let (mut med, mut worst, mut jumps) = (Vec::new(), Vec::new(), 0u32);
    for m in lines {
        med.extend(num(m, "median "));
        worst.extend(num(m, "worst "));
        jumps += num(m, "jumped ").unwrap_or(0.0) as u32;
    }
    if med.is_empty() {
        return String::new();
    }
    med.sort_by(|a, b| a.total_cmp(b));
    let w = worst.iter().copied().max_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap_or(0.0);
    format!("Over {} song{}: median of medians {:+.2} s, worst {:+.2} s, {jumps} jump{} in all\n", med.len(), if med.len() == 1 { "" } else { "s" },
            med[med.len() / 2], w, if jumps == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gap_is_the_median_and_worst_and_jumps_are_counted() {
        let mut g = AppGap::default();
        // the app 0.5 s ahead of the sound, read once a second; at 10 s its position jumps 0.8 s ahead
        for i in 0..20 {
            let t = i as f64;
            let app = t + 0.5 + if i >= 10 { 0.8 } else { 0.0 };
            g.sample(app, t, t);
            g.sample(app + 0.1, t + 0.1, t + 0.1);       // (more often than once a second: not counted)
        }
        let s = g.summary().unwrap();
        assert_eq!(s["checks"], 20);
        assert_eq!(s["jumps"], 1);
        assert!((s["biggest_jump_s"].as_f64().unwrap() - 0.8).abs() < 1e-9);
        assert!((s["worst_s"].as_f64().unwrap() - 1.3).abs() < 1e-9);
        assert!((s["median_s"].as_f64().unwrap() - 1.3).abs() < 1e-9);
        let line = g.line("Bad Day", "Spotify").unwrap();
        assert!(line.starts_with("app vs sound: Bad Day: Spotify's position"), "{line}");
        assert!(line.contains("median +1.30 s") && line.contains("jumped 1 time (largest +0.80 s)"), "{line}");
        // a pause: no jump counted across it
        g.pause();
        g.sample(100.0, 99.0, 40.0);
        assert_eq!(g.jumps, 1);
        assert!(AppGap::default().line("x", "y").is_none());
    }

    #[test]
    fn captured_sound_counts_silence_and_clipping() {
        let mut c = Captured::default();
        let block = |m: f32, v: f32| (0..480).flat_map(|_| [m, -m, v]).collect::<Vec<f32>>();
        for _ in 0..3 {
            c.add(&block(0.0, 0.1));
        }
        c.add(&block(0.5, 1.0));
        let line = c.line(0.01);
        assert_eq!((c.blocks, c.music_silent, c.mic_silent), (4, 3, 0));
        assert_eq!((c.music_clipped, c.mic_clipped), (0, 480));
        assert!(line.contains("music silent 75% of the time (longest 0.0 s)") && line.contains("peak 0 dBFS, 480 samples clipped"), "{line}");
    }

    #[test]
    fn an_outputs_owner_is_left_out_of_its_name() {
        assert_eq!(device_name("Headphones (Bob's AirPods Pro)"), "Headphones ([name]'s AirPods Pro)");
        assert_eq!(device_name("Speakers (Realtek(R) Audio)"), "Speakers (Realtek(R) Audio)");
        assert_eq!(device_name("Anna\u{2019}s Buds"), "[name]\u{2019}s Buds");
    }

    #[test]
    fn a_session_with_no_quit_did_not_close_normally() {
        let clean = "2026-10-28T10:00:00 ==== OnKey 0.1\n2026-10-28T10:05:00 quitting: keeping what's been learned\n2026-10-28T10:05:01 quitting: done in 0.1 s\n";
        assert_eq!(unclean(clean), None);
        let crashed = format!("{clean}2026-10-28T11:00:00 ==== OnKey 0.1\n2026-10-28T11:02:00 capture started: Spotify\n\n");
        let said = unclean(&crashed).unwrap();
        assert!(said.contains("didn't close normally") && said.contains("2026-10-28T11:02:00"), "{said}");
        assert_eq!(unclean(""), None);
        // Windows signing out (platform/windows/events.rs) is a normal end too
        let signed_out = format!("{crashed}2026-10-28T12:00:00 ==== OnKey 0.1
2026-10-28T12:30:00 told to stop (Windows is signing out, restarting or shutting down)
");
        assert_eq!(unclean(&signed_out), None);
    }

    #[test]
    fn the_summary_puts_each_line_under_its_question() {
        let log = "2026-10-28T10:00:00 ==== OnKey 0.1.0 (abc) on Dell XPS\n\
                   2026-10-28T10:00:01 gpu 0: NVIDIA RTX 3060, 12 GB\n\
                   2026-10-28T10:00:01 mic access (Windows privacy): allowed\n\
                   2026-10-28T10:00:02 ui: window loaded\n\
                   2026-10-28T10:01:00 media session: Spotify (app id \"Spotify.exe\"): \"Bad Day\" by \"Daniel Powter\", timeline yes\n\
                   2026-10-28T10:01:01 capture (Windows): only spotify.exe and what it started (per-process loopback)\n\
                   2026-10-28T10:04:00 app vs sound: Bad Day: Spotify's position minus where the sound placed it: median +0.40 s, worst -1.20 s (range -1.20 to +0.70 s, 90% within 0.25 s of the median, 180 checks); its position jumped 2 times (largest +0.80 s)\n\
                   2026-10-28T10:08:00 app vs sound: Hello: Spotify's position minus where the sound placed it: median +0.60 s, worst +0.90 s (range +0.10 to +0.90 s, 95% within 0.25 s of the median, 200 checks); its position jumped 1 time (largest +0.30 s)\n\
                   2026-10-28T10:05:00 learn times: Bad Day: separation 40 s (separation on DirectML)\n\
                   2026-10-28T10:05:01 take audio: 183 s saved as AAC in 0.40 s\n\
                   2026-10-28T10:06:00 ui: error: x is undefined at app.js:1\n\
                   2026-10-28T10:06:30 ui: error: x is undefined at app.js:1\n\
                   2026-10-28T10:07:00 panic in thread 'engine' at src\\engine.rs:12:5: index out of bounds\n";
        let facts = json!({"pc": {"macos": "Windows 11 24H2 (build 26100)", "model": "Dell XPS 15", "chip": "Intel i7", "ram_gb": 16.0, "cores": 8},
                           "webview2": "130.0.2849.56", "build": {"version": "0.1.0", "sha": "abc"}});
        let s = summary(log, &facts);
        let sec = |title: &str| s.split("\n## ").find(|x| x.starts_with(title)).unwrap_or("").to_string();
        assert!(s.contains("Windows 11 24H2 (build 26100) | Dell XPS 15 | Intel i7 | 16.0 GB RAM, 8 cores") && s.contains("WebView2 130.0.2849.56"), "{s}");
        assert!(sec("This PC").contains("gpu 0: NVIDIA"));
        assert!(sec("Audio").contains("mic access") && sec("Audio").contains("per-process loopback"));
        assert!(sec("Music apps").contains("media session: Spotify"));
        assert!(sec("The app's position").contains("Over 2 songs: median of medians +0.60 s, worst -1.20 s, 3 jumps in all"), "{s}");
        assert!(sec("Learning").contains("learn times: Bad Day"));
        assert!(sec("Takes").contains("saved as AAC"));
        let errors = sec("Errors");
        assert!(errors.contains("ui: error: x is undefined at app.js:1  [x2, last 2026-10-28T10:06:30]"), "{errors}");
        assert!(errors.contains("panic in thread 'engine'"));
        assert!(!s.contains("window loaded"));
        assert!(s.contains("1 launch in these logs, 0 normal quits"));
        assert!(summary("", &json!({})).contains("(nothing in these logs)"));
    }
}
