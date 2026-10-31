//! AppleScript (osascript), how OnKey talks to the music apps and browsers on macOS: one script at a time per
//! call, with a time limit; macOS's refusals (Automation off) remembered for the window, an app that didn't answer
//! left alone for a while (source.rs keeps both).
use std::sync::Mutex;

/// AppleScript's errors when macOS won't let OnKey send an app Apple events (refused, or not yet asked and
/// asking isn't allowed).
pub fn is_denial(stderr: &str) -> bool {
    stderr.contains("-1743") || stderr.contains("-1744")
}

/// Run AppleScript that talks to `app` (its name as people know it). A refusal by macOS is remembered for the
/// window; a script that got through to the app forgets it.
///
/// Spotify and Apple Music are never sent anything while you've chosen not to let OnKey control them (listening
/// mode with their control off in Settings), whoever asks: an Apple event to them is what makes macOS ask
/// for control, which listening alone must never do.
pub fn osa(app: &'static str, script: &str) -> Option<String> {
    // refusals in a row: (how many, since when). One alone can be macOS hiccupping (live: a single -1743 two
    // minutes into a song, Automation allowed all along, split the take in three); refused 3 times over 5 s is real
    static STREAK: Mutex<(u32, Option<std::time::Instant>)> = Mutex::new((0, None));
    if !crate::source::may_control(app) {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::engine::log(&format!("not talking to {app}: its control is off (listen only)"));
        }
        return None;
    }
    // an app that didn't answer in time is left alone for a while: one browser stuck (live, 20 Oct: Chrome waiting on
    // its Automation prompt for 4 minutes) mustn't stop OnKey asking Spotify or Music
    if crate::source::resting(app) {
        return None;
    }
    let sent = std::time::Instant::now();
    let out = match run_timed(script, OSA_TIMEOUT) {
        Some(o) => Ok::<_, std::io::Error>(o),
        None => {
            crate::source::rest(app);
            crate::engine::log(&format!("{app} didn't answer in {:.0} s (a permission prompt may be waiting): not asked again for {:.0} s",
                                        OSA_TIMEOUT.as_secs_f64(), crate::source::OSA_REST.as_secs_f64()));
            crate::records::apple_event(app, sent.elapsed().as_secs_f64() * 1000.0, "timeout");
            return None;
        }
    };
    crate::records::apple_event(app, sent.elapsed().as_secs_f64() * 1000.0, match &out {
        Ok(o) if o.status.success() => "ok",
        Ok(o) if is_denial(&String::from_utf8_lossy(&o.stderr)) => "denied",
        _ => "error",
    });
    let out = out.ok()?;
    let mut denied = crate::source::DENIED.lock().unwrap();
    let mut streak = STREAK.lock().unwrap();
    if out.status.success() {
        if script.contains("tell application") && denied.is_some_and(|(a, _)| a == app) {
            *denied = None;
        }
        *streak = (0, None);
    } else if is_denial(&String::from_utf8_lossy(&out.stderr)) {
        let since = *streak.1.get_or_insert_with(std::time::Instant::now);
        if really_refused(&mut streak.0, since.elapsed().as_secs_f64()) {
            if denied.is_none() {
                crate::engine::log(&format!("macOS doesn't let OnKey control {app} (Automation): refused {} times in {:.0} s",
                                            streak.0, since.elapsed().as_secs_f64()));
            }
            *denied = Some((app, std::time::Instant::now()));
        } else if streak.0 == 1 {
            crate::engine::log(&format!("{app} refused one request ({}): not taken as Automation off yet",
                                        String::from_utf8_lossy(&out.stderr).trim()));
        }
    }
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// How long an app has to answer one Apple event (they take 50-300 ms; starting Spotify a few seconds).
const OSA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// osascript running `script`, given `limit` to finish: its output, or None when it didn't (it's stopped).
fn run_timed(script: &str, limit: std::time::Duration) -> Option<std::process::Output> {
    use std::process::Stdio;
    let child = std::process::Command::new("osascript").args(["-e", script])
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let id = child.id();
    // the output is read on its own thread (a full pipe would otherwise stall the script); here, only the clock
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(child.wait_with_output()); });
    match rx.recv_timeout(limit) {
        Ok(out) => out.ok(),
        Err(_) => {
            let _ = std::process::Command::new("kill").arg(id.to_string()).status();
            None
        }
    }
}

/// One more refusal in a row (`count` of them, the first `secs` ago): is Automation really off? 3 over 5 s.
fn really_refused(count: &mut u32, secs: f64) -> bool {
    *count += 1;
    *count >= 3 && secs >= 5.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apple_event_that_takes_too_long_is_stopped() {
        let t = std::time::Instant::now();
        assert!(run_timed("delay 5", std::time::Duration::from_millis(300)).is_none());
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "waited for the slow script");
        let out = run_timed("return \"hi\"", std::time::Duration::from_secs(5)).expect("a quick script finishes");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
    }

    #[test]
    fn automation_refusals_are_told_from_other_errors() {
        assert!(is_denial("execution error: Not authorized to send Apple events to Spotify. (-1743)"));
        assert!(is_denial("execution error: Music got an error: (-1744)"));
        // one refusal (or a few at once) is a hiccup; refused again and again over 5 s, it's off
        let mut n = 0;
        assert!(!really_refused(&mut n, 0.0));
        assert!(!really_refused(&mut n, 0.4));
        assert!(!really_refused(&mut n, 0.8), "3 at once");
        assert!(really_refused(&mut n, 5.2));
        assert!(!is_denial("execution error: Spotify got an error: Can't get current track. (-1728)"));
        assert!(!is_denial(""));
    }
}
