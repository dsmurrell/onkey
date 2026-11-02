//! The edit list of an MPEG-4 audio file (.m4a): which stretch of the decoded track is the recording. An AAC
//! encoder puts some audio of its own ahead of the sound (priming) and pads the end to a whole packet; the edit
//! list ('elst', in the track's 'edts') says where the sound starts and how long it is, and players (WebView2's
//! among them) skip the rest. Windows' encoder (platform/windows/mf.rs) leaves its priming in, so the take's
//! file gets an edit list here after it's written, and Windows' decoder is told the same when it reads one.
//! Plain box surgery on the file, no codec: also tested on macOS.
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// What an edit list says, in seconds of the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edit {
    /// where the recording starts in the decoded track (the priming)
    pub start: f64,
    /// how long the recording is
    pub length: f64,
}

/// A box: its type, where it starts, where its contents start and where it ends (offsets into the buffer).
#[derive(Debug, Clone, Copy)]
struct Bx {
    kind: [u8; 4],
    at: usize,
    body: usize,
    end: usize,
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// The boxes in `b[from..to]`, in order (stopping at the first that doesn't fit).
fn children(b: &[u8], from: usize, to: usize) -> Vec<Bx> {
    let mut out = Vec::new();
    let mut at = from;
    while at + 8 <= to {
        let Some(size) = be32(b, at) else { break };
        let kind: [u8; 4] = b[at + 4..at + 8].try_into().unwrap();
        let (len, head) = match size {
            0 => ((to - at) as u64, 8),
            1 => match be64(b, at + 8) { Some(l) => (l, 16), None => break },
            n => (n as u64, 8),
        };
        let end = at as u64 + len;
        if len < head as u64 || end > to as u64 {
            break;
        }
        out.push(Bx { kind, at, body: at + head, end: end as usize });
        at = end as usize;
    }
    out
}

fn child(b: &[u8], parent: Bx, kind: &[u8; 4]) -> Option<Bx> {
    children(b, parent.body, parent.end).into_iter().find(|c| &c.kind == kind)
}

/// The movie's 'moov', read from the file at `path` (the rest, the audio itself, is skipped): its bytes, and
/// where it starts in the file.
fn read_moov(path: &Path) -> Option<(Vec<u8>, u64)> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut at = 0u64;
    while at + 8 <= len {
        let mut h = [0u8; 16];
        f.seek(SeekFrom::Start(at)).ok()?;
        f.read_exact(&mut h[..8]).ok()?;
        let size = match be32(&h, 0)? {
            0 => len - at,
            1 => {
                f.read_exact(&mut h[8..]).ok()?;
                be64(&h, 8)?
            }
            n => n as u64,
        };
        if size < 8 || at + size > len {
            return None;
        }
        if &h[4..8] == b"moov" {
            let mut m = vec![0u8; size as usize];
            f.seek(SeekFrom::Start(at)).ok()?;
            f.read_exact(&mut m).ok()?;
            return Some((m, at));
        }
        at += size;
    }
    None
}

/// The movie's time scale (units a second) from its 'mvhd'.
fn movie_scale(b: &[u8], moov: Bx) -> Option<u32> {
    let h = child(b, moov, b"mvhd")?;
    be32(b, h.body + if b[h.body] == 1 { 20 } else { 12 }).filter(|s| *s > 0)
}

/// The first sound track's 'trak' and its media's time scale (from 'mdhd').
fn sound_track(b: &[u8], moov: Bx) -> Option<(Bx, u32)> {
    children(b, moov.body, moov.end).into_iter().filter(|t| &t.kind == b"trak").find_map(|t| {
        let mdia = child(b, t, b"mdia")?;
        let hdlr = child(b, mdia, b"hdlr")?;
        if b.get(hdlr.body + 8..hdlr.body + 12)? != b"soun" {
            return None;
        }
        let mdhd = child(b, mdia, b"mdhd")?;
        let scale = be32(b, mdhd.body + if b[mdhd.body] == 1 { 20 } else { 12 })?;
        (scale > 0).then_some((t, scale))
    })
}

