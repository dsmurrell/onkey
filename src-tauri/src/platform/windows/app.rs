//! The app on Windows: no menu bar (it would sit inside the window; Report a problem is in Settings), the window's
//! title bar as its buttons, a report saved straight to the Desktop. What macOS alone does (hiding the music app
//! again, the Dock, being opened again while quitting) has nothing to do here.
use std::path::PathBuf;

/// No menu bar.
pub fn menu(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
}

/// (No menu bar to put in another language.)
pub fn relabel(_app: &tauri::AppHandle) {}

/// The window's buttons are its title bar (decorations): shown or hidden.
pub fn window_buttons(window: &tauri::WebviewWindow, visible: bool) -> Result<(), String> {
    window.set_decorations(visible).map_err(|e| e.to_string())
}

/// (The app isn't hidden again yet.)
pub fn hide_if_front(_window: &tauri::WebviewWindow, _bundle: &'static str) {}

/// (No Dock.)
pub fn hide_dock(_app: &tauri::AppHandle) {}

/// (No events of its own: Windows doesn't hand a second open to a quitting copy, and onkey:// links aren't
/// registered here.)
pub fn on_event(_app: &tauri::AppHandle, _event: &tauri::RunEvent) {}

/// (Never asked: see `on_event`.)
pub fn relaunch() {}

/// Where to save a report: no dialog, straight to the Desktop (then shown selected in File Explorer: lib.rs
/// report_save), one file a time so a second report doesn't replace the first.
pub fn report_destination(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    let name = format!("OnKey-report-{}.zip", crate::store::format_time(std::time::SystemTime::now(), "%Y-%m-%d-%H%M%S"));
    Some(app.path().desktop_dir().ok()?.join(name))
}

/// A report was saved to the Desktop with no dialog: shown selected in File Explorer, ready to attach.
pub fn report_saved(path: &std::path::Path) {
    super::shell::reveal(path);
}
