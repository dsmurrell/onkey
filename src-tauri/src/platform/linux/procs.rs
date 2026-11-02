//! Which programs are running (/proc), and which Linux program a macOS bundle id stands for.

/// The program names (lower case, as /proc/<pid>/comm and the sound server's application.process.binary say them)
/// that a bundle id, or the start of one, means on Linux. The engine names apps by their macOS bundle ids
/// throughout (settings, records, the window); this is where they meet. (comm is cut to 15 characters.)
pub fn exes_for(bundle: &str) -> &'static [&'static str] {
    const MAP: &[(&str, &[&str])] = &[
        ("com.spotify.client", &["spotify"]),
        ("com.google.Chrome", &["chrome", "google-chrome", "chromium", "chromium-browse", "chromium-browser"]),
        ("com.microsoft.edgemac", &["msedge", "microsoft-edge"]),
        ("org.mozilla.firefox", &["firefox", "firefox-bin", "firefox-esr"]),
        ("com.brave.Browser", &["brave", "brave-browser"]),
        ("com.operasoftware.Opera", &["opera"]),
        ("com.vivaldi.Vivaldi", &["vivaldi-bin", "vivaldi"]),
    ];
    MAP.iter().find(|(b, _)| b.starts_with(bundle) || bundle.starts_with(b)).map_or(&[], |(_, e)| e)
}

/// Whether a bundle id is OnKey's own: the demo song plays in OnKey's process (src/demo.rs `bundle`).
pub fn own(bundle: &str) -> bool {
    bundle == crate::demo::bundle()
}

/// Every running process: (process id, program name in lower case).
pub fn list() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) {
            out.push((pid, comm.trim().to_lowercase()));
        }
    }
    out
}

/// The processes of the app a bundle id stands for (OnKey's own: this process).
pub fn pids_for(bundle: &str) -> Vec<u32> {
    if own(bundle) {
        return vec![std::process::id()];
    }
    let exes = exes_for(bundle);
    if exes.is_empty() {
        return Vec::new();
    }
    list().into_iter().filter(|(_, n)| exes.contains(&n.as_str())).map(|(p, _)| p).collect()
}

pub fn running(bundle: &str) -> bool {
    !pids_for(bundle).is_empty()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundle_ids_map_to_programs() {
        assert_eq!(super::exes_for("com.spotify.client"), &["spotify"]);
        assert!(super::exes_for("com.google.Chrome").contains(&"chrome"));
        assert!(super::exes_for("com.example.nothing").is_empty());
    }

    #[test]
    fn onkey_itself_is_this_process_and_the_list_has_it() {
        let me = crate::demo::bundle();
        assert_eq!(super::pids_for(me), vec![std::process::id()]);
        assert!(super::running(me));
        assert!(super::list().iter().any(|(p, _)| *p == std::process::id()), "this process is in /proc");
    }
}
