//! "Share this take" on Windows: the frames drawn by share_draw.rs (what native/share.swift draws on macOS),
//! encoded with Media Foundation's sink writer to an .mp4 as the Mac's is: H.264 at 1080x1920, 30 fps, about 6 Mbit/s,
//! with the voice (the .m4a share.rs wrote) copied in as it is, cut at the clip's end. Return codes as the Swift
//! one's: 0, or a negative step that failed.
use std::path::Path;

use ::windows::core::{GUID, HSTRING};
use ::windows::Win32::Media::MediaFoundation::*;

use crate::share_draw::{nv12, Fonts, Frames, Spec, FPS, H, W};

/// The video's bit rate, as the Mac's (AVVideoAverageBitRateKey).
const VIDEO_BIT_RATE: u32 = 6_000_000;
/// The voice's, when it has to be encoded again (it's copied as it is when it can be).
const AUDIO_BYTES_PER_S: u32 = 16_000;
/// Media Foundation's time unit: 100 ns.
const SECOND: i64 = 10_000_000;

/// Media Foundation, started for as long as this lives.
struct Mf;

impl Mf {
    fn start() -> Option<Mf> {
        super::com();
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.ok().map(|_| Mf)
    }
}

impl Drop for Mf {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

fn media_type(major: &GUID, sub: &GUID) -> ::windows::core::Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, major)?;
        t.SetGUID(&MF_MT_SUBTYPE, sub)?;
        Ok(t)
    }
}

fn pair(a: u32, b: u32) -> u64 {
    ((a as u64) << 32) | b as u64
}

/// The video as stored (H.264 High) or as given to the encoder (NV12, top-down).
fn video_type(sub: &GUID) -> ::windows::core::Result<IMFMediaType> {
    unsafe {
        let t = media_type(&MFMediaType_Video, sub)?;
        t.SetUINT64(&MF_MT_FRAME_SIZE, pair(W, H))?;
        t.SetUINT64(&MF_MT_FRAME_RATE, pair(FPS, 1))?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pair(1, 1))?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        t.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
        t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
        t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
        t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
        if *sub == MFVideoFormat_H264 {
            t.SetUINT32(&MF_MT_AVG_BITRATE, VIDEO_BIT_RATE)?;
            t.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
        } else {
            t.SetUINT32(&MF_MT_DEFAULT_STRIDE, W)?;
            t.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        }
        Ok(t)
    }
}

/// 16-bit PCM at `rate` with `channels`.
fn pcm_type(rate: u32, channels: u32) -> ::windows::core::Result<IMFMediaType> {
    unsafe {
        let t = media_type(&MFMediaType_Audio, &MFAudioFormat_PCM)?;
        t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
        t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 2 * channels)?;
        t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 2 * channels * rate)?;
        t.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        Ok(t)
    }
}

/// AAC (LC) at `rate` (44.1 or 48 kHz: what Windows' encoder takes) with `channels`, `bytes_per_s` (12000, 16000,
/// 20000 or 24000).
fn aac_type(rate: u32, channels: u32, bytes_per_s: u32) -> ::windows::core::Result<IMFMediaType> {
    unsafe {
        let t = media_type(&MFMediaType_Audio, &MFAudioFormat_AAC)?;
        t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
        t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_s)?;
        Ok(t)
    }
}

fn sample(bytes: &[u8], time: i64, duration: i64) -> ::windows::core::Result<IMFSample> {
    unsafe {
        let buf = MFCreateMemoryBuffer(bytes.len() as u32)?;
        let mut p = std::ptr::null_mut();
        buf.Lock(&mut p, None, None)?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        buf.Unlock()?;
        buf.SetCurrentLength(bytes.len() as u32)?;
        let s = MFCreateSample()?;
        s.AddBuffer(&buf)?;
        s.SetSampleTime(time)?;
        s.SetSampleDuration(duration)?;
        Ok(s)
    }
}

fn writer(out: &Path) -> ::windows::core::Result<IMFSinkWriter> {
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.unwrap();
        attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
        MFCreateSinkWriterFromURL(&HSTRING::from(out.as_os_str()), None, &attrs)
    }
}

/// The voice for the clip: its samples (times from 0, cut at `end`) and the type they're stored as.
struct Voice {
    samples: Vec<IMFSample>,
    stored: IMFMediaType,
    /// what's written to the writer: the stored type (copied as it is) or PCM (encoded to `stored`)
    given: IMFMediaType,
}

