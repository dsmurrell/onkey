//! Takes' audio through Media Foundation, the counterpart of native/audiofile.swift: a take's voice to AAC .m4a
//! by its sink writer (Microsoft's AAC encoder), and any file it reads (AAC, WAV, MP3) decoded to float by its
//! source reader. Everything runs on the caller's thread.
//!
//! Microsoft's AAC encoder takes 44.1 or 48 kHz only (anything else is resampled here first) and 96, 128, 160 or
//! 192 kbps (the nearest to the bit rate asked for is used: a take is 96 kbps, not macOS's 64). It leaves its
//! priming in the file with nothing to say so, so after writing, the file gets an edit list (platform/mp4.rs)
//! saying where the voice starts and how long it is: measured once per rate by encoding a test sound and finding
//! it again in what decoding gives (`priming`), so it holds whatever the encoder and decoder do.
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ::windows::core::PCWSTR;
use ::windows::Win32::Media::MediaFoundation::*;

use crate::platform::mp4;

/// Media Foundation's errors are HRESULTs: passed on as the error code (negative), as the Swift side passes
/// Core Audio's. -50 (Core Audio's "bad parameter") where there's no HRESULT.
pub type Res<T> = Result<T, i32>;
pub const BAD: i32 = -50;

fn hr(e: ::windows::core::Error) -> i32 {
    e.code().0
}

/// Media Foundation started (once per process; never shut down) and COM on this thread.
fn startup() -> Res<()> {
    super::com();
    static UP: OnceLock<i32> = OnceLock::new();
    match *UP.get_or_init(|| unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) }.map_or_else(hr, |_| 0)) {
        0 => Ok(()),
        e => Err(e),
    }
}

fn wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// The rate a take is encoded at: 44.1 or 48 kHz as captured, 11.025 and 22.05 kHz up to 44.1, the rest 48.
pub fn aac_rate(rate: f64) -> f64 {
    if rate == 44_100.0 || rate == 48_000.0 {
        rate
    } else if rate == 11_025.0 || rate == 22_050.0 {
        44_100.0
    } else {
        48_000.0
    }
}

/// The encoder's bit rates (bits/s), nearest to `target` first.
fn bit_rates(target: u32) -> Vec<u32> {
    let mut r = vec![96_000u32, 128_000, 160_000, 192_000];
    r.sort_by_key(|b| b.abs_diff(target));
    r
}

/// An audio media type: `subtype` (PCM, float or AAC), mono or not, at `rate`, with its other numbers.
unsafe fn audio_type(subtype: &::windows::core::GUID, numbers: &[(&::windows::core::GUID, u32)]) -> ::windows::core::Result<IMFMediaType> {
    let t = MFCreateMediaType()?;
    t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    t.SetGUID(&MF_MT_SUBTYPE, subtype)?;
    for (k, v) in numbers {
        t.SetUINT32(*k, *v)?;
    }
    Ok(t)
}

/// A sink writer making a mono AAC .m4a at `path` from 16-bit mono PCM at `rate`: the first of the encoder's
/// bit rates (nearest `bit_rate` first) it takes.
unsafe fn sink_writer(path: &Path, rate: u32, bit_rate: u32) -> Res<IMFSinkWriter> {
    let mut last = BAD;
    for br in bit_rates(bit_rate) {
        let made = (|| -> ::windows::core::Result<IMFSinkWriter> {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 2)?;
            let attrs = attrs.ok_or_else(|| ::windows::core::Error::from(::windows::Win32::Foundation::E_FAIL))?;
            // (the file is a temp name without .m4a: the container is said, not guessed from it)
            attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
            attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            let url = wide(path);
            let w = MFCreateSinkWriterFromURL(PCWSTR(url.as_ptr()), None::<&IMFByteStream>, &attrs)?;
            let aac = audio_type(&MFAudioFormat_AAC, &[(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16), (&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate),
                (&MF_MT_AUDIO_NUM_CHANNELS, 1), (&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, br / 8)])?;
            let stream = w.AddStream(&aac)?;
            let pcm = audio_type(&MFAudioFormat_PCM, &[(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16), (&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate),
                (&MF_MT_AUDIO_NUM_CHANNELS, 1), (&MF_MT_AUDIO_BLOCK_ALIGNMENT, 2), (&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * 2)])?;
            w.SetInputMediaType(stream, &pcm, None::<&IMFAttributes>)?;
            w.BeginWriting()?;
            Ok(w)
        })();
        match made {
            Ok(w) => return Ok(w),
            Err(e) => last = hr(e),
        }
    }
    Err(last)
}

const CHUNK: usize = 16_384;

