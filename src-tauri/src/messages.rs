//! The words OnKey's Rust side shows itself (the Mac's menu bar, the share video's end card, the report's save
//! panel; macOS's permission prompts are build.rs's InfoPlist.strings), in the chosen language, and the window's
//! status lines as a kind and its parameters (`Line`) so the window can say them in its own.
//!
//! The words are in src-tauri/messages/<lang>.json (nested keys: `menu.app.pauseListening`), English's en.json the
//! source of truth; every file there is built in (build.rs). A key missing from a language is English's.
//! The language: Settings' choice ("language", settings.rs), else the first of the computer's preferred languages
//! (platform::sys::preferred_languages) there's a file for, else English.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/messages.rs"));

/// Every language's words, flattened ("menu.app.quit" → "Quit OnKey").
fn catalogs() -> &'static HashMap<&'static str, HashMap<String, String>> {
    static ALL: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    ALL.get_or_init(|| CATALOGS.iter().map(|(lang, text)| (*lang, flatten(&serde_json::from_str(text).unwrap_or(Value::Null)))).collect())
}

fn flatten(v: &Value) -> HashMap<String, String> {
    fn walk(prefix: &str, v: &Value, out: &mut HashMap<String, String>) {
        match v {
            Value::Object(o) => {
                for (k, v) in o {
                    walk(&if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") }, v, out);
                }
            }
            Value::String(s) => {
                out.insert(prefix.to_string(), s.clone());
            }
            _ => {}
        }
    }
    let mut out = HashMap::new();
    walk("", v, &mut out);
    out
}

/// The languages there are words for ("en", "es"…), sorted.
pub fn languages() -> Vec<&'static str> {
    let mut l: Vec<&str> = catalogs().keys().copied().collect();
    l.sort();
    l
}

/// The language with words that best fits `wanted` (preferred first: "pt-BR", "zh-Hans-CN", "en_GB.UTF-8"), from
/// `have`: each one as it is, then shorter ("zh-Hans-CN", "zh-Hans", "zh"), ignoring case; the first that fits wins.
/// So "es-MX", "es-419" and plain "es" are `es` (Latin American), "es-ES" is `es-ES`, "pt-PT" is `pt`. Chinese goes
/// by script, never to the other one: zh-TW, zh-HK, zh-MO and any "…-Hant…" are `zh-Hant`; every other zh (zh-CN,
/// zh-SG, plain zh) is `zh-Hans`. The same rule as the window's (ui/lib/i18n.ts pickLocale). English when none fits.
pub fn best_of(wanted: &[String], have: &[&str]) -> String {
    let find = |tag: &str| have.iter().find(|h| h.eq_ignore_ascii_case(tag)).map(|h| h.to_string());
    for w in wanted {
        let w = w.split(['.', '@']).next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
        let parts: Vec<&str> = w.split('-').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        for n in (1..=parts.len()).rev() {
            if let Some(h) = find(&parts[..n].join("-")) {
                return h;
            }
        }
        if parts[0] == "zh" {
            let rest = &parts[1..];
            let traditional = rest.contains(&"hant") || (!rest.contains(&"hans") && rest.iter().any(|r| matches!(*r, "tw" | "hk" | "mo")));
            if let Some(h) = find(if traditional { "zh-hant" } else { "zh-hans" }) {
                return h;
            }
        }
    }
    "en".into()
}

/// The Mac's folder of strings for a language, <name>.lproj (build.rs lproj_name: Apple's names, es-419 for our es).
pub fn lproj_name(lang: &str) -> &'static str {
    LPROJ.iter().find(|(l, _)| *l == lang).map(|(_, n)| *n).unwrap_or("en")
}

/// The language the native words are in now: Settings' choice, else the computer's.
pub fn current() -> String {
    let chosen = crate::settings::language();
    let wanted = if chosen == "auto" { crate::platform::sys::preferred_languages() } else { vec![chosen] };
    best_of(&wanted, &languages())
}

