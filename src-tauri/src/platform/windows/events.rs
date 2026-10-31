//! What Windows tells only a window, and the default output changing: heard by a hidden window of OnKey's own on
//! its own thread (a top-level one: a message-only window isn't sent these), started at launch (diag.rs `started`).
//! - the PC woke from sleep: counted (`wakes`, native.rs `onkey_wake_count`), so the capture restarts at once
//!   (capture.rs Watchdog) rather than after its seconds of silence;
//! - the default output changed while the capture hears the whole output (audio.rs MODE "system": per-process
//!   loopback follows the app to any output): counted the same way, as the old output's stream goes silent;
//! - Windows signing out, restarting or shutting down: logged as "told to stop" and the take playing kept (as
//!   SIGTERM does on macOS: lib.rs `on_terminate`), so the next launch doesn't say OnKey didn't close normally.
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Once;

use ::windows::core::{w, PCWSTR};
use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, PROPERTYKEY, WPARAM};
use ::windows::Win32::Media::Audio::{eConsole, eRender, EDataFlow, ERole, IMMNotificationClient, IMMNotificationClient_Impl, DEVICE_STATE};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, MSG, PBT_APMRESUMEAUTOMATIC,
    WINDOW_EX_STYLE, WM_ENDSESSION, WM_POWERBROADCAST, WNDCLASSW, WS_OVERLAPPED,
};

static WAKES: AtomicI32 = AtomicI32::new(0);
static WATCH: Once = Once::new();

/// Start listening (once; later calls do nothing).
pub fn watch() {
    WATCH.call_once(|| {
        let _ = std::thread::Builder::new().name("onkey-events".into()).spawn(run);
    });
}

/// Wakes from sleep (and default output changes while the whole output is heard) since launch.
pub fn wakes() -> i32 {
    watch();
    WAKES.load(Ordering::SeqCst)
}

fn run() {
    super::com();
    // the default output: watched for as long as this thread runs (the enumerator keeps the callback)
    let devices = super::audio::enumerator().ok();
    let client: IMMNotificationClient = Outputs.into();
    if let Some(en) = &devices {
        if let Err(e) = unsafe { en.RegisterEndpointNotificationCallback(&client) } {
            crate::engine::log(&format!("events: the default output isn't watched ({e})"));
        }
    }
    unsafe {
        let Ok(module) = GetModuleHandleW(PCWSTR::null()) else { return };
        let class = w!("OnKeyEvents");
        let wc = WNDCLASSW { lpfnWndProc: Some(window), hInstance: module.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        // (never shown)
        if let Err(e) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("OnKey"), WS_OVERLAPPED, 0, 0, 0, 0, None, None, Some(module.into()), None) {
            crate::engine::log(&format!("events: no window to hear sleep and sign-out ({e})"));
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn window(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_POWERBROADCAST if wp.0 as u32 == PBT_APMRESUMEAUTOMATIC => {
            WAKES.fetch_add(1, Ordering::SeqCst);
            crate::engine::log("events: the PC woke from sleep: the capture restarts");
            LRESULT(1)
        }
        // (wParam 0: the end was called off)
        WM_ENDSESSION if wp.0 != 0 => {
            crate::engine::log("told to stop (Windows is signing out, restarting or shutting down)");
            // (a few seconds at most: Windows waits about five before it says OnKey is holding it up)
            crate::engine::shutdown(std::time::Duration::from_secs(4));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Hears the default output change.
#[::windows::core::implement(IMMNotificationClient)]
struct Outputs;

impl IMMNotificationClient_Impl for Outputs_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> ::windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _id: &PCWSTR) -> ::windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> ::windows::core::Result<()> {
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> ::windows::core::Result<()> {
        if flow == eRender && role == eConsole && *super::audio::MODE.lock().unwrap_or_else(|e| e.into_inner()) == "system" {
            WAKES.fetch_add(1, Ordering::SeqCst);
            crate::engine::log("events: the default output changed while the whole output is heard: the capture restarts");
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> ::windows::core::Result<()> {
        Ok(())
    }
}