/// `pcm` (mono, at `rate`, one the encoder takes) as AAC at `path`, as the encoder leaves it (no edit list of ours).
fn encode(path: &Path, pcm: &[i16], rate: u32, bit_rate: u32) -> Res<()> {
    startup()?;
    unsafe {
        let w = sink_writer(path, rate, bit_rate)?;
        let at = |frames: usize| (frames as u128 * 10_000_000 / rate as u128) as i64;     // (100 ns units)
        for (k, chunk) in pcm.chunks(CHUNK).enumerate() {
            let sample = (|| -> ::windows::core::Result<IMFSample> {
                let bytes = (chunk.len() * 2) as u32;
                let buf = MFCreateMemoryBuffer(bytes)?;
                let mut p: *mut u8 = std::ptr::null_mut();
                buf.Lock(&mut p, None, None)?;
                std::ptr::copy_nonoverlapping(chunk.as_ptr() as *const u8, p, bytes as usize);
                buf.Unlock()?;
                buf.SetCurrentLength(bytes)?;
                let s = MFCreateSample()?;
                s.AddBuffer(&buf)?;
                let from = k * CHUNK;
                s.SetSampleTime(at(from))?;
                s.SetSampleDuration(at(from + chunk.len()) - at(from))?;
                Ok(s)
            })().map_err(hr)?;
            w.WriteSample(0, &sample).map_err(hr)?;
        }
        w.Finalize().map_err(hr)?;
    }
    Ok(())
}

/// Where the sound starts (frames) in what `decode` gives of a file `encode` made at `rate`, beyond any edit list
/// the file already has: measured once per rate (encoding 0.1 s of silence then a sweep, and finding the sweep).
fn priming(rate: u32) -> i64 {
    static KNOWN: Mutex<Vec<(u32, i64)>> = Mutex::new(Vec::new());
    let mut known = KNOWN.lock().unwrap();
    if let Some(p) = known.iter().find(|k| k.0 == rate) {
        return p.1;
    }
    let r = rate as f64;
    let lead = (0.1 * r) as usize;
    let sweep: Vec<i16> = (0..(0.5 * r) as usize).map(|i| {
        let t = i as f64 / r;
        (0.4 * 32767.0 * (std::f64::consts::TAU * (200.0 * t + 4800.0 * t * t)).sin()) as i16   // 200 Hz rising to 5 kHz
    }).collect();
    let mut input = vec![0i16; lead];
    input.extend(&sweep);
    input.extend(std::iter::repeat(0).take((0.2 * r) as usize));
    let tmp = std::env::temp_dir().join(format!("onkey-aac-priming-{}-{rate}.m4a", std::process::id()));
    let found = encode(&tmp, &input, rate, 96_000).and_then(|_| decode(&tmp, true)).map(|d| {
        let x = d.samples;
        // roughly where the sweep begins, then exactly: the lag that best lines it up, near there
        let onset = x.iter().position(|v| v.abs() > 0.02).unwrap_or(lead) as i64 - lead as i64;
        let window = &sweep[..sweep.len().min((0.2 * r) as usize)];
        let score = |lag: i64| -> f64 {
            window.iter().enumerate().map(|(i, s)| {
                let j = lead as i64 + lag + i as i64;
                if j < 0 || j as usize >= x.len() { 0.0 } else { *s as f64 * x[j as usize] as f64 }
            }).sum()
        };
        (onset - 512..=onset + 512).max_by(|a, b| score(*a).total_cmp(&score(*b))).unwrap_or(0)
    });
    let _ = std::fs::remove_file(&tmp);
    let p = match found {
        Ok(p) => p,
        Err(e) => {
            crate::engine::log(&format!("take audio: couldn't measure the AAC encoder's priming at {rate} Hz (error {e}); taken as none"));
            0
        }
    };
    known.push((rate, p));
    p
}

/// `pcm` (mono, at `rate`, one the encoder takes) to AAC at `path`, with an edit list saying where it is.
fn write_aac(path: &Path, pcm: &[i16], rate: u32, bit_rate: u32) -> Res<()> {
    // (nothing to write: a moment of silence, edited down to nothing, so the file is still a valid one)
    let silence = [0i16; 1024];
    encode(path, if pcm.is_empty() { &silence } else { pcm }, rate, bit_rate)?;
    let lag = priming(rate);
    let had = mp4::read(path).map_or(0.0, |e| e.start);
    let edit = mp4::Edit { start: (had + lag as f64 / rate as f64).max(0.0), length: pcm.len() as f64 / rate as f64 };
    mp4::write(path, edit).map_err(|_| BAD)
}

