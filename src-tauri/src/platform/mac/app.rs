//! The app on macOS: the menu bar (Help ▸ Report a Problem…), the window's own buttons, the music app hidden
//! again when it comes to the front, the Dock icon while quitting, the events only macOS sends (opened again,
//! onkey:// links), starting a fresh copy, and the save panel for a report.
use std::path::PathBuf;

/// The standard menus (as Tauri's `Menu::default`), with "Report a Problem…" in Help (the window opens it:
/// components/ReportProblem.tsx), "Pause Listening" (⌘⇧P) and OnKey's own Quit (⌘Q) in the app's menu, every item
/// in OnKey's language (messages.rs: the standard items too, which Tauri would leave in English).
pub fn menu(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        .menu(|handle| build(handle))
        .on_menu_event(|app, ev| {
            if ev.id() == "report" {
                use tauri::Emitter;
                let _ = app.emit("onkey-menu", "report");
            }
            if ev.id() == "listening" {
                let _ = crate::set_listening_paused(!crate::settings::listening_paused());
            }
            if ev.id() == "quit" && !crate::engine::quitting() {
                crate::quit_out_of_sight(app);
            }
        })
}

/// The menu bar again in the language now chosen (Settings' language changed).
pub fn relabel(app: &tauri::AppHandle) {
    match build(app) {
        Ok(m) => {
            let _ = app.set_menu(m);
        }
        Err(e) => crate::engine::log(&format!("menu: couldn't be rebuilt: {e}")),
    }
}

fn build(handle: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as P, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID};
    let lang = crate::messages::current();
    let t = |key: &str| crate::messages::in_lang(&lang, key);
    let pkg = handle.package_info();
    let config = handle.config();
    let about = AboutMetadata {
        name: Some(pkg.name.clone()),
        version: Some(pkg.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };
    // "Pause Listening" (⌘⇧P) under About: the shortcut works from every page (named as the setting is)
    let listening = MenuItem::with_id(handle, "listening", t(crate::listening_menu_key()), true, Some("CmdOrCtrl+Shift+P"))?;
    *crate::LISTENING_ITEM.lock().unwrap() = Some(listening.clone());
    let app_menu = Submenu::with_items(handle, pkg.name.clone(), true, &[
        &P::about(handle, Some(&t("menu.app.about")), Some(about))?,
        &P::separator(handle)?,
        &listening,
        &P::separator(handle)?,
        &P::services(handle, Some(&t("menu.app.services")))?,
        &P::separator(handle)?,
        &P::hide(handle, Some(&t("menu.app.hide")))?,
        &P::hide_others(handle, Some(&t("menu.app.hideOthers")))?,
        &P::separator(handle)?,
        // Quit (⌘Q) is OnKey's own: the standard one went straight to the app's exit, which finished the song being
        // learned with the window still up and frozen (a spinning beachball for up to 25 s)
        &MenuItem::with_id(handle, "quit", t("menu.app.quit"), true, Some("CmdOrCtrl+Q"))?,
    ])?;
    let file = Submenu::with_items(handle, t("menu.file.title"), true, &[&P::close_window(handle, Some(&t("menu.file.closeWindow")))?])?;
    let edit = Submenu::with_items(handle, t("menu.edit.title"), true, &[
        &P::undo(handle, Some(&t("menu.edit.undo")))?,
        &P::redo(handle, Some(&t("menu.edit.redo")))?,
        &P::separator(handle)?,
        &P::cut(handle, Some(&t("menu.edit.cut")))?,
        &P::copy(handle, Some(&t("menu.edit.copy")))?,
        &P::paste(handle, Some(&t("menu.edit.paste")))?,
        &P::select_all(handle, Some(&t("menu.edit.selectAll")))?,
    ])?;
    let view = Submenu::with_items(handle, t("menu.view.title"), true, &[&P::fullscreen(handle, Some(&t("menu.view.fullScreen")))?])?;
    let window = Submenu::with_id_and_items(handle, WINDOW_SUBMENU_ID, t("menu.window.title"), true, &[
        &P::minimize(handle, Some(&t("menu.window.minimize")))?,
        &P::maximize(handle, Some(&t("menu.window.zoom")))?,
        &P::separator(handle)?,
        &P::close_window(handle, Some(&t("menu.window.closeWindow")))?,
    ])?;
    let help = Submenu::with_id_and_items(handle, HELP_SUBMENU_ID, t("menu.help.title"), true, &[
        &MenuItem::with_id(handle, "report", t("menu.help.reportProblem"), true, None::<&str>)?,
    ])?;
    Menu::with_items(handle, &[&app_menu, &file, &edit, &view, &window, &help])
}

