//! Small Macs. With 8 GB (a base MacBook Air) and Spotify and a browser open, there's room for OnKey's window and
//! its live pitch, not for the separator, two pitch models and the word timer loaded together while a song plays
//! (opening OnKey slowed a friend's M1 Air right down, the Mac swapping). On 8 GB or less, learning waits for the
//! song to end, runs one model at a time and lets each go after its step (lowmem::low_memory() is asked wherever
//! that's decided); 16 GB and up keep the faster path. ONKEY_LOW_MEM=1 (or 0) overrides the check, in development builds.

static LOW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Physical memory up to 8 GB (or ONKEY_LOW_MEM set): learn after the song, one model at a time.
pub fn low_memory() -> bool {
    *LOW.get_or_init(|| {
        let forced = crate::devtest::knob!("ONKEY_LOW_MEM").map(|v| v != "0" && !v.is_empty());
        let low = forced.unwrap_or_else(|| physical_memory().is_some_and(|b| b <= 8 << 30));
        if low {
            crate::engine::log(&format!("low-memory mode ({}): songs are learned after they end, one model at a time",
                                        match forced { Some(_) => "forced, for testing".to_string(),
                                                       None => format!("{:.0} GB", physical_memory().unwrap_or(0) as f64 / (1u64 << 30) as f64) }));
        }
        low
    })
}

/// Keep the learning models (the separator, CREPE-full, the word timer and GAME: about 1.5 GB with their Core ML
/// programs and arenas) loaded between songs? Only on Macs with 32 GB or more, unless Settings says otherwise
/// ("keep_models"); never on a small Mac. Everywhere else they're let go once a song's learning is done, so singing a
/// learned song never carries them (an M2 Air with 24 GB held 1.8 GB while singing). The next song to learn loads
/// them again: seconds, from the compiled cache. ONKEY_KEEP_MODELS=1 (or 0) overrides it, in development builds.
pub fn keep_learning_models() -> bool {
    if let Some(v) = crate::devtest::knob!("ONKEY_KEEP_MODELS") {
        return v != "0" && !v.is_empty();
    }
    if low_memory() {
        return false;
    }
    crate::settings::get_opt("keep_models").unwrap_or_else(keep_by_default)
}

/// What `keep_learning_models` is with no choice made: Macs with 32 GB or more.
pub fn keep_by_default() -> bool {
    physical_memory().is_some_and(|b| b >= 32 << 30)
}

/// The computer's physical memory in bytes, if it can be read.
pub fn physical_memory() -> Option<u64> {
    crate::platform::sys::physical_memory()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_mac_says_how_much_memory_it_has() {
        let b = super::physical_memory().expect("hw.memsize");
        assert!(b >= 1 << 30 && b <= 1 << 42, "{b} bytes");
    }
}
