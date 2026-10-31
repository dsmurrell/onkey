//! What macOS's `open` and `pbcopy` do for the engine, on Linux: open a link or file with the desktop's default
//! app (xdg-open), show a file in the file manager (its D-Bus ShowItems, else its folder opened), put text on the
//! clipboard (GTK's own, the window's toolkit; else wl-copy, xclip or xsel), the home and Downloads folders, and the
//! Trash (~/.local/share/Trash, as the desktop's own file manager uses it).
//!
//! Every program started here gets the environment OnKey was started with, not the AppImage's: its GTK, GIO and
//! library paths point at the libraries inside the AppImage, which other programs (the sound settings, a browser
//! started by a link) can't use (`command`).
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A program to run, without the AppImage's own GTK, GIO and library paths ($APPDIR's): variables that point only
/// inside it are dropped, lists (PATH, XDG_DATA_DIRS, LD_LIBRARY_PATH…) lose their entries inside it.
pub fn command(program: &str) -> Command {
    let mut c = Command::new(program);
    let Some(appdir) = std::env::var("APPDIR").ok().filter(|d| d.len() > 1) else { return c };
    for (k, v) in std::env::vars() {
        if k == "APPDIR" || k == "APPIMAGE" || k == "ARGV0" || !v.contains(&appdir) {
            continue;
        }
        let kept: Vec<&str> = v.split(':').filter(|p| !p.is_empty() && !p.contains(&appdir)).collect();
        if kept.is_empty() || !v.contains(':') {
            c.env_remove(&k);
        } else {
            c.env(&k, kept.join(":"));
        }
    }
    // (the AppImage's runtime keeps the original library path here when it changes it)
    if let Ok(orig) = std::env::var("APPIMAGE_ORIGINAL_LD_LIBRARY_PATH") {
        if orig.is_empty() { c.env_remove("LD_LIBRARY_PATH"); } else { c.env("LD_LIBRARY_PATH", orig); }
    }
    c
}

/// Open a URL (https:, spotify:…) or a file or folder with what the desktop opens it with. True if xdg-open took it.
/// Never waits long: on a desktop with no opener of its own xdg-open runs the app in the foreground until it's
/// closed, so after a second it's taken as done and left running (waited for on a thread of its own).
pub fn open(target: impl AsRef<std::ffi::OsStr>) -> bool {
    let Ok(mut child) = command("xdg-open").arg(target).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return false;
    };
    let until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < until {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return false,
        }
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    true
}

/// A path as a file:// address, each part percent-encoded (a Desktop folder named "Área de Trabalho" included).
fn file_uri(path: &Path) -> String {
    let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let enc = full.to_string_lossy().split('/').map(|s| urlencoding::encode(s).into_owned()).collect::<Vec<_>>().join("/");
    format!("file://{enc}")
}

/// Does an app on this computer open `scheme:` links ("spotify")? (xdg-mime's default handler for the scheme.)
pub fn handles(scheme: &str) -> bool {
    command("xdg-mime").args(["query", "default", &format!("x-scheme-handler/{scheme}")]).stderr(Stdio::null()).output()
        .is_ok_and(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
}

/// Open an app's own link (spotify:track:…) if an app takes its kind; false (nothing shown) if none does.
pub fn open_app_link(uri: &str) -> bool {
    uri.split_once(':').is_some_and(|(scheme, _)| handles(scheme)) && open(uri)
}

/// A song's link: an app's link (spotify:…) in its app when one takes it, else its web page (trackids.rs);
/// anything else as is.
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

/// A file shown selected in the file manager (org.freedesktop.FileManager1, which Files, Dolphin and others
/// answer), else its folder opened.
pub fn reveal(path: &Path) {
    let uri = file_uri(path);
    let shown = command("gdbus").args(["call", "--session", "--dest", "org.freedesktop.FileManager1", "--object-path",
                                       "/org/freedesktop/FileManager1", "--method", "org.freedesktop.FileManager1.ShowItems",
                                       &format!("['{}']", uri.replace('\'', "%27")), ""])
        .stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    if !shown {
        if let Some(dir) = path.parent() {
            open(dir);
        }
    }
}

/// Text piped to a clipboard program; true if it took it.
fn pipe_to(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut c) = command(program).args(args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return false };
    let ok = c.stdin.take().is_some_and(|mut i| i.write_all(text.as_bytes()).is_ok());
    ok && c.wait().is_ok_and(|s| s.success())
}

/// Text on the clipboard: through GTK, the window's own toolkit (on its main thread; nothing to install), once the
/// window is up; before that (and in tests) wl-copy on Wayland, else xclip or xsel. False when none could.
pub fn copy(text: &str) -> bool {
    if gtk::is_initialized() {
        let text = text.to_string();
        glib::MainContext::default().invoke(move || {
            gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&text);
        });
        return true;
    }
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    (wayland && pipe_to("wl-copy", &[], text))
        || pipe_to("xclip", &["-selection", "clipboard"], text)
        || pipe_to("xsel", &["--clipboard", "--input"], text)
}

/// The home folder ("" if unknown).
pub fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// The Downloads folder: XDG_DOWNLOAD_DIR from ~/.config/user-dirs.dirs (it may be translated, "Téléchargements"),
/// else ~/Downloads.
pub fn downloads() -> PathBuf {
    let home = home();
    let config = std::env::var("XDG_CONFIG_HOME").ok().filter(|c| !c.is_empty()).unwrap_or_else(|| format!("{home}/.config"));
    if let Ok(dirs) = std::fs::read_to_string(Path::new(&config).join("user-dirs.dirs")) {
        if let Some(d) = user_dir(&dirs, "XDG_DOWNLOAD_DIR", &home) {
            return d;
        }
    }
    Path::new(&home).join("Downloads")
}