/// The window's close, minimise and zoom buttons shown or hidden (the title bar itself stays see-through).
pub fn window_buttons(window: &tauri::WebviewWindow, visible: bool) -> Result<(), String> {
    let ns = window.ns_window().map_err(|e| e.to_string())? as usize;
    window.run_on_main_thread(move || {
        use objc2_app_kit::{NSWindow, NSWindowButton};
        let ns = unsafe { &*(ns as *const NSWindow) };
        for b in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
            if let Some(button) = ns.standardWindowButton(b) { button.setHidden(!visible); }
        }
    }).map_err(|e| e.to_string())
}

/// If the app with this bundle id is in front, hide it (it keeps playing) and give OnKey's window the focus.
pub fn hide_if_front(window: &tauri::WebviewWindow, bundle: &'static str) {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let front = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication();
        if let Some(a) = front.filter(|a| a.bundleIdentifier().is_some_and(|b| b.to_string() == bundle)) {
            a.hide();
            let _ = w.set_focus();
        }
    });
}

/// No Dock icon (quitting out of sight).
pub fn hide_dock(app: &tauri::AppHandle) {
    let _ = app.set_dock_visibility(false);
}

/// The events only macOS sends: opened again while quitting out of sight (macOS hands the open to this quitting
/// copy, so it starts a fresh one as it exits, else nothing would appear), and an onkey:// link (links.rs).
pub fn on_event(app: &tauri::AppHandle, event: &tauri::RunEvent) {
    match event {
        tauri::RunEvent::Reopen { .. } if crate::engine::quitting() => {
            crate::RELAUNCH.store(true, std::sync::atomic::Ordering::SeqCst);
            crate::engine::log("quitting: opened again meanwhile, so it starts afresh when done");
        }
        tauri::RunEvent::Opened { urls } => {
            for u in urls {
                crate::links::opened(app, u.as_str());
            }
        }
        _ => {}
    }
}

/// Open a new copy of the app, a second after this one has gone.
pub fn relaunch() {
    // .../OnKey.app/Contents/MacOS/onkey -> .../OnKey.app
    if let Some(bundle) = std::env::current_exe().ok().and_then(|e| e.ancestors().nth(3).map(|p| p.to_path_buf())) {
        let _ = std::process::Command::new("/bin/sh").arg("-c").arg("sleep 1; /usr/bin/open -n \"$0\"").arg(&bundle).spawn();
    }
}

/// Where to save a report: macOS's save panel (on the main thread), starting at ~/Desktop/OnKey-report-<date>.zip.
/// None: cancelled.
pub fn report_destination(app: &tauri::AppHandle) -> Option<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel();
    let name = format!("OnKey-report-{}.zip", crate::store::format_time(std::time::SystemTime::now(), "%Y-%m-%d"));
    let desktop = std::env::var("HOME").map(|h| format!("{h}/Desktop")).unwrap_or_default();
    let message = crate::messages::t("report.savePanelMessage");
    let _ = app.run_on_main_thread(move || {
        use objc2_app_kit::{NSModalResponseOK, NSSavePanel};
        use objc2_foundation::{MainThreadMarker, NSString, NSURL};
        let Some(mtm) = MainThreadMarker::new() else { let _ = tx.send(None); return };
        let panel = NSSavePanel::savePanel(mtm);
        panel.setNameFieldStringValue(&NSString::from_str(&name));
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(&desktop))));
        panel.setCanCreateDirectories(true);
        panel.setMessage(Some(&NSString::from_str(&message)));
        let chosen = (panel.runModal() == NSModalResponseOK).then(|| panel.URL()).flatten().and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()));
        let _ = tx.send(chosen);
    });
    rx.recv().ok().flatten()
}

/// A report was saved where the save panel said: nothing more to show.
pub fn report_saved(_path: &std::path::Path) {}
