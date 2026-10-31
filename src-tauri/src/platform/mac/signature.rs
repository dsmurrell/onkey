//! Songs' signatures and their matching: ShazamKit's (native/shazam.swift), kept as "SHS1".
use std::ffi::c_char;

use super::native::*;

/// The kind learned songs keep (store.rs tags each with it).
pub const KIND: &[u8; 4] = b"SHS1";

/// A signature of mono audio at `rate`.
pub fn of(x: &[f32], rate: f64) -> Option<Vec<u8>> {
    let mut n = 0i32;
    let p = unsafe { onkey_shz_signature(x.as_ptr(), x.len() as i32, rate, &mut n) };
    if p.is_null() {
        return None;
    }
    let v = unsafe { std::slice::from_raw_parts(p, n as usize) }.to_vec();
    unsafe { onkey_shz_free(p) };
    Some(v)
}

/// ShazamKit's custom catalog.
pub struct Catalog {
    handle: i32,
}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog { handle: unsafe { onkey_shz_catalog_new() } }
    }

    /// Add a song's signature under `id`. False: it couldn't be read (another kind, say).
    pub fn add(&mut self, id: &str, sig: &[u8]) -> bool {
        let id = std::ffi::CString::new(id).unwrap_or_default();
        unsafe { onkey_shz_catalog_add(self.handle, sig.as_ptr(), sig.len() as i32, id.as_ptr()) == 0 }
    }

    /// The song a signature is from, and where in its signature's audio the query's starts (s).
    pub fn find(&mut self, sig: &[u8]) -> Option<(String, f64)> {
        let mut id = [0 as c_char; 512];
        let mut offset = 0.0;
        let r = unsafe { onkey_shz_match(self.handle, sig.as_ptr(), sig.len() as i32, id.as_mut_ptr(), id.len() as i32, &mut offset) };
        if r != 1 {
            return None;
        }
        Some((unsafe { std::ffi::CStr::from_ptr(id.as_ptr()) }.to_string_lossy().into_owned(), offset))
    }
}

impl Drop for Catalog {
    fn drop(&mut self) {
        unsafe { onkey_shz_catalog_free(self.handle) };
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new()
    }
}