/// The voice in `audio`: copied as it is (`copy`) or decoded to PCM to be encoded again, up to `end` (100 ns).
fn voice(audio: &Path, end: i64, copy: bool) -> ::windows::core::Result<Voice> {
    unsafe {
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(audio.as_os_str()), None)?;
        let first = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        reader.SetStreamSelection(first, true)?;
        let native = reader.GetNativeMediaType(first, 0)?;
        let (stored, given) = if copy {
            (native.clone(), native)
        } else {
            // (Windows' AAC encoder takes 44.1 or 48 kHz: the reader resamples anything else)
            let rate = match native.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)? {
                r @ (44_100 | 48_000) => r,
                _ => 48_000,
            };
            let ch = native.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?.clamp(1, 2);
            let pcm = pcm_type(rate, ch)?;
            reader.SetCurrentMediaType(first, None, &pcm)?;
            (aac_type(rate, ch, AUDIO_BYTES_PER_S)?, pcm)
        };
        let block = given.GetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT).unwrap_or(0);
        let rate = given.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0) as i64;
        let mut samples = Vec::new();
        let mut t0 = None;
        loop {
            let (mut flags, mut time, mut s) = (0u32, 0i64, None);
            reader.ReadSample(first, 0, None, Some(&mut flags), Some(&mut time), Some(&mut s))?;
            if let Some(s) = s {
                // times from the file's first sample, so the voice starts with the clip
                let t = time - *t0.get_or_insert(time);
                if t >= end {
                    break;
                }
                s.SetSampleTime(t)?;
                let d = s.GetSampleDuration().unwrap_or(0);
                if !copy && block > 0 && rate > 0 && t + d > end {
                    // PCM over the end: cut to it (an AAC packet is kept whole, as the Mac's copy does)
                    let buf = s.ConvertToContiguousBuffer()?;
                    let keep = ((end - t) * rate / SECOND) as u32 * block;
                    buf.SetCurrentLength(keep.min(buf.GetCurrentLength()?))?;
                    s.SetSampleDuration(end - t)?;
                    samples.push(s);
                    break;
                }
                samples.push(s);
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
        Ok(Voice { samples, stored, given })
    }
}

/// A step that failed: its code (as the Swift one's numbers), and whether it was the voice's stream not taking the
/// file's AAC as it is (then it's encoded again).
struct Fail(i32, bool);

fn write(frames: &mut Frames, audio: &Path, out: &Path, copy: bool) -> Result<(), Fail> {
    let n = frames.spec().frames();
    let end = (frames.spec().seconds * SECOND as f64).round() as i64;
    let voice = voice(audio, end, copy).map_err(|_| Fail(-10, copy))?;
    let _ = std::fs::remove_file(out);
    let w = writer(out).map_err(|_| Fail(-2, false))?;
    unsafe {
        let video = w.AddStream(&video_type(&MFVideoFormat_H264).map_err(|_| Fail(-3, false))?).map_err(|_| Fail(-3, false))?;
        let input = video_type(&MFVideoFormat_NV12).map_err(|_| Fail(-3, false))?;
        // no B-frames: with them Microsoft's encoder starts the frames' times a frame late (the first CI run measured
        // 2.033 s of video for 60 frames), which would put the picture 33 ms behind the voice
        let mut enc = None;
        MFCreateAttributes(&mut enc, 1).map_err(|_| Fail(-3, false))?;
        let enc = enc.ok_or(Fail(-3, false))?;
        enc.SetUINT32(&CODECAPI_AVEncMPVDefaultBPictureCount, 0).map_err(|_| Fail(-3, false))?;
        w.SetInputMediaType(video, &input, &enc).map_err(|_| Fail(-3, false))?;
        let sound = w.AddStream(&voice.stored).map_err(|_| Fail(-10, copy))?;
        w.SetInputMediaType(sound, &voice.given, None).map_err(|_| Fail(-10, copy))?;
        w.BeginWriting().map_err(|_| Fail(-4, copy))?;
        let mut pending = voice.samples.iter().peekable();
        let at = |k: usize| k as i64 * SECOND / FPS as i64;
        for k in 0..n {
            // the voice up to this frame first, so the two are interleaved as they're written
            while let Some(s) = pending.next_if(|s| s.GetSampleTime().unwrap_or(0) <= at(k)) {
                w.WriteSample(sound, s).map_err(|_| Fail(-7, false))?;
            }
            let pix = frames.draw(k as f64 / FPS as f64);
            let s = sample(&nv12(pix.data(), W as usize, H as usize), at(k), at(k + 1) - at(k)).map_err(|_| Fail(-6, false))?;
            w.WriteSample(video, &s).map_err(|_| Fail(-7, false))?;
        }
        for s in pending {
            w.WriteSample(sound, s).map_err(|_| Fail(-7, false))?;
        }
        w.Finalize().map_err(|_| Fail(-8, false))
    }
}

