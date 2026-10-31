//! onkey:// links (the take page's "Copy take link"): `onkey://take/<take id>?t=<s into the take>` opens OnKey on
//! that take at that moment. Any page or app can open a link, so only a take id (letters, digits, `-`, `_`) and a
//! time are taken from it; nothing else in it is acted on.

use std::sync::Mutex;

/// The page a link opens ("/take?id=…&t=…"), or None for anything else.
pub fn page(url: &str) -> Option<String> {
    let rest = url.strip_prefix("onkey://take/")?;
    let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
    let id = id.trim_end_matches('/');
    if id.is_empty() || id.len() > 200 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let t = query.split('&').find_map(|kv| kv.strip_prefix("t=")).and_then(|v| v.parse::<f64>().ok())
        .filter(|t| t.is_finite() && *t >= 0.0);
    Some(match t {
        Some(t) => format!("/take?id={id}&t={t:.1}"),
        None => format!("/take?id={id}"),
    })
}

/// The page the last link asked for, until the window takes it (a link can open OnKey before its window has loaded).
static PENDING: Mutex<Option<String>> = Mutex::new(None);

/// A link opened: kept for the window, which is told (and brought forward).
pub fn opened(app: &tauri::AppHandle, url: &str) {
    use tauri::{Emitter, Manager};
    let Some(p) = page(url) else {
        crate::engine::log(&format!("link: not one OnKey opens ({})", url.chars().take(80).collect::<String>()));
        return;
    };
    crate::engine::log(&format!("link: opening {p}"));
    *PENDING.lock().unwrap() = Some(p);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("onkey-link", ());
}

/// The page a link asked for, once (the window calls this when it loads and when told of a link).
#[tauri::command]
pub fn link_take() -> Option<String> {
    PENDING.lock().unwrap().take()
}

#[cfg(test)]
mod tests {
    use super::page;

    #[test]
    fn a_take_link_opens_that_take_at_that_moment() {
        assert_eq!(page("onkey://take/20261020-091442-Red-Hot-Chili-Peppers-Scar-Tissue?t=15.0&song=15.4").as_deref(),
                   Some("/take?id=20261020-091442-Red-Hot-Chili-Peppers-Scar-Tissue&t=15.0"));
        assert_eq!(page("onkey://take/20261014-163511-alt-J-Matilda").as_deref(), Some("/take?id=20261014-163511-alt-J-Matilda"));
        assert_eq!(page("onkey://take/abc?song=3&t=nope").as_deref(), Some("/take?id=abc"));
    }

    #[test]
    fn anything_else_in_a_link_is_refused() {
        for bad in ["onkey://take/", "onkey://take/../settings", "onkey://take/a%2Fb", "onkey://take/a b", "onkey://live/3?song=1",
                    "https://take/abc", "onkey://take/abc&x=<script>"] {
            assert_eq!(page(bad), None, "{bad}");
        }
        assert_eq!(page("onkey://take/abc?t=-5").as_deref(), Some("/take?id=abc"), "no time before the take");
    }
}