/// `x` (mono, at `from` Hz) at `to` Hz, the same length in time.
fn resample(x: &[f32], from: f64, to: f64) -> Vec<f32> {
    let mut r = crate::resample::Resampler::with_cutoff(from, to, 0.45 * from.min(to));
    let want = (x.len() as f64 * to / from).round() as usize;
    let mut out = Vec::with_capacity(want + 64);
    r.process(x, &mut out);
    // (the filter holds back its last few input samples until it sees what follows: silence)
    r.process(&vec![0.0; 64 + 32 * (from / to).ceil() as usize], &mut out);
    out.truncate(want);
    out
}

fn to_i16(x: &[f32]) -> Vec<i16> {
    x.iter().map(|v| (v * 32768.0).round().clamp(-32768.0, 32767.0) as i16).collect()
}

/// 16-bit mono samples at `rate` to a mono AAC .m4a at `path`, at about `bit_rate` bits/s (onkey_aac_write).
pub fn aac_write(samples: &[i16], rate: f64, bit_rate: u32, path: &Path) -> Res<()> {
    if rate <= 0.0 {
        return Err(BAD);
    }
    let to = aac_rate(rate);
    if to == rate {
        return write_aac(path, samples, rate as u32, bit_rate);
    }
    let x: Vec<f32> = samples.iter().map(|s| *s as f32 / 32768.0).collect();
    write_aac(path, &to_i16(&resample(&x, rate, to)), to as u32, bit_rate)
}

/// Any audio file Media Foundation reads to a mono AAC .m4a at `dst` (several channels mixed to one; resampled
/// when the encoder doesn't take its rate), at a take's bit rate (onkey_aac_convert).
pub fn aac_convert(src: &Path, dst: &Path) -> Res<()> {
    let d = decode(src, true)?;
    let to = aac_rate(d.rate);
    let x = if to == d.rate { d.samples } else { resample(&d.samples, d.rate, to) };
    write_aac(dst, &to_i16(&x), to as u32, crate::takeaudio::TAKE_BIT_RATE)
}

/// A whole file decoded.
pub struct Decoded {
    /// interleaved, `channels` a frame (or mono, as asked)
    pub samples: Vec<f32>,
    pub rate: f64,
    /// the file's channels (even when mixed to mono)
    pub channels: u32,
    pub frames: usize,
}