/// A folder from user-dirs.dirs (`XDG_DOWNLOAD_DIR="$HOME/Downloads"`).
fn user_dir(dirs: &str, key: &str, home: &str) -> Option<PathBuf> {
    let line = dirs.lines().map(str::trim).find(|l| l.starts_with(key) && l[key.len()..].trim_start().starts_with('='))?;
    let v = line.split_once('=')?.1.trim().trim_matches('"').replace("$HOME", home);
    (v.starts_with('/') && v != home && v != format!("{home}/")).then(|| PathBuf::from(v))
}

/// Move a file or folder to the Trash (~/.local/share/Trash/files, with the .trashinfo the desktop's file manager
/// restores it by); an error, and nothing moved, if it can't (the Trash on another drive, say).
pub fn trash(p: &Path) -> Result<(), String> {
    if !p.exists() {
        return Ok(());
    }
    let data = std::env::var("XDG_DATA_HOME").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| format!("{}/.local/share", home()));
    if data.starts_with("/.") {
        return Err("no home folder".into());
    }
    let trash = Path::new(&data).join("Trash");
    let (files, info) = (trash.join("files"), trash.join("info"));
    std::fs::create_dir_all(&files).and_then(|_| std::fs::create_dir_all(&info)).map_err(|e| e.to_string())?;
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "item".into());
    let stamp = crate::store::now_iso().replace(':', ".");
    let base = format!("OnKey {stamp} {name}");
    let path = file_uri(p).trim_start_matches("file://").to_string();
    let when = crate::store::format_time(std::time::SystemTime::now(), "%Y-%m-%dT%H:%M:%S");
    std::fs::write(info.join(format!("{base}.trashinfo")), format!("[Trash Info]\nPath={path}\nDeletionDate={when}\n")).map_err(|e| e.to_string())?;
    std::fs::rename(p, files.join(&base)).map_err(|e| {
        let _ = std::fs::remove_file(info.join(format!("{base}.trashinfo")));
        format!("couldn't move it to the Trash: {e}")
    })
}

/// The settings for "mic" (the sound settings, where the input is chosen); the others (system audio, automation)
/// need nothing on Linux. GNOME's, KDE's, else pavucontrol.
pub fn open_settings(which: &str) {
    if which != "mic" {
        return;
    }
    for (prog, args) in [("gnome-control-center", &["sound"][..]), ("systemsettings", &["kcm_pulseaudio"][..]), ("pavucontrol", &["--tab=4"][..])] {
        if command(prog).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_downloads_folder_is_read_from_user_dirs() {
        let dirs = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n";
        assert_eq!(super::user_dir(dirs, "XDG_DOWNLOAD_DIR", "/home/a"), Some("/home/a/Téléchargements".into()));
        assert_eq!(super::user_dir("XDG_DOWNLOAD_DIR=\"$HOME/\"", "XDG_DOWNLOAD_DIR", "/home/a"), None, "the home folder itself isn't it");
        assert_eq!(super::user_dir("", "XDG_DOWNLOAD_DIR", "/home/a"), None);
        assert!(super::downloads().is_absolute() || super::home().is_empty());
    }

    #[test]
    fn a_path_becomes_a_percent_encoded_file_address() {
        assert_eq!(super::file_uri(std::path::Path::new("/nowhere/Área de Trabalho/OnKey-report.zip")),
                   "file:///nowhere/%C3%81rea%20de%20Trabalho/OnKey-report.zip");
    }

    #[test]
    fn programs_started_lose_the_appimages_own_paths() {
        std::env::set_var("APPDIR", "/tmp/.mount_OnKeyX");
        std::env::set_var("ONKEY_TEST_GIO", "/tmp/.mount_OnKeyX/usr/lib/gio/modules");
        std::env::set_var("ONKEY_TEST_LIST", "/tmp/.mount_OnKeyX/usr/share:/usr/share");
        let c = super::command("true");
        std::env::remove_var("APPDIR");
        let envs: Vec<_> = c.get_envs().map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string()))).collect();
        assert!(envs.contains(&("ONKEY_TEST_GIO".into(), None)), "{envs:?}");
        assert!(envs.contains(&("ONKEY_TEST_LIST".into(), Some("/usr/share".into()))), "{envs:?}");
    }

    #[test]
    fn a_file_goes_to_the_trash_with_its_info() {
        let dir = std::env::temp_dir().join(format!("onkey-trash-{}", std::process::id()));
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let f = dir.join("take.m4a");
        std::fs::write(&f, b"x").unwrap();
        std::env::set_var("XDG_DATA_HOME", &data);
        assert_eq!(super::trash(&f), Ok(()));
        std::env::remove_var("XDG_DATA_HOME");
        assert!(!f.exists(), "moved, not left");
        let moved: Vec<_> = std::fs::read_dir(data.join("Trash/files")).unwrap().flatten().collect();
        assert_eq!(moved.len(), 1);
        let info = std::fs::read_to_string(data.join("Trash/info").join(format!("{}.trashinfo", moved[0].file_name().to_string_lossy()))).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/") && info.contains("take.m4a") && info.contains("DeletionDate="), "{info}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