/// The edit list of the file at `path`'s sound track: None when it has none (or isn't an MPEG-4 file). Empty
/// edits (a delay before the track) are passed over; the first that plays some of the track is the one.
pub fn read(path: &Path) -> Option<Edit> {
    let (b, _) = read_moov(path)?;
    let moov = children(&b, 0, b.len()).into_iter().next()?;
    let movie = movie_scale(&b, moov)? as f64;
    let (trak, media) = sound_track(&b, moov)?;
    let elst = child(&b, child(&b, trak, b"edts")?, b"elst")?;
    let v1 = b[elst.body] == 1;
    let n = be32(&b, elst.body + 4)? as usize;
    let size = if v1 { 20 } else { 12 };
    (0..n).find_map(|i| {
        let e = elst.body + 8 + i * size;
        let (dur, time) = if v1 { (be64(&b, e)?, be64(&b, e + 8)? as i64) } else { (be32(&b, e)? as u64, be32(&b, e + 4)? as i32 as i64) };
        (time >= 0).then(|| Edit { start: time as f64 / media as f64, length: dur as f64 / movie })
    })
}

/// Give the file at `path`'s sound track this edit list (replacing any it has), and the movie and track this
/// length. The audio isn't touched; when the movie comes before the audio, the audio's offsets move with it.
pub fn write(path: &Path, edit: Edit) -> std::io::Result<()> {
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("MPEG-4 edit list: {what}"));
    let mut file = std::fs::read(path)?;
    let top = children(&file, 0, file.len());
    let moov = *top.iter().find(|x| &x.kind == b"moov").ok_or_else(|| bad("no moov"))?;
    let movie = movie_scale(&file, moov).ok_or_else(|| bad("no mvhd"))?;
    let (trak, media) = sound_track(&file, moov).ok_or_else(|| bad("no sound track"))?;
    let length = (edit.length * movie as f64).round();
    let start = (edit.start * media as f64).round();
    if !(0.0..=u32::MAX as f64).contains(&length) || !(0.0..=i32::MAX as f64).contains(&start) {
        return Err(bad("out of range"));
    }
    let (length, start) = (length as u32, start as u32);

    // the new 'edts': one edit, the whole recording at normal speed
    let mut edts = Vec::with_capacity(36);
    edts.extend_from_slice(&36u32.to_be_bytes());
    edts.extend_from_slice(b"edts");
    edts.extend_from_slice(&28u32.to_be_bytes());
    edts.extend_from_slice(b"elst");
    edts.extend_from_slice(&[0, 0, 0, 0]);
    edts.extend_from_slice(&1u32.to_be_bytes());
    edts.extend_from_slice(&length.to_be_bytes());
    edts.extend_from_slice(&start.to_be_bytes());
    edts.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    // where it goes: in place of the old one, else after the track's header
    let (from, to) = match child(&file, trak, b"edts") {
        Some(old) => (old.at, old.end),
        None => {
            let tkhd = child(&file, trak, b"tkhd").ok_or_else(|| bad("no tkhd"))?;
            (tkhd.end, tkhd.end)
        }
    };
    let grow = edts.len() as i64 - (to - from) as i64;

    // what changes in place first (nothing moves yet): the lengths, and the audio's offsets if it's after the movie
    let set32 = |f: &mut Vec<u8>, i: usize, v: u32| f[i..i + 4].copy_from_slice(&v.to_be_bytes());
    let set64 = |f: &mut Vec<u8>, i: usize, v: u64| f[i..i + 8].copy_from_slice(&v.to_be_bytes());
    let mvhd = child(&file, moov, b"mvhd").ok_or_else(|| bad("no mvhd"))?;
    match file[mvhd.body] {
        1 => set64(&mut file, mvhd.body + 24, length as u64),
        _ => set32(&mut file, mvhd.body + 16, length),
    }
    if let Some(tkhd) = child(&file, trak, b"tkhd") {
        match file[tkhd.body] {
            1 => set64(&mut file, tkhd.body + 28, length as u64),
            _ => set32(&mut file, tkhd.body + 20, length),
        }
    }
    if grow != 0 && top.iter().any(|x| &x.kind == b"mdat" && x.at > moov.at) {
        for t in children(&file, moov.body, moov.end).into_iter().filter(|t| &t.kind == b"trak") {
            let Some(stbl) = child(&file, t, b"mdia").and_then(|m| child(&file, m, b"minf")).and_then(|m| child(&file, m, b"stbl")) else { continue };
            for co in children(&file, stbl.body, stbl.end) {
                let wide = match &co.kind { b"stco" => false, b"co64" => true, _ => continue };
                let n = be32(&file, co.body + 4).unwrap_or(0) as usize;
                for i in 0..n {
                    let e = co.body + 8 + i * if wide { 8 } else { 4 };
                    if wide {
                        let v = be64(&file, e).ok_or_else(|| bad("short co64"))?;
                        set64(&mut file, e, (v as i64 + grow) as u64);
                    } else {
                        let v = be32(&file, e).ok_or_else(|| bad("short stco"))?;
                        let moved = u32::try_from(v as i64 + grow).map_err(|_| bad("offset out of range"))?;
                        set32(&mut file, e, moved);
                    }
                }
            }
        }
    }
    // the boxes holding it grow (both start before it, so their size fields stay where they are)
    for parent in [moov, trak] {
        match be32(&file, parent.at) {
            Some(1) => {
                let v = be64(&file, parent.at + 8).unwrap_or(0);
                set64(&mut file, parent.at + 8, (v as i64 + grow) as u64);
            }
            Some(v) => {
                let grown = u32::try_from(v as i64 + grow).map_err(|_| bad("box too big"))?;
                set32(&mut file, parent.at, grown);
            }
            None => return Err(bad("short box")),
        }
    }
    file.splice(from..to, edts);
    std::fs::write(path, file)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// A take written by macOS's encoder (Core Audio keeps its priming in iTunes's metadata, not an edit list).
    fn take(dir: &Path, seconds: f64) -> std::path::PathBuf {
        let rate = 48_000.0;
        let pcm: Vec<i16> = (0..(rate * seconds) as usize).map(|i| ((i as f64 * 0.05).sin() * 8000.0) as i16).collect();
        let p = dir.join("mic.m4a");
        crate::takeaudio::write_m4a(&p, &pcm, rate, crate::takeaudio::TAKE_BIT_RATE).unwrap();
        p
    }

    #[test]
    fn an_edit_list_is_added_then_replaced_and_the_audio_still_decodes() {
        let d = std::env::temp_dir().join(format!("onkey-mp4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = take(&d, 3.0);
        assert_eq!(read(&p), None, "Core Audio writes none");
        let before = crate::takeaudio::decode_mono(&p).unwrap();

        write(&p, Edit { start: 2112.0 / 48_000.0, length: 3.0 }).unwrap();
        let e = read(&p).unwrap();
        assert!((e.start - 0.044).abs() < 1e-3 && (e.length - 3.0).abs() < 1e-3, "{e:?}");
        // a second write replaces it (no second edts)
        write(&p, Edit { start: 1024.0 / 48_000.0, length: 2.5 }).unwrap();
        let e = read(&p).unwrap();
        assert!((e.start - 1024.0 / 48_000.0).abs() < 1e-4 && (e.length - 2.5).abs() < 1e-3, "{e:?}");
        let b = std::fs::read(&p).unwrap();
        assert_eq!(b.windows(4).filter(|w| w == b"edts").count(), 1);

        // the audio is where it was: the file still decodes, and to the same samples where both play
        let (x, rate) = crate::takeaudio::decode_mono(&p).unwrap();
        assert_eq!(rate, 48_000.0);
        let n = x.len().min(before.0.len()).min(9_600);
        let diff = x[..n].iter().zip(&before.0[..n]).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        let shifted = (0..2200).any(|s| {
            x[s..n].iter().zip(&before.0[..n - s]).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max) < 1e-4
        }) || (0..2200).any(|s| before.0[s..n].iter().zip(&x[..n - s]).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max) < 1e-4);
        assert!(diff < 1e-4 || shifted, "decodes to other audio: {diff}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn not_an_mpeg4_file_has_no_edit_list_and_isnt_written() {
        let d = std::env::temp_dir().join(format!("onkey-mp4-bad-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("x.m4a");
        std::fs::write(&p, b"not a movie at all").unwrap();
        assert_eq!(read(&p), None);
        assert!(write(&p, Edit { start: 0.0, length: 1.0 }).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"not a movie at all");
        let _ = std::fs::remove_dir_all(&d);
    }
}