/// Everything in the file at `path` (its first audio stream), as float, mixed to mono if `mono`. An MPEG-4
/// file's edit list is followed (its priming and padding left out) when the decoder hasn't already.
pub fn decode(path: &Path, mono: bool) -> Res<Decoded> {
    startup()?;
    let first = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    let (mut samples, rate, channels) = unsafe {
        let url = wide(path);
        let r = MFCreateSourceReaderFromURL(PCWSTR(url.as_ptr()), None::<&IMFAttributes>).map_err(hr)?;
        r.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).map_err(hr)?;
        r.SetStreamSelection(first, true).map_err(hr)?;
        let float = audio_type(&MFAudioFormat_Float, &[]).map_err(hr)?;
        r.SetCurrentMediaType(first, None, &float).map_err(hr)?;
        let shape = || -> Res<(u32, u32)> {
            let t = r.GetCurrentMediaType(first).map_err(hr)?;
            Ok((t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).map_err(hr)?, t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).map_err(hr)?))
        };
        let (rate, mut ch) = shape()?;
        if rate == 0 || ch == 0 {
            return Err(BAD);
        }
        let channels = ch;
        let mut out = Vec::new();
        loop {
            let (mut flags, mut sample) = (0u32, None::<IMFSample>);
            r.ReadSample(first, 0, None, Some(&mut flags as *mut u32), None, Some(&mut sample as *mut Option<IMFSample>)).map_err(hr)?;
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                ch = shape()?.1.max(1);
            }
            if let Some(s) = sample {
                let buf = s.ConvertToContiguousBuffer().map_err(hr)?;
                let (mut p, mut len) = (std::ptr::null_mut::<u8>(), 0u32);
                buf.Lock(&mut p, None, Some(&mut len as *mut u32)).map_err(hr)?;
                let x = std::slice::from_raw_parts(p as *const f32, len as usize / 4);
                if mono {
                    out.extend(x.chunks_exact(ch as usize).map(|f| f.iter().sum::<f32>() / ch as f32));
                } else if ch == channels {
                    out.extend_from_slice(x);
                } else {
                    // (the channels changed mid-file: each frame fitted to the first count)
                    for f in x.chunks_exact(ch as usize) {
                        out.extend((0..channels as usize).map(|c| f[c.min(f.len() - 1)]));
                    }
                }
                buf.Unlock().map_err(hr)?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
        (out, rate as f64, channels)
    };
    let per = if mono { 1 } else { channels as usize };
    let mut frames = samples.len() / per;
    if let Some(e) = mp4::read(path) {
        let start = (e.start * rate).round() as usize;
        let length = (e.length * rate).round() as usize;
        // all of the track decoded (the priming still there), or only the edit's part (the decoder followed it)
        let whole = start > 0 && frames > length + start / 2;
        let from = if whole { start.min(frames) } else { 0 };
        let to = (from + length).min(frames);
        samples.truncate(to * per);
        samples.drain(..from * per);
        frames = to - from;
    }
    Ok(Decoded { samples, rate, channels, frames })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("onkey-mf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Notes of a sung-like tone with gaps, 16-bit.
    fn tone(rate: f64, seconds: f64) -> Vec<i16> {
        (0..(rate * seconds) as usize).map(|i| {
            let t = i as f64 / rate;
            if t % 0.5 < 0.35 { (8000.0 * (std::f64::consts::TAU * (220.0 + 40.0 * (t * 2.0).sin()) * t).sin()) as i16 } else { 0 }
        }).collect()
    }

    fn correlation(a: &[f32], b: &[f32]) -> f64 {
        let n = a.len().min(b.len());
        let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let (x, y) = (a[i] as f64, b[i] as f64);
            ab += x * y;
            aa += x * x;
            bb += y * y;
        }
        ab / (aa * bb).sqrt().max(1e-12)
    }

    #[test]
    fn aac_written_then_decoded_is_the_same_sound_in_step() {
        let d = dir("roundtrip");
        for rate in [48_000.0, 44_100.0, 16_000.0] {
            let pcm = tone(rate, 5.0);
            let p = d.join(format!("x-{rate}.m4a"));
            aac_write(&pcm, rate, 64_000, &p).unwrap();
            let got = decode(&p, true).unwrap();
            assert_eq!(got.channels, 1, "{rate}");
            assert_eq!(got.rate, aac_rate(rate), "{rate}");
            let want = (5.0 * got.rate) as usize;
            assert!(got.frames.abs_diff(want) < (0.01 * got.rate) as usize, "{rate}: {} frames, want {want}", got.frames);
            let src: Vec<f32> = if got.rate == rate {
                pcm.iter().map(|s| *s as f32 / 32768.0).collect()
            } else {
                resample(&pcm.iter().map(|s| *s as f32 / 32768.0).collect::<Vec<_>>(), rate, got.rate)
            };
            let c = correlation(&src, &got.samples);
            assert!(c > 0.9, "{rate}: correlation {c:.3} (out of step?)");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_stereo_wav_converts_to_mono_aac() {
        let d = dir("convert");
        let rate = 44_100u32;
        let mono = tone(rate as f64, 3.0);
        // a 16-bit stereo WAV, the voice in both channels
        let mut wav = Vec::new();
        let data = (mono.len() * 4) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for s in &mono {
            wav.extend_from_slice(&s.to_le_bytes());
            wav.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(d.join("mic.wav"), wav).unwrap();
        let w = decode(&d.join("mic.wav"), false).unwrap();
        assert_eq!((w.channels, w.rate, w.frames), (2, 44_100.0, mono.len()));
        aac_convert(&d.join("mic.wav"), &d.join("mic.m4a")).unwrap();
        let a = decode(&d.join("mic.m4a"), true).unwrap();
        assert_eq!((a.channels, a.rate), (1, 44_100.0));
        assert!(a.frames.abs_diff(mono.len()) < 441, "{} frames", a.frames);
        let src: Vec<f32> = mono.iter().map(|s| *s as f32 / 32768.0).collect();
        assert!(correlation(&src, &a.samples) > 0.9);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_or_broken_file_fails_cleanly() {
        let d = dir("missing");
        assert!(decode(&d.join("nothing-here.m4a"), true).is_err());
        std::fs::write(d.join("bad.m4a"), b"not audio at all").unwrap();
        assert!(decode(&d.join("bad.m4a"), true).is_err());
        assert!(aac_convert(&d.join("nothing-here.wav"), &d.join("out.m4a")).is_err());
        assert!(!d.join("out.m4a").exists());
        assert_eq!(aac_write(&[0; 10], 0.0, 64_000, &d.join("zero.m4a")), Err(BAD));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nothing_to_write_is_an_empty_recording() {
        let d = dir("empty");
        aac_write(&[], 48_000.0, 64_000, &d.join("e.m4a")).unwrap();
        assert_eq!(decode(&d.join("e.m4a"), true).unwrap().frames, 0);
        let _ = std::fs::remove_dir_all(&d);
    }
}
