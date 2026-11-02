//! The large models OnKey needs to learn songs ship inside the app (Contents/Resources/models, about 380 MB),
//! and only there: nothing is downloaded. At launch each one is checked to be there at its full size; a set that
//! isn't (a damaged install) means new songs can't be learned, and the window says so and suggests reinstalling.
//! The SHA-256s are checked on the built app before it ships (scripts/check-release.sh reads them from here).
use std::path::Path;

pub struct Model {
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

/// Every model the app bundles (tauri.conf.json `bundle.resources`), with its size and SHA-256.
pub const MODELS: [Model; 8] = [
    // Ultimate Vocal Remover's karaoke separator (MIT, credit UVR)
    Model { file: "UVR_MDXNET_KARA_2.onnx",
            sha256: "bf32e15105a09c0f7dddd2b67346146334d6f3ecb399ed7638eba2ab07cbf5f4", bytes: 52_786_726 },
    Model { file: "crepe-full-b32.onnx",
            sha256: "a41c3191fd398754eb46bacd7bb79eb71bfc3b0e7cd47989c2175c64adfe9d28", bytes: 88_991_852 },
    Model { file: "wav2vec2-base-960h-fp16.onnx",
            sha256: "0726ce2cdbb06fd0e90af436833f609d0f5366a914f4a456936c8851a367a6d9", bytes: 189_113_245 },
    // GAME small (OpenVPI, MIT): note transcription for the syllable bars' notes
    Model { file: "game/bd2dur.onnx",
            sha256: "6672d56b00f27a20e9bfaa92801c15e394bb520e49031d3ed2515b0a6cf27402", bytes: 4_569 },
    Model { file: "game/dur2bd.onnx",
            sha256: "49d10d1ea186e43c51fd2c51cedb01a3852d363f1c7056ae77df50f7e8dd0996", bytes: 2_632 },
    Model { file: "game/encoder.onnx",
            sha256: "a6868a702d9c88ccc06b19e54f49d88b7f8cba4a68517f20806bd81ef6f94e06", bytes: 10_805_372 },
    Model { file: "game/estimator.onnx",
            sha256: "a84738b2504472a1f22773cc6b0eccad1353a8f36229ad490b8fea85388bb5e1", bytes: 19_335_163 },
    Model { file: "game/segmenter.onnx",
            sha256: "7724db4ba59a388fff5aba2f08f0ac025d4ca59caef80e7f474053a1455c88e5", bytes: 20_906_659 },
];

/// Whether new songs can be learned, for the window: {"state": "ready"}, or {"state": "missing" | "broken", "code",
/// "error"} (the code: src-tauri/messages/KINDS.md; the error: the English details, for the log and a report).
static STATE: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);

pub fn set_state(v: serde_json::Value) {
    *STATE.lock().unwrap() = Some(v);
}

pub fn state() -> serde_json::Value {
    STATE.lock().unwrap().clone().unwrap_or(serde_json::json!({"state": "ready"}))
}

/// Whether `dir` (the app's bundled models folder) holds every model at its full size; if not, which isn't.
pub fn check(dir: &Path) -> Result<(), String> {
    let bad: Vec<String> = MODELS.iter().filter_map(|m| match std::fs::metadata(dir.join(m.file)) {
        Ok(md) if md.len() == m.bytes => None,
        Ok(md) => Some(format!("{} is {} bytes, not {}", m.file, md.len(), m.bytes)),
        Err(_) => Some(format!("{} isn't there", m.file)),
    }).collect();
    if bad.is_empty() { Ok(()) } else { Err(bad.join("; ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_a_sha256_and_a_size() {
        for m in &MODELS {
            assert!(m.sha256.len() == 64 && m.sha256.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "{}", m.file);
            assert!(m.bytes > 0, "{}", m.file);
        }
    }

    /// Every model listed here is one the app bundles, and the other way round.
    #[test]
    fn the_list_is_what_tauri_bundles() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut bundled: Vec<String> = conf["bundle"]["resources"].as_object().unwrap().keys()
            .filter_map(|k| k.strip_prefix("models/").map(str::to_string)).collect();
        let mut listed: Vec<String> = MODELS.iter().map(|m| m.file.to_string()).collect();
        bundled.sort();
        listed.sort();
        assert_eq!(bundled, listed);
    }

    #[test]
    fn a_missing_or_short_model_is_named() {
        let dir = std::env::temp_dir().join(format!("onkey-models-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        for m in &MODELS {
            let f = std::fs::File::create(dir.join(m.file)).unwrap();
            f.set_len(m.bytes).unwrap();
        }
        assert_eq!(check(&dir), Ok(()));
        std::fs::File::create(dir.join("game/encoder.onnx")).unwrap().set_len(10).unwrap();
        std::fs::remove_file(dir.join("crepe-full-b32.onnx")).unwrap();
        let e = check(&dir).unwrap_err();
        assert!(e.contains("crepe-full-b32.onnx isn't there") && e.contains("game/encoder.onnx is 10 bytes"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
