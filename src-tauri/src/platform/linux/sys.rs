//! What the engine asks of Linux about the computer and this process: memory (/proc/meminfo, /proc/self/status),
//! CPU time (getrusage), the model and version (/sys/class/dmi, /proc/cpuinfo, /etc/os-release), the region (LANG),
//! the battery (/sys/class/power_supply), threads' priority (nice), the user's full name (passwd), which programs
//! run (/proc: procs.rs) and the UI fonts (fontconfig's usual places).
use std::path::PathBuf;

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// A "Key:   123 kB" line's number, in bytes.
fn kb_line(text: &str, key: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with(key) && l[key.len()..].starts_with(':'))?;
    line[key.len() + 1..].split_whitespace().next()?.parse::<u64>().ok().map(|k| k * 1024)
}

/// The computer's physical memory in bytes.
pub fn physical_memory() -> Option<u64> {
    kb_line(&std::fs::read_to_string("/proc/meminfo").ok()?, "MemTotal")
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

/// This process's memory in RAM (its resident set, what System Monitor shows), MB.
pub fn footprint_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| kb_line(&s, "VmRSS")).map_or(0.0, |b| b as f64 / 1e6)
}

/// A value from /etc/os-release ("PRETTY_NAME"), unquoted.
fn os_release(key: &str) -> Option<String> {
    let text = read("/etc/os-release").or_else(|| read("/usr/lib/os-release"))?;
    os_value(&text, key)
}

fn os_value(text: &str, key: &str) -> Option<String> {
    let line = text.lines().find(|l| l.starts_with(key) && l[key.len()..].starts_with('='))?;
    Some(line[key.len() + 1..].trim().trim_matches('"').trim_matches('\'').to_string()).filter(|s| !s.is_empty())
}

/// What macOS's sysctl names say, from Linux: the maker and model ("hw.model", /sys/class/dmi), the CPU's name
/// ("machdep.cpu.brand_string", /proc/cpuinfo) and the distribution with the kernel ("kern.osproductversion").
pub fn machine(name: &str) -> Option<String> {
    match name {
        "hw.model" => {
            let junk = |s: &String| { let l = s.to_lowercase(); l.contains("to be filled") || l.contains("system product name") || l == "default string" };
            let model = read("/sys/class/dmi/id/product_name").filter(|s| !junk(s))
                .or_else(|| read("/sys/firmware/devicetree/base/model").map(|s| s.trim_end_matches('\0').to_string()))?;
            Some(match read("/sys/class/dmi/id/sys_vendor").filter(|s| !junk(s)) {
                Some(maker) if !model.starts_with(&maker) => format!("{maker} {model}"),
                _ => model,
            })
        }
        "machdep.cpu.brand_string" => {
            let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
            info.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split_once(':')).map(|(_, v)| v.trim().to_string())
        }
        "kern.osproductversion" => {
            let distro = os_release("PRETTY_NAME").unwrap_or_else(|| "Linux".into());
            let mut u: libc::utsname = unsafe { std::mem::zeroed() };
            let kernel = if unsafe { libc::uname(&mut u) } == 0 {
                unsafe { std::ffi::CStr::from_ptr(u.release.as_ptr()) }.to_string_lossy().into_owned()
            } else {
                "?".into()
            };
            let session = std::env::var("XDG_SESSION_TYPE").ok().filter(|s| !s.is_empty()).map(|s| format!(", {s}")).unwrap_or_default();
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok().filter(|s| !s.is_empty()).map(|s| format!(", {s}")).unwrap_or_default();
            Some(format!("{distro} (kernel {kernel}{session}{desktop})"))
        }
        _ => None,
    }
}

/// How the OS is named before its version in the log's first line: nothing ("Ubuntu 24.04.1 LTS (kernel …)" says it).
pub const OS_NAME: &str = "";

