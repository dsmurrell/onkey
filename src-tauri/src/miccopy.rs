//! Which inputs of the microphone's device carry the music instead of a voice. Some audio interfaces have "loopback"
//! inputs: a copy of whatever the computer plays (a Focusrite Scarlett's inputs 3 and 4 of 4). The capture takes the
//! voice from the loudest input (native/capture.swift), and with the song playing the loudest is the loopback, so the
//! "microphone" was the song itself and nobody's singing was heard (issue #2: four plays of a song, no voice in any).
//! OnKey has the music on its own (the tap), so an input that is a delayed copy of it can be told, and is never used.
//!
//! The capture hands over half a second of the music and of each input at about 6 kHz (`onkey_mic_snapshot`); the
//! inputs found to be copies go back as a bit mask (`onkey_mic_set_copies`). See capture::note_mic_channel.

/// Delays searched (samples at the snapshot's ~6 kHz): an input 5 ms ahead of the tap to 80 ms after it (a loopback
/// comes back after the device's output and input buffers).
const LAG_BEFORE: usize = 32;
const LAG_AFTER: usize = 480;

/// An input is a copy of the music from this likeness up, and stops being one below `NOT_COPY` (between them it
/// stays what it was). A loopback is the same samples (0.95 or more, less only for the delay falling between
/// samples); a microphone that hears the song from speakers is far from it (the room's echo, the voice on top).
const COPY: f32 = 0.85;
const NOT_COPY: f32 = 0.6;

/// The quietest music (RMS) inputs are judged on, and the quietest input (RMS) that can be a copy of it.
const MUSIC_MIN: f32 = 1e-3;
const INPUT_MIN: f32 = 1e-5;

