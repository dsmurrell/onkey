//! What OnKey keeps on disk, under the app's data folder:
//!   songs/<track>.bin + .json   a learned song: the singer's pitch line, the rhythm fingerprint and which
//!                               10 ms frames have been learned (never audio)
//!   songs/<track>.words       when each lyric word was heard sung when it was learned (times and places in the
//!                               lyrics, never the words), for checking a play's lyrics against the singing
//!   songs/<track>.shsig         its Shazam signature (a compact fingerprint, not audio), so listening mode
//!                               recognises it on this Mac with no network
//!   takes/<time>-<song>/        one sung take: mic.m4a (your voice, AAC; mic.wav before: takeaudio.rs), result.json (scores), meta.json
//!   favorites-pending.json      hearts given to songs still being learned (no songs/<track>.json yet), moved
//!                               into the song's .json ("favorite", "favorited_at") once it's saved
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::dsp::{locate, FP_BANDS, FRAME_S};
use crate::source::{Source, Track};

/// A song is ready to sing once this much of it has been learned.
pub const READY_COVERAGE: f64 = 0.7;

#[derive(Clone, Default)]
pub struct Learned {
    pub midi: Vec<f32>,
    pub covered: Vec<bool>,
    pub onset: Vec<[f32; FP_BANDS]>,
}

impl Learned {
    pub fn fraction(&self, duration: f64) -> f64 {
        self.covered.iter().filter(|c| **c).count() as f64 * FRAME_S / duration.max(1.0)
    }
    pub fn ready(&self, duration: f64) -> bool {
        !self.onset.is_empty() && self.fraction(duration) >= READY_COVERAGE
    }
}

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}

const MAGIC: &[u8; 4] = b"OKS1";

/// A song id as a file name: letters and digits, every other character '_'. That alone isn't one name per id
/// ("youtube:ab-cd_efghi" and "youtube:ab_cd-efghi" were one file, so learning one played the other's notes and
/// forgetting one trashed both): an id with anything but letters, digits and ':' also gets a hash of itself, after
/// a '-' no plain name has. Ids of letters, digits and ':' (Spotify, Apple Music, ISRC, Shazam) keep the names
/// they always had; the others' files are renamed once at start-up (`rename_to_safe_names`).
fn safe(id: &str) -> String {
    let plain = plain_name(id);
    if id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':') {
        plain
    } else {
        format!("{plain}-{:016x}", fnv1a(id))
    }
}

/// The file name every song id had before `safe` added a hash (see there).
fn plain_name(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// FNV-1a, 64-bit: the same for an id on every run and every Mac (std's hasher isn't promised to be).
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// A take's id as the store names them (`new_take_dir`: "20261015-120000-Adele-Hello", "…-2"): letters,
/// digits, '-', '_' and '.', starting with a letter or digit and never "..". So never "", "." or "..", a hidden
/// name or a path: an id from the window can't reach outside takes/ (delete_take("..") once resolved to the data
/// folder itself, and a failed move to the Trash fell through to deleting it all).
pub fn valid_take_id(id: &str) -> bool {
    id.len() <= 255
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && id.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        && !id.contains("..")
}

/// A song id from the window: something with a letter or digit in it (its file name is `safe`, letters, digits,
/// '_' and '-' only, so it can't be a path whatever it is; this only keeps out ids that would name no song at all).
pub fn valid_song_id(id: &str) -> bool {
    id.len() <= 255 && id.chars().any(|c| c.is_ascii_alphanumeric())
}

/// Write a file so that whoever reads it next (or a crash, a kill, a power cut mid-write) sees the old file or
/// the new one, never part of one: written to a temp file beside it, named for this writer alone (process and
/// a counter, so two writers never share one), flushed to disk, then renamed over it. Every file OnKey keeps
/// is written this way.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_beside(path)?;
    let written = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// A temp file's name beside `path` for this writer alone (".<file>.tmp-<pid>-<n>"), for writing it atomically.
pub(crate) fn temp_beside(path: &Path) -> std::io::Result<PathBuf> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let name = path.file_name().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name"))?;
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(path.with_file_name(format!(".{}{TMP_MARK}{}-{n}", name.to_string_lossy(), std::process::id())))
}

/// One writer at a time for the store's read-change-write updates (a song's .bin and .json,
/// favorites-pending.json): two takes of a song finishing together would each read the old song, add their part
/// and write, and the last write would drop the other's learning; the cover finder, a heart from the window and
/// a score thread likewise. Store-wide (every Store on any folder: they're cheap to make and cloned freely) and
/// re-entrant, since `canonical` → `merge_into` → `save_song` take it on one thread.
struct WriteLock {
    owner: std::sync::Mutex<(Option<std::thread::ThreadId>, usize)>,
    free: std::sync::Condvar,
}

static WRITES: WriteLock = WriteLock { owner: std::sync::Mutex::new((None, 0)), free: std::sync::Condvar::new() };

/// Held while one thread updates the store (see `WriteLock`).
pub(crate) struct Writing;

pub(crate) fn writing() -> Writing {
    let me = std::thread::current().id();
    let mut o = WRITES.owner.lock().unwrap_or_else(|e| e.into_inner());
    while o.0.is_some_and(|t| t != me) {
        o = WRITES.free.wait(o).unwrap_or_else(|e| e.into_inner());
    }
    o.0 = Some(me);
    o.1 += 1;
    Writing
}

impl Drop for Writing {
    fn drop(&mut self) {
        let mut o = WRITES.owner.lock().unwrap_or_else(|e| e.into_inner());
        o.1 -= 1;
        if o.1 == 0 {
            o.0 = None;
            WRITES.free.notify_one();
        }
    }
}