/// The region, as Apple Music's store code ("gb"), from the locale (LC_ALL, LC_MESSAGES, LANG: "en_GB.UTF-8").
pub fn region() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).find(|v| !v.is_empty()).and_then(|v| region_of(&v))
}

fn region_of(locale: &str) -> Option<String> {
    let cc = locale.split(['.', '@']).next()?.split_once('_')?.1.to_ascii_lowercase();
    (cc.len() == 2 && cc.chars().all(|c| c.is_ascii_alphabetic())).then_some(cc)
}

/// The preferred languages, most preferred first, as language tags ("en-GB", "pt-BR"): LANGUAGE's list
/// ("pt_BR:pt:en", gettext's, which outranks the rest unless the locale is C), else the locale (LC_ALL, LC_MESSAGES, LANG:
/// "en_GB.UTF-8"). Empty when nothing says (or the "C"/"POSIX" locale).
pub fn preferred_languages() -> Vec<String> {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    languages_of(var("LANGUAGE"), ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| var(k)))
}

fn languages_of(language: Option<String>, locale: Option<String>) -> Vec<String> {
    let tag = |l: &str| -> Option<String> {
        let base = l.split(['.', '@']).next()?.trim();
        (!base.is_empty() && base != "C" && base != "POSIX").then(|| base.replace('_', "-"))
    };
    let locale = locale.as_deref().and_then(tag);
    // (gettext ignores LANGUAGE when the locale is C)
    let mut out: Vec<String> = match (&language, &locale) {
        (Some(list), Some(_)) => list.split(':').filter_map(tag).collect(),
        _ => Vec::new(),
    };
    if let Some(l) = locale {
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// (Linux has no one "low power mode" to read: the window always draws at full rate.)
pub fn low_power() -> bool {
    false
}

/// Running on a battery: a battery is discharging and no mains supply is online.
pub fn on_battery() -> bool {
    let mut battery_discharging = false;
    for e in std::fs::read_dir("/sys/class/power_supply").into_iter().flatten().flatten() {
        let p = e.path();
        let kind = std::fs::read_to_string(p.join("type")).unwrap_or_default();
        match kind.trim() {
            "Mains" if std::fs::read_to_string(p.join("online")).is_ok_and(|o| o.trim() == "1") => return false,
            "Battery" if std::fs::read_to_string(p.join("status")).is_ok_and(|s| s.trim() == "Discharging") => battery_discharging = true,
            _ => {}
        }
    }
    battery_discharging
}

/// This thread does background work (converting, measuring): the lowest priority (nice 19, idle I/O class).
pub fn background_thread() {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, libc::syscall(libc::SYS_gettid) as libc::id_t, 19);
        // ioprio_set(IOPRIO_WHO_PROCESS, this thread, IOPRIO_CLASS_IDLE << 13)
        libc::syscall(libc::SYS_ioprio_set, 1, 0, 3 << 13);
    }
}

/// This thread learns: after the window, the audio and the live pitch (nice 10).
pub fn utility_thread() {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, libc::syscall(libc::SYS_gettid) as libc::id_t, 10);
    }
}

/// The user's names a report mustn't carry, besides the home folder's and $USER: their full name (passwd's GECOS).
pub fn user_names() -> Vec<String> {
    let full = unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_gecos.is_null() {
            return Vec::new();
        }
        let g = std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy().into_owned();
        g.split(',').next().unwrap_or("").trim().to_string()
    };
    let mut names: Vec<String> = std::env::var("LOGNAME").into_iter().collect();
    names.extend([full].into_iter().filter(|n| !n.is_empty()));
    names
}

/// Is the program a macOS bundle id stands for running (procs.rs)?
pub fn app_running(bundle: &str) -> bool {
    super::procs::running(bundle)
}

/// The UI font's faces (path, face index, weight) and its fallbacks (path, face index).
pub type UiFonts = (Vec<(PathBuf, u32, u16)>, Vec<(PathBuf, u32)>);

