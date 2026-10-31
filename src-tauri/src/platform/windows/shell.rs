//! What macOS's `open` and `pbcopy` do for the engine, on Windows: open a link or folder with its default app
//! (ShellExecuteW), show a file selected in Explorer, put text on the clipboard.
use ::windows::core::PCWSTR;
use ::windows::Win32::UI::Shell::ShellExecuteW;
use ::windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(Some(0)).collect()
}

/// Open a URL (https:, spotify:, ms-settings:…) or a file or folder with what Windows opens it with. True if it did.
pub fn open(target: impl AsRef<std::ffi::OsStr>) -> bool {
    super::com();
    let (verb, what) = (wide("open".as_ref()), wide(target.as_ref()));
    let r = unsafe { ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(what.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    r.0 as isize > 32   // (ShellExecute's "success": a value over 32)
}

/// Does an app on this PC open `scheme:` links ("spotify", "music")? Asked first, as opening a link no app takes
/// shows Windows' "get an app for this link" box. ASSOCF_IS_PROTOCOL also finds Store apps' links.
pub fn handles(scheme: &str) -> bool {
    use ::windows::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_IS_PROTOCOL, ASSOCSTR_COMMAND, ASSOCSTR_FRIENDLYAPPNAME};
    let s = wide(scheme.as_ref());
    [ASSOCSTR_FRIENDLYAPPNAME, ASSOCSTR_COMMAND].into_iter().any(|what| {
        let mut n = 0u32;
        let r = unsafe { AssocQueryStringW(ASSOCF_IS_PROTOCOL, what, PCWSTR(s.as_ptr()), PCWSTR::null(), None, &mut n) };
        r.is_ok() && n > 1
    })
}

/// Open an app's own link (spotify:track:…, music://…) if an app takes its kind; false (nothing shown) if none does.
pub fn open_app_link(uri: &str) -> bool {
    uri.split_once(':').is_some_and(|(scheme, _)| handles(scheme)) && open(uri)
}

/// A file shown selected in an Explorer window. The switch as Explorer expects it, `/select,"C:\…"`: given as one
/// argument Rust would quote it whole (`"/select,C:\Users\Dan Smith\…"`), which can open Documents instead.
pub fn reveal(path: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    let mut arg = std::ffi::OsString::from("/select,\"");
    arg.push(path.as_os_str());
    arg.push("\"");
    let _ = std::process::Command::new("explorer.exe").raw_arg(arg).status();
}

/// Text on the clipboard (as Unicode text).
pub fn copy(text: &str) -> bool {
    use ::windows::Win32::Foundation::{GlobalFree, E_FAIL, HANDLE, HGLOBAL};
    use ::windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use ::windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    const CF_UNICODETEXT: u32 = 13;
    let w: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let ok = (|| -> ::windows::core::Result<()> {
            EmptyClipboard()?;
            let h: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2)?;
            let p = GlobalLock(h) as *mut u16;
            if p.is_null() {
                let _ = GlobalFree(Some(h));
                return Err(::windows::core::Error::from(E_FAIL));
            }
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
            let _ = GlobalUnlock(h);
            // (the clipboard owns the memory once this succeeds)
            if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0))).is_err() {
                let _ = GlobalFree(Some(h));
                return Err(::windows::core::Error::from(E_FAIL));
            }
            Ok(())
        })().is_ok();
        let _ = CloseClipboard();
        ok
    }
}

/// Windows' Settings page for "mic" (microphone privacy); the others (system audio, automation) have none.
pub fn open_settings(which: &str) {
    if which == "mic" {
        open("ms-settings:privacy-microphone");
    }
}

/// A song's link: an app's link (spotify:…, music://…) in its app when one takes it, else its web page
/// (trackids.rs); anything else as is.
pub fn open_song_link(url: &str) {
    match crate::trackids::web_link(url) {
        Some(web) => {
            if !open_app_link(url) {
                open(web);
            }
        }
        None => {
            open(url);
        }
    }
}

/// The home folder: the profile folder (C:\Users\<name>), "" if unknown.
pub fn home() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    std::env::var("USERPROFILE").unwrap_or(home)
}

/// The Downloads folder: where Windows says it is (people move it, to another drive say), else <profile>\Downloads.
pub fn downloads() -> std::path::PathBuf {
    use ::windows::Win32::System::Com::CoTaskMemFree;
    use ::windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
            let path = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            if let Some(path) = path.filter(|s| !s.is_empty()) {
                return path.into();
            }
        }
    }
    std::path::Path::new(&home()).join("Downloads")
}

/// Move a file or folder to the Recycle Bin (it can be restored from there, as from the Mac's Trash), with no
/// dialog; an error, and nothing moved, if it can't (a drive with no Recycle Bin, say: then it isn't deleted either).
pub fn trash(p: &std::path::Path) -> Result<(), String> {
    use ::windows::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
                                      FOF_WANTNUKEWARNING, FO_DELETE, SHFILEOPSTRUCTW};
    // a list of paths, each NUL-ended, the list ended by a second NUL
    let mut from = wide(p.as_os_str());
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        // FOF_WANTNUKEWARNING with no confirmation: where there's no Recycle Bin it fails rather than deleting
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT | FOF_WANTNUKEWARNING).0 as u16,
        ..Default::default()
    };
    let r = unsafe { SHFileOperationW(&mut op) };
    if r != 0 || op.fAnyOperationsAborted.as_bool() || p.exists() {
        return Err(format!("the Recycle Bin refused it (code {r})"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_file_goes_to_the_recycle_bin_and_downloads_is_a_folder() {
        let dir = std::env::temp_dir().join(format!("onkey-trash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("take.m4a");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(super::trash(&f), Ok(()));
        assert!(!f.exists(), "moved, not left");
        assert_eq!(super::trash(&dir), Ok(()), "a folder too");
        assert!(!dir.exists());
        let d = super::downloads();
        assert!(d.is_absolute() && d.to_string_lossy().to_lowercase().contains("downloads"), "{}", d.display());
    }
}
