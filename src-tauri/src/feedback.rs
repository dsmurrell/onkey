//! "Send feedback": a thumbs up or down and a few words, sent to OnKey's maker only when you press Send, or tap a thumb on a take's card (the top
//! bar's speech bubble, Settings → Send feedback, or the card on a take's page). What goes: your thumb, what you wrote, the email address you gave
//! (only if you gave one), OnKey's version, the system (macOS, Windows or Linux, and its version), the app's language,
//! and where you sent it from. Nothing else: no logs, no songs, no recordings.
//!
//! It goes to feedback.singonkey.app, a small Cloudflare Worker (server/feedback) that
//! emails it on. The card on a take's page asks at most twice: after your 3rd sung take, then your 20th (`ASK_AT`).
use serde_json::{json, Value};
use std::time::Duration;

/// Where feedback goes (scripts/release-hosts.txt lists it).
pub const URL: &str = "https://feedback.singonkey.app/";

/// The take page asks after this many sung takes, once each.
pub const ASK_AT: [usize; 2] = [3, 20];

/// A take counts as sung with a tenth of the singer's part sung (as share.rs counts it).
const SUNG: f64 = 0.1;

/// Only a take sung in the last hour is asked about, not one opened later from History.
const FRESH_S: u64 = 3600;

/// In the data folder: the ask stages already shown, one per line.
const ASKED: &str = "feedback-asked";

pub const MAX_TEXT: usize = 4000;
pub const MAX_EMAIL: usize = 200;

fn asked_stages(root: &std::path::Path) -> Vec<usize> {
    std::fs::read_to_string(root.join(ASKED)).unwrap_or_default().lines().filter_map(|l| l.trim().parse().ok()).collect()
}

/// The stage to ask at for `sung` sung takes, given the stages already asked: the highest one reached and not yet
/// asked (someone with 50 takes when this arrives is asked once, not twice in a row).
pub fn stage(sung: usize, asked: &[usize]) -> Option<usize> {
    let s = ASK_AT.iter().copied().filter(|&s| sung >= s).max()?;
    (!asked.contains(&s)).then_some(s)
}

/// Should take `id`'s page show the card? The stage it's for, or None.
pub fn ask(store: &crate::store::Store, id: &str) -> Option<usize> {
    let dir = store.take_dir(id)?;
    let age = std::fs::metadata(dir.join("result.json")).ok()?.modified().ok()?.elapsed().unwrap_or_default();
    if age.as_secs() > FRESH_S {
        return None;
    }
    let all = store.takes();
    let sung = |t: &Value| t["coverage"].as_f64().unwrap_or(0.0) >= SUNG;
    all.iter().find(|t| t["id"] == id).filter(|t| sung(t))?;
    stage(all.iter().filter(|t| sung(t)).count(), &asked_stages(&store.root))
}

/// The card was shown for `stage`: it, and every stage before it, isn't asked again.
pub fn asked(store: &crate::store::Store, stage: usize) {
    let mut all = asked_stages(&store.root);
    all.extend(ASK_AT.iter().copied().filter(|&s| s <= stage));
    all.sort_unstable();
    all.dedup();
    let text: String = all.iter().map(|s| format!("{s}\n")).collect();
    let _ = std::fs::write(store.root.join(ASKED), text);
}

/// What's sent, checked: a thumb ("up" or "down") or some words, or both. Err: why it can't be sent.
pub fn body(mood: Option<&str>, text: &str, email: &str, from: &str, language: &str) -> Result<Value, String> {
    let mood = match mood {
        Some(m @ ("up" | "down")) => Some(m),
        Some(_) => return Err("unknown thumb".into()),
        None => None,
    };
    let text = text.trim();
    let email = email.trim();
    if mood.is_none() && text.is_empty() {
        return Err("nothing to send".into());
    }
    if text.chars().count() > MAX_TEXT || email.chars().count() > MAX_EMAIL {
        return Err("too long".into());
    }
    let from = if matches!(from, "take" | "settings" | "bar") { from } else { "settings" };
    let sys = crate::records::mac();
    Ok(json!({
        "mood": mood, "text": text, "email": email, "from": from,
        "version": crate::records::build()["version"], "os": std::env::consts::OS,
        // (macOS's version on a Mac; Windows keeps its version there too: platform::diag)
        "os_version": sys["macos"], "language": language.chars().take(16).collect::<String>(),
    }))
}

/// Send it. Ok once the Worker has it.
pub fn send(body: &Value) -> Result<(), String> {
    let r = ureq::post(URL)
        .set("User-Agent", &crate::lyrics::user_agent())
        .timeout(Duration::from_secs(15))
        .send_json(body.clone());
    match r {
        Ok(_) => {
            crate::engine::log(&format!("feedback: sent ({})", body["mood"].as_str().unwrap_or("words")));
            Ok(())
        }
        Err(ureq::Error::Status(code, _)) => Err(format!("the feedback server said {code}")),
        Err(e) => Err(format!("couldn't reach the feedback server: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_at_3_then_20_once_each() {
        assert_eq!(stage(2, &[]), None);
        assert_eq!(stage(3, &[]), Some(3));
        assert_eq!(stage(5, &[3]), None);
        assert_eq!(stage(20, &[3]), Some(20));
        assert_eq!(stage(25, &[3, 20]), None);
        // already 50 takes when this arrives: one ask, for 20 (asked() then marks 3 too)
        assert_eq!(stage(50, &[]), Some(20));
    }

    #[test]
    fn body_needs_a_thumb_or_words() {
        assert!(body(None, "  ", "", "take", "en").is_err());
        assert!(body(Some("sideways"), "hi", "", "take", "en").is_err());
        let b = body(Some("up"), "", "", "take", "en").unwrap();
        assert_eq!(b["mood"], "up");
        let b = body(None, " love it ", " a@b.c ", "elsewhere", "es").unwrap();
        assert_eq!(b["text"], "love it");
        assert_eq!(b["email"], "a@b.c");
        assert_eq!(b["from"], "settings");
        assert_eq!(body(Some("up"), "", "", "bar", "en").unwrap()["from"], "bar");
        assert!(body(None, &"x".repeat(MAX_TEXT + 1), "", "take", "en").is_err());
    }
}
