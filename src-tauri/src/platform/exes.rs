//! Which Windows program a macOS bundle id stands for (windows/procs.rs). The engine names apps by their macOS bundle
//! ids throughout (settings, records, the window); this is where they meet. Plain text, so tested on macOS too.

/// The program names (lower case, "spotify.exe") that a bundle id, or one under it ("com.google.Chrome.helper"),
/// means on Windows; none for "" or anything else.
pub fn exes_for(bundle: &str) -> &'static [&'static str] {
    const MAP: &[(&str, &[&str])] = &[
        ("com.spotify.client", &["spotify.exe"]),
        ("com.apple.Music", &["applemusic.exe", "itunes.exe"]),
        ("com.google.Chrome", &["chrome.exe"]),
        ("com.microsoft.edgemac", &["msedge.exe"]),
        ("org.mozilla.firefox", &["firefox.exe"]),
        ("com.brave.Browser", &["brave.exe"]),
        ("company.thebrowser.Browser", &["arc.exe"]),
        ("com.operasoftware.Opera", &["opera.exe"]),
        ("com.vivaldi.Vivaldi", &["vivaldi.exe"]),
    ];
    // (only a bundle id under an app's: the start of one, "" or "com", is no app, not the first in the list)
    MAP.iter().find(|(b, _)| bundle.starts_with(b)).map_or(&[], |(_, e)| e)
}

#[cfg(test)]
mod tests {
    use super::exes_for;

    #[test]
    fn bundle_ids_map_to_programs() {
        assert_eq!(exes_for("com.spotify.client"), &["spotify.exe"]);
        assert_eq!(exes_for("com.google.Chrome"), &["chrome.exe"]);
        assert_eq!(exes_for("com.google.Chrome.helper"), &["chrome.exe"]);
        assert!(exes_for("com.example.nothing").is_empty());
    }

    #[test]
    fn no_bundle_or_the_start_of_one_is_no_app() {
        for b in ["", "com", "com.spotify", "com.apple.WebKit.GPU"] {
            assert!(exes_for(b).is_empty(), "{b:?}");
        }
    }
}
