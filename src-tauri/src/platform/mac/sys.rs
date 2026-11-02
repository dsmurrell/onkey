//! What the engine asks of macOS about the Mac and this process: memory, CPU time, the model and version
//! (sysctl), the region, Low Power Mode and the battery, threads' priority (QoS), the user's full name.

/// The Mac's physical memory in bytes (sysctl hw.memsize), if it can be read.
pub fn physical_memory() -> Option<u64> {
    let mut size: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let r = unsafe {
        libc::sysctlbyname(c"hw.memsize".as_ptr(), &mut size as *mut u64 as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0)
    };
    (r == 0 && len == std::mem::size_of::<u64>()).then_some(size)
}

/// CPU time this process has used (user + system), seconds.
pub fn cpu_seconds() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
        return 0.0;
    }
    let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    tv(ru.ru_utime) + tv(ru.ru_stime)
}

/// OnKey's memory as Activity Monitor shows it (the physical footprint), MB.
pub fn footprint_mb() -> f64 {
    let mut ri: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::proc_pid_rusage(libc::getpid(), libc::RUSAGE_INFO_V2, &mut ri as *mut _ as *mut libc::rusage_info_t) };
    if r == 0 { ri.ri_phys_footprint as f64 / 1e6 } else { 0.0 }
}

/// A fact about the Mac by its sysctl name ("hw.model", "machdep.cpu.brand_string", "kern.osproductversion").
pub fn machine(name: &str) -> Option<String> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut len = 0usize;
    unsafe {
        if libc::sysctlbyname(c.as_ptr(), std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0) != 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if libc::sysctlbyname(c.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) != 0 {
            return None;
        }
        buf.truncate(len);
        Some(String::from_utf8_lossy(&buf).trim_end_matches('\0').trim().to_string()).filter(|s| !s.is_empty())
    }
}

/// How the OS is named before its version in the log's first line ("macOS 15.5").
pub const OS_NAME: &str = "macOS ";

/// The Mac's region (System Settings → General → Language & Region), as Apple Music's store code ("gb"). Read on
/// the Mac only: nothing about it is sent anywhere.
pub fn region() -> Option<String> {
    let cc = objc2_foundation::NSLocale::currentLocale().countryCode()?.to_string().to_ascii_lowercase();
    (cc.len() == 2).then_some(cc)
}

/// The Mac's preferred languages, most preferred first (System Settings → General → Language & Region:
/// "en-GB", "zh-Hans-CN"…), for OnKey's own words (messages.rs) and the window's (lib.rs `ui_locale`).
pub fn preferred_languages() -> Vec<String> {
    objc2_foundation::NSLocale::preferredLanguages().iter().map(|l| l.to_string()).collect()
}

/// macOS Low Power Mode is on.
pub fn low_power() -> bool {
    unsafe {
        use objc2::runtime::AnyObject;
        let info: *mut AnyObject = objc2::msg_send![objc2::class!(NSProcessInfo), processInfo];
        if info.is_null() {
            return false;
        }
        objc2::msg_send![info, isLowPowerModeEnabled]
    }
}

/// The Mac is running on its battery (IOKit's power source; a desktop Mac never is).
pub fn on_battery() -> bool {
    unsafe {
        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOPSCopyPowerSourcesInfo() -> *const std::ffi::c_void;
            fn IOPSGetProvidingPowerSourceType(blob: *const std::ffi::c_void) -> *const std::ffi::c_void;
        }
        extern "C" {
            fn CFStringGetCString(s: *const std::ffi::c_void, buf: *mut std::ffi::c_char, size: isize, encoding: u32) -> u8;
            fn CFRelease(cf: *const std::ffi::c_void);
        }
        let info = IOPSCopyPowerSourcesInfo();
        if info.is_null() {
            return false;
        }
        let kind = IOPSGetProvidingPowerSourceType(info);       // (not owned: no release)
        let mut buf = [0 as std::ffi::c_char; 64];
        let ok = !kind.is_null() && CFStringGetCString(kind, buf.as_mut_ptr(), 64, 0x0800_0100) != 0;   // UTF-8
        CFRelease(info);
        ok && std::ffi::CStr::from_ptr(buf.as_ptr()).to_bytes() == b"Battery Power"
    }
}

/// This thread does background work (converting, measuring): QoS background (the efficiency cores, throttled I/O).
pub fn background_thread() {
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0);
    }
}

/// This thread learns: QoS utility, after the window, the audio and the live pitch.
pub fn utility_thread() {
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
}

/// The user's names a report mustn't carry, besides the home folder's and $USER: their full name (the account's).
pub fn user_names() -> Vec<String> {
    let full = unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_gecos.is_null() {
            return Vec::new();
        }
        let g = std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy().into_owned();
        g.split(',').next().unwrap_or("").trim().to_string()
    };
    [full].into_iter().filter(|n| !n.is_empty()).collect()
}

/// Is the app with this bundle id running? Asked of macOS directly (NSRunningApplication): no osascript process and
/// no Apple event (each AppleScript "is running" was a process launched, several times a second while following).
pub fn app_running(bundle: &str) -> bool {
    objc2::rc::autoreleasepool(|_| {
        let id = objc2_foundation::NSString::from_str(bundle);
        objc2_app_kit::NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).count() > 0
    })
}

/// The UI font's faces (path, face index, weight) and its fallbacks (path, face index).
pub type UiFonts = (Vec<(std::path::PathBuf, u32, u16)>, Vec<(std::path::PathBuf, u32)>);

/// The OS's own UI font (path, face index, weight) and its fallbacks (path, face index), for drawing text.
pub fn ui_fonts() -> UiFonts {
    let f = |n: &str| std::path::PathBuf::from("/System/Library/Fonts").join(n);
    (vec![(f("SFNS.ttf"), 0, 400)], vec![(f("Hiragino Sans GB.ttc"), 0), (f("AppleSDGothicNeo.ttc"), 0), (f("Apple Symbols.ttf"), 0)])
}
