//! What a tester's report needs beyond the shared logs: nothing more on macOS (its own crash reports and
//! Console cover what Windows' diag.rs logs).

/// How the last session ended, if it wasn't a quit (read before this session's first log line).
pub fn starting() -> Option<String> {
    None
}

/// Launched: what's worth logging about this computer once (nothing more here).
pub fn started(_previous: Option<String>) {}

/// Files a report adds from the logs it gathered (`logs`: that text): (name, contents, what it is).
pub fn report_files(_logs: &str) -> Vec<(String, String, String)> {
    Vec::new()
}

/// What a report's about.json says of this OS beyond the shared facts.
pub fn report_about(_about: &mut serde_json::Value) {}

/// (Each song's learning times aren't logged per stage here.)
pub const LEARN_TIMES: bool = false;