/// The words for `key` in `lang`, English's if that language hasn't them (the key itself if English hasn't either).
pub fn in_lang(lang: &str, key: &str) -> String {
    let all = catalogs();
    all.get(lang).and_then(|c| c.get(key)).or_else(|| all.get("en").and_then(|c| c.get(key))).cloned().unwrap_or_else(|| key.to_string())
}

/// The words for `key` in the current language.
pub fn t(key: &str) -> String {
    in_lang(&current(), key)
}

/// A line the window shows that the engine decides (the status line, an error): a stable `kind`, its parameters,
/// and its words in English (for the log, and for a window that doesn't know the kind). src-tauri/messages/KINDS.md
/// lists every kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: &'static str,
    pub params: Vec<(&'static str, String)>,
    pub text: String,
}

impl Line {
    pub fn new(kind: &'static str, text: impl Into<String>) -> Line {
        Line { kind, params: Vec::new(), text: text.into() }
    }

    pub fn with(mut self, name: &'static str, value: impl Into<String>) -> Line {
        self.params.push((name, value.into()));
        self
    }

    /// {"kind", "params": {…}, "text"} for the window.
    pub fn json(&self) -> Value {
        let params: serde_json::Map<String, Value> = self.params.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
        json!({"kind": self.kind, "params": params, "text": self.text})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_built_in_and_every_language_has_only_english_keys() {
        let all = catalogs();
        let en = &all["en"];
        assert_eq!(en["menu.app.pauseListening"], "Pause Listening");
        assert_eq!(en["infoPlist.NSMicrophoneUsageDescription"].split('.').next(), Some("OnKey listens to your singing to show how in tune you are"));
        for (lang, words) in all {
            for k in words.keys() {
                assert!(en.contains_key(k), "messages/{lang}.json has {k}, which English no longer has");
            }
        }
    }

    #[test]
    fn a_missing_key_falls_back_to_english_and_an_unknown_one_to_itself() {
        assert_eq!(in_lang("xx", "menu.app.quit"), "Quit OnKey", "no such language: English");
        assert_eq!(in_lang("en", "no.such.key"), "no.such.key");
    }

    #[test]
    fn a_fixture_language_works() {
        // a language file as a contributor would write it: some keys translated, some not yet
        let es = flatten(&serde_json::from_str(r#"{"menu": {"app": {"quit": "Salir de OnKey"}, "edit": {"copy": 7}}}"#).unwrap());
        assert_eq!(es.get("menu.app.quit").map(String::as_str), Some("Salir de OnKey"));
        assert_eq!(es.get("menu.edit.copy"), None, "not words: as if missing");
        let lookup = |k: &str| es.get(k).cloned().unwrap_or_else(|| in_lang("en", k));
        assert_eq!(lookup("menu.app.quit"), "Salir de OnKey");
        assert_eq!(lookup("menu.app.pauseListening"), "Pause Listening", "not translated yet: English");
    }

    #[test]
    fn the_best_language_is_the_first_preferred_one_there_are_words_for() {
        let have = ["en", "es", "pt", "zh-Hans", "zh-Hant"];
        let w = |v: &[&str]| best_of(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &have);
        assert_eq!(w(&["es-419", "en"]), "es");
        assert_eq!(w(&["fr-FR", "pt-BR"]), "pt", "French has no words: the next preferred");
        assert_eq!(w(&["zh-Hans-CN"]), "zh-Hans");
        assert_eq!(w(&["ZH-hant-tw"]), "zh-Hant");
        assert_eq!(w(&["es_ES.UTF-8"]), "es", "a POSIX locale");
        assert_eq!(w(&["de", "fr"]), "en", "none: English");
        assert_eq!(w(&[]), "en");
        assert_eq!(w(&[""]), "en");
    }

    #[test]
    fn spanish_portuguese_and_chinese_go_to_the_right_catalog() {
        let have = ["en", "es", "es-ES", "pt", "zh-Hans", "zh-Hant", "ja"];
        let w = |v: &[&str]| best_of(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &have);
        for (os, want) in [
            ("es-ES", "es-ES"), ("es_ES.UTF-8", "es-ES"), ("es-MX", "es"), ("es-419", "es"), ("es", "es"), ("es-AR", "es"),
            ("pt-BR", "pt"), ("pt-PT", "pt"), ("pt", "pt"),
            ("zh-Hans-CN", "zh-Hans"), ("zh-CN", "zh-Hans"), ("zh_CN.UTF-8", "zh-Hans"), ("zh-SG", "zh-Hans"), ("zh", "zh-Hans"),
            ("zh-Hant-TW", "zh-Hant"), ("zh-TW", "zh-Hant"), ("zh-HK", "zh-Hant"), ("zh-MO", "zh-Hant"), ("zh-Hant-HK", "zh-Hant"), ("ZH_TW", "zh-Hant"),
            ("ja-JP", "ja"), ("en-GB", "en"), ("fr-FR", "en"),
        ] {
            assert_eq!(w(&[os]), want, "{os}");
        }
        // every catalog there is: an OS language of each is found (the window's picker offers the same list)
        for lang in languages() {
            assert_eq!(best_of(&[lang.to_string()], &languages()), lang);
        }
    }

    #[test]
    fn a_line_says_its_kind_params_and_english() {
        let l = Line::new("stopped", "Not learning Yellow").with("song", "Yellow");
        assert_eq!(l.json(), json!({"kind": "stopped", "params": {"song": "Yellow"}, "text": "Not learning Yellow"}));
        assert_eq!(Line::new("finding", "x").json()["params"], json!({}));
    }

    /// The Mac bundle carries an InfoPlist.strings for every language (tauri.conf.json `bundle.macOS.files`) and
    /// Info.plist lists them all (CFBundleLocalizations), both under the Mac's name for the language (build.rs
    /// lproj_name: es-419 for our es, es for our es-ES): a new messages/<lang>.json needs both.
    #[test]
    fn every_language_is_in_the_mac_bundle() {
        let mut langs: Vec<&str> = LPROJ.iter().map(|(l, _)| *l).collect();
        langs.sort();
        assert_eq!(langs, languages(), "LPROJ names every language");
        assert_eq!((lproj_name("es"), lproj_name("es-ES"), lproj_name("pt"), lproj_name("zh-Hans"), lproj_name("en")), ("es-419", "es", "pt", "zh-Hans", "en"));
        let mut names: Vec<String> = LPROJ.iter().map(|(_, n)| n.to_string()).collect();
        names.sort();
        assert_eq!(names.iter().collect::<std::collections::BTreeSet<_>>().len(), names.len(), "two languages share an lproj name");
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut bundled: Vec<String> = conf["bundle"]["macOS"]["files"].as_object().unwrap().iter()
            .filter_map(|(to, from)| {
                let name = to.strip_prefix("Resources/")?.strip_suffix(".lproj")?;
                assert_eq!(from.as_str(), Some(format!("gen/lproj/{name}.lproj").as_str()), "{to}");
                Some(name.to_string())
            }).collect();
        bundled.sort();
        assert_eq!(bundled, names, "tauri.conf.json bundle.macOS.files: \"Resources/<lproj>.lproj\": \"gen/lproj/<lproj>.lproj\" for each language");
        let plist = include_str!("../Info.plist");
        let list = plist.split("<key>CFBundleLocalizations</key>").nth(1).and_then(|r| r.split("</array>").next()).expect("CFBundleLocalizations");
        let mut listed: Vec<String> = list.split("<string>").skip(1).filter_map(|s| s.split("</string>").next()).map(str::to_string).collect();
        listed.sort();
        assert_eq!(listed, names, "Info.plist CFBundleLocalizations");
        // Info.plist's own prompts (what macOS shows without a language's file) are English's
        for (k, v) in &catalogs()["en"] {
            if let Some(key) = k.strip_prefix("infoPlist.") {
                assert!(plist.contains(&format!("<key>{key}</key>\n  <string>{v}</string>")), "Info.plist's {key} isn't en.json's");
            }
        }
    }
}
