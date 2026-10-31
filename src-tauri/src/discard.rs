//! Songs you stopped learning ("Stop learning and delete" in the Library) and YouTube videos found not to be songs.
//!
//! A discard drops the play under way (its learner stops at once), a learn waiting its turn or under way (it stops
//! at its next safe point, nothing saved), and what was learned of the song before. Then the track isn't learned
//! again while it plays: a YouTube video never again (kept in notsongs.json: a talk or a vlog won't turn into a
//! song), anything else until another track plays (you may well play it again on purpose later).
//!
//! Discards are by take version: a take that started after the discard (the song played again, on purpose) is
//! never dropped by an old one.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const FILE: &str = "notsongs.json";

/// YouTube videos never learned (by video id: "youtube:<id>"), and where they're kept.
static FOREVER: Mutex<Option<(PathBuf, HashSet<String>)>> = Mutex::new(None);
/// A track (not a video) ignored until another plays.
static UNTIL_CHANGE: Mutex<Option<String>> = Mutex::new(None);
/// Discards this session: (track id, the latest take version then). A take up to that version is dropped.
static DROPPED: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
/// Learns that can be stopped: (watch key, the ids the song goes by, its learner's stop flag).
static WATCHED: Mutex<Vec<(u64, Vec<String>, Arc<AtomicBool>)>> = Mutex::new(Vec::new());
/// The name of what plays now, if it's ignored (for the status line: "Not learning <name>").
static PLAYING_IGNORED: Mutex<Option<String>> = Mutex::new(None);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn is_video(id: &str) -> bool {
    id.starts_with(crate::youtube::ID_PREFIX)
}

/// Read notsongs.json from the data folder.
pub fn set_data_dir(dir: &std::path::Path) {
    let path = dir.join(FILE);
    // there but unreadable: not kept this run, so it isn't written over with only this run's videos
    let Some(ids) = crate::store::read_kept::<Vec<String>>(&path) else {
        crate::store::log_unreadable(&path);
        *FOREVER.lock().unwrap() = None;
        return;
    };
    *FOREVER.lock().unwrap() = Some((path, ids.into_iter().collect()));
}

/// Stop learning `id` and don't learn it while it plays (a video: never). `version`: the latest take's version now
/// (engine::take_version), so this take and any before it are dropped, and none after.
pub fn discard(id: &str, version: u64) {
    if is_video(id) {
        let mut f = FOREVER.lock().unwrap();
        if let Some((path, ids)) = f.as_mut() {
            if ids.insert(id.to_string()) {
                save(path, ids);
            }
        }
    } else {
        *UNTIL_CHANGE.lock().unwrap() = Some(id.to_string());
    }
    DROPPED.lock().unwrap().push((id.to_string(), version));
    for (_, ids, stop) in WATCHED.lock().unwrap().iter() {
        if ids.iter().any(|i| i == id) {
            stop.store(true, Ordering::SeqCst);
        }
    }
    crate::progress::drop_learns(id);
}

/// A video taken for talk is a song after all ("Learn it anyway"): learned the next time it plays.
pub fn undo(id: &str) {
    if let Some((path, ids)) = FOREVER.lock().unwrap().as_mut() {
        if ids.remove(id) {
            save(path, ids);
        }
    }
    let mut u = UNTIL_CHANGE.lock().unwrap();
    if u.as_deref() == Some(id) {
        *u = None;
    }
}

fn save(path: &std::path::Path, ids: &HashSet<String>) {
    let mut v: Vec<&String> = ids.iter().collect();
    v.sort();
    if let Ok(b) = serde_json::to_vec_pretty(&v) {
        let _ = crate::store::write_atomic(path, &b);
    }
}

/// Isn't to be learned now: a video found not to be a song (or stopped), or a track stopped while it still plays.
pub fn ignored(id: &str) -> bool {
    FOREVER.lock().unwrap().as_ref().is_some_and(|(_, ids)| ids.contains(id)) || UNTIL_CHANGE.lock().unwrap().as_deref() == Some(id)
}

/// The music app now plays `id`: a track ignored until another plays is ignored no longer once one does.
pub fn now_playing(id: &str) {
    let mut u = UNTIL_CHANGE.lock().unwrap();
    if u.as_deref().is_some_and(|x| x != id) {
        *u = None;
    }
}

