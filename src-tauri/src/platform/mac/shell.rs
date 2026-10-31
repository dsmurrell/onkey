//! Finder, the pasteboard and System Settings: `open` (a link, file or folder with its app), `open -R` (a file
//! shown in Finder), `pbcopy`, the home folder, the privacy panes.
use std::process::Command;

/// Open a URL (https:, spotify:, music://…) or a file or folder with what macOS opens it with. True if it did.
pub fn open(target: impl AsRef<std::ffi::OsStr>) -> bool {
    Command::new("open").arg(target).status().is_ok_and(|s| s.success())
}

/// Open an app's own link (spotify:…, music://…); false when nothing took it.
pub fn open_app_link(uri: &str) -> bool {
    open(uri)
}

/// A song's link (spotify:track:…, a search, a YouTube or Apple Music page): macOS opens each in what takes it.
pub fn open_song_link(url: &str) {
    open(url);
}

/// A file shown selected in a Finder window.
pub fn reveal(path: &std::path::Path) {
    let _ = Command::new("open").arg("-R").arg(path).status();
}

/// Text on the clipboard.
pub fn copy(text: &str) -> bool {
    use std::io::Write;
    let Ok(mut c) = Command::new("/usr/bin/pbcopy").stdin(std::process::Stdio::piped()).spawn() else { return false };
    let ok = c.stdin.take().is_some_and(|mut i| i.write_all(text.as_bytes()).is_ok());
    ok && c.wait().is_ok_and(|s| s.success())
}

/// The home folder ("" if unknown).
pub fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// The Downloads folder (~/Downloads).
pub fn downloads() -> std::path::PathBuf {
    std::path::Path::new(&home()).join("Downloads")
}

/// Move a file or folder to the Trash (~/.Trash, named so it doesn't clash); an error, and nothing moved, if it can't.
pub fn trash(p: &std::path::Path) -> Result<(), String> {
    let home = home();
    if home.is_empty() {
        return Err("no home folder".into());
    }
    crate::store::trash_into(p, &std::path::Path::new(&home).join(".Trash"))
}

/// System Settings' privacy pane for "mic", "audio" (system audio recording) or "automation" (control of apps).
pub fn open_settings(which: &str) {
    let pane = match which {
        "mic" => "Privacy_Microphone",
        "automation" => "Privacy_Automation",
        _ => "Privacy_AudioCapture",
    };
    let _ = Command::new("open").arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}")).status();
}