/// A small file OnKey keeps whole and writes back changed (a list or a map): its contents, or the default when
/// it isn't there yet. None when it's there but couldn't be read: then the caller mustn't write it back, as what
/// it would write (the default plus one change) would replace everything in it. One that reads but is garbled
/// is first moved aside, kept as "<name>.unreadable-<time>", and starts afresh.
pub(crate) fn read_kept<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Option<T> {
    match std::fs::read(path) {
        Ok(b) => serde_json::from_slice(&b).ok().or_else(|| {
            let name = path.file_name()?.to_string_lossy().to_string();
            let aside = path.with_file_name(format!("{name}.unreadable-{}", now_iso().replace(':', ".")));
            std::fs::rename(path, &aside).ok()?;
            if !cfg!(test) {
                crate::engine::log(&format!("{name} was garbled: kept as {}, starting afresh", aside.display()));
            }
            Some(T::default())
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(T::default()),
        Err(_) => None,
    }
}

/// Logged when a file couldn't be read and so was left as it is, not written over.
pub(crate) fn log_unreadable(path: &Path) {
    if !cfg!(test) {
        crate::engine::log(&format!("couldn't read {}: left as it is, not written over", path.display()));
    }
}

/// In the name of `write_atomic`'s temp files: ".<file>.tmp-<pid>-<n>".
pub(crate) const TMP_MARK: &str = ".tmp-";

/// Temp files left in `dir` by a write that never finished (OnKey was killed mid-write): another process's, and
/// over ten minutes old (so a lab tool opening the same folder never takes one from under a running OnKey).
pub(crate) fn sweep_temp_files(dir: &Path) {
    let mine = format!("{TMP_MARK}{}-", std::process::id());
    let old = |e: &std::fs::DirEntry| e.metadata().ok().and_then(|m| m.modified().ok())
        .and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 600);
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let left = (name.starts_with('.') && name.contains(TMP_MARK) && !name.contains(&mine)) || name.ends_with(".bin.tmp");
        if left && e.path().is_file() && old(&e) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

impl Store {
    pub fn new(root: PathBuf) -> Store {
        let _ = std::fs::create_dir_all(root.join("songs"));
        let _ = std::fs::create_dir_all(root.join("takes"));
        sweep_temp_files(&root.join("songs"));
        sweep_temp_files(&root);
        Store { root }
    }

    /// A new take's folder, takes/<name>; "<name>-2", "-3", … when a take of the same song was saved in the same
    /// second (two takes finishing together), so one never overwrites the other. Returns it and its name.
    pub fn new_take_dir(&self, name: &str) -> std::io::Result<(PathBuf, String)> {
        std::fs::create_dir_all(self.takes_dir())?;
        for k in 1..1000 {
            let n = if k == 1 { name.to_string() } else { format!("{name}-{k}") };
            let d = self.takes_dir().join(&n);
            match std::fs::create_dir(&d) {
                Ok(()) => return Ok((d, n)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no free take folder name"))
    }

    fn song_path(&self, id: &str, ext: &str) -> PathBuf {
        self.root.join("songs").join(format!("{}.{ext}", safe(id)))
    }

    /// Where a song's file with this extension is kept ("bin", "json", "syl.json", …): for installing the demo song.
    pub fn song_file(&self, id: &str, ext: &str) -> PathBuf {
        self.song_path(id, ext)
    }

    pub fn takes_dir(&self) -> PathBuf {
        self.root.join("takes")
    }

    pub fn load_song(&self, id: &str) -> Option<Learned> {
        let mut b = Vec::new();
        std::fs::File::open(self.song_path(id, "bin")).ok()?.read_to_end(&mut b).ok()?;
        if b.len() < 8 || &b[..4] != MAGIC {
            return None;
        }
        let n = u32::from_le_bytes(b[4..8].try_into().ok()?) as usize;
        if b.len() < 8 + n * (4 + 1 + 2 * FP_BANDS) {
            return None;
        }
        let mut p = 8;
        let midi = (0..n).map(|i| f32::from_le_bytes(b[p + 4 * i..p + 4 * i + 4].try_into().unwrap())).collect();
        p += 4 * n;
        let covered = b[p..p + n].iter().map(|v| *v != 0).collect();
        p += n;
        let onset = (0..n).map(|i| {
            let mut row = [0f32; FP_BANDS];
            for (k, r) in row.iter_mut().enumerate() {
                let o = p + 2 * (i * FP_BANDS + k);
                *r = i16::from_le_bytes([b[o], b[o + 1]]) as f32 / 256.0;
            }
            row
        }).collect();
        let l = Learned { midi, covered, onset };
        // a song saved without a fingerprint has all-zero rows; treat it as having none
        Some(if l.onset.iter().all(|r| r.iter().all(|v| *v == 0.0)) { Learned { onset: Vec::new(), ..l } } else { l })
    }

    fn write_song(&self, id: &str, l: &Learned) -> std::io::Result<()> {
        let n = l.midi.len();
        let mut b = Vec::with_capacity(8 + n * (5 + 2 * FP_BANDS));
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&(n as u32).to_le_bytes());
        for v in &l.midi {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend(l.covered.iter().map(|c| *c as u8));
        for i in 0..n {
            let row = l.onset.get(i).copied().unwrap_or([0.0; FP_BANDS]);
            for v in row {
                b.extend_from_slice(&((v * 256.0).round().clamp(-32768.0, 32767.0) as i16).to_le_bytes());
            }
        }
        write_atomic(&self.song_path(id, "bin"), &b)
    }

    /// Merge a take's singer line + fingerprint (song-time frames) into the song. If the song is already
    /// partly learned, the new part is first lined up with it by fingerprint (start times can be off).
    /// Returns false when the new part was rejected: it overlaps what's learned but its fingerprint doesn't
    /// confirm where it belongs (merging it would overwrite good data with misplaced notes), or it couldn't be
    /// written. Load, merge and write happen under the store's write lock, so two takes of the song finishing
    /// together are both kept.
    pub fn save_song(&self, track: &Track, midi: Vec<f32>, covered: Vec<bool>, onset: Vec<[f32; FP_BANDS]>) -> bool {
        self.save_song_noted(track, midi, covered, onset, (false, false))
    }

    /// save_song, for a play whose length is a guess and/or that was a YouTube video (`note`; see keep_known).
    fn save_song_noted(&self, track: &Track, mut midi: Vec<f32>, mut covered: Vec<bool>, mut onset: Vec<[f32; FP_BANDS]>,
                       note: (bool, bool)) -> bool {
        let _writing = writing();
        // its .json there but garbled (a sync tool, a disk fault): moved aside, kept, before this play writes one
        // afresh (it was written over, its heart, lyric times and the rest lost)
        let json = self.song_path(&track.id, "json");
        if json.exists() && read_kept::<Value>(&json).is_none() {
            log_unreadable(&json);
            return false;
        }
        // what's learned of it already, or its .json (its heart, lyrics), there but unreadable just now: not saved
        // over (this play's part alone, a .json without them)
        if let Some(p) = ["bin", "json"].map(|e| self.song_path(&track.id, e)).into_iter().find(|p| p.exists() && std::fs::read(p).is_err()) {
            log_unreadable(&p);
            return false;
        }
        // learning it again: this play replaces what was learned (only once it learned enough to sing)
        let again = self.learning_again(&track.id);
        if again && !(Learned { midi: midi.clone(), covered: covered.clone(), onset: onset.clone() }).ready(track.duration) {
            if !cfg!(test) {
                crate::engine::log(&format!("{} - {}: learning it again, but this play heard too little of it to replace what was learned: the next play tries again",
                                            track.artist, track.name));
            }
            return false;
        }
        let old = if again { None } else { self.load_song(&track.id) };
        let first = !self.song_path(&track.id, "json").exists();
        if let Some(old) = &old {
            if !old.onset.is_empty() && !onset.is_empty() {
                let Some(sh) = fingerprint_shift(&onset, &old.onset, &covered, &old.covered) else { return false };
                if sh != 0 {
                    midi = shift(&midi, sh, f32::NAN);
                    covered = shift(&covered, sh, false);
                    onset = shift(&onset, sh, [0.0; FP_BANDS]);
                }
            }
        }
        let merged = match old {
            Some(old) => {
                let n = old.midi.len().max(midi.len());
                let mut m = fit(&old.midi, n, f32::NAN);
                let mut cov = fit(&old.covered, n, false);
                let mut env = fit(&old.onset, n, [0.0; FP_BANDS]);
                let new_cov = fit(&covered, n, false);
                let new_m = fit(&midi, n, f32::NAN);
                let new_env = fit(&onset, n, [0.0; FP_BANDS]);
                for i in 0..n {
                    if new_cov[i] {
                        m[i] = new_m[i];
                        if !onset.is_empty() {
                            env[i] = new_env[i];
                        }
                        cov[i] = true;
                    }
                }
                Learned { midi: m, covered: cov, onset: env }
            }
            None => Learned { midi, covered, onset },
        };
        if let Err(e) = self.write_song(&track.id, &merged) {
            if !cfg!(test) {
                crate::engine::log(&format!("{} - {}: couldn't be saved: {e}", track.artist, track.name));
            }
            return false;
        }
        if again {
            // what was made from the copy replaced (its signature, syllable bars, words heard) goes with it: made
            // again from this play, as for a first one
            for ext in ["shsig", "syl.json", "words"] {
                let _ = std::fs::remove_file(self.song_path(&track.id, ext));
            }
            crate::lyrics::forget_learned(&track.id);
        }
        {
            let mut track = track.clone();
            // keep a cover found earlier, and the Apple Music page found for the same cover
            if let Some(c) = self.cover(&track.id).filter(|c| track.art.is_none() || track.art.as_ref() == Some(&c.art)) {
                track.art = Some(c.art);
                track.link = track.link.or(c.link);
            }
            // what the song already knows about itself isn't lost to a play that knew less (see keep_known)
            let (mut guessed, youtube) = note;
            if let Some(m) = self.song_meta(&track.id) {
                if let Ok(old) = serde_json::from_value::<Track>(m["track"].clone()) {
                    (track, guessed) = keep_known(&old, m["length_guessed"].as_bool().unwrap_or(false), track, guessed, youtube);
                }
            }
            let mut meta = json!({"track": track, "learned": now_iso(), "fraction": merged.fraction(track.duration)});
            if guessed {
                meta["length_guessed"] = json!(true);
            }
            if let Some(old) = self.song_meta(&track.id) {
                // keep lyrics already lined up with this song, and its heart (learned again: not the lyric times,
                // lined up with the copy replaced; this play lines them up afresh)
                for k in ["lyric_times", "lyric_check", "lyric_lines", "lyrics", "lyrics_placed", "favorite", "favorited_at", "played_in", "aliases", "not_melody"] {
                    if again && k.starts_with("lyric") {
                        continue;
                    }
                    if !old[k].is_null() {
                        meta[k] = old[k].clone();
                    }
                }
            }
            // hearted while it was being learned
            if let Some(at) = self.take_pending_favorite(&track.id) {
                mark_favorite(&mut meta, true, &at);
            }
            if let Err(e) = write_atomic(&self.song_path(&track.id, "json"), meta.to_string().as_bytes()) {
                if !cfg!(test) {
                    crate::engine::log(&format!("{} - {}: its details couldn't be saved: {e}", track.artist, track.name));
                }
                // a first save with notes and no .json would be a song sung but never listed, hearted or forgotten:
                // the notes go too, and the next play learns it as a first play again
                if first {
                    let _ = std::fs::remove_file(self.song_path(&track.id, "bin"));
                }
                return false;
            }
        }
        true
    }

    /// Change a song's .json with `f` (which says whether it changed anything); written only if it did.
    fn with_meta(&self, id: &str, f: impl FnOnce(&mut Value) -> bool) {
        let _writing = writing();
        if let Some(mut m) = self.song_meta(id) {
            if f(&mut m) {
                let _ = write_atomic(&self.song_path(id, "json"), m.to_string().as_bytes());
            }
        }
    }

    /// A song kept as `id` was played (sung or learned) in `app`, as `played_as` there (Spotify's track id, Apple
    /// Music's, a YouTube video's; or the song's own id): kept with it, so it's opened there again.
    pub fn note_play(&self, id: &str, played_as: &str, app: Source) {
        self.with_meta(id, |m| {
            let mut changed = add_alias(m, id, played_as);
            if let Some(k) = app_key(app) {
                changed |= m["played_in"].as_str() != Some(k);
                m["played_in"] = json!(k);
            }
            changed
        });
    }

    /// The song kept as `id` is `alias` in an app (its Spotify track id, say).
    pub fn note_alias(&self, id: &str, alias: &str) {
        self.with_meta(id, |m| add_alias(m, id, alias));
    }

    /// The app you usually sing from: the one with the most takes in History (None when no take says).
    pub fn usual_app(&self) -> Option<Source> {
        usual_app(&self.takes())
    }

    /// Where to open the song kept as `id` (or known by it in an app) in your music app; `running`: the app
    /// being listened to now, if any. With the song's track as stored, when it's learned here.
    pub fn open_pick(&self, id: &str, running: Option<Source>) -> (Pick, Option<Track>) {
        let (pick, _, track) = self.pick_with_ids(id, running);
        (pick, track)
    }

    /// The pick (see `open_pick`), the song's ids (the one it's kept under and its aliases), and its track.
    fn pick_with_ids(&self, id: &str, running: Option<Source>) -> (Pick, Vec<String>, Option<Track>) {
        let m = self.song_meta(id).or_else(|| self.learned_metas().into_iter().find(|m| aliases(m).iter().any(|a| a == id)));
        let own = m.as_ref().and_then(|m| m["track"]["id"].as_str().map(String::from)).unwrap_or_else(|| id.to_string());
        let ids: Vec<String> = m.as_ref().map(aliases).unwrap_or_default();
        let played_in = m.as_ref().and_then(|m| m["played_in"].as_str().and_then(app_of_key));
        let track = m.and_then(|m| serde_json::from_value::<Track>(m["track"].clone()).ok());
        let pick = pick_app(&own, &ids, played_in, self.usual_app(), running);
        let mut all = vec![id.to_string(), own];
        all.extend(ids);
        (pick, all, track)
    }

    /// How OnKey starts the song kept as `id` (or known by it in an app) when you press Play or Sing: in the app its
    /// "in Apple Music ↗" link names, by its id there (see `play_plan`). `open`: whether an app is running now.
    pub fn play_plan(&self, id: &str, running: Option<Source>, open: impl Fn(Source) -> bool) -> PlayPlan {
        let (pick, ids, _) = self.pick_with_ids(id, running);
        play_plan(&ids, &pick, open)
    }

    fn learned_metas(&self) -> Vec<Value> {
        std::fs::read_dir(self.root.join("songs")).into_iter().flatten().flatten()
            .filter(|e| is_song_meta(&e.path()))
            .filter_map(|e| serde_json::from_slice::<Value>(&std::fs::read(e.path()).ok()?).ok())
            .filter(|m| m["track"]["id"].is_string())
            .collect()
    }

    /// Once at start-up: songs whose files were named before `safe` gave ids with punctuation a name of their own
    /// are renamed to it (each file, its .json last, so one stopped part way is finished at the next start). A
    /// song whose new name is taken already is left as it is. Returns how many were renamed.
    pub fn rename_to_safe_names(&self) -> usize {
        let _writing = writing();
        let songs = self.root.join("songs");
        let mut n = 0;
        for e in std::fs::read_dir(&songs).into_iter().flatten().flatten() {
            let p = e.path();
            if !is_song_meta(&p) {
                continue;
            }
            let Some(id) = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|m| m["track"]["id"].as_str().map(String::from)) else { continue };
            let (old, new) = (plain_name(&id), safe(&id));
            if old == new || p.file_stem().is_none_or(|s| s.to_string_lossy() != old) || self.song_path(&id, "json").exists() {
                continue;
            }
            let moved = ["bin", "syl.json", "shsig", "words", "json"].iter().try_for_each(|ext| {
                let from = songs.join(format!("{old}.{ext}"));
                if from.exists() { std::fs::rename(&from, self.song_path(&id, ext)) } else { Ok(()) }
            });
            match moved {
                Ok(()) => n += 1,
                Err(e) => crate::engine::log(&format!("library: {id} couldn't be renamed to its own file name: {e}")),
            }
        }
        n
    }

    /// Once at start-up: the apps' ids and the app played in, for songs learned before they were kept: from the
    /// entries merged into them (songs/merged/) and from your takes (their track ids, newest first).
    pub fn backfill_sources(&self) -> usize {
        let learned = self.learned_tracks();
        let find = |t: &Track| learned.iter().find(|k| k.id == t.id).or_else(|| learned.iter().find(|k| same_title(k, t))).map(|k| k.id.clone());
        let mut found: Vec<(String, String, Option<Source>)> = Vec::new();   // (song id, id played as, app)
        for e in std::fs::read_dir(self.root.join("songs").join("merged")).into_iter().flatten().flatten() {
            let t = std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|m| serde_json::from_value::<Track>(m["track"].clone()).ok());
            if let Some((t, id)) = t.and_then(|t| find(&t).map(|id| (t, id))) {
                found.push((id, t.id, None));
            }
        }
        for t in self.takes() {
            let Ok(track) = serde_json::from_value::<Track>(t["track"].clone()) else { continue };
            let app = t["app"].as_str().and_then(app_of_key).or(Some(Source::of_id(&track.id))).filter(|a| *a != Source::Other);
            if let Some(id) = find(&track) {
                found.push((id, track.id, app));
            }
        }
        let mut changed = 0;
        for id in learned.iter().map(|k| &k.id) {
            let mine: Vec<&(String, String, Option<Source>)> = found.iter().filter(|f| f.0 == *id).collect();
            if mine.is_empty() {
                continue;
            }
            self.with_meta(id, |m| {
                let mut c = mine.iter().rev().fold(false, |c, f| add_alias(m, id, &f.1) | c);   // the newest first
                if m["played_in"].is_null() {
                    if let Some(k) = mine.iter().find_map(|f| f.2).and_then(app_key) {
                        m["played_in"] = json!(k);
                        c = true;
                    }
                }
                changed += c as usize;
                c
            });
        }
        changed
    }

    /// The id a song's heart is kept under: its own when it's learned under it, else the learned song with its
    /// artist and title (a Spotify id for a song kept by its ISRC), else its own (a song still being learned).
    fn favorite_id(&self, t: &Track) -> String {
        if t.name.is_empty() || self.song_path(&t.id, "json").exists() {
            return t.id.clone();
        }
        self.known_as(t).map_or_else(|| t.id.clone(), |k| k.id)
    }

    /// Heart a song (or take its heart away). Kept in the song's .json, so it goes wherever the song goes; a song
    /// not saved yet (being learned for the first time) gets it once it's saved. Returns whether it's a favourite.
    pub fn set_favorite(&self, t: &Track, on: bool) -> bool {
        let _writing = writing();
        let id = self.favorite_id(t);
        if let Some(mut m) = self.song_meta(&id) {
            if m["favorite"].as_bool().unwrap_or(false) != on {
                mark_favorite(&mut m, on, &now_iso());
                let _ = write_atomic(&self.song_path(&id, "json"), m.to_string().as_bytes());
            }
        } else {
            let Some(mut p) = self.pending_favorites() else {
                log_unreadable(&self.root.join("favorites-pending.json"));
                return on;
            };
            if on {
                p.entry(id).or_insert_with(|| json!(now_iso()));
            } else {
                p.remove(&id);
            }
            self.write_pending_favorites(&p);
        }
        on
    }

    /// The song a track's takes belong to, as its heart is kept (`favorite_id`): a Spotify id and the ISRC
    /// it's learned under are one song.
    pub fn song_key(&self, t: &Track) -> String {
        self.favorite_id(t)
    }

    pub fn is_favorite(&self, t: &Track) -> bool {
        let id = self.favorite_id(t);
        match self.song_meta(&id) {
            Some(m) => m["favorite"].as_bool().unwrap_or(false),
            None => self.pending_favorites().is_some_and(|p| p.contains_key(&id)),
        }
    }

    /// Hearts given to songs not saved yet; None if the file is there but couldn't be read (then it isn't
    /// written over: see `read_kept`).
    fn pending_favorites(&self) -> Option<serde_json::Map<String, Value>> {
        read_kept(&self.root.join("favorites-pending.json"))
    }

    fn write_pending_favorites(&self, p: &serde_json::Map<String, Value>) {
        let path = self.root.join("favorites-pending.json");
        if p.is_empty() {
            let _ = std::fs::remove_file(path);
        } else {
            let _ = write_atomic(&path, Value::Object(p.clone()).to_string().as_bytes());
        }
    }

    /// A heart given to `id` before it was saved: when (and it's no longer pending).
    fn take_pending_favorite(&self, id: &str) -> Option<String> {
        let _writing = writing();
        let mut p = self.pending_favorites()?;
        let at = p.remove(id)?;
        self.write_pending_favorites(&p);
        Some(at.as_str().map_or_else(now_iso, String::from))
    }

    /// Whether a learned song needs (more) syllable bars: none, or too few to show (see `thin_bars`).
    pub fn needs_bars(&self, id: &str, duration: f64) -> bool {
        let span = match self.kept_lyrics(id) {
            Some(KeptLyrics::Times(k)) => k.times.iter().copied().fold(None, |s: Option<(f64, f64)>, t| Some(s.map_or((t, t), |(a, b)| (a.min(t), b.max(t))))),
            _ => None,
        };
        thin_bars(self.syllables(id, None).as_deref().unwrap_or(&[]), duration, span)
    }

    /// Whether a play has placed the song's lyrics by the words heard (so its times are no longer LRCLIB's).
    pub fn lyrics_placed(&self, id: &str) -> bool {
        self.song_meta(id).is_some_and(|m| m["lyrics_placed"].as_bool() == Some(true))
    }

    /// A play placed the song's lyrics by the words heard, believed (see lyrics_placed).
    pub fn mark_lyrics_placed(&self, id: &str) {
        self.with_meta(id, |m| {
            let changed = m["lyrics_placed"].as_bool() != Some(true);
            m["lyrics_placed"] = json!(true);
            changed
        });
    }

    /// The stretches of a learned song (song seconds, [from, to), in order) marked "Not the melody" on a take's page:
    /// its learned notes there follow a backing vocal or a harmony instead of the lead, so they're left out of
    /// every take's score (analysis::leave_out) and of the live one (Take::leave_out).
    pub fn not_melody(&self, id: &str) -> Vec<(f64, f64)> {
        self.song_meta(id).map(|m| stretches_of(&m["not_melody"])).unwrap_or_default()
    }

    /// Keep a learned song's "Not the melody" stretches (see `not_melody`), tidied (`tidy_stretches`); nothing
    /// left removes them. Returns them as kept, or None when the song isn't learned.
    pub fn set_not_melody(&self, id: &str, stretches: &[(f64, f64)]) -> Option<Vec<(f64, f64)>> {
        let tidy = tidy_stretches(stretches);
        let r = |x: f64| (x * 100.0).round() / 100.0;
        let mut saved = None;
        self.with_meta(id, |m| {
            saved = Some(tidy.clone());
            let was = m["not_melody"].clone();
            match m.as_object_mut() {
                Some(o) if tidy.is_empty() => { o.remove("not_melody"); }
                Some(o) => { o.insert("not_melody".into(), json!(tidy.iter().map(|&(a, b)| [r(a), r(b)]).collect::<Vec<_>>())); }
                None => return false,
            }
            m["not_melody"] != was
        });
        saved
    }

    /// The takes of a learned song (kept under its id, or under one it has played as): their ids.
    pub fn takes_of(&self, id: &str) -> Vec<String> {
        let ids: Vec<String> = std::iter::once(id.to_string()).chain(self.song_meta(id).map(|m| aliases(&m)).unwrap_or_default()).collect();
        let mut out: Vec<String> = std::fs::read_dir(self.takes_dir()).into_iter().flatten().flatten()
            .filter(|e| e.path().is_dir())
            .filter(|e| std::fs::read(e.path().join("meta.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .is_some_and(|m| m["track"]["id"].as_str().is_some_and(|t| ids.iter().any(|i| i == t))))
            .map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        out.sort();
        out
    }

    /// Score a take again with the song's "Not the melody" stretches left out (`stretches`, song seconds; none: as
    /// first scored), and keep it, marked as rescored then. Returns its score if it changed.
    pub fn rescore_take(&self, take: &str, stretches: &[(f64, f64)]) -> Option<i64> {
        let d = self.take_dir(take)?;
        let meta: Value = serde_json::from_slice(&std::fs::read(d.join("meta.json")).ok()?).ok()?;
        let off = meta["song_offset"].as_f64()?;   // (never placed in the song: nothing to leave out)
        let mut r: Value = serde_json::from_slice(&std::fs::read(d.join("result.json")).ok()?).ok()?;
        let was = r.clone();
        // each part of the take at its own place in the song (a take kept before its places were: its offset throughout)
        let mut places: Vec<(i64, i64, i64)> = meta["places"].as_array().map(|a| a.iter()
            .filter_map(|p| Some((p[0].as_i64()?, p[1].as_i64()?, p[2].as_i64()?))).collect()).unwrap_or_default();
        if places.is_empty() {
            places.push((0, i64::MAX, (off / FRAME_S).round() as i64));
        }
        let frames = crate::analysis::take_frames(stretches, &places);
        crate::analysis::leave_out(&mut r, &frames);
        if r == was {
            return None;
        }
        match r.as_object_mut() {
            Some(o) if frames.is_empty() => { o.remove("rescored"); }
            _ => r["rescored"] = json!(now_iso()),
        }
        write_atomic(&d.join("result.json"), r.to_string().as_bytes()).ok()?;
        (r["score"] != was["score"]).then(|| r["score"].as_i64()).flatten()
    }

    /// A learned song's length as kept (s), when it has one.
    pub fn song_duration(&self, id: &str) -> Option<f64> {
        self.song_meta(id)?["track"]["duration"].as_f64().filter(|d| *d > 0.0)
    }

    fn song_meta(&self, id: &str) -> Option<Value> {
        serde_json::from_slice(&std::fs::read(self.song_path(id, "json")).ok()?).ok()
    }

    /// Forget a learned song (to the Trash, so it can be recovered); your takes stay. If a file can't be moved
    /// to the Trash it stops there with an error and nothing is deleted: its .json goes last, so a song only part
    /// moved still shows in the Library and can be deleted again.
    pub fn forget(&self, id: &str) -> Result<(), String> {
        if !valid_song_id(id) {
            return Err("not a song".into());
        }
        let _writing = writing();
        for ext in ["bin", "syl.json", "shsig", "words", "json"] {
            to_trash(&self.song_path(id, ext))?;
        }
        crate::lyrics::forget_learned(id);
        Ok(())
    }

    /// Keep a song's Shazam signature, made from `seconds` of its audio starting `start` s into the song. A
    /// longer one replaces a shorter one (a later, fuller play of the song).
    pub fn save_signature(&self, id: &str, sig: &[u8], start: f64, seconds: f64) {
        let _writing = writing();
        if self.signature(id).is_some_and(|(_, _, had)| had >= seconds) {
            return;
        }
        let mut b = Vec::with_capacity(sig.len() + 20);
        b.extend_from_slice(b"SHS1");
        b.extend_from_slice(&start.to_le_bytes());
        b.extend_from_slice(&seconds.to_le_bytes());
        b.extend_from_slice(sig);
        let _ = write_atomic(&self.song_path(id, "shsig"), &b);
    }

    /// A song's Shazam signature: (bytes, song seconds where it starts, seconds it covers). None while it's to be
    /// learned again: made from the copy that's off, it would place that play off in the same way.
    pub fn signature(&self, id: &str) -> Option<(Vec<u8>, f64, f64)> {
        let b = std::fs::read(self.song_path(id, "shsig")).ok()?;
        if self.learning_again(id) {
            return None;
        }
        if b.len() < 20 || &b[..4] != b"SHS1" {
            return None;
        }
        let start = f64::from_le_bytes(b[4..12].try_into().ok()?);
        let seconds = f64::from_le_bytes(b[12..20].try_into().ok()?);
        Some((b[20..].to_vec(), start, seconds))
    }

    /// The song learned here that `t` is, when it's kept under another id (Spotify's track id for a song learned
    /// from Shazam's match, by its ISRC, say): the same artist and title (letters and digits, any case). None when
    /// `t` is kept under its own id, or isn't learned. Another version ("Castle on the Hill - Acoustic") is
    /// another song.
    pub fn known_as(&self, t: &Track) -> Option<Track> {
        if self.song_path(&t.id, "json").exists() {
            return None;
        }
        self.learned_tracks().into_iter().find(|k| same_title(k, t))
    }

    /// The ids "Stop learning and delete" forgets for song `t` being learned: its own, and the entry it's kept under
    /// by its title (its ISRC) when that one is itself unfinished (not ready to sing, or being learned again). A
    /// song learned in full before isn't touched: another video with the same title stopped is that video only.
    pub fn stop_learning_ids(&self, t: &Track) -> Vec<String> {
        let mut ids = vec![t.id.clone()];
        if let Some(k) = self.known_as(t) {
            let ready = self.load_song(&k.id).is_some_and(|l| l.ready(self.song_duration(&k.id).unwrap_or(k.duration)));
            if !ready || self.learning_again(&k.id) {
                ids.push(k.id);
            }
        }
        ids
    }

    /// The id song `t` is kept under: one entry per song, by its ISRC when it has one (Shazam's), so a song heard
    /// from Spotify (Spotify's track id), Apple Music or Shazam is one song. Other entries for it (the same artist
    /// and title under other ids) are merged into that one first. Returns `t` with that id.
    pub fn canonical(&self, t: &Track) -> Track {
        let _writing = writing();
        let same: Vec<Track> = self.learned_tracks().into_iter().filter(|k| k.id != t.id && same_title(k, t)).collect();
        if same.is_empty() {
            return t.clone();
        }
        let isrc = |id: &str| id.starts_with("isrc:");
        let id = if isrc(&t.id) {
            t.id.clone()
        } else if let Some(k) = same.iter().find(|k| isrc(&k.id)) {
            k.id.clone()
        } else if self.song_path(&t.id, "json").exists() {
            t.id.clone()
        } else {
            same[0].id.clone()
        };
        let target = Track { id, ..t.clone() };
        let others: Vec<String> = same.iter().map(|k| k.id.clone()).chain(std::iter::once(t.id.clone()))
            .filter(|k| *k != target.id && self.song_path(k, "json").exists()).collect();
        for k in others {
            self.merge_into(&k, &target);
        }
        // a heart given to the song under another id while it was being learned goes with it
        for k in same.iter().map(|k| k.id.as_str()).chain(std::iter::once(t.id.as_str())).filter(|k| *k != target.id) {
            if let Some(at) = self.take_pending_favorite(k) {
                if let Some(mut p) = self.pending_favorites() {
                    p.entry(target.id.clone()).or_insert(json!(at));
                    self.write_pending_favorites(&p);
                }
            }
        }
        if let Some(mut m) = self.song_meta(&target.id) {
            if let Some(at) = self.take_pending_favorite(&target.id) {
                mark_favorite(&mut m, true, &at);
                let _ = write_atomic(&self.song_path(&target.id, "json"), m.to_string().as_bytes());
            }
        }
        target
    }

    /// Save a play's learning (see `save_song`) under the song's one id (see `canonical`), the two together: plays
    /// of one song saved at the same moment under two ids end up as one entry. Returns whether the play was added,
    /// and the track as kept.
    pub fn save_as_one(&self, track: &Track, midi: Vec<f32>, covered: Vec<bool>, onset: Vec<[f32; FP_BANDS]>) -> (bool, Track) {
        static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _one = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        // what this play knew less well, for this save only (not a merge canonical makes on the way)
        let note = take_play_note();
        let canon = self.canonical(track);
        (self.save_song_noted(&canon, midi, covered, onset, note), canon)
    }

    /// Merge the song kept as `from` into `to` (lined up by fingerprint where both have some), and put `from`'s
    /// files aside in songs/merged/.
    fn merge_into(&self, from: &str, to: &Track) {
        let _writing = writing();
        let Some(old) = self.load_song(from) else { return };
        let from_meta = self.song_meta(from);
        let kept = match self.load_song(&to.id) {
            None => {
                // nothing under `to` yet: `from` becomes it
                let _ = self.write_song(&to.id, &old);
                let mut meta = self.song_meta(from).unwrap_or_else(|| json!({}));
                meta["track"] = json!(Track { id: to.id.clone(), ..to.clone() });
                meta["fraction"] = json!(old.fraction(to.duration));
                let _ = write_atomic(&self.song_path(&to.id, "json"), meta.to_string().as_bytes());
                true
            }
            Some(had) => {
                let into = self.song_meta(&to.id).and_then(|m| serde_json::from_value::<Track>(m["track"].clone()).ok()).unwrap_or_else(|| to.clone());
                let merged = self.save_song(&into, old.midi.clone(), old.covered.clone(), old.onset.clone());
                // couldn't be lined up: the one with more of the song learned is kept
                if !merged && old.fraction(to.duration) > had.fraction(to.duration) {
                    let _ = self.write_song(&to.id, &old);
                }
                merged
            }
        };
        // the ids it was known by in the apps, and the app it played in, go with it
        if let Some(f) = &from_meta {
            let mut ids = aliases(f);
            ids.insert(0, from.to_string());
            self.with_meta(&to.id, |m| {
                let mut changed = ids.iter().fold(false, |c, a| add_alias(m, &to.id, a) | c);
                if m["played_in"].is_null() && f["played_in"].is_string() {
                    m["played_in"] = f["played_in"].clone();
                    changed = true;
                }
                // stretches marked "Not the melody" on either are left out of the score of the one kept
                let mine = stretches_of(&m["not_melody"]);
                let theirs = stretches_of(&f["not_melody"]);
                if !theirs.is_empty() {
                    let both = tidy_stretches(&[mine.clone(), theirs].concat());
                    if both != mine {
                        m["not_melody"] = json!(both.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>());
                        changed = true;
                    }
                }
                // its lyrics lined up with the song, when the one kept has none (and whether they were placed by the
                // words heard, which goes with them)
                if m["lyric_times"].is_null() && !f["lyric_times"].is_null() {
                    for k in ["lyric_times", "lyric_check", "lyric_lines", "lyrics", "lyrics_placed"] {
                        if !f[k].is_null() {
                            m[k] = f[k].clone();
                        }
                    }
                    changed = true;
                }
                changed
            });
        }
        // a favourite stays one if either entry was (hearted the latest time either was)
        if let Some(f) = from_meta.filter(|f| f["favorite"].as_bool() == Some(true)) {
            if let Some(mut m) = self.song_meta(&to.id) {
                let at = f["favorited_at"].as_str().unwrap_or("");
                let had = m["favorite"].as_bool() == Some(true);
                if !had || m["favorited_at"].as_str().unwrap_or("") < at {
                    mark_favorite(&mut m, true, at);
                    let _ = write_atomic(&self.song_path(&to.id, "json"), m.to_string().as_bytes());
                }
            }
        }
        let aside = self.root.join("songs").join("merged");
        let _ = std::fs::create_dir_all(&aside);
        for ext in ["shsig", "syl.json", "words"] {
            if !self.song_path(&to.id, ext).exists() {
                let _ = std::fs::rename(self.song_path(from, ext), self.song_path(&to.id, ext));
            }
        }
        for ext in ["bin", "json", "syl.json", "shsig", "words"] {
            let p = self.song_path(from, ext);
            if p.exists() {
                let _ = std::fs::rename(&p, aside.join(p.file_name().unwrap_or_default()));
            }
        }
        if !cfg!(test) {
            crate::engine::log(&format!("{} - {}: kept as {} was merged into {}{}", to.artist, to.name, from, to.id,
                                        if kept { "" } else { " (it couldn't be lined up: the fuller one kept)" }));
        }
    }

    /// One-off: songs kept twice (the same ISRC, or the same artist and title under two ids, learned before songs
    /// were kept under one id) are merged into one. Returns how many entries were merged away.
    pub fn merge_duplicates(&self) -> usize {
        let before = self.learned_tracks().len();
        let mut done: Vec<String> = Vec::new();
        for t in self.learned_tracks() {
            if done.contains(&t.id) || !self.song_path(&t.id, "json").exists() {
                continue;
            }
            done.push(self.canonical(&t).id);
        }
        before - self.learned_tracks().len()
    }

    /// Every learned song's track (id, name, artist, …) as saved with it.
    pub fn learned_tracks(&self) -> Vec<Track> {
        std::fs::read_dir(self.root.join("songs")).into_iter().flatten().flatten()
            .filter(|e| is_song_meta(&e.path()))
            .filter_map(|e| serde_json::from_slice::<Value>(&std::fs::read(e.path()).ok()?).ok())
            .filter_map(|m| serde_json::from_value::<Track>(m["track"].clone()).ok())
            .collect()
    }

    /// Delete a take (to the Trash, so it can be recovered). Not a take id: refused; already gone: done; can't be
    /// moved to the Trash: an error, and it stays.
    pub fn delete_take(&self, id: &str) -> Result<(), String> {
        if !valid_take_id(id) {
            return Err("not a take".into());
        }
        self.take_dir(id).map_or(Ok(()), |d| to_trash(&d))
    }

    /// Syllable bars for a learned song, if made: (start frame, end frame, MIDI note, syllable), song frames.
    /// The syllables' words aren't kept, only where each sits in the song's lyrics, so `lyrics` (fetched again
    /// from LRCLIB) gives them back; without them, or if the lyrics have changed since, the bars come back with
    /// no words. Songs learned before this kept the words themselves, and those are read as they are.
    ///
    /// LRCLIB doesn't always answer with the same lyrics (another entry for the song, or a search when a
    /// rate-limited lookup failed): then each line is found by its own check (kept since), and for songs saved
    /// before that, the lines' syllables are put back on the bars sung in each line's time (`lyrics` already
    /// carrying the times lined up with the singer, see `lyrics_on_kept_times`), where they number the same.
    pub fn syllables(&self, id: &str, lyrics: Option<&[(f64, String)]>) -> Option<Vec<(i64, i64, i32, String)>> {
        let v: Value = serde_json::from_slice(&std::fs::read(self.song_path(id, "syl.json")).ok()?).ok()?;
        let same = lyrics.is_some_and(|l| v["check"].as_str() == Some(lyrics_check(l).as_str()));
        // the line each kept line is in these lyrics, when it isn't the same lyrics
        let moved: Option<Vec<Option<usize>>> = match (same, lyrics, v["lines"].as_array()) {
            (false, Some(l), Some(kept)) => Some(match_lines(&kept.iter().map(|h| h.as_str().unwrap_or("").to_string()).collect::<Vec<_>>(), l)),
            _ => None,
        };
        let mut bars: Vec<(i64, i64, i32, String)> = v["bars"].as_array()?.iter().filter_map(|b| {
            let text = match &b[3] {
                Value::String(t) => t.clone(),
                Value::Array(span) => lyrics.filter(|_| same || moved.is_some()).and_then(|l| {
                    let (line, start, len) = (span.first()?.as_u64()? as usize, span.get(1)?.as_u64()? as usize, span.get(2)?.as_u64()? as usize);
                    let line = if same { line } else { (*moved.as_ref()?.get(line)?)? };
                    l.get(line)?.1.get(start..start + len).map(String::from)
                }).unwrap_or_default(),
                _ if b[4].as_bool() == Some(true) => "~".to_string(),
                _ => String::new(),
            };
            Some((b[0].as_i64()?, b[1].as_i64()?, b[2].as_i64()? as i32, text))
        }).filter(|b| b.1 > b.0).collect();
        // bars whose syllable wasn't found where it sits (another entry; or, for songs saved before syllable_spans
        // went by the bars' times, lost to a syllable sung over the next line's first): their line's syllables
        if let Some(l) = lyrics.filter(|_| bars.iter().any(|b| b.3.trim().is_empty())) {
            crate::syllables::relabel(&mut bars, l);
        }
        (bars.len() >= 10).then_some(bars)
    }

    /// Keep syllable bars with where each syllable sits in `lyrics` (line, byte start, byte length), not its words.
    pub fn save_syllables(&self, id: &str, bars: &[crate::syllables::SyllableBar], lyrics: &[(f64, String)]) {
        let _writing = writing();
        let spans = syllable_spans(bars, lyrics);
        let v = json!({"check": lyrics_check(lyrics), "lines": line_checks(lyrics), "bars": bars.iter().zip(&spans).map(|(b, s)| {
            let held = b.3.trim() == "~";
            json!([b.0, b.1, b.2, s.map(|(l, a, n)| json!([l, a, n])).unwrap_or(Value::Null), held])
        }).collect::<Vec<_>>()});
        if let Err(e) = write_atomic(&self.song_path(id, "syl.json"), v.to_string().as_bytes()) {
            if !cfg!(test) {
                crate::engine::log(&format!("{id}: its syllable bars couldn't be saved: {e}"));
            }
        }
    }

    /// When each lyric word was heard sung, from the word timer's whole-song alignment of the separated singer when
    /// the song was learned (songs/<id>.words, JSON): times and places only, never the words. `heard` is the aligner's
    /// letters per line of `lyrics` (word, letter, start s, end s), `believed` the lines that placed the lyrics (see
    /// syllables::check_placement). Kept so a play's lyrics can be checked against the singing without a rerun.
    pub fn save_words(&self, id: &str, lyrics: &[(f64, String)], heard: &crate::syllables::Heard, believed: &crate::syllables::Heard) {
        let mut words: Vec<Value> = Vec::new();
        for (line, h) in heard.iter().enumerate() {
            let Some(letters) = h else { continue };
            let kept = believed.get(line).is_some_and(|b| b.is_some());
            let mut by_word: Vec<(usize, f64, f64)> = Vec::new();
            for &(w, _, a, b) in letters {
                match by_word.iter_mut().find(|x| x.0 == w) {
                    Some(x) => { x.1 = x.1.min(a); x.2 = x.2.max(b); }
                    None => by_word.push((w, a, b)),
                }
            }
            by_word.sort_by_key(|x| x.0);
            let r = |x: f64| (x * 1000.0).round() / 1000.0;
            words.extend(by_word.into_iter().map(|(w, a, b)| json!([line, w, r(a), r(b), kept as u8])));
        }
        let v = json!({"v": 1, "check": lyrics_check(lyrics), "lines": lyrics.len(), "made": now_iso(),
                       "fields": ["line", "word", "start_s", "end_s", "believed"], "words": words});
        let _ = write_atomic(&self.song_path(id, "words"), v.to_string().as_bytes());
    }

    /// The heard words kept with a song (see `save_words`).
    pub fn words(&self, id: &str) -> Option<Value> {
        serde_json::from_slice(&std::fs::read(self.song_path(id, "words")).ok()?).ok()
    }

    /// Lyrics lined up with the singer, kept with the song once it has been learned: only their times (and a
    /// check of the words they were made for), not the words, which come from LRCLIB each time. Songs learned
    /// before this kept the words, and those are given back as they are.
    pub fn kept_lyrics(&self, id: &str) -> Option<KeptLyrics> {
        let m = self.song_meta(id)?;
        if let (Some(times), Some(check)) = (m["lyric_times"].as_array(), m["lyric_check"].as_str()) {
            let lines = m["lyric_lines"].as_array().map(|v| v.iter().map(|h| h.as_str().unwrap_or("").to_string()).collect());
            return Some(KeptLyrics::Times(Kept { times: times.iter().filter_map(|t| t.as_f64()).collect(), check: check.to_string(), lines }));
        }
        serde_json::from_value(m["lyrics"].clone()).ok().map(KeptLyrics::Text)
    }

    /// For the lab's test stores only (scored offline, with no lyrics lookup): keep the words too.
    pub fn save_lyrics_keeping_words(&self, id: &str, lyrics: &[(f64, String)]) {
        let _writing = writing();
        if let Some(mut m) = self.song_meta(id) {
            m["lyrics"] = json!(lyrics);
            if let Some(o) = m.as_object_mut() {
                o.remove("lyric_times");
                o.remove("lyric_check");
            }
            let _ = write_atomic(&self.song_path(id, "json"), m.to_string().as_bytes());
        }
    }

    pub fn save_lyrics(&self, id: &str, lyrics: &[(f64, String)]) {
        let _writing = writing();
        if let Some(mut m) = self.song_meta(id) {
            m["lyric_times"] = json!(lyrics.iter().map(|l| l.0).collect::<Vec<_>>());
            m["lyric_check"] = json!(lyrics_check(lyrics));
            m["lyric_lines"] = json!(line_checks(lyrics));
            if let Some(o) = m.as_object_mut() {
                o.remove("lyrics");
            }
            let _ = write_atomic(&self.song_path(id, "json"), m.to_string().as_bytes());
        }
    }

    /// The album cover kept for a learned song, and its Apple Music page if the cover is Apple's.
    pub fn cover(&self, id: &str) -> Option<crate::music::Cover> {
        let m = self.song_meta(id)?;
        Some(crate::music::Cover { art: m["track"]["art"].as_str()?.to_string(), link: m["track"]["link"].as_str().map(String::from) })
    }

    /// Find covers for learned songs that don't have one yet (songs learned before covers were kept), and the
    /// Apple Music page for Apple covers kept before their links were (an Apple cover is shown linked to it).
    pub fn find_missing_covers(&self) {
        for e in std::fs::read_dir(self.root.join("songs")).into_iter().flatten().flatten() {
            if !is_song_meta(&e.path()) {
                continue;
            }
            let read = || std::fs::read(e.path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            let Some(m) = read() else { continue };
            let art = m["track"]["art"].as_str();
            if art.is_some_and(|a| !is_apple_art(a) || m["track"]["link"].is_string()) {
                continue;
            }
            let had_art = art.is_some();
            // looked up (over the network) outside the store's lock; then the song's file is read again under it,
            // as a take may have saved the song meanwhile, and left alone if its cover changed
            let found = crate::source::cover_for(&m["track"]);
            let _writing = writing();
            let Some(mut m) = read().filter(|now| now["track"]["art"] == m["track"]["art"]) else { continue };
            if let Some(c) = found {
                if !had_art {
                    m["track"]["art"] = json!(c.art);
                }
                if let Some(link) = c.link {
                    m["track"]["link"] = json!(link);
                } else if had_art {
                    continue;
                }
                let _ = write_atomic(&e.path(), m.to_string().as_bytes());
            }
        }
    }

    /// "Learn again" asked for this song: its next play learns it from scratch, as a first play does (the copy
    /// learned before is set aside, not merged into), and replaces it once that play learned enough to sing
    /// (READY_COVERAGE). Until then the old copy is kept, so asking can be taken back.
    pub fn learning_again(&self, id: &str) -> bool {
        self.song_meta(id).is_some_and(|m| m["learn_again"].is_string())
    }

    /// A learned song's "Learn again" state, for its page: asked for, and `relearn` (see note_slips).
    pub fn song_state(&self, id: &str) -> Value {
        let m = self.song_meta(id).unwrap_or(Value::Null);
        json!({"learned": !m.is_null(), "learn_again": m["learn_again"].is_string(), "relearn": m["relearn"]})
    }

    /// Ask for a song to be learned again on its next play (`on`), or take that back.
    pub fn set_learn_again(&self, id: &str, on: bool) -> Result<(), String> {
        if !valid_song_id(id) {
            return Err("not a song".into());
        }
        let _writing = writing();
        let mut m = self.song_meta(id).ok_or("not a learned song")?;
        if on {
            m["learn_again"] = json!(now_iso());
        } else if let Some(o) = m.as_object_mut() {
            o.remove("learn_again");
        }
        write_atomic(&self.song_path(id, "json"), m.to_string().as_bytes()).map_err(|e| e.to_string())
    }

    /// Jumps that blamed this song's learned copy on one play (listen::take_slips: where, by how much), kept with
    /// it, the last few plays' worth. The same jump on two plays means the copy is off there: the song is marked
    /// `relearn` (where and by how much), returned. A new learn writes the record afresh, without either.
    pub fn note_slips(&self, id: &str, slips: &[(f64, f64)]) -> Option<(f64, f64)> {
        if slips.is_empty() {
            return None;
        }
        let _writing = writing();
        let mut m = self.song_meta(id)?;
        let mut plays: Vec<Vec<(f64, f64)>> = serde_json::from_value(m["slips"].clone()).unwrap_or_default();
        plays.push(slips.to_vec());
        let n = plays.len();
        if n > 5 {
            plays.drain(..n - 5);
        }
        let again = repeated_slip(&plays);
        m["slips"] = json!(plays);
        if let Some((at, by)) = again {
            m["relearn"] = json!({"at": (at * 10.0).round() / 10.0, "by": (by * 100.0).round() / 100.0, "since": now_iso()});
        }
        let _ = write_atomic(&self.song_path(id, "json"), m.to_string().as_bytes());
        again
    }

    /// A learned song was just played (sung or learned): it goes first in the library.
    pub fn played(&self, id: &str) {
        let _writing = writing();
        if let Some(mut m) = self.song_meta(id) {
            m["played"] = json!(now_iso());
            let _ = write_atomic(&self.song_path(id, "json"), m.to_string().as_bytes());
        }
    }

    /// Learned songs, the last played or learned first, with your best score from your takes and how many there are.
    /// The best is like with like: of the takes scored at the level of your latest one (`best_level`).
    pub fn library(&self) -> Vec<Value> {
        let usual = self.usual_app();
        let mut best: std::collections::HashMap<String, i64> = Default::default();
        let mut best_level: std::collections::HashMap<String, String> = Default::default();
        let mut last: std::collections::HashMap<String, String> = Default::default();
        let mut sung: std::collections::HashMap<String, u32> = Default::default();
        for t in self.takes() {
            let id = t["track"]["id"].as_str().unwrap_or("").to_string();
            if let Some(r) = t["recorded"].as_str() {
                let e = last.entry(id.clone()).or_default();
                if r > e.as_str() {
                    *e = r.to_string();
                }
            }
            if t["coverage"].as_f64().unwrap_or(0.0) >= 0.1 {
                *sung.entry(id.clone()).or_default() += 1;
                // takes come newest first: the first one sets the level the best is taken at
                let level = t["level"].as_str().unwrap_or("relaxed");
                if best_level.entry(id.clone()).or_insert_with(|| level.to_string()) != level {
                    continue;
                }
                let s = t["score"].as_i64().unwrap_or(0);
                let e = best.entry(id).or_insert(s);
                *e = (*e).max(s);
            }
        }
        let mut rows: Vec<Value> = std::fs::read_dir(self.root.join("songs")).into_iter().flatten().flatten()
            .filter(|e| is_song_meta(&e.path()))
            .filter_map(|e| serde_json::from_slice::<Value>(&std::fs::read(e.path()).ok()?).ok())
            .filter(|m| m["track"]["id"].is_string())
            .map(|m| {
                let id = m["track"]["id"].as_str().unwrap_or("").to_string();
                let frac = m["fraction"].as_f64().unwrap_or(0.0);
                // the last thing that happened to it: learned, played, or a take of it
                let at = [m["learned"].as_str(), m["played"].as_str(), last.get(&id).map(|s| s.as_str())]
                    .into_iter().flatten().max().unwrap_or("").to_string();
                json!({"track": m["track"], "learned": m["learned"], "fraction": frac, "difficulty": self.difficulty(&id), "takes": sung.get(&id).copied().unwrap_or(0),
                       "ready": frac >= READY_COVERAGE, "best": best.get(&id), "best_level": best_level.get(&id), "active": at,
                       "favorite": m["favorite"].as_bool().unwrap_or(false), "favorited_at": m["favorited_at"],
                       "learn_again": m["learn_again"].is_string(), "relearn": m["relearn"],
                       "app": pick_app(&id, &aliases(&m), m["played_in"].as_str().and_then(app_of_key), usual, None).app.name()})
            }).collect();
        rows.sort_by(|a, b| b["active"].as_str().unwrap_or("").cmp(a["active"].as_str().unwrap_or("")));
        rows
    }

    /// How hard a learned song is to sing, 1 (easy) to 5 (very hard), from its notes (`difficulty`).
    pub fn difficulty(&self, id: &str) -> Option<u8> {
        difficulty(&self.song_bars(id)?)
    }

    /// A learned song's syllable bars (start and end in 10 ms song frames, MIDI note), if it has them.
    pub fn song_bars(&self, id: &str) -> Option<Vec<(i64, i64, i64)>> {
        let v: Value = serde_json::from_slice(&std::fs::read(self.song_path(id, "syl.json")).ok()?).ok()?;
        Some(v["bars"].as_array()?.iter().filter_map(|b| Some((b[0].as_i64()?, b[1].as_i64()?, b[2].as_i64()?))).collect())
    }

    /// Whether the folder of takes (and, with `songs`, of learned songs) can be read: an empty list from one that
    /// can't is "couldn't read your takes", not "no takes yet". A folder not there (nothing saved yet) is fine.
    pub fn readable(&self, songs: bool) -> Result<(), String> {
        let dirs = [Some(("takes", self.takes_dir())), songs.then(|| ("songs", self.root.join("songs")))];
        for (what, d) in dirs.into_iter().flatten() {
            match std::fs::read_dir(&d) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    if !cfg!(test) {
                        crate::engine::log(&format!("couldn't read {}: {e}", d.display()));
                    }
                    return Err(format!("couldn't read the {what} folder ({e})"));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Every take's summary, newest first.
    pub fn takes(&self) -> Vec<Value> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(self.takes_dir()).into_iter().flatten().flatten()
            .map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        dirs.reverse();
        dirs.iter().filter_map(|d| take_summary(d)).map(|mut t| {
            // the learned song's cover (and Apple Music page) when the take has none, or the page for the same cover
            if !t["track"]["link"].is_string() {
                if let Some(c) = t["track"]["id"].as_str().and_then(|id| self.cover(id)) {
                    let art = t["track"]["art"].as_str();
                    if art.is_none() || art == Some(c.art.as_str()) {
                        t["track"]["art"] = json!(c.art);
                        if let Some(link) = c.link {
                            t["track"]["link"] = json!(link);
                        }
                    }
                }
            }
            t
        }).collect()
    }

    /// A take's folder, takes/<id>, if `id` is a take id (`valid_take_id`) and it's there. Every IPC command that
    /// names a take (take, take_audio, delete_take) comes through here.
    pub fn take_dir(&self, id: &str) -> Option<PathBuf> {
        let d = self.takes_dir().join(id);
        (valid_take_id(id) && d.is_dir()).then_some(d)
    }

    pub fn take_result(&self, id: &str) -> Option<Value> {
        let d = self.take_dir(id)?;
        let mut r: Value = serde_json::from_slice(&std::fs::read(d.join("result.json")).ok()?).ok()?;
        // where in the song the recording starts, for playing it back with the song
        if let Some(m) = std::fs::read(d.join("meta.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
            r["song_offset"] = m["song_offset"].clone();
            // (and, a take with a jump in it, each stretch's own place: Take::stretches)
            r["places"] = m["places"].clone();
        }
        // every take's recording is kept, up to a cap for them all (takeaudio::retain): past it the oldest go
        r["audio_kept"] = json!(crate::takeaudio::recording(&d).is_some());
        Some(r)
    }
}

/// The app's word in a song's .json ("played_in") and a take's result ("app").
pub fn app_key(app: Source) -> Option<&'static str> {
    match app {
        Source::Spotify => Some("spotify"),
        Source::Music => Some("music"),
        Source::YouTube => Some("youtube"),
        Source::Other | Source::Demo => None,
    }
}

fn app_of_key(k: &str) -> Option<Source> {
    match k {
        "spotify" => Some(Source::Spotify),
        "music" => Some(Source::Music),
        "youtube" => Some(Source::YouTube),
        _ => None,
    }
}

/// The ids a song is known by in the apps (Spotify's, Apple Music's, YouTube's), the latest first.
fn aliases(meta: &Value) -> Vec<String> {
    meta["aliases"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Keep `alias` as an id of the song kept as `id`, if it's an app's own id (not an ISRC or Shazam's) and new.
fn add_alias(meta: &mut Value, id: &str, alias: &str) -> bool {
    let mut a = aliases(meta);
    if alias.is_empty() || alias == id || Source::of_id(alias) == Source::Other || a.iter().any(|x| x == alias) {
        return false;
    }
    a.insert(0, alias.to_string());
    meta["aliases"] = json!(a);
    true
}

/// The app with the most takes (each take's app, else its track id's); ties go to the latest take's.
fn usual_app(takes: &[Value]) -> Option<Source> {
    let mut n = [0usize; 3];
    let mut first: Option<Source> = None;
    for t in takes {
        let app = t["app"].as_str().and_then(app_of_key).unwrap_or_else(|| Source::of_id(t["track"]["id"].as_str().unwrap_or("")));
        if let Some(i) = Source::ALL.iter().position(|s| *s == app) {
            n[i] += 1;
            first.get_or_insert(app);
        }
    }
    let most = *n.iter().max()?;
    if most == 0 {
        return None;
    }
    first.filter(|f| n[Source::ALL.iter().position(|s| s == f).unwrap_or(0)] == most)
        .or_else(|| Source::ALL.iter().zip(n).find(|(_, k)| *k == most).map(|(s, _)| *s))
}

/// A song's syllable bars are too few when they span under half of where it's sung: its lyrics' span (first to last
/// line) when known, else its length (`duration` s; unknown: any bars do). A short play once replaced whole songs'
/// bars with its own (Shake It Off, 19 Oct: 21 s of a 4-minute song), and a learned song's plays are sung, not
/// learned, so only a play that makes bars when they're this thin repairs it. (By the lyrics' span, a song with a
/// long instrumental end isn't separated again on every play.)
pub fn thin_bars(bars: &[crate::syllables::SyllableBar], duration: f64, lyrics: Option<(f64, f64)>) -> bool {
    let (Some(a), Some(b)) = (bars.iter().map(|b| b.0).min(), bars.iter().map(|b| b.1).max()) else { return true };
    let sung = lyrics.map(|(f, l)| l - f).filter(|s| *s > 10.0).unwrap_or(duration);
    sung > 0.0 && ((b - a) as f64 * crate::dsp::FRAME_S) < 0.5 * sung
}

/// A play's track, keeping what the song already knew where the play knew less: a play from YouTube (a video's
/// title, thumbnail and length, no Apple Music page) or one named without a length mustn't replace a
/// Spotify-learned song's name, cover, page or length. `*_guessed`: the length is a guess (how far a play got,
/// when Apple's lookup failed, or a video's), not the app's or Apple's figure; `new_youtube`: the play was a
/// YouTube video. Returns the track and whether its length is a guess.
///  - length: an exact one wins (the play's over the song's); between two guesses the longer (the shorter is a
///    play cut short: The Kill, 19 Oct, paused at 3:17, became 3:17 long and "116% learned")
///  - name, artist, album: a video's don't replace a song's from an app (Shazam, Spotify, Music); otherwise the
///    play's, so a renamed video or the app's name for a song first learned from YouTube take over; a blank one
///    never replaces a known one
///  - cover and page: the song's Apple cover and page over a play's other cover
pub fn keep_known(old: &Track, old_guessed: bool, mut new: Track, new_guessed: bool, new_youtube: bool) -> (Track, bool) {
    let mut guessed = new_guessed;
    if old.duration > 0.0 && (new.duration <= 0.0 || (new_guessed && (!old_guessed || old.duration > new.duration))) {
        new.duration = old.duration;
        guessed = old_guessed;
    }
    let old_youtube = old.id.starts_with(crate::youtube::ID_PREFIX) || old.art.as_deref().is_some_and(|a| a.contains("ytimg.com"));
    let keep_names = new_youtube && !old_youtube;
    for (o, n) in [(&old.name, &mut new.name), (&old.artist, &mut new.artist), (&old.album, &mut new.album)] {
        if !o.trim().is_empty() && (keep_names || n.trim().is_empty()) {
            *n = o.clone();
        }
    }
    let apple = |t: &Track| t.art.as_deref().is_some_and(is_apple_art);
    if apple(old) && !apple(&new) {
        new.art = old.art.clone();
        new.link = old.link.clone().or(new.link);
    } else if new.link.is_none() {
        new.link = old.link.clone();
    }
    (new, guessed)
}

thread_local! {
    /// What the play being saved on this thread knew less well (see keep_known): (length guessed, a YouTube
    /// video). Each play is scored and saved on its own thread, so a note can't outlive its play or reach another.
    static PLAY_NOTE: std::cell::Cell<(bool, bool)> = const { std::cell::Cell::new((false, false)) };
}

/// The play this thread is about to save has a guessed length and/or is a YouTube video (see keep_known).
pub fn note_play(length_guessed: bool, youtube: bool) {
    PLAY_NOTE.with(|n| {
        let (g, y) = n.get();
        n.set((g || length_guessed, y || youtube));
    });
}

fn take_play_note() -> (bool, bool) {
    PLAY_NOTE.with(|n| n.replace((false, false)))
}

/// Where a song opens: the app, and its id there when known (a Spotify track id, an Apple Music or YouTube one).
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub app: Source,
    pub id: Option<String>,
}

/// Which app to open a song in (`id`: the id it's kept under; `aliases`: its ids in the apps; `played_in`: the app
/// it was last played in; `usual`: the app you sing from most; `running`: the app being listened to now):
/// Spotify by its track id when it was last played there or Spotify is your usual app; else the app it was last
/// played in, your usual app, or the app running, in that order; else Spotify (a search). Never Apple Music
/// unless you've played something there.
pub fn pick_app(id: &str, aliases: &[String], played_in: Option<Source>, usual: Option<Source>, running: Option<Source>) -> Pick {
    let ids: Vec<&str> = std::iter::once(id).chain(aliases.iter().map(|a| a.as_str())).collect();
    let id_in = |app: Source| ids.iter().find(|i| Source::of_id(i) == app).map(|i| i.to_string());
    let real = |s: Option<Source>| s.filter(|s| *s != Source::Other);
    // a song kept by an app's own id was played there
    let played_in = real(played_in).or(real(Some(Source::of_id(id))));
    let spotify = id_in(Source::Spotify);
    if spotify.is_some() && (played_in == Some(Source::Spotify) || usual == Some(Source::Spotify)) {
        return Pick { app: Source::Spotify, id: spotify };
    }
    match played_in.or(real(usual)).or(real(running)) {
        Some(app) => Pick { app, id: id_in(app) },
        None => Pick { app: Source::Spotify, id: spotify },
    }
}

/// How a song is started from OnKey: played in `app` by `id`, or not (`NoId`: OnKey knows no id for it in `app`,
/// the app it opens in, nor in another app that's running, so there's nothing to tell an app to play).
#[derive(Debug, Clone, PartialEq)]
pub enum PlayPlan {
    Start { app: Source, id: String },
    NoId { app: Source },
}

/// Which app starts a song and by which id. The app it opens in (`pick`, as its "in Apple Music ↗" link says) by its
/// id there, whatever the id it was asked by belongs to: a song learned from Spotify and last played in Apple Music
/// starts in Apple Music, as its link says (it was started in Spotify while the link said Apple Music). With no id
/// there (learned from Spotify, or only heard), another app's id when that app is running (`open`); else `NoId`,
/// and the window says so and offers to open it in its app. `ids`: the id asked by, the one it's kept under, its aliases.
pub fn play_plan(ids: &[String], pick: &Pick, open: impl Fn(Source) -> bool) -> PlayPlan {
    if let Some(id) = &pick.id {
        return PlayPlan::Start { app: pick.app, id: id.clone() };
    }
    let other = ids.iter().find(|i| {
        let a = Source::of_id(i);
        a != Source::Other && a != pick.app && open(a)
    });
    match other {
        Some(id) => PlayPlan::Start { app: Source::of_id(id), id: id.clone() },
        None => PlayPlan::NoId { app: pick.app },
    }
}

/// Set a song's heart in its .json, and when it was given (kept, not cleared, when it's taken away).
fn mark_favorite(meta: &mut Value, on: bool, at: &str) {
    meta["favorite"] = json!(on);
    if on {
        meta["favorited_at"] = json!(at);
    }
}

/// What the window shows when a delete couldn't move something to the Trash (nothing was deleted).
pub const TRASH_FAILED: &str = "Couldn't move it to the Trash, so nothing was deleted.";

/// Move a file or folder to the user's Trash (~/.Trash), named so it doesn't clash. Never deleted outright: if
/// the move fails it stays where it is and this says so (a failed move once fell through to deleting it).
fn to_trash(p: &Path) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| TRASH_FAILED.to_string())?;
    trash_into(p, &Path::new(&home).join(".Trash"))
}

/// `to_trash` into `trash` (a test's folder). Nothing there: nothing to do.
fn trash_into(p: &Path, trash: &Path) -> Result<(), String> {
    if !p.exists() {
        return Ok(());
    }
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let dest = trash.join(format!("OnKey {} {name}", now_iso().replace(':', ".")));
    std::fs::rename(p, &dest).map_err(|e| {
        if !cfg!(test) {
            crate::engine::log(&format!("couldn't move {} to the Trash ({e}): left where it is", p.display()));
        }
        TRASH_FAILED.to_string()
    })
}

/// songs/<track>.json (not the .syl.json syllable files beside it)
/// Two tracks with the same artist and title (letters and digits, any case): the same song under two ids.
pub fn same_title(a: &Track, b: &Track) -> bool {
    // the demo song is its own entry: never merged with (or into) a song of the same name learned from an app
    if crate::demo::is_demo(&a.id) || crate::demo::is_demo(&b.id) {
        return false;
    }
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    !norm(&a.name).is_empty() && norm(&a.name) == norm(&b.name) && same_artist(&a.artist, &b.artist)
}

/// The same artist, as apps credit a song differently: one's artists are all among the other's (Spotify credits
/// Shallow to "Lady Gaga", Apple Music to "Lady Gaga & Bradley Cooper": it was learned again from Spotify).
pub fn same_artist(a: &str, b: &str) -> bool {
    let names = |s: &str| -> Vec<String> {
        let s = s.to_lowercase();
        let s = [" feat. ", " feat ", " ft. ", " with ", " x ", " and ", " & ", ", "].iter().fold(s, |s, sep| s.replace(sep, "|"));
        s.split('|').map(|n| n.chars().filter(|c| c.is_alphanumeric()).collect::<String>()).filter(|n| !n.is_empty()).collect()
    };
    let (na, nb) = (names(a), names(b));
    let whole = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    if whole(a) == whole(b) {
        return true;    // (as before: "Simon & Garfunkel" is one name either way)
    }
    let (short, long) = if na.len() <= nb.len() { (na, nb) } else { (nb, na) };
    !short.is_empty() && short.iter().all(|n| long.contains(n))
}

fn is_song_meta(p: &Path) -> bool {
    p.extension().is_some_and(|x| x == "json") && !p.to_string_lossy().ends_with(".syl.json")
}

/// A cover image from Apple (iTunes Search or Shazam's artwork), which is shown linked to its Apple Music page.
fn is_apple_art(url: &str) -> bool {
    url.strip_prefix("https://").and_then(|r| r.split('/').next()).is_some_and(|host| host.ends_with(".mzstatic.com"))
}

/// Stretches (s) as JSON kept them: [[from, to], …].
fn stretches_of(v: &Value) -> Vec<(f64, f64)> {
    v.as_array().map(|a| a.iter().filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?))).collect()).unwrap_or_default()
}

/// Stretches of a song (s) in order, none before 0 or shorter than a tenth of a second, overlapping or touching
/// ones made one.
pub fn tidy_stretches(x: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = x.iter().filter(|(a, b)| a.is_finite() && b.is_finite())
        .map(|&(a, b)| (a.min(b).max(0.0), a.max(b).max(0.0))).filter(|(a, b)| b - a >= 0.1).collect();
    v.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(l) if a <= l.1 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Takes' summaries as last made, by folder: (result.json's time, the level shown, the summary). Scoring a take at
/// another level takes a few ms, so a long history is scored once per level change, not on every look.
static SUMMARIES: std::sync::Mutex<Option<std::collections::HashMap<PathBuf, (std::time::SystemTime, &'static str, Value)>>> =
    std::sync::Mutex::new(None);

/// A take's summary, scored at the level chosen in Settings (analysis::at_level: a take sung at another level is
/// scored again, so every score shown is at one level; it keeps "sung_level" and "sung_score").
fn take_summary(d: &Path) -> Option<Value> {
    let level = crate::dsp::Level::current();
    let when = std::fs::metadata(d.join("result.json")).and_then(|m| m.modified()).ok()?;
    if let Some((w, l, v)) = SUMMARIES.lock().unwrap().as_ref().and_then(|c| c.get(d)) {
        if *w == when && *l == level.name() {
            return Some(v.clone());
        }
    }
    let v = take_summary_at(d, level)?;
    SUMMARIES.lock().unwrap().get_or_insert_with(Default::default).insert(d.to_path_buf(), (when, level.name(), v.clone()));
    Some(v)
}

fn take_summary_at(d: &Path, level: crate::dsp::Level) -> Option<Value> {
    let mut r: Value = serde_json::from_slice(&std::fs::read(d.join("result.json")).ok()?).ok()?;
    crate::analysis::at_level(&mut r, level);
    Some(json!({"id": d.file_name()?.to_string_lossy(), "track": r["track"], "recorded": r["recorded"],
        "score": r["score"], "accuracy": r["accuracy"], "coverage": r["coverage"], "within50": r["within50"],
        "bias_cents": r["bias_cents"], "latency_ms": r["latency_ms"], "live_score": r["live"]["score"],
        "repeat": r["live"]["cached"].as_bool().unwrap_or(false), "mode": r["mode"], "blind": r["blind"],
        "notes_hit": r["notes_hit"], "notes_total": r["notes_total"], "app": r["app"], "rescored": r["rescored"],
        "level": crate::dsp::Level::parse(r["level"].as_str()).name(), "sung_level": r["sung_level"], "sung_score": r["sung_score"]}))
}

pub fn now_iso() -> String {
    chrono_like(std::time::SystemTime::now())
}

/// Local time as 2026-10-13T09:15:00 (via the C library, to avoid a date crate).
pub fn chrono_like(t: std::time::SystemTime) -> String {
    format_time(t, "%Y-%m-%dT%H:%M:%S")
}

pub fn format_time(t: std::time::SystemTime, fmt: &str) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    unsafe {
        let mut tm: libc_tm = std::mem::zeroed();
        localtime_r(&secs, &mut tm);
        let mut buf = [0u8; 64];
        let f = std::ffi::CString::new(fmt).unwrap();
        let n = strftime(buf.as_mut_ptr() as *mut i8, buf.len(), f.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

#[repr(C)]
struct libc_tm {
    tm_sec: i32, tm_min: i32, tm_hour: i32, tm_mday: i32, tm_mon: i32, tm_year: i32, tm_wday: i32, tm_yday: i32,
    tm_isdst: i32, tm_gmtoff: i64, tm_zone: *const i8,
}

extern "C" {
    fn localtime_r(t: *const i64, out: *mut libc_tm) -> *mut libc_tm;
    fn strftime(s: *mut i8, max: usize, fmt: *const i8, tm: *const libc_tm) -> usize;
}

pub fn fit<T: Clone>(x: &[T], n: usize, fill: T) -> Vec<T> {
    let mut out = vec![fill; n];
    let k = n.min(x.len());
    out[..k].clone_from_slice(&x[..k]);
    out
}

/// x moved earlier by sh frames (later if negative), keeping its length.
pub fn shift<T: Clone>(x: &[T], sh: i64, fill: T) -> Vec<T> {
    let n = x.len() as i64;
    (0..n).map(|i| {
        let j = i + sh;
        if j >= 0 && j < n { x[j as usize].clone() } else { fill.clone() }
    }).collect()
}

/// Frames by which a newly learned part sits later than the stored song, checked where the two overlap
/// (searching ±5 s). Some(0) when they don't overlap enough to check; None when they overlap but the
/// fingerprint can't confirm the placement.
fn fingerprint_shift(new_env: &[[f32; FP_BANDS]], old_env: &[[f32; FP_BANDS]], new_cov: &[bool], old_cov: &[bool]) -> Option<i64> {
    let n = new_env.len().min(new_cov.len());
    let old_cov = fit(old_cov, old_env.len(), false);
    let overlap = |i: usize| new_cov[i] && old_cov.get(i).copied().unwrap_or(false);
    // the longest stretch both cover; probe 10 s from inside it
    let (mut best, mut run_start, mut run) = ((0usize, 0usize), 0usize, 0usize);
    for i in 0..n {
        if overlap(i) {
            if run == 0 {
                run_start = i;
            }
            run += 1;
            if run > best.1 {
                best = (run_start, run);
            }
        } else {
            run = 0;
        }
    }
    if best.1 < 1200 {
        return Some(0);
    }
    let a = best.0 + 100;
    let probe = &new_env[a..a + 1000];
    let masked: Vec<[f32; FP_BANDS]> = old_env.iter().zip(&old_cov).map(|(r, c)| if *c { *r } else { [0.0; FP_BANDS] }).collect();
    let (k, conf) = locate(probe, &masked, a as i64 - 500, Some(a as i64 + 501));
    (conf > 1.3).then_some(a as i64 - k)
}

/// A learned song's lyrics as kept: their times only (with a check of the words), or, for songs learned before
/// that, the lyrics themselves.
pub enum KeptLyrics {
    Times(Kept),
    Text(Vec<(f64, String)>),
}

/// A learned song's lyric line times, with a check of the words they were made for (all of them, and each line's).
#[derive(Clone, Debug)]
pub struct Kept {
    pub times: Vec<f64>,
    pub check: String,
    /// each line's check (songs learned before these were kept have none)
    pub lines: Option<Vec<String>>,
}

/// Fetched lyrics (`fetched`, LRCLIB's times) on the times kept for a learned song. The same lyrics: the kept times.
/// Otherwise (LRCLIB answered with another entry), each line found among the kept ones by its own check takes that
/// line's time, or, for songs kept before lines had checks, each line the kept times clearly pair with (in order,
/// LRCLIB's timing moved as a whole, within 1 s). Lines not paired move with the nearest paired ones. None when
/// fewer than half the sung lines pair up: other lyrics than the song was learned with.
pub fn lyrics_on_kept_times(kept: &Kept, fetched: &[(f64, String)]) -> Option<Vec<(f64, String)>> {
    if kept.times.len() == fetched.len() && lyrics_check(fetched) == kept.check {
        return Some(fetched.iter().zip(&kept.times).map(|(l, t)| (*t, l.1.clone())).collect());
    }
    let pairs: Vec<Option<usize>> = match &kept.lines {
        Some(lines) => {
            let mut inv = vec![None; fetched.len()];
            for (k, j) in match_lines(lines, fetched).into_iter().enumerate() {
                if let Some(j) = j {
                    inv[j] = Some(k);
                }
            }
            inv
        }
        None => pair_by_time(&kept.times, fetched),
    };
    let sung = fetched.iter().filter(|l| !l.1.trim().is_empty()).count();
    let paired: Vec<(usize, f64)> = pairs.iter().enumerate().filter_map(|(i, k)| Some((i, kept.times.get((*k)?)? - fetched[i].0))).collect();
    if sung == 0 || paired.len() * 2 < sung {
        return None;
    }
    let mut out: Vec<(f64, String)> = fetched.iter().enumerate().map(|(i, (t, text))| {
        let t = match pairs[i].and_then(|k| kept.times.get(k)) {
            Some(k) => *k,
            None => {
                let mut near = paired.clone();
                near.sort_by_key(|(j, _)| j.abs_diff(i));
                let mut d: Vec<f32> = near.iter().take(3).map(|x| x.1 as f32).collect();
                t + crate::dsp::median(&mut d) as f64
            }
        };
        (t, text.clone())
    }).collect();
    for i in 1..out.len() {
        if out[i].0 < out[i - 1].0 {
            out[i].0 = out[i - 1].0;
        }
    }
    Some(out)
}

/// How a learned song's play got its lyrics (see learned_play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LearnedHow {
    /// the entry the kept times were made for: exactly the kept times
    Exact,
    /// another entry, its lines paired with the kept times, and the result fits the singer's phrases (kept with
    /// the song from now on)
    Repaired,
    /// another entry whose pairing doesn't fit the singing as well: shown paired, but not kept
    Unchecked,
    /// no entry pairs with the kept times: LRCLIB's own timing
    Unfit,
}

/// A learned song's lyrics for one play.
#[derive(Debug, Clone)]
pub struct LearnedPlay {
    /// the lines to show (song s)
    pub lyrics: Vec<(f64, String)>,
    /// the LRCLIB entry they're from (LRCLIB's own times): what's pinned for the song's next plays
    pub entry: Vec<(f64, String)>,
    pub how: LearnedHow,
}

/// Share of the sung lines that start within 0.6 s of a phrase start (`starts`, s, sorted) of the learned singing.
fn on_phrases(lyrics: &[(f64, String)], starts: &[f64]) -> f64 {
    let sung: Vec<f64> = lyrics.iter().filter(|l| l.1.chars().any(|c| c.is_alphanumeric())).map(|l| l.0).collect();
    let hit = sung.iter().filter(|&&t| {
        let i = starts.partition_point(|&p| p < t - 0.6);
        starts.get(i).is_some_and(|p| (p - t).abs() <= 0.6)
    }).count();
    hit as f64 / sung.len().max(1) as f64
}

/// A learned song's lyrics for a play, the same every play. `candidates` are LRCLIB entries for the song: the one
/// pinned on this Mac first (lyrics::learned), then LRCLIB's answers. The entry the kept times were made for, if any
/// candidate is it, gets exactly the kept times. Otherwise the first entry whose lines pair with the kept times
/// (lyrics_on_kept_times) is shown paired: kept with the song (Repaired: its times saved and the entry pinned, so
/// later plays are Exact) only when it fits the learned singing (`voiced`, song frames of the learned line) at least
/// nearly as well as the kept times do (and half its lines on phrase starts); a pairing that doesn't is shown but not kept (Unchecked). None pair: the
/// first entry with LRCLIB's timing (Unfit). None when there are no candidates.
/// A jump of the latest play (the last of `plays`) seen on an earlier one too: at the same place (within 3 s) and
/// by the same amount (within 0.25 s). Where and by how much, as the latest play saw it.
fn repeated_slip(plays: &[Vec<(f64, f64)>]) -> Option<(f64, f64)> {
    let (last, before) = plays.split_last()?;
    last.iter().copied().find(|&(at, by)| before.iter().flatten().any(|&(a, b)| (a - at).abs() <= 3.0 && (b - by).abs() <= 0.25))
}

pub fn learned_play(kept: &Kept, candidates: &[Vec<(f64, String)>], voiced: Option<&[bool]>) -> Option<LearnedPlay> {
    if let Some(e) = candidates.iter().find(|e| kept.times.len() == e.len() && lyrics_check(e) == kept.check) {
        let lyrics = e.iter().zip(&kept.times).map(|(l, t)| (*t, l.1.clone())).collect();
        return Some(LearnedPlay { lyrics, entry: e.clone(), how: LearnedHow::Exact });
    }
    for e in candidates {
        if let Some(lyrics) = lyrics_on_kept_times(kept, e) {
            let fits = voiced.is_some_and(|v| {
                let starts = crate::lyrics_align::phrase_starts(v);
                // the kept times as the reference (which of them are sung isn't known without their words)
                let mut k: Vec<(f64, String)> = kept.times.iter().map(|t| (*t, "x".to_string())).collect();
                k.dedup_by(|a, b| (a.0 - b.0).abs() < 0.05);
                let got = on_phrases(&lyrics, &starts);
                got >= 0.5 && got >= on_phrases(&k, &starts) - 0.1
            });
            return Some(LearnedPlay { lyrics, entry: e.clone(), how: if fits { LearnedHow::Repaired } else { LearnedHow::Unchecked } });
        }
    }
    candidates.first().map(|e| LearnedPlay { lyrics: e.clone(), entry: e.clone(), how: LearnedHow::Unfit })
}

/// Each line's own check (FNV-1a of its words, 8 hex digits).
pub fn line_checks(lyrics: &[(f64, String)]) -> Vec<String> {
    lyrics.iter().map(|(_, l)| {
        let mut h: u32 = 0x811c9dc5;
        for b in l.trim().bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(0x01000193);
        }
        format!("{h:08x}")
    }).collect()
}

/// For each kept line (by its check), the line of `lyrics` with the same words, in order (longest common
/// sequence); empty lines aren't paired.
fn match_lines(kept: &[String], lyrics: &[(f64, String)]) -> Vec<Option<usize>> {
    let now = line_checks(lyrics);
    let empty = line_checks(&[(0.0, String::new())])[0].clone();
    let (n, m) = (kept.len(), now.len());
    let mut d = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            d[i][j] = if kept[i] == now[j] && kept[i] != empty { d[i + 1][j + 1] + 1 } else { d[i + 1][j].max(d[i][j + 1]) };
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if kept[i] == now[j] && kept[i] != empty {
            out[i] = Some(j);
            i += 1;
            j += 1;
        } else if d[i + 1][j] >= d[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Songs kept before lines had checks: pair fetched lines with kept times by time alone. LRCLIB's entries for one
/// song differ by a near-constant shift (another release, another transcriber) or slightly in pace: the shift
/// (and a pace within 2%) that pairs the most sung lines within 1 s, each line then paired with the nearest kept
/// time if within 1 s of it, in order. Returns, per fetched line, the kept line it pairs with.
fn pair_by_time(kept: &[f64], fetched: &[(f64, String)]) -> Vec<Option<usize>> {
    let sung: Vec<usize> = (0..fetched.len()).filter(|&i| !fetched[i].1.trim().is_empty()).collect();
    let nearest = |t: f64| -> Option<(usize, f64)> {
        let i = kept.partition_point(|&k| k < t);
        [i.checked_sub(1), Some(i)].into_iter().flatten().filter(|&k| k < kept.len())
            .map(|k| (k, (kept[k] - t).abs())).min_by(|a, b| a.1.total_cmp(&b.1))
    };
    let t0 = sung.first().map_or(0.0, |&i| fetched[i].0);
    let mut best = (0usize, 0.0f64, 0.0f64, 1.0f64);       // (paired, error, shift, pace)
    for pace in [0.98, 0.985, 0.99, 0.995, 1.0, 1.005, 1.01, 1.015, 1.02] {
        let mut shift = -10.0;
        while shift <= 10.0 + 1e-9 {
            let (mut n, mut err) = (0usize, 0.0f64);
            for &i in &sung {
                if let Some((_, e)) = nearest(t0 + (fetched[i].0 - t0) * pace + shift).filter(|x| x.1 <= 1.0) {
                    n += 1;
                    err += e;
                }
            }
            if n > best.0 || (n == best.0 && err < best.1) {
                best = (n, err, shift, pace);
            }
            shift += 0.05;
        }
    }
    let (_, _, shift, pace) = best;
    let mut out = vec![None; fetched.len()];
    let mut last: Option<usize> = None;
    for &i in &sung {
        if let Some((k, _)) = nearest(t0 + (fetched[i].0 - t0) * pace + shift).filter(|x| x.1 <= 1.0) {
            if last.map_or(true, |l| k > l) {
                out[i] = Some(k);
                last = Some(k);
            }
        }
    }
    out
}

/// A short check of the lyrics' words (FNV-1a of the lines), so lyrics fetched again can be matched with the
/// times kept for them.
pub fn lyrics_check(lyrics: &[(f64, String)]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for (i, (_, line)) in lyrics.iter().enumerate() {
        for b in line.bytes().chain((i + 1 < lyrics.len()).then_some(b'\n')) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

/// Where each bar's syllable sits in the lyrics the bars were made from (lined up with the singer): (line, byte
/// start, byte length), or None for a held note ("~") or a syllable that can't be found (its words then aren't
/// shown). Found in order, from where the last one was, in the lines sung around the bar's time (from 2 s before a
/// line to 2 s after the next one starts): found by order alone (in that line or the next three), a line's last
/// syllable sung over the next line's first (bars go by time, so it comes after it) was found lines further on, and
/// lines with no bars (a part not heard) left the rest behind, and every syllable after that went unfound. A
/// syllable sung over the next line's first is found in the line before (or earlier in this one), and the place
/// it's found from stays.
pub fn syllable_spans(bars: &[crate::syllables::SyllableBar], lyrics: &[(f64, String)]) -> Vec<Option<(usize, usize, usize)>> {
    let (mut line, mut pos) = (0usize, 0usize);
    bars.iter().map(|b| {
        let t = b.3.trim();
        if t.is_empty() || t == "~" {
            return None;
        }
        let at = b.0 as f64 * crate::dsp::FRAME_S;
        for (k, l) in lyrics.iter().enumerate().skip(line) {
            if k > line && l.0 > at + 2.0 {
                break;
            }
            if lyrics.get(k + 1).is_some_and(|n| n.0 + 2.0 < at) {
                continue;
            }
            let from = if k == line { pos } else { 0 };
            if let Some(a) = find_ignoring_case(&l.1, t, from) {
                (line, pos) = (k, a + t.len());
                return Some((k, a, t.len()));
            }
        }
        [line.checked_sub(1), Some(line)].into_iter().flatten()
            .find_map(|k| find_ignoring_case(&lyrics.get(k)?.1, t, 0).map(|a| (k, a, t.len())))
    }).collect()
}

fn find_ignoring_case(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    (from..=h.len().checked_sub(n.len())?)
        .find(|&i| hay.is_char_boundary(i) && hay.is_char_boundary(i + n.len()) && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

#[cfg(test)]
mod tests {
    #[test]
    fn bars_spanning_under_half_the_song_are_too_few() {
        let bar = |a: i64, b: i64| (a, b, 60, "la".to_string());
        assert!(thin_bars(&[], 240.0, None));
        // Shake It Off: 3:11 to 3:32 of 4:02
        assert!(thin_bars(&[bar(19_100, 19_150), bar(21_150, 21_200)], 241.6, None));
        assert!(!thin_bars(&[bar(300, 350), bar(24_650, 24_700)], 263.4, None));
        // length unknown: any bars do
        assert!(!thin_bars(&[bar(19_100, 19_150)], 0.0, None));
        // sung only in its first 100 s of 300 (a long instrumental end): bars over 90 of them are plenty
        assert!(!thin_bars(&[bar(1_000, 1_050), bar(9_950, 10_000)], 300.0, Some((5.0, 100.0))));
        assert!(thin_bars(&[bar(1_000, 1_050), bar(9_950, 10_000)], 300.0, None));
    }

    #[test]
    fn a_play_that_knew_less_keeps_what_the_song_knew() {
        let t = |d: f64, art: Option<&str>, link: Option<&str>, name: &str| Track { id: "isrc:X".into(), name: name.into(), artist: "A".into(),
            album: String::new(), duration: d, art: art.map(String::from), link: link.map(String::from) };
        let apple = Some("https://is1-ssl.mzstatic.com/image/thumb/x/600x600bb.jpg");
        let thumb = Some("https://i.ytimg.com/vi/x/hqdefault.jpg");
        let spotify_learned = t(231.0, apple, Some("https://music.apple.com/gb/album/x/1?i=2"), "The Kill (Bury Me)");
        // a YouTube play: a video's title, thumbnail and length (a guess), no page: the song keeps its own
        let (k, g) = keep_known(&spotify_learned, false, t(236.0, thumb, None, "The Kill (Official Video)"), true, true);
        assert_eq!((k.name.as_str(), k.art.as_deref(), k.link.is_some(), k.duration, g), ("The Kill (Bury Me)", apple, true, 231.0, false));
        // a song first learned from YouTube: the app's name (or a renamed video's) takes over
        let video = t(236.0, thumb, None, "The Kill (Official Video)");
        let (k, g) = keep_known(&video, true, t(231.0, apple, None, "The Kill (Bury Me)"), false, false);
        assert_eq!((k.name.as_str(), k.duration, g), ("The Kill (Bury Me)", 231.0, false));
        assert_eq!(keep_known(&video, true, t(236.0, thumb, None, "The Kill"), true, true).0.name, "The Kill");
        // a blank name never replaces a known one
        assert_eq!(keep_known(&spotify_learned, false, t(231.0, None, None, " "), false, false).0.name, "The Kill (Bury Me)");
        // a play named without a length, guessed from where it paused: the song's exact length stays
        let (k, g) = keep_known(&spotify_learned, false, t(197.2, None, None, "The Kill (Bury Me)"), true, false);
        assert_eq!((k.duration, g), (231.0, false));
        // two guesses: the longer; an exact length beats a guess, shorter or not
        let guessed = t(229.7, None, None, "The Kill (Bury Me)");
        assert_eq!(keep_known(&guessed, true, t(197.2, None, None, "x"), true, false).0.duration, 229.7);
        assert_eq!(keep_known(&guessed, true, t(240.0, None, None, "x"), true, false).0.duration, 240.0);
        assert_eq!(keep_known(&guessed, true, t(228.0, None, None, "x"), false, false).0.duration, 228.0);
        // a play with no length at all keeps the song's
        assert_eq!(keep_known(&guessed, true, t(0.0, None, None, "x"), false, false).0.duration, 229.7);
    }

    #[test]
    fn a_play_note_goes_to_its_own_save_only() {
        note_play(true, false);
        note_play(false, true);
        assert_eq!(take_play_note(), (true, true));
        // taken once: the next save on this thread has none
        assert_eq!(take_play_note(), (false, false));
        // another thread's note isn't this one's
        std::thread::spawn(|| note_play(true, true)).join().unwrap();
        assert_eq!(take_play_note(), (false, false));
    }

    use super::*;

    #[test]
    fn a_jump_seen_again_at_the_same_place_suggests_a_relearn() {
        // Take On Me, 28 Oct: +1.27 s at 2:06 on every play
        assert_eq!(repeated_slip(&[vec![(126.4, 1.27)]]), None);
        assert_eq!(repeated_slip(&[vec![(126.4, 1.27)], vec![(125.6, 1.27)]]), Some((125.6, 1.27)));
        // elsewhere, or by another amount: not the same fault
        assert_eq!(repeated_slip(&[vec![(126.4, 1.27)], vec![(60.0, 1.27)]]), None);
        assert_eq!(repeated_slip(&[vec![(126.4, 1.27)], vec![(126.0, 2.1)]]), None);
        // an older play's jump that this play didn't see: nothing new to say
        assert_eq!(repeated_slip(&[vec![(126.4, 1.27)], vec![(126.0, 1.3)], vec![]]), None);
    }

    #[test]
    fn syllables_found_in_the_lyrics_and_given_back() {
        // made-up words (no song lyrics in the repo)
        let lyrics = vec![(10.0, "We've walked the hills, sung them all,".to_string()), (14.0, String::new()),
                          (15.5, "Lanterns, glowing, by the Old Mill now".to_string())];
        let syl = ["We've", "walked", "the", "hills,", "sung", "~", "them", "all,", "Lan", "~", "terns,", "glow", "ing,", "by", "the", "Old", "Mill", "now", "xyz"];
        // bars 0.3 s apart from 10 s, the second line's from 15.5 s
        let bars: Vec<crate::syllables::SyllableBar> = syl.iter().enumerate()
            .map(|(i, s)| { let f = if i < 8 { 1000 + 30 * i as i64 } else { 1550 + 30 * (i - 8) as i64 }; (f, f + 25, 60, s.to_string()) }).collect();
        let back = |spans: Vec<Option<(usize, usize, usize)>>, lyrics: &[(f64, String)]| -> Vec<String> {
            spans.iter().map(|s| s.map_or(String::new(), |(l, a, n)| lyrics[l].1[a..a + n].to_string())).collect()
        };
        let want: Vec<String> = syl.iter().map(|s| if *s == "~" || *s == "xyz" { String::new() } else { s.to_string() }).collect();
        assert_eq!(back(syllable_spans(&bars, &lyrics), &lyrics), want);
        // a line's last syllable sung over the next line's first (so it comes after it): it, and every syllable
        // after it, still found (it was found two lines on, and the rest of the song lost)
        let lyrics = vec![(10.0, "Paper lanterns all aglow".to_string()), (12.0, "Down the harbour, row by row".to_string()),
                          (14.0, "Lanterns for the long way home".to_string()), (20.0, "Sing it low, aglow, aglow".to_string())];
        let syl = ["Pa", "per", "lan", "terns", "all", "a", "Down", "glow", "the", "har", "bour,", "row", "by", "row", "Lan", "terns", "for", "the", "long", "way", "home"];
        let bars: Vec<crate::syllables::SyllableBar> = syl.iter().enumerate().map(|(i, s)| {
            let f = [1000, 1030, 1060, 1090, 1120, 1150, 1200, 1210, 1240, 1270, 1300, 1330, 1360, 1390][..].get(i).copied().unwrap_or(1400 + 30 * (i as i64 - 14));
            (f, f + 25, 60, s.to_string())
        }).collect();
        assert_eq!(back(syllable_spans(&bars, &lyrics), &lyrics), syl.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(syllable_spans(&bars, &lyrics)[7], Some((0, 20, 4)));
        assert_eq!(lyrics_check(&lyrics), lyrics_check(&lyrics.iter().map(|l| (0.0, l.1.clone())).collect::<Vec<_>>()));
        assert_ne!(lyrics_check(&lyrics), lyrics_check(&lyrics[..2]));
    }

    #[test]
    fn kept_times_go_on_other_lyrics_for_the_song() {
        // made-up words; kept times lined up with the singer, ~1 s before LRCLIB's first entry
        let learned: Vec<(f64, String)> = [(10.0, "Lanterns by the mill"), (14.0, "Walking down the hill"), (18.5, ""),
            (21.0, "Lanterns by the mill"), (25.0, "Singing down the hill")].iter().map(|(t, s)| (*t, s.to_string())).collect();
        let times = vec![9.1, 13.2, 17.4, 20.0, 24.3];
        let with_lines = Kept { times: times.clone(), check: lyrics_check(&learned), lines: Some(line_checks(&learned)) };
        // the same lyrics: the kept times
        assert_eq!(lyrics_on_kept_times(&with_lines, &learned).unwrap().iter().map(|l| l.0).collect::<Vec<_>>(), times);
        // another entry: other times (+0.1 s), one line spelt differently, an extra line at the end
        let mut other = learned.iter().map(|(t, s)| (t + 0.1, s.clone())).collect::<Vec<_>>();
        other[1].1 = "Walkin' down the hill".into();
        other.push((29.0, "Lanterns by the mill".into()));
        let got = lyrics_on_kept_times(&with_lines, &other).unwrap();
        assert_eq!(got[0].0, 9.1);
        assert_eq!(got[3].0, 20.0);
        assert!((got[1].0 - 13.2).abs() < 0.3, "{got:?}");    // unpaired: moved with its neighbours
        assert!((got[5].0 - 28.1).abs() < 0.3, "{got:?}");
        // kept before lines had checks: paired by time (LRCLIB's 0.9-1.0 s late, 0.5% slower)
        let old = Kept { times: times.clone(), check: "x".into(), lines: None };
        let slower = learned.iter().map(|(t, s)| (t * 1.005 + 0.9, s.clone())).collect::<Vec<_>>();
        let got = lyrics_on_kept_times(&old, &slower).unwrap();
        for (g, t) in got.iter().zip(&times) {
            if !g.1.is_empty() {
                assert_eq!(g.0, *t);
            }
        }
        // other words altogether: none
        let other_song = vec![(3.0, "a".to_string()), (40.0, "b".to_string()), (80.0, "c".to_string())];
        assert!(lyrics_on_kept_times(&old, &other_song).is_none());
    }

    #[test]
    fn a_learned_song_shows_the_same_lyrics_whatever_lrclib_answers() {
        // made-up words; kept times made for `learned`, the singer starting each phrase at those times
        let learned: Vec<(f64, String)> = [(10.0, "Lanterns by the mill"), (14.0, "Walking down the hill"), (18.5, ""),
            (21.0, "Lanterns by the mill"), (25.0, "Singing down the hill")].iter().map(|(t, s)| (*t, s.to_string())).collect();
        let times = vec![9.1, 13.2, 17.4, 20.0, 24.3];
        let kept = Kept { times: times.clone(), check: lyrics_check(&learned), lines: None };
        let mut other = learned.iter().map(|(t, s)| (t + 0.1, s.clone())).collect::<Vec<_>>();
        other[1].1 = "Walkin' down the hill".into();
        let unrelated = vec![(3.0, "a".to_string()), (40.0, "b".to_string()), (80.0, "c".to_string())];
        // the entry it was learned with, wherever it is among the answers (pinned or not): exactly the kept times
        for cands in [vec![learned.clone()], vec![other.clone(), learned.clone()], vec![unrelated.clone(), other.clone(), learned.clone()]] {
            let p = learned_play(&kept, &cands, None).unwrap();
            assert_eq!(p.how, LearnedHow::Exact);
            assert_eq!(p.lyrics, times.iter().zip(&learned).map(|(t, l)| (*t, l.1.clone())).collect::<Vec<_>>());
            assert_eq!(p.entry, learned);
        }
        // another entry: paired; kept with the song only when it fits the singing (voiced from each kept time for 2 s)
        let mut voiced = vec![false; 3000];
        for t in &times {
            let k = (t * 100.0) as usize;
            voiced[k..k + 200].iter_mut().for_each(|v| *v = true);
        }
        let p = learned_play(&kept, &[other.clone()], Some(&voiced)).unwrap();
        assert_eq!(p.how, LearnedHow::Repaired);
        for (l, t) in p.lyrics.iter().zip(&times).filter(|(l, _)| !l.1.is_empty()) {
            assert_eq!(l.0, *t);
        }
        assert_eq!(learned_play(&kept, &[other.clone()], Some(&vec![false; 3000])).unwrap().how, LearnedHow::Unchecked);
        assert_eq!(learned_play(&kept, &[other], None).unwrap().how, LearnedHow::Unchecked);
        // nothing pairs: LRCLIB's timing; nothing at all: none
        assert_eq!(learned_play(&kept, &[unrelated.clone()], None).unwrap().lyrics, unrelated);
        assert!(learned_play(&kept, &[], None).is_none());
    }

    #[test]
    fn learning_again_replaces_the_copy_once_a_play_learned_enough() {
        let dir = std::env::temp_dir().join(format!("onkey-learnagain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        let song = Track { id: "isrc:GBY".into(), name: "Song".into(), artist: "A".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / FRAME_S) as usize;
        assert!(store.set_learn_again(&song.id, true).is_err(), "not learned: nothing to learn again");
        assert!(store.save_song(&song, vec![60.0; n], vec![true; n], vec![[1.0; FP_BANDS]; n]));
        store.save_signature(&song.id, b"sig", 0.0, 90.0);
        assert!(store.set_favorite(&song, true));
        assert!(store.set_learn_again(&song.id, true).is_ok());
        assert!(store.learning_again(&song.id) && store.song_state(&song.id)["learn_again"] == true);
        assert!(store.signature(&song.id).is_none(), "set aside while it's learned again");
        // taken back: as it was
        store.set_learn_again(&song.id, false).unwrap();
        assert!(!store.learning_again(&song.id) && store.signature(&song.id).is_some());
        store.set_learn_again(&song.id, true).unwrap();
        // a play that heard half of it: not enough to sing, the old copy stays and so does the ask
        assert!(!store.save_song(&song, vec![62.0; n], (0..n).map(|i| i < n / 2).collect(), vec![[2.0; FP_BANDS]; n]));
        assert_eq!(store.load_song(&song.id).unwrap().midi[0], 60.0);
        assert!(store.learning_again(&song.id));
        // one that heard 90%: replaces it, not merged in (the last 10% isn't the old copy's), the ask is done,
        // the old signature gone with it, the heart kept
        assert!(store.save_song(&song, vec![63.0; n], (0..n).map(|i| i < n * 9 / 10).collect(), vec![[3.0; FP_BANDS]; n]));
        let now = store.load_song(&song.id).unwrap();
        assert_eq!(now.midi[0], 63.0);
        assert!(!now.covered[n - 1]);
        assert!(!store.learning_again(&song.id));
        assert!(store.signature(&song.id).is_none());
        assert_eq!(store.song_meta(&song.id).unwrap()["favorite"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn not_the_melody_is_kept_with_the_song_and_rescores_its_takes() {
        let dir = std::env::temp_dir().join(format!("onkey-notmelody-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let song = Track { id: "isrc:GBX".into(), name: "Song".into(), artist: "A".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / FRAME_S) as usize;
        assert!(store.set_not_melody(&song.id, &[(1.0, 2.0)]).is_none(), "not learned: nothing kept");
        assert!(store.save_song(&song, vec![60.0; n], vec![true; n], vec![[1.0; FP_BANDS]; n]));
        assert!(store.not_melody(&song.id).is_empty());
        // tidied: in order, overlapping ones made one, a slip of a click dropped
        let kept = store.set_not_melody(&song.id, &[(50.0, 55.0), (12.0, 10.0), (54.0, 58.25), (70.0, 70.05)]).unwrap();
        assert_eq!(kept, vec![(10.0, 12.0), (50.0, 58.25)]);
        assert_eq!(store.not_melody(&song.id), kept);
        // kept when more of the song is learned (its first 10 s again)
        assert!(store.save_song(&song, vec![61.0; n], (0..n).map(|i| i < 1000).collect(), vec![[1.0; FP_BANDS]; n]));
        assert_eq!(store.not_melody(&song.id), kept);

        // a take of it starting 40 s in (song frame 4000), right for 10 s then 10 s on a backing vocal's notes;
        // another song's take
        let take = |name: &str, id: &str| {
            let (d, _) = store.new_take_dir(name).unwrap();
            let err: Vec<Option<f32>> = (0..2000).map(|i| Some(if i < 1000 { 0.0 } else { 300.0 })).collect();
            std::fs::write(d.join("meta.json"), json!({"track": {"id": id}, "song_offset": 40.0}).to_string()).unwrap();
            std::fs::write(d.join("result.json"), json!({"track": {"id": id}, "score": 67, "accuracy": 0.667, "coverage": 1.0,
                "notes": [], "notes_hit": 0, "notes_total": 0, "lines": [], "ref": vec![Some(60.0); 2000], "err": err}).to_string()).unwrap();
        };
        take("20261026-010000-A-Song", &song.id);
        take("20261026-020000-B-Other", "isrc:OTHER");
        let takes = store.takes_of(&song.id);
        assert_eq!(takes, vec!["20261026-010000-A-Song".to_string()]);
        assert_eq!(store.rescore_take(&takes[0], &[(50.0, 60.0)]), Some(100));
        let r = store.take_result(&takes[0]).unwrap();
        assert_eq!((r["left_out"].clone(), r["rescored"].is_string()), (json!([[1000, 2000]]), true));
        assert!(store.takes().iter().any(|t| t["id"] == takes[0] && t["rescored"].is_string() && t["score"] == 100));
        // the same again changes nothing; put back, it's as first scored and no longer marked
        assert_eq!(store.rescore_take(&takes[0], &[(50.0, 60.0)]), None);
        assert!(store.set_not_melody(&song.id, &[]).unwrap().is_empty());
        assert!(store.not_melody(&song.id).is_empty());
        assert_eq!(store.rescore_take(&takes[0], &[]), Some(67));
        let r = store.take_result(&takes[0]).unwrap();
        assert!(r["rescored"].is_null() && r["left_out"].is_null() && r["scored_first"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_song_learned_under_another_id_is_found_by_its_title() {
        // learned from Shazam's match (by ISRC), then played with Spotify's own id: the same song, ready to sing
        let dir = std::env::temp_dir().join(format!("onkey-knownas-{}", std::process::id()));
        let store = Store::new(dir.clone());
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let t = |id: &str, name: &str| Track { id: id.into(), name: name.into(), artist: "Ed Sheeran".into(), duration: 100.0, ..Default::default() };
        let learned = t("isrc:GBAHS1700004", "Castle on the Hill");
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        assert!(store.save_song(&learned, vec![60.0; n], vec![true; n], vec![[1.0; FP_BANDS]; n]));
        let spotify = t("spotify:track:6PCUP3dWmTjcTtXY02oFdT", "Castle On The Hill");
        let found = store.known_as(&spotify).expect("found by title");
        assert_eq!(found.id, learned.id);
        assert!(store.load_song(&found.id).is_some_and(|s| s.ready(found.duration)));
        assert!(store.known_as(&t("spotify:track:x", "Castle on the Hill - Acoustic")).is_none(), "another version");
        assert!(store.known_as(&learned).is_none(), "kept under its own id");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stopping_a_song_being_learned_never_forgets_a_learned_one_with_its_title() {
        let dir = std::env::temp_dir().join(format!("onkey-stopids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let t = |id: &str| Track { id: id.into(), name: "Hello".into(), artist: "Adele".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        // learned in full from Spotify; a video with the same title being learned is stopped: the video only
        let learned = t("spotify:track:hello");
        assert!(store.save_song(&learned, vec![60.0; n], vec![true; n], vec![[1.0; FP_BANDS]; n]));
        let video = t("youtube:YQHsXMglC9A");
        assert_eq!(store.stop_learning_ids(&video), vec![video.id.clone()]);
        assert!(store.song_path(&learned.id, "json").exists(), "nothing changed on the way");
        // asked to be learned again: that entry is unfinished, and goes too
        store.set_learn_again(&learned.id, true).unwrap();
        assert_eq!(store.stop_learning_ids(&video), vec![video.id.clone(), learned.id.clone()]);
        // only part learned (kept under its ISRC): it goes too
        let _ = std::fs::remove_dir_all(dir.join("songs"));
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let part = t("isrc:GBBKS1500214");
        assert!(store.save_song(&part, vec![60.0; n], (0..n).map(|i| i < n / 4).collect(), vec![[1.0; FP_BANDS]; n]));
        assert_eq!(store.stop_learning_ids(&video), vec![video.id.clone(), part.id.clone()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_differing_only_in_punctuation_have_files_of_their_own() {
        // the names songs always had stay theirs
        assert_eq!(safe("spotify:track:6PCUP3dWmTjcTtXY02oFdT"), "spotify_track_6PCUP3dWmTjcTtXY02oFdT");
        assert_eq!(safe("isrc:GBAHS1700004"), "isrc_GBAHS1700004");
        // two YouTube ids one '-'/'_' apart: two names, the same on every run
        let (a, b) = (safe("youtube:ab-cd_efghi"), safe("youtube:ab_cd-efghi"));
        assert_ne!(a, b);
        assert_eq!(a, safe("youtube:ab-cd_efghi"));
        assert!(a.starts_with("youtube_ab_cd_efghi-") && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        assert_ne!(safe("a:b"), safe("a_b"));
        // a song kept under the old shared name is renamed to its own, once, every file with it
        let dir = std::env::temp_dir().join(format!("onkey-safenames-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        let id = "youtube:ab-cd_efghi";
        let old = dir.join("songs").join("youtube_ab_cd_efghi");
        std::fs::write(old.with_extension("json"), json!({"track": {"id": id, "name": "Hello", "artist": "Adele"}}).to_string()).unwrap();
        std::fs::write(old.with_extension("bin"), b"bin").unwrap();
        std::fs::write(dir.join("songs/spotify_track_x.json"), json!({"track": {"id": "spotify:track:x", "name": "X", "artist": "Y"}}).to_string()).unwrap();
        assert_eq!(store.rename_to_safe_names(), 1);
        assert!(!old.with_extension("json").exists() && !old.with_extension("bin").exists());
        assert_eq!(std::fs::read(store.song_path(id, "bin")).unwrap(), b"bin");
        assert!(store.song_path(id, "json").exists() && dir.join("songs/spotify_track_x.json").exists());
        assert_eq!(store.rename_to_safe_names(), 0, "once");
        // the other id, one '-'/'_' apart, has nothing learned now
        assert!(!store.song_path("youtube:ab_cd-efghi", "json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cant_be_read_is_never_written_over() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("onkey-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        let set = |p: &Path, mode| std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
        let t = |id: &str| Track { id: id.into(), name: "Hello".into(), artist: "Adele".into(), duration: 100.0, ..Default::default() };
        // read_kept: not there is the default; there and whole is it; garbled is moved aside (kept), then afresh
        let list = dir.join("list.json");
        assert_eq!(read_kept::<Vec<String>>(&list), Some(vec![]));
        std::fs::write(&list, r#"["a","b"]"#).unwrap();
        assert_eq!(read_kept::<Vec<String>>(&list), Some(vec!["a".to_string(), "b".to_string()]));
        std::fs::write(&list, r#"["a","#).unwrap();
        assert_eq!(read_kept::<Vec<String>>(&list), Some(vec![]));
        let aside: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("list.json.unreadable-")).collect();
        assert!(aside.len() == 1 && std::fs::read(aside[0].path()).unwrap() == br#"["a","#);
        // hearts given to songs being learned: an unreadable file of them isn't replaced by one with only the new heart
        let pending = dir.join("favorites-pending.json");
        std::fs::write(&pending, r#"{"spotify:track:a":"2026-10-01T10:00:00"}"#).unwrap();
        set(&pending, 0o000);
        let unreadable = std::fs::read(&pending).is_err();      // (not as root)
        store.set_favorite(&t("spotify:track:b"), true);
        assert!(store.take_pending_favorite("spotify:track:a").is_none() || !unreadable);
        set(&pending, 0o644);
        if unreadable {
            assert_eq!(std::fs::read_to_string(&pending).unwrap(), r#"{"spotify:track:a":"2026-10-01T10:00:00"}"#);
        }
        // a learned song whose .bin can't be read just now isn't saved over with this play's part alone
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        let part = |a: usize, b: usize| (0..n).map(|i| (a..b).contains(&i)).collect::<Vec<bool>>();
        assert!(store.save_song(&t("isrc:X"), vec![60.0; n], part(0, 9000), Vec::new()));
        let bin = store.song_path("isrc:X", "bin");
        let before = std::fs::read(&bin).unwrap();
        set(&bin, 0o000);
        let saved = store.save_song(&t("isrc:X"), vec![62.0; n], part(9000, 10000), Vec::new());
        set(&bin, 0o644);
        if unreadable {
            assert!(!saved, "refused");
            assert_eq!(std::fs::read(&bin).unwrap(), before, "left as it was");
        }
        assert!(store.save_song(&t("isrc:X"), vec![62.0; n], part(9000, 10000), Vec::new()), "readable again: saved");
        assert!(store.load_song("isrc:X").unwrap().fraction(100.0) > 0.99);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_cant_be_read_is_an_error_not_an_empty_list() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("onkey-unreadable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        assert_eq!(store.readable(true), Ok(()), "empty folders: no takes yet, not an error");
        // not there at all (nothing saved yet): fine too
        let fresh = Store { root: dir.join("nowhere") };
        assert_eq!(fresh.readable(true), Ok(()));
        // a folder that can't be listed
        let set = |d: &Path, mode| std::fs::set_permissions(d, std::fs::Permissions::from_mode(mode)).unwrap();
        set(&dir.join("songs"), 0o000);
        let songs = store.readable(true);
        let takes_only = store.readable(false);
        set(&dir.join("songs"), 0o755);
        set(&dir.join("takes"), 0o000);
        let takes = store.readable(false);
        set(&dir.join("takes"), 0o755);
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::read_dir("/var/root").is_ok() {
            return;     // (as root every folder can be read)
        }
        assert!(songs.is_err_and(|e| e.contains("songs")));
        assert_eq!(takes_only, Ok(()));
        assert!(takes.is_err_and(|e| e.contains("takes")));
    }

    #[test]
    fn a_delete_that_cant_reach_the_trash_deletes_nothing() {
        let dir = std::env::temp_dir().join(format!("onkey-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.join("data"));
        let (take, _) = store.new_take_dir("20261026-120000-Adele-Hello").unwrap();
        std::fs::write(take.join("result.json"), "{}").unwrap();
        let song = store.song_path("isrc:X", "json");
        std::fs::write(&song, "{}").unwrap();
        // a Trash that can't be moved into (not there): an error, and both stay
        let nowhere = dir.join("no-trash");
        assert_eq!(trash_into(&take, &nowhere), Err(TRASH_FAILED.to_string()));
        assert_eq!(trash_into(&song, &nowhere), Err(TRASH_FAILED.to_string()));
        assert!(take.join("result.json").exists() && song.exists(), "nothing deleted outright");
        // a Trash that's there: moved into it, whole
        let trash = dir.join("Trash");
        std::fs::create_dir_all(&trash).unwrap();
        assert_eq!(trash_into(&take, &trash), Ok(()));
        assert!(!take.exists());
        let moved: Vec<_> = std::fs::read_dir(&trash).unwrap().flatten().map(|e| e.path()).collect();
        assert!(moved.len() == 1 && moved[0].join("result.json").exists(), "{moved:?}");
        // already gone: nothing to do
        assert_eq!(trash_into(&take, &nowhere), Ok(()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_artist_credited_differently_is_the_same_artist() {
        assert!(same_artist("Lady Gaga", "Lady Gaga & Bradley Cooper"));
        assert!(same_artist("Ed Sheeran feat. Khalid", "Ed Sheeran"));
        assert!(same_artist("Simon & Garfunkel", "Simon & Garfunkel"));
        assert!(same_artist("Beyoncé, JAY-Z", "JAY-Z & Beyoncé"));
        assert!(!same_artist("Lady Gaga", "Bradley Cooper & Lukas Nelson"));
        assert!(!same_artist("Paul Simon", "Simon & Garfunkel"));
        assert!(!same_artist("", "Adele"));
        let t = |id: &str, artist: &str| Track { id: id.into(), name: "Shallow".into(), artist: artist.into(), ..Default::default() };
        assert!(same_title(&t("spotify:track:2VxeLyX666F8uXCJ0dZF8B", "Lady Gaga"), &t("isrc:USUM71813192", "Lady Gaga & Bradley Cooper")));
    }

    #[test]
    fn a_song_is_kept_once_whatever_id_it_was_played_by() {
        let dir = std::env::temp_dir().join(format!("onkey-dupes-{}", std::process::id()));
        let store = Store::new(dir.clone());
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let t = |id: &str, name: &str| Track { id: id.into(), name: name.into(), artist: "Adele".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        let onset: Vec<[f32; FP_BANDS]> = (0..n).map(|i| [((i * 37 % 101) as f32) / 10.0; FP_BANDS]).collect();
        let part = |a: usize, b: usize| (0..n).map(|i| (a..b).contains(&i)).collect::<Vec<bool>>();
        // learned twice before: a ready entry by ISRC and 10% under Spotify's id; and a song only Spotify ever named, twice
        assert!(store.save_song(&t("isrc:USSM12105970", "Easy On Me"), vec![60.0; n], part(0, 9000), onset.clone()));
        assert!(store.save_song(&t("spotify:track:46IZ0fSY2mpAiktS3KOqds", "Easy on Me"), vec![60.0; n], part(9000, 10000), onset.clone()));
        assert!(store.save_song(&t("spotify:track:a", "Hello"), vec![60.0; n], part(0, 5000), onset.clone()));
        assert!(store.save_song(&t("spotify:track:b", "Hello"), vec![60.0; n], part(5000, 10000), onset.clone()));
        assert_eq!(store.merge_duplicates(), 2);
        let ids: Vec<String> = store.learned_tracks().into_iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert!(ids.contains(&"isrc:USSM12105970".to_string()), "{ids:?}");
        // merged by fingerprint: the ISRC entry now has both parts
        assert!(store.load_song("isrc:USSM12105970").unwrap().fraction(100.0) > 0.99);
        assert!(store.load_song("spotify:track:46IZ0fSY2mpAiktS3KOqds").is_none());
        // a later play by Spotify's id goes to the ISRC entry, and one by the ISRC takes over an entry by Spotify's id
        assert_eq!(store.canonical(&t("spotify:track:46IZ0fSY2mpAiktS3KOqds", "Easy On Me")).id, "isrc:USSM12105970");
        let hello = ids.iter().find(|i| i.starts_with("spotify:")).unwrap().clone();
        assert_eq!(store.canonical(&t("isrc:GBX", "Hello")).id, "isrc:GBX");
        assert!(store.load_song(&hello).is_none() && store.load_song("isrc:GBX").is_some());
        assert_eq!(store.merge_duplicates(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_garbled_song_json_is_kept_aside_not_written_over() {
        let dir = std::env::temp_dir().join(format!("onkey-garbled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        let t = Track { id: "spotify:track:g".into(), name: "Hello".into(), artist: "Adele".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        std::fs::write(store.song_path(&t.id, "json"), br#"{"track": {"id": "spotify:track:g"}, "favorite": tr"#).unwrap();
        assert!(store.save_song(&t, vec![60.0; n], vec![true; n], vec![[1.0; FP_BANDS]; n]));
        let aside: Vec<_> = std::fs::read_dir(dir.join("songs")).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("spotify_track_g.json.unreadable-")).collect();
        assert_eq!(aside.len(), 1, "the garbled one kept beside it");
        assert!(store.song_meta(&t.id).is_some(), "and a whole one written");
        assert_eq!(store.learned_tracks().len(), 1, "the one set aside isn't listed as a song");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_merge_keeps_what_was_marked_on_the_entry_merged_away() {
        let dir = std::env::temp_dir().join(format!("onkey-mergekeep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        let t = |id: &str| Track { id: id.into(), name: "Hello".into(), artist: "Adele".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        let onset: Vec<[f32; FP_BANDS]> = (0..n).map(|i| [((i * 37 % 101) as f32) / 10.0; FP_BANDS]).collect();
        let (from, to) = (t("spotify:track:a"), t("isrc:GBX"));
        assert!(store.save_song(&from, vec![60.0; n], (0..n).map(|i| i < n / 2).collect(), onset.clone()));
        assert!(store.save_song(&to, vec![60.0; n], (0..n).map(|i| i >= n / 2).collect(), onset.clone()));
        store.set_not_melody(&from.id, &[(10.0, 20.0)]);
        store.set_not_melody(&to.id, &[(15.0, 30.0), (60.0, 70.0)]);
        store.with_meta(&from.id, |m| { m["lyric_times"] = json!([1.0, 2.0]); m["lyrics_placed"] = json!(true); true });
        store.merge_into(&from.id, &to);
        assert_eq!(store.not_melody(&to.id), vec![(10.0, 30.0), (60.0, 70.0)], "both entries' stretches");
        let m = store.song_meta(&to.id).unwrap();
        assert_eq!(m["lyric_times"], json!([1.0, 2.0]), "the lyric times of the one merged away, the kept one having none");
        assert!(store.lyrics_placed(&to.id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_app_and_id_a_song_starts_by() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let pick = |app, id: Option<&str>| Pick { app, id: id.map(String::from) };
        let start = |app, id: &str| PlayPlan::Start { app, id: id.to_string() };
        let none = |_: Source| false;
        let spotify_open = |s: Source| s == Source::Spotify;
        // learned from Spotify, last played in Apple Music (its link says Apple Music): Apple Music, by its id there,
        // though it was asked for by its Spotify id (it used to start in Spotify)
        let both = ids(&["spotify:track:5fVZ", "spotify:track:5fVZ", "music:AB12"]);
        assert_eq!(play_plan(&both, &pick(Source::Music, Some("music:AB12")), none), start(Source::Music, "music:AB12"));
        // kept by its ISRC, opens in Apple Music, no Apple Music id: nothing to play it by (it used to "start" and do nothing)
        let heard = ids(&["isrc:X", "isrc:X"]);
        assert_eq!(play_plan(&heard, &pick(Source::Music, None), none), PlayPlan::NoId { app: Source::Music });
        assert_eq!(play_plan(&heard, &pick(Source::Music, None), |_| true), PlayPlan::NoId { app: Source::Music });
        // learned from Spotify, heard in Apple Music since (no id there): Spotify's id when Spotify is running, else none
        let sp = ids(&["isrc:X", "isrc:X", "spotify:track:5fVZ"]);
        assert_eq!(play_plan(&sp, &pick(Source::Music, None), spotify_open), start(Source::Spotify, "spotify:track:5fVZ"));
        assert_eq!(play_plan(&sp, &pick(Source::Music, None), none), PlayPlan::NoId { app: Source::Music });
        // learned from Apple Music, opens in Spotify with no id there, Apple Music not running: none (it isn't launched
        // for a song you'd expect in Spotify)
        let mu = ids(&["isrc:X", "isrc:X", "music:AB12"]);
        assert_eq!(play_plan(&mu, &pick(Source::Spotify, None), none), PlayPlan::NoId { app: Source::Spotify });
        assert_eq!(play_plan(&mu, &pick(Source::Spotify, None), |s| s == Source::Music), start(Source::Music, "music:AB12"));
        // a Spotify song that opens in Spotify: as before
        let own = ids(&["spotify:track:5fVZ", "spotify:track:5fVZ"]);
        assert_eq!(play_plan(&own, &pick(Source::Spotify, Some("spotify:track:5fVZ")), none), start(Source::Spotify, "spotify:track:5fVZ"));
    }

    #[test]
    fn the_demo_song_starts_in_onkey_whatever_app_is_usual_or_running() {
        // (test-all: library-play's play plan with demo-song's Source::Demo) the Library's Play on the demo starts
        // OnKey's own player, never Spotify or Apple Music
        let id = "demo:stand-by-me-placeholder";
        for (usual, running) in [(None, None), (Some(Source::Spotify), Some(Source::Music)), (Some(Source::Music), Some(Source::Spotify))] {
            let pick = pick_app(id, &[], None, usual, running);
            assert_eq!(pick, Pick { app: Source::Demo, id: Some(id.to_string()) });
            assert_eq!(play_plan(&[id.to_string(), id.to_string()], &pick, |_| true), PlayPlan::Start { app: Source::Demo, id: id.to_string() });
        }
    }

    #[test]
    fn a_song_starts_where_its_link_opens() {
        let dir = std::env::temp_dir().join(format!("onkey-playplan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let t = Track { id: "spotify:track:5fVZ".into(), name: "Yellow".into(), artist: "Coldplay".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        assert!(store.save_song(&t, vec![60.0; n], vec![true; n], vec![[1.0; FP_BANDS]; n]));
        // played in Apple Music since: the link and Play both say Apple Music, by its id there
        store.note_play("spotify:track:5fVZ", "music:AB12", Source::Music);
        assert_eq!(store.open_pick("spotify:track:5fVZ", None).0, Pick { app: Source::Music, id: Some("music:AB12".into()) });
        assert_eq!(store.play_plan("spotify:track:5fVZ", None, |_| false), PlayPlan::Start { app: Source::Music, id: "music:AB12".into() });
        // asked by its Apple Music id (a take's), the same
        assert_eq!(store.play_plan("music:AB12", None, |_| false), PlayPlan::Start { app: Source::Music, id: "music:AB12".into() });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_app_a_song_opens_in() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let pick = |app, id: Option<&str>| Pick { app, id: id.map(String::from) };
        let (sp, mu, yt) = (Some(Source::Spotify), Some(Source::Music), Some(Source::YouTube));
        // How To Save A Life: learned from Spotify, kept by its ISRC, nothing playing now: Spotify, by its track id
        let a = ids(&["spotify:track:5fVZ"]);
        assert_eq!(pick_app("isrc:X", &a, sp, None, None), pick(Source::Spotify, Some("spotify:track:5fVZ")));
        assert_eq!(pick_app("isrc:X", &a, None, sp, None), pick(Source::Spotify, Some("spotify:track:5fVZ")));
        // last played in Apple Music (your usual app isn't Spotify): there, by its id
        let both = ids(&["music:AB12", "spotify:track:5fVZ"]);
        assert_eq!(pick_app("isrc:X", &both, mu, mu, sp), pick(Source::Music, Some("music:AB12")));
        // played on YouTube last, though Spotify is usual: Spotify's id wins (you usually sing from Spotify)
        assert_eq!(pick_app("isrc:X", &ids(&["youtube:v1", "spotify:track:5fVZ"]), yt, sp, None), pick(Source::Spotify, Some("spotify:track:5fVZ")));
        assert_eq!(pick_app("isrc:X", &ids(&["youtube:v1"]), yt, sp, None), pick(Source::YouTube, Some("youtube:v1")));
        // only heard (an ISRC, no app ids): the app it played in, else your usual one, else the one running
        assert_eq!(pick_app("isrc:X", &[], sp, mu, None), pick(Source::Spotify, None));
        assert_eq!(pick_app("isrc:X", &[], None, sp, mu), pick(Source::Spotify, None));
        assert_eq!(pick_app("isrc:X", &[], None, None, mu), pick(Source::Music, None));
        // nothing known: never Apple Music; a song kept by an app's own id opens there
        assert_eq!(pick_app("isrc:X", &[], None, None, None), pick(Source::Spotify, None));
        assert_eq!(pick_app("music:AB12", &[], None, sp, None), pick(Source::Music, Some("music:AB12")));
        assert_eq!(pick_app("spotify:track:5fVZ", &[], None, None, mu), pick(Source::Spotify, Some("spotify:track:5fVZ")));
        // your usual app: the most takes (each take's app, else its id's)
        let take = |id: &str, app: Option<&str>| json!({"track": {"id": id}, "app": app});
        assert_eq!(usual_app(&[take("isrc:A", Some("spotify")), take("music:B", None), take("spotify:track:C", None)]), sp);
        assert_eq!(usual_app(&[take("isrc:A", None), take("shazam:1", None)]), None);
    }

    #[test]
    fn where_a_song_played_is_kept_with_it() {
        let dir = std::env::temp_dir().join(format!("onkey-played-in-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let t = |id: &str, name: &str| Track { id: id.into(), name: name.into(), artist: "The Fray".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        let onset: Vec<[f32; FP_BANDS]> = (0..n).map(|i| [((i * 37 % 101) as f32) / 10.0; FP_BANDS]).collect();
        let part = |a: usize, b: usize| (0..n).map(|i| (a..b).contains(&i)).collect::<Vec<bool>>();
        let meta = |id: &str| store.song_meta(id).unwrap();

        // learned from Spotify, kept by its ISRC: Spotify's id and the app are kept, and survive another save
        let save = t("isrc:USSM10603618", "How to Save a Life");
        assert!(store.save_song(&save, vec![60.0; n], part(0, 9000), onset.clone()));
        store.note_play(&save.id, "spotify:track:5fVZ", Source::Spotify);
        assert!(store.save_song(&save, vec![60.0; n], part(9000, 10000), onset.clone()));
        assert_eq!(aliases(&meta(&save.id)), vec!["spotify:track:5fVZ"]);
        assert_eq!(meta(&save.id)["played_in"], "spotify");
        store.note_play(&save.id, &save.id, Source::Other);           // only heard: nothing new
        assert_eq!(aliases(&meta(&save.id)).len(), 1);
        let (p, track) = store.open_pick(&save.id, Some(Source::Music));
        assert_eq!(p, Pick { app: Source::Spotify, id: Some("spotify:track:5fVZ".into()) });
        assert_eq!(track.unwrap().name, "How to Save a Life");
        // found by its Spotify id too
        assert_eq!(store.open_pick("spotify:track:5fVZ", None).0.id.as_deref(), Some("spotify:track:5fVZ"));

        // the duplicate merge keeps the union of the ids (the merged-away entry's own id among them)
        assert!(store.save_song(&t("music:AB12", "Over My Head"), vec![60.0; n], part(0, 5000), onset.clone()));
        store.note_play("music:AB12", "music:AB12", Source::Music);
        assert!(store.save_song(&t("isrc:OVER", "Over My Head"), vec![60.0; n], part(5000, 10000), onset.clone()));
        store.note_alias("isrc:OVER", "youtube:v1");
        assert_eq!(store.merge_duplicates(), 1);
        let mut a = aliases(&meta("isrc:OVER"));
        a.sort();
        assert_eq!(a, vec!["music:AB12", "youtube:v1"]);
        assert_eq!(meta("isrc:OVER")["played_in"], "music");

        // backfilled at start-up: a song learned before this, merged away under Spotify's id, and History's takes
        assert!(store.save_song(&t("isrc:OLD", "You Found Me"), vec![60.0; n], part(0, 9000), onset.clone()));
        let merged = dir.join("songs").join("merged");
        std::fs::create_dir_all(&merged).unwrap();
        std::fs::write(merged.join("spotify_track_old.json"), json!({"track": t("spotify:track:old", "You Found Me")}).to_string()).unwrap();
        let take = dir.join("takes").join("20261001-120000-The-Fray-You-Found-Me");
        std::fs::create_dir_all(&take).unwrap();
        std::fs::write(take.join("result.json"), json!({"track": t("spotify:track:old", "You Found Me"), "recorded": "2026-10-01T12:00:00"}).to_string()).unwrap();
        assert_eq!(store.backfill_sources(), 1);
        assert_eq!(aliases(&meta("isrc:OLD")), vec!["spotify:track:old"]);
        assert_eq!(meta("isrc:OLD")["played_in"], "spotify");
        assert_eq!(store.backfill_sources(), 0, "once");
        assert_eq!(store.usual_app(), Some(Source::Spotify));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_heart_is_kept_with_the_song() {
        let dir = std::env::temp_dir().join(format!("onkey-fav-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let t = |id: &str, name: &str| Track { id: id.into(), name: name.into(), artist: "Adele".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        let onset: Vec<[f32; FP_BANDS]> = (0..n).map(|i| [((i * 37 % 101) as f32) / 10.0; FP_BANDS]).collect();
        let part = |a: usize, b: usize| (0..n).map(|i| (a..b).contains(&i)).collect::<Vec<bool>>();
        let fav = |id: &str| store.library().into_iter().find(|r| r["track"]["id"] == id).map(|r| r["favorite"] == true);

        // set and get on a learned song; it stays through another save of the song
        let hello = t("isrc:GBHELLO", "Hello");
        assert!(store.save_song(&hello, vec![60.0; n], part(0, 9000), onset.clone()));
        assert!(!store.is_favorite(&hello) && fav("isrc:GBHELLO") == Some(false));
        store.set_favorite(&hello, true);
        assert!(store.is_favorite(&hello) && fav("isrc:GBHELLO") == Some(true));
        assert!(store.library()[0]["favorited_at"].is_string());
        assert!(store.save_song(&hello, vec![60.0; n], part(9000, 10000), onset.clone()));
        assert_eq!(fav("isrc:GBHELLO"), Some(true));
        // hearted by Spotify's id for the same song: it's the ISRC entry's heart
        assert!(store.is_favorite(&t("spotify:track:h", "Hello")));
        store.set_favorite(&t("spotify:track:h", "Hello"), false);
        assert_eq!(fav("isrc:GBHELLO"), Some(false));

        // hearted while being learned for the first time: it's the song's once saved
        let new = t("spotify:track:new", "Skyfall");
        store.set_favorite(&new, true);
        assert!(store.is_favorite(&new) && fav("spotify:track:new").is_none());
        assert!(store.save_song(&new, vec![60.0; n], part(0, 5000), onset.clone()));
        assert_eq!(fav("spotify:track:new"), Some(true));
        assert!(!dir.join("favorites-pending.json").exists());
        // and taken away again before it's saved: no heart
        let other = t("spotify:track:other", "Rolling in the Deep");
        store.set_favorite(&other, true);
        store.set_favorite(&other, false);
        assert!(store.save_song(&other, vec![60.0; n], part(0, 5000), onset.clone()));
        assert_eq!(fav("spotify:track:other"), Some(false));

        // the duplicate merge keeps it when either entry had it
        assert!(store.save_song(&t("isrc:GBSKY", "Skyfall"), vec![60.0; n], part(5000, 10000), onset.clone()));
        assert!(store.save_song(&t("isrc:GBROLL", "Rolling In The Deep"), vec![60.0; n], part(5000, 10000), onset.clone()));
        store.set_favorite(&t("isrc:GBROLL", "Rolling In The Deep"), true);
        assert_eq!(store.merge_duplicates(), 2);
        assert_eq!(fav("isrc:GBSKY"), Some(true), "the merged-away entry was the favourite");
        assert_eq!(fav("isrc:GBROLL"), Some(true), "the kept entry was the favourite");
        assert_eq!(fav("isrc:GBHELLO"), Some(false));

        // a heart given under Spotify's id to a song being learned, which is then kept by its ISRC
        let first = t("spotify:track:first", "Someone Like You");
        store.set_favorite(&first, true);
        assert!(store.save_song(&t("isrc:GBSLY", "Someone Like You"), vec![60.0; n], part(0, 9000), onset.clone()));
        assert_eq!(store.canonical(&first).id, "isrc:GBSLY");
        assert_eq!(fav("isrc:GBSLY"), Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_plays_of_one_song_saved_at_once_are_one_entry() {
        // a take split in two (Spotify's id, then the ISRC), both scored at the same moment
        let dir = std::env::temp_dir().join(format!("onkey-saveone-{}", std::process::id()));
        let store = Store::new(dir.clone());
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let t = |id: &str| Track { id: id.into(), name: "My Love, My Life".into(), artist: "ABBA".into(), duration: 100.0, ..Default::default() };
        let n = (100.0 / crate::dsp::FRAME_S) as usize;
        let onset: Vec<[f32; FP_BANDS]> = (0..n).map(|i| [((i * 37 % 101) as f32) / 10.0; FP_BANDS]).collect();
        let part = |a: usize, b: usize| (0..n).map(|i| (a..b).contains(&i)).collect::<Vec<bool>>();
        let threads: Vec<_> = [("spotify:track:7qbQ", 0, 4000), ("isrc:SEAYD7601030", 4000, 8000)].into_iter().map(|(id, a, b)| {
            let (store, track, onset, cov) = (store.clone(), t(id), onset.clone(), part(a, b));
            std::thread::spawn(move || store.save_as_one(&track, vec![60.0; n], cov, onset))
        }).collect();
        for th in threads {
            assert!(th.join().unwrap().0);
        }
        let ids: Vec<String> = store.learned_tracks().into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["isrc:SEAYD7601030".to_string()]);
        assert!(store.load_song("isrc:SEAYD7601030").unwrap().fraction(100.0) > 0.79);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apple_covers_are_told_apart() {
        assert!(is_apple_art("https://is1-ssl.mzstatic.com/image/thumb/a/600x600bb.jpg"));
        assert!(!is_apple_art("https://i.scdn.co/image/ab67616d"));
        assert!(!is_apple_art("https://i.ytimg.com/vi/x/hqdefault.jpg"));
        assert!(!is_apple_art("https://evil.example/x.mzstatic.com/y"));
    }

    #[test]
    fn two_takes_saved_the_same_second_get_their_own_folders() {
        let dir = std::env::temp_dir().join(format!("onkey-takedirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());
        let name = "20261015-120000-Adele-Hello";
        let (a, na) = store.new_take_dir(name).unwrap();
        let (b, nb) = store.new_take_dir(name).unwrap();
        assert_eq!(na, name);
        assert_eq!(nb, format!("{name}-2"));
        assert_ne!(a, b);
        assert!(a.is_dir() && b.is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_writes_leave_no_temp_files_and_old_ones_are_swept() {
        let dir = std::env::temp_dir().join(format!("onkey-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("songs")).unwrap();
        let p = dir.join("songs").join("x.json");
        write_atomic(&p, b"{\"a\":1}").unwrap();
        write_atomic(&p, b"{\"a\":2}").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"{\"a\":2}");
        let names = |d: &Path| std::fs::read_dir(d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect::<Vec<_>>();
        assert_eq!(names(&dir.join("songs")), vec!["x.json"], "no temp file left behind");
        // a temp file a killed OnKey left (another process's, old) is cleared when the store opens; a fresh one isn't
        let stale = dir.join("songs").join(".x.json.tmp-1-0");
        let fresh = dir.join("songs").join(".x.json.tmp-2-0");
        std::fs::write(&stale, b"{\"a\"").unwrap();
        std::fs::write(&fresh, b"{\"a\"").unwrap();
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&stale).unwrap().set_modified(hour_ago).unwrap();
        Store::new(dir.clone());
        assert!(!stale.exists() && fresh.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// How hard a melody is to sing, 1 (easy) to 5 (very hard), from its bars (start and end in 10 ms frames, MIDI note).
/// The range it covers sets the level: the span between its 10th and 90th percentile notes (so a stray note or an
/// octave slip doesn't count), 6 semitones or less 1, up to 9 2, up to 12 3, up to 15 4, more 5. A fast melody (over
/// 4.2 notes a second of singing) or a leapy one (over 15% of notes a jump of 5 semitones or more straight after the
/// last) is one harder. Bands set on 155 learned songs (18 Oct 2026) to spread them over the five levels; scores
/// didn't track any measure of the melody well enough to set them (range -0.13), so it's a guide, not a promise.
/// None under 40 bars: too little to judge.
pub fn difficulty(bars: &[(i64, i64, i64)]) -> Option<u8> {
    if bars.len() < 40 {
        return None;
    }
    let mut notes: Vec<i64> = bars.iter().map(|b| b.2).collect();
    notes.sort_unstable();
    let n = notes.len();
    let range = notes[n * 9 / 10] - notes[n / 10];
    let level = match range {
        ..=6 => 1,
        7..=9 => 2,
        10..=12 => 3,
        13..=15 => 4,
        _ => 5,
    };
    let singing: f64 = bars.iter().map(|b| (b.1 - b.0) as f64 / 100.0).sum();
    let fast = n as f64 / singing.max(1.0) > 4.2;
    let leaps = bars.windows(2).filter(|w| w[1].0 - w[0].1 < 30 && (w[1].2 - w[0].2).abs() >= 5).count();
    let leapy = leaps as f64 / n as f64 > 0.15;
    Some((level + u8::from(fast || leapy)).min(5))
}

#[cfg(test)]
mod difficulty_tests {
    use super::difficulty;

    /// `notes` sung one after another, each `len` frames with a 5-frame gap.
    fn bars(notes: &[i64], len: i64) -> Vec<(i64, i64, i64)> {
        notes.iter().enumerate().map(|(i, &m)| (i as i64 * (len + 5), i as i64 * (len + 5) + len, m)).collect()
    }

    #[test]
    fn levels_follow_range_speed_and_leaps() {
        let narrow: Vec<i64> = (0..60).map(|i| 60 + (i % 5)).collect();           // 4 semitones
        let wide: Vec<i64> = (0..60).map(|i| 55 + (i % 24)).collect();            // 23 semitones (18 from 10% to 90%)
        assert_eq!(difficulty(&bars(&narrow, 40)), Some(1));
        assert_eq!(difficulty(&bars(&wide, 40)), Some(5));
        assert_eq!(difficulty(&bars(&narrow, 15)), Some(2), "fast: over 4.2 notes a second");
        let leapy: Vec<i64> = (0..60).map(|i| if i % 2 == 0 { 60 } else { 65 }).collect();
        assert_eq!(difficulty(&bars(&leapy, 40)), Some(1 + 1), "a 5-semitone range, every note a leap");
        let mut stray = narrow.clone();
        stray[7] = 90;                                                           // one stray note doesn't count
        assert_eq!(difficulty(&bars(&stray, 40)), Some(1));
        assert_eq!(difficulty(&bars(&narrow[..30], 40)), None);
    }
}
