//! Takes' audio files on Linux. Decoding is done in Rust (Symphonia: WAV, AAC in .m4a, MP3, FLAC, Ogg Vorbis), an
//! .m4a's edit list followed (platform/mp4.rs: the AAC encoder's priming and padding left out), as macOS and
//! Windows decode them. Encoding AAC is not done yet: there's no AAC encoder OnKey can ship in Rust (the usual
//! one, FDK AAC, has a licence that needs deciding first), so `aac_write` and `aac_convert` fail and a take's
//! recording stays a WAV (takeaudio.rs keeps the WAV whenever converting fails: about 5.8 MB a minute instead of
//! 0.5 MB).
use std::path::Path;

/// "Core Audio" style error codes (four letters, as takeaudio.rs prints them).
const NO_ENCODER: i32 = i32::from_be_bytes(*b"noAC");
const BAD_FILE: i32 = i32::from_be_bytes(*b"!dec");

/// A whole file decoded.
pub struct Decoded {
    /// interleaved, `channels` a frame (or mono, as asked)
    pub samples: Vec<f32>,
    pub rate: f64,
    /// the file's channels (even when mixed to mono)
    pub channels: u32,
    pub frames: usize,
}

/// Everything in the file at `path` (its first audio track), as float, mixed to mono if `mono`.
pub fn decode(path: &Path, mono: bool) -> Result<Decoded, i32> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let f = std::fs::File::open(path).map_err(|_| BAD_FILE)?;
    let mss = MediaSourceStream::new(Box::new(f), Default::default());
    let mut hint = Hint::new();
    if let Some(e) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(e);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(|_| BAD_FILE)?;
    let mut format = probed.format;
    let track = format.default_track().ok_or(BAD_FILE)?.clone();
    let mut dec = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|_| BAD_FILE)?;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0) as f64;
    let mut ch = track.codec_params.channels.map(|c| c.count()).unwrap_or(0);
    let mut all: Vec<f32> = Vec::new();
    loop {
        let pkt = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(_) => return Err(BAD_FILE),
        };
        if pkt.track_id() != track.id {
            continue;
        }
        let d = match dec.decode(&pkt) {
            Ok(d) => d,
            // a damaged packet: skipped, as players do
            Err(Error::DecodeError(_)) => continue,
            Err(_) => return Err(BAD_FILE),
        };
        let spec = *d.spec();
        rate = spec.rate as f64;
        ch = spec.channels.count();
        let mut sb = SampleBuffer::<f32>::new(d.capacity() as u64, spec);
        sb.copy_interleaved_ref(d);
        all.extend_from_slice(sb.samples());
    }
    if ch == 0 || rate <= 0.0 {
        return Err(BAD_FILE);
    }
    let mut frames = all.len() / ch;
    // an .m4a's edit list: where the recording starts in the decoded track (the priming) and how long it is
    if let Some(e) = crate::platform::mp4::read(path) {
        let start = ((e.start * rate).round() as usize).min(frames);
        let len = ((e.length * rate).round() as usize).min(frames - start);
        all = all[start * ch..(start + len) * ch].to_vec();
        frames = len;
    }
    let samples = if mono && ch > 1 {
        all.chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect()
    } else {
        all
    };
    Ok(Decoded { samples, rate, channels: ch as u32, frames })
}

/// (No AAC encoder on Linux yet: see the module's note.)
pub fn aac_write(_samples: &[i16], _rate: f64, _bit_rate: u32, _path: &Path) -> Result<(), i32> {
    Err(NO_ENCODER)
}

/// (No AAC encoder on Linux yet: the WAV is kept.)
pub fn aac_convert(_src: &Path, _dst: &Path) -> Result<(), i32> {
    Err(NO_ENCODER)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_wav_decodes_to_its_samples_and_mono_is_the_mix() {
        let d = std::env::temp_dir().join(format!("onkey-af-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("mic.wav");
        let pcm: Vec<i16> = (0..48_000).map(|i| ((i as f64 * 0.05).sin() * 8000.0) as i16).collect();
        crate::engine::write_wav(&p, &pcm, 48_000).unwrap();
        let x = super::decode(&p, true).unwrap();
        assert_eq!((x.frames, x.rate, x.channels), (48_000, 48_000.0, 1));
        assert!((x.samples[100] - pcm[100] as f32 / 32768.0).abs() < 1e-4);
        assert!(super::decode(&d.join("missing.wav"), true).is_err());
        std::fs::write(d.join("bad.m4a"), b"not audio").unwrap();
        assert!(super::decode(&d.join("bad.m4a"), true).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
