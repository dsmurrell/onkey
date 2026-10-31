//! The app on macOS: the menu bar (Help ▸ Report a Problem…), the window's own buttons, the music app hidden
//! again when it comes to the front, the Dock icon while quitting, the events only macOS sends (opened again,
//! onkey:// links), starting a fresh copy, and the save panel for a report.
use std::path::PathBuf;

/// The standard menus, with "Report a Problem…" in Help (the window opens it: components/ReportProblem.tsx),
/// "Pause Listening" (⌘⇧P) and OnKey's own Quit (⌘Q) in the app's menu.
pub fn menu(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        .menu(|handle| {
            let menu = tauri::menu::Menu::default(handle)?;
            if let Some(help) = menu.get(tauri::menu::HELP_SUBMENU_ID).and_then(|h| h.as_submenu().cloned()) {
                help.append(&tauri::menu::MenuItem::with_id(handle, "report", "Report a Problem…", true, None::<&str>)?)?;
            }
            // "Pause Listening" (⌘⇧P) under About: the shortcut works from every page (named as the setting is
            // once it's read, in setup)
            if let Some(app_menu) = menu.items()?.first().and_then(|m| m.as_submenu().cloned()) {
                let item = tauri::menu::MenuItem::with_id(handle, "listening", "Pause Listening", true, Some("CmdOrCtrl+Shift+P"))?;
                let at = app_menu.items()?.len().min(2);
                app_menu.insert_items(&[&item, &tauri::menu::PredefinedMenuItem::separator(handle)?], at)?;
                let _ = crate::LISTENING_ITEM.set(item);
                // Quit (⌘Q) is OnKey's own: the standard one went straight to the app's exit, which finished the
                // song being learned with the window still up and frozen (a spinning beachball for up to 25 s)
                for it in app_menu.items()? {
                    if let tauri::menu::MenuItemKind::Predefined(p) = &it {
                        if p.text().is_ok_and(|t| t.starts_with("Quit")) {
                            app_menu.remove(p)?;
                        }
                    }
                }
                app_menu.append(&tauri::menu::MenuItem::with_id(handle, "quit", "Quit OnKey", true, Some("CmdOrCtrl+Q"))?)?;
            }
            Ok(menu)
        })
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
    let _ = app.run_on_main_thread(move || {
        use objc2_app_kit::{NSModalResponseOK, NSSavePanel};
        use objc2_foundation::{MainThreadMarker, NSString, NSURL};
        let Some(mtm) = MainThreadMarker::new() else { let _ = tx.send(None); return };
        let panel = NSSavePanel::savePanel(mtm);
        panel.setNameFieldStringValue(&NSString::from_str(&name));
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(&desktop))));
        panel.setCanCreateDirectories(true);
        panel.setMessage(Some(&NSString::from_str("OnKey's records for a bug report: logs and numbers, no lyrics or audio.")));
        let chosen = (panel.runModal() == NSModalResponseOK).then(|| panel.URL()).flatten().and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()));
        let _ = tx.send(chosen);
    });
    rx.recv().ok().flatten()
}

/// A report was saved where the save panel said: nothing more to show.
pub fn report_saved(_path: &std::path::Path) {}
