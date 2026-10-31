//! Songs' signatures and their matching where there's no ShazamKit: OnKey's own landmark ones (landmark.rs), kept
//! as "LMS1".

/// The kind learned songs keep (store.rs tags each with it).
pub const KIND: &[u8; 4] = b"LMS1";

/// A signature of mono 16 kHz audio (landmark.rs works at 16 kHz only).
pub fn of(x16: &[f32], _rate: f64) -> Option<Vec<u8>> {
    Some(crate::landmark::signature(x16))
}

/// landmark.rs's catalog.
pub struct Catalog {
    cat: crate::landmark::Catalog,
}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog { cat: crate::landmark::Catalog::new() }
    }

    /// Add a song's signature under `id`. False: it couldn't be read (another kind, say).
    pub fn add(&mut self, id: &str, sig: &[u8]) -> bool {
        self.cat.add(id, sig)
    }

    /// The song a signature is from, and where in its signature's audio the query's starts (s).
    pub fn find(&mut self, sig: &[u8]) -> Option<(String, f64)> {
        self.cat.find(sig).map(|h| (h.id, h.offset))
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new()
    }
}