/// The mean taken out and four samples averaged (so a delay that falls between two samples costs little).
fn smooth(x: &[f32]) -> Vec<f32> {
    let mean = x.iter().sum::<f32>() / x.len().max(1) as f32;
    (0..x.len().saturating_sub(3)).map(|i| 0.25 * (x[i] + x[i + 1] + x[i + 2] + x[i + 3]) - mean).collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// How like a delayed copy of `music` the input `x` is (0..1, both smoothed): the best normalised correlation over
/// the delays searched.
fn likeness(music: &[f32], x: &[f32]) -> f32 {
    let n = music.len().min(x.len());
    if n < 4 * LAG_AFTER {
        return 0.0;
    }
    let corr = |a: &[f32], b: &[f32]| -> f32 {
        let (mut ab, mut aa, mut bb) = (0f32, 0f32, 0f32);
        for (p, q) in a.iter().zip(b) {
            ab += p * q;
            aa += p * p;
            bb += q * q;
        }
        if aa <= 0.0 || bb <= 0.0 { 0.0 } else { ab / (aa * bb).sqrt() }
    };
    let mut best = 0f32;
    for lag in 0..=LAG_AFTER {
        best = best.max(corr(&music[..n - lag], &x[lag..n]));
    }
    for lag in 1..=LAG_BEFORE {
        best = best.max(corr(&music[lag..n], &x[..n - lag]));
    }
    best
}

/// The inputs that are copies of the music, as a bit mask (bit k: input k), given the mask so far. `left` and `right`
/// are the music, `inputs` the same stretch of each input. None when the music is too quiet to judge on.
pub fn copies(left: &[f32], right: &[f32], inputs: &[&[f32]], so_far: u32) -> Option<u32> {
    let mono: Vec<f32> = left.iter().zip(right).map(|(l, r)| 0.5 * (l + r)).collect();
    if rms(&mono) < MUSIC_MIN && rms(left) < MUSIC_MIN && rms(right) < MUSIC_MIN {
        return None;
    }
    let refs = [smooth(left), smooth(right), smooth(&mono)];
    let mut mask = so_far;
    for (k, x) in inputs.iter().enumerate().take(32) {
        // (an input with nothing on it while the music plays is no copy)
        let like = if rms(x) < INPUT_MIN { 0.0 } else {
            let x = smooth(x);
            refs.iter().map(|m| likeness(m, &x)).fold(0f32, f32::max)
        };
        if like >= COPY {
            mask |= 1 << k;
        } else if like < NOT_COPY {
            mask &= !(1 << k);
        }
    }
    Some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 3072;

    /// Something like music or a voice at 6 kHz: a few drifting tones and noise, different for each seed.
    fn sound(seed: u32, n: usize) -> Vec<f32> {
        let mut s = seed.wrapping_mul(2654435761).max(1);
        let mut rnd = move || { s ^= s << 13; s ^= s >> 17; s ^= s << 5; (s as f32 / u32::MAX as f32) - 0.5 };
        let tones: Vec<(f32, f32, f32)> = (0..5).map(|_| (80.0 + 900.0 * (rnd() + 0.5), 0.1 + 0.2 * (rnd() + 0.5), std::f32::consts::TAU * rnd())).collect();
        let mut low = 0f32;
        (0..n).map(|i| {
            let t = i as f32 / 6000.0;
            low = 0.8 * low + 0.2 * rnd();
            // (each tone comes and goes, as notes do)
            tones.iter().map(|(f, a, p)| a * (std::f32::consts::TAU * f * t + p).sin() * (0.5 + 0.5 * (3.0 * t * (1.0 + p.abs()) + p).sin())).sum::<f32>() + 0.3 * low
        }).collect()
    }

    fn delayed(x: &[f32], by: usize, gain: f32) -> Vec<f32> {
        (0..x.len()).map(|i| if i >= by { gain * x[i - by] } else { 0.0 }).collect()
    }

    /// What a microphone hears of `x` from speakers in a room: later, and smeared by 150 ms of echoes.
    fn through_a_room(x: &[f32]) -> Vec<f32> {
        let mut s = 12345u32;
        let ir: Vec<f32> = (0..900).map(|k| {
            s ^= s << 13; s ^= s >> 17; s ^= s << 5;
            (s as f32 / u32::MAX as f32 - 0.5) * (-(k as f32) / 250.0).exp() * if k < 30 { 0.0 } else { 1.0 }
        }).collect();
        (0..x.len()).map(|i| ir.iter().enumerate().filter(|(k, _)| *k <= i).map(|(k, h)| h * x[i - k]).sum::<f32>()).collect()
    }

    #[test]
    fn a_loopback_input_is_a_copy_of_the_music_and_a_microphone_is_not() {
        let (left, right) = (sound(1, N), sound(2, N));
        let voice = sound(3, N);
        // a Scarlett with 4 inputs: the mic on 1, nothing on 2, the loopback (left, right) on 3 and 4, 20 ms late
        let quiet: Vec<f32> = sound(4, N).iter().map(|v| 1e-6 * v).collect();
        let (l3, l4) = (delayed(&left, 123, 0.5), delayed(&right, 123, 0.5));
        assert_eq!(copies(&left, &right, &[&voice, &quiet, &l3, &l4], 0), Some(0b1100));
        // however loud or quiet the loopback, a mono one (left + right), and one that arrives just ahead of the tap
        let mono: Vec<f32> = left.iter().zip(&right).map(|(l, r)| 0.05 * (l + r)).collect();
        let ahead: Vec<f32> = left[10..].iter().copied().chain(std::iter::repeat(0.0).take(10)).collect();
        assert_eq!(copies(&left, &right, &[&mono, &ahead, &delayed(&left, 400, 2.0)], 0), Some(0b111));
        // a delay that falls between two samples
        let half: Vec<f32> = (0..N).map(|i| if i > 60 { 0.5 * (left[i - 60] + left[i - 61]) } else { 0.0 }).collect();
        assert_eq!(copies(&left, &right, &[&half], 0), Some(1));
    }

    #[test]
    fn a_microphone_that_hears_the_song_from_speakers_stays_a_microphone() {
        let (left, right) = (sound(1, N), sound(2, N));
        let mono: Vec<f32> = left.iter().zip(&right).map(|(l, r)| 0.5 * (l + r)).collect();
        let bleed = through_a_room(&mono);
        let voice = sound(3, N);
        let with_voice: Vec<f32> = bleed.iter().zip(&voice).map(|(b, v)| b + 0.5 * v).collect();
        // the song alone from the speakers (nobody singing yet), and with a voice on top
        assert_eq!(copies(&left, &right, &[&bleed, &with_voice], 0), Some(0));
        // an input already found to be a loopback stays one through a moment it is less like the music
        let murky: Vec<f32> = left.iter().zip(&voice).map(|(l, v)| l + 0.75 * v).collect();
        let like = likeness(&smooth(&left), &smooth(&murky));
        assert!((NOT_COPY..COPY).contains(&like), "{like}");
        assert_eq!(copies(&left, &right, &[&murky], 1), Some(1));
        assert_eq!(copies(&left, &right, &[&murky], 0), Some(0));
        // and one that turns out to be a voice after all is let back in
        assert_eq!(copies(&left, &right, &[&voice], 1), Some(0));
    }

    #[test]
    fn nothing_is_judged_without_music() {
        let hush: Vec<f32> = sound(1, N).iter().map(|v| 1e-5 * v).collect();
        let voice = sound(3, N);
        assert_eq!(copies(&hush, &hush, &[&voice], 0b10), None);
    }
}