/// A UI font for drawing text, from the usual places the distributions put them: Cantarell or Noto Sans, else
/// DejaVu Sans; Noto's CJK and emoji as fallbacks. Only the faces found are given.
pub fn ui_fonts() -> UiFonts {
    let dirs = ["/usr/share/fonts", "/usr/local/share/fonts"];
    let find = |names: &[&str]| -> Option<PathBuf> {
        fn walk(dir: &std::path::Path, names: &[&str], depth: u32) -> Option<PathBuf> {
            for e in std::fs::read_dir(dir).ok()?.flatten() {
                let p = e.path();
                if p.is_dir() && depth < 4 {
                    if let Some(f) = walk(&p, names, depth + 1) {
                        return Some(f);
                    }
                } else if p.file_name().is_some_and(|n| names.iter().any(|w| n.to_string_lossy() == *w)) {
                    return Some(p);
                }
            }
            None
        }
        dirs.iter().find_map(|d| walk(std::path::Path::new(d), names, 0))
    };
    let mut faces = Vec::new();
    for (names, weight) in [(&["NotoSans-Regular.ttf", "Cantarell-Regular.otf", "DejaVuSans.ttf"][..], 400),
                            (&["NotoSans-SemiBold.ttf", "NotoSans-Medium.ttf", "Cantarell-Bold.otf"][..], 600),
                            (&["NotoSans-Bold.ttf", "DejaVuSans-Bold.ttf"][..], 700)] {
        if let Some(f) = find(names) {
            faces.push((f, 0, weight));
        }
    }
    let fallbacks = [&["NotoSansCJK-Regular.ttc"][..], &["NotoColorEmoji.ttf"][..], &["DejaVuSans.ttf"][..]].iter()
        .filter_map(|n| find(n)).map(|f| (f, 0)).collect();
    (faces, fallbacks)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_region_is_read_from_the_locale() {
        assert_eq!(super::region_of("en_GB.UTF-8"), Some("gb".into()));
        assert_eq!(super::region_of("de_DE@euro"), Some("de".into()));
        assert_eq!(super::region_of("C.UTF-8"), None);
        assert_eq!(super::region_of("POSIX"), None);
    }

    #[test]
    fn the_preferred_languages_are_read_from_language_and_the_locale() {
        let s = |v: Option<&str>| v.map(str::to_string);
        assert_eq!(super::languages_of(s(Some("pt_BR:pt:en")), s(Some("pt_BR.UTF-8"))), ["pt-BR", "pt", "en"]);
        assert_eq!(super::languages_of(None, s(Some("en_GB.UTF-8"))), ["en-GB"]);
        assert_eq!(super::languages_of(s(Some("es")), s(Some("de_DE@euro"))), ["es", "de-DE"], "the locale last");
        assert_eq!(super::languages_of(s(Some("es")), s(Some("C.UTF-8"))), Vec::<String>::new(), "LANGUAGE is ignored with C");
        assert_eq!(super::languages_of(s(Some("es")), None), Vec::<String>::new());
        assert_eq!(super::languages_of(None, s(Some("POSIX"))), Vec::<String>::new());
    }

    #[test]
    fn os_release_values_are_unquoted() {
        let t = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(super::os_value(t, "PRETTY_NAME"), Some("Ubuntu 24.04.1 LTS".into()));
        assert_eq!(super::os_value(t, "ID"), Some("ubuntu".into()));
        assert_eq!(super::os_value(t, "NAME"), Some("Ubuntu".into()));
    }

    #[test]
    fn this_computer_and_process_are_measured() {
        assert!(super::physical_memory().unwrap_or(0) > 100 << 20);
        assert!(super::footprint_mb() > 1.0);
        assert!(super::machine("kern.osproductversion").is_some_and(|v| v.contains("kernel")));
        let _ = (super::on_battery(), super::machine("hw.model"), super::machine("machdep.cpu.brand_string"), super::ui_fonts());
    }
}