/// Renders the clip described by `json` (share.rs) with the voice in `audio` (an .m4a) to `out` (.mp4); `icon`:
/// PNG bytes for the footer and end card.
pub fn render(json: &str, audio: &Path, out: &Path, icon: &[u8]) -> i32 {
    let Some(spec) = Spec::parse(json) else { return -1 };
    let Some(_mf) = Mf::start() else { return -2 };
    let mut frames = Frames::new(spec, Fonts::system(), icon);
    let r = match write(&mut frames, audio, out, true) {
        // the file's AAC not taken as it is: decoded and encoded again
        Err(Fail(_, true)) => write(&mut frames, audio, out, false),
        r => r,
    };
    match r {
        Ok(()) => 0,
        Err(Fail(code, _)) => {
            let _ = std::fs::remove_file(out);
            code
        }
    }
}

/// The tracks in a movie file: video count * 10 + audio count. -1 when it doesn't open.
pub fn tracks(path: &Path) -> i32 {
    let Some(_mf) = Mf::start() else { return -1 };
    let Ok(reader) = (unsafe { MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None) }) else { return -1 };
    let (mut v, mut a) = (0, 0);
    for i in 0.. {
        let Ok(t) = (unsafe { reader.GetNativeMediaType(i, 0) }) else { break };
        match unsafe { t.GetMajorType() } {
            Ok(m) if m == MFMediaType_Video => v += 1,
            Ok(m) if m == MFMediaType_Audio => a += 1,
            _ => {}
        }
    }
    v * 10 + a
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `secs` of a 440 Hz tone, mono at 48 kHz, written as a take's voice is (takeaudio, so mf.rs's AAC encoder).
    fn tone_m4a(path: &Path, secs: f64) {
        let rate = 48_000.0;
        let pcm: Vec<i16> = (0..(secs * rate) as usize)
            .map(|i| ((i as f64 * 440.0 * std::f64::consts::TAU / rate).sin() * 8000.0) as i16)
            .collect();
        let _ = std::fs::remove_file(path);
        crate::takeaudio::write_m4a(path, &pcm, rate, crate::takeaudio::TAKE_BIT_RATE).unwrap();
    }

    /// The samples in `path`'s first video or audio stream, and where the last one ends (s).
    fn contents(path: &Path, stream: MF_SOURCE_READER_CONSTANTS) -> (usize, f64) {
        let _mf = Mf::start().unwrap();
        unsafe {
            let r = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).unwrap();
            r.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).unwrap();
            r.SetStreamSelection(stream.0 as u32, true).unwrap();
            let (mut n, mut end) = (0, 0i64);
            loop {
                let (mut flags, mut time, mut s) = (0u32, 0i64, None);
                r.ReadSample(stream.0 as u32, 0, None, Some(&mut flags), Some(&mut time), Some(&mut s)).unwrap();
                if let Some(s) = s {
                    n += 1;
                    end = end.max(time + s.GetSampleDuration().unwrap_or(0));
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
            }
            (n, end as f64 / SECOND as f64)
        }
    }

    #[test]
    fn a_share_clip_renders_to_an_mp4_with_its_voice() {
        let dir = std::env::temp_dir().join(format!("onkey-share-win-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("voice.m4a");
        tone_m4a(&audio, 3.0);
        assert_eq!(tracks(&audio), 1, "the voice: one audio track");
        let line: Vec<_> = (0..200).map(|i| json!([i as f64 * 0.01, 60.0 + (i as f64 / 9.0).sin(), 30.0])).collect();
        let spec = json!({"title": "Test Song 🎤 歌", "artist": "Someone", "footer": "SingOnKey.app", "seconds": 2.0, "lo": 56.0,
                          "hi": 66.0, "notes": [[0.2, 0.9, 60.0, 1.0], [1.0, 1.8, 62.0, 0.2]], "line": line,
                          "end": ["Sing on key", "Free for Mac", "singonkey.app"], "end_s": 0.5});
        let out = dir.join("clip.mp4");
        let t = std::time::Instant::now();
        let icon = include_bytes!("../../../../design/share-icon.png");
        assert_eq!(render(&spec.to_string(), &audio, &out, icon), 0);
        eprintln!("rendered 2 s in {:.2} s", t.elapsed().as_secs_f64());
        assert!(std::fs::metadata(&out).unwrap().len() > 10_000);
        assert_eq!(tracks(&out), 11, "one video track and one audio track");
        let (frames, video_end) = contents(&out, MF_SOURCE_READER_FIRST_VIDEO_STREAM);
        assert_eq!(frames, 60, "2 s at 30 fps");
        assert!((video_end - 2.0).abs() < 0.01, "{video_end} s of video");
        let (_, audio_end) = contents(&out, MF_SOURCE_READER_FIRST_AUDIO_STREAM);
        assert!((1.8..=2.05).contains(&audio_end), "the voice cut at the clip's end: {audio_end} s");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_isnt_there_has_no_tracks() {
        assert_eq!(tracks(Path::new(r"C:\no\such\clip.mp4")), -1);
    }
}