/// What the music app plays now (`None`: nothing), and whether it's ignored: what `playing_ignored` says.
pub fn note_playing(id_name: Option<(&str, &str)>) {
    let n = id_name.filter(|(id, _)| ignored(id)).map(|(_, name)| name.to_string());
    *PLAYING_IGNORED.lock().unwrap() = n;
}

/// The name of the song playing now that you stopped learning (or a video that isn't a song), if one is.
pub fn playing_ignored() -> Option<String> {
    PLAYING_IGNORED.lock().unwrap().clone()
}

/// The take `version`, going by any of `ids`, was discarded (after it started).
pub fn dropped(ids: &[&str], version: u64) -> bool {
    DROPPED.lock().unwrap().iter().any(|(id, v)| version <= *v && ids.contains(&id.as_str()))
}

/// While the guard lives, a discard of any of `ids` stops this learn (its learner's `stop` flag is set).
pub fn watch(ids: Vec<String>, stop: Arc<AtomicBool>, version: u64) -> Watch {
    let key = NEXT.fetch_add(1, Ordering::Relaxed);
    if dropped(&ids.iter().map(String::as_str).collect::<Vec<_>>(), version) {
        stop.store(true, Ordering::SeqCst);
    }
    WATCHED.lock().unwrap().push((key, ids, stop));
    Watch(key)
}

pub struct Watch(u64);

impl Watch {
    /// The song is now kept under another id too.
    pub fn also(&self, id: &str) {
        if let Some(w) = WATCHED.lock().unwrap().iter_mut().find(|w| w.0 == self.0) {
            if !w.1.iter().any(|i| i == id) {
                w.1.push(id.to_string());
            }
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        WATCHED.lock().unwrap().retain(|w| w.0 != self.0);
    }
}

#[cfg(test)]
pub(crate) fn reset_for_test(dir: &std::path::Path) {
    set_data_dir(dir);
    *UNTIL_CHANGE.lock().unwrap() = None;
    DROPPED.lock().unwrap().clear();
    WATCHED.lock().unwrap().clear();
}

#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("onkey-discard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_video_stopped_is_never_learned_again_even_after_a_restart() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = dir("video");
        reset_for_test(&d);
        discard("youtube:abc", 7);
        assert!(ignored("youtube:abc"));
        now_playing("spotify:track:other");                 // another track playing doesn't bring it back
        assert!(ignored("youtube:abc"));
        set_data_dir(&d);                                   // OnKey started again: still not a song
        assert!(ignored("youtube:abc"));
        assert!(!ignored("youtube:xyz"));
        undo("youtube:abc");                                // "Learn it anyway"
        assert!(!ignored("youtube:abc"));
        set_data_dir(&d);
        assert!(!ignored("youtube:abc"), "and not after a restart either");
    }

    #[test]
    fn a_track_stopped_is_ignored_only_until_another_plays() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test(&dir("track"));
        discard("spotify:track:1", 3);
        now_playing("spotify:track:1");
        assert!(ignored("spotify:track:1"), "still playing: still ignored");
        now_playing("spotify:track:2");
        assert!(!ignored("spotify:track:1"), "another track played: it can be learned again");
    }

    #[test]
    fn only_takes_up_to_the_discard_are_dropped() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test(&dir("versions"));
        discard("isrc_X", 5);
        assert!(dropped(&["spotify:track:9", "isrc_X"], 5), "the take playing then");
        assert!(dropped(&["isrc_X"], 2), "one queued from before");
        assert!(!dropped(&["isrc_X"], 6), "played again later, on purpose: kept");
        assert!(!dropped(&["isrc_Y"], 1));
    }

    #[test]
    fn a_discard_stops_a_learn_under_way_by_any_of_its_ids() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test(&dir("watch"));
        let stop = Arc::new(AtomicBool::new(false));
        let w = watch(vec!["spotify:track:1".into()], stop.clone(), 4);
        w.also("isrc_Z");                                   // kept under its ISRC as it's learned
        discard("isrc_Z", 4);
        assert!(stop.load(Ordering::SeqCst));
        drop(w);
        // a learn whose song was already discarded stops as soon as it's watched
        let late = Arc::new(AtomicBool::new(false));
        let _w2 = watch(vec!["isrc_Z".into()], late.clone(), 3);
        assert!(late.load(Ordering::SeqCst));
        let fresh = Arc::new(AtomicBool::new(false));
        let _w3 = watch(vec!["isrc_Z".into()], fresh.clone(), 9);
        assert!(!fresh.load(Ordering::SeqCst), "a later play of it isn't stopped");
    }
}
