//! Which programs are running (the Toolhelp snapshot), and which Windows program a macOS bundle id stands for
//! (platform/exes.rs).
use ::windows::Win32::Foundation::CloseHandle;
use ::windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};

/// The program names a bundle id means (platform/exes.rs).
pub use crate::platform::exes::exes_for;

/// Every running process: (process id, program name in lower case).
pub fn list() -> Vec<(u32, String)> {
    list_with_parents().into_iter().map(|(p, _, n)| (p, n)).collect()
}

/// Whether a bundle id is OnKey's own: the demo song plays in OnKey's process (src/demo.rs `bundle`), and is
/// captured from it as the Mac captures it from OnKey's bundle.
fn own(bundle: &str) -> bool {
    bundle == crate::demo::bundle()
}

/// The app's first process: one of its processes whose parent isn't one of them (Spotify runs several; its audio
/// comes from a child of this one, which process loopback of the tree includes). For OnKey itself, this process
/// (its tree takes in WebView2's processes too, so the window's own sounds would be heard with the demo).
pub fn root_pid_for(bundle: &str) -> Option<u32> {
    if own(bundle) {
        return Some(std::process::id());
    }
    let exes = exes_for(bundle);
    let all = list_with_parents();
    let mine: Vec<u32> = all.iter().filter(|(_, _, n)| exes.contains(&n.as_str())).map(|(p, _, _)| *p).collect();
    all.iter().find(|(p, parent, _)| mine.contains(p) && !mine.contains(parent)).map(|(p, _, _)| *p)
}

/// Every running process: (process id, its parent's, program name in lower case).
fn list_with_parents() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                out.push((e.th32ProcessID, e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..n]).to_lowercase()));
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// The processes of the app a bundle id stands for.
pub fn pids_for(bundle: &str) -> Vec<u32> {
    pids_in(&list(), bundle)
}

/// `pids_for` from a list already taken (`list`).
pub fn pids_in(all: &[(u32, String)], bundle: &str) -> Vec<u32> {
    if own(bundle) {
        return vec![std::process::id()];
    }
    let exes = exes_for(bundle);
    all.iter().filter(|(_, n)| exes.contains(&n.as_str())).map(|(p, _)| *p).collect()
}

pub fn running(bundle: &str) -> bool {
    !pids_for(bundle).is_empty()
}

/// A process's program name in lower case, by process id (from the snapshot).
pub fn exe_of(pid: u32) -> Option<String> {
    list().into_iter().find(|(p, _)| *p == pid).map(|(_, n)| n)
}

#[cfg(test)]
mod tests {
    #[test]
    fn onkey_itself_is_this_process() {
        let me = crate::demo::bundle();
        assert_eq!(super::root_pid_for(me), Some(std::process::id()));
        assert_eq!(super::pids_for(me), vec![std::process::id()]);
        assert!(super::running(me));
    }
}
