//! The tuner: sing any note, with no song playing, and see which note it is. The mic alone is listened to (the same
//! listener as setup's mic check, permissions.rs `meter`: the mic OnKey's takes use, its loudest input that doesn't
//! carry the music), and your pitch is found exactly as in a take (CREPE-tiny every 10 ms, then `LivePitch`: kept when
//! confident, above the room's background and in a voice's range).
//!
//! While it listens, the engine follows no music (`on`, as with "Pause listening": a take under way ends and is
//! scored), and takes up again when the page is left.
//!
//! The window asks for the new pitches about 30 times a second while its Tuner page is open (`read`); nothing runs
//! between asks, and nothing is kept: no audio, no record of what was sung.
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::pitch::{Crepe, HOP, WIN};
use crate::platform::native::{onkey_mic_level, onkey_mic_meter, onkey_mic_samples};
use crate::resample::Resampler;
use crate::take::LivePitch;

const SR: usize = 16_000;

struct Tuner {
    crepe: Crepe,
    /// the mic's rate (it can change with the device) and its resampler to 16 kHz
    rs: Option<(f64, Resampler)>,
    /// the latest sound at 16 kHz (two seconds at most), and how many samples came before it
    x16: Vec<f32>,
    before: usize,
    /// pitch frames found so far
    done: usize,
    live: LivePitch,
}

static TUNER: Mutex<Option<Tuner>> = Mutex::new(None);
/// Whether the tuner is listening. While it is, the engine follows no music, exactly as with "Pause listening"
/// (engine.rs): a song playing would start a take behind the Tuner page, and the mic is the tuner's.
static ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn on() -> bool {
    ON.load(std::sync::atomic::Ordering::Relaxed)
}

/// Start listening. Err says why not, for the window: "mic" (no microphone, or it wouldn't open) or "model".
pub fn start() -> Result<(), &'static str> {
    let mut t = TUNER.lock().unwrap_or_else(|e| e.into_inner());
    if t.is_some() {
        return Ok(());
    }
    if unsafe { onkey_mic_meter(1) } != 0 {
        return Err("mic");
    }
    let crepe = Crepe::new().map_err(|e| {
        crate::engine::log(&format!("tuner: the pitch model couldn't load: {e}"));
        unsafe { onkey_mic_meter(0) };
        "model"
    })?;
    *t = Some(Tuner { crepe, rs: None, x16: Vec::new(), before: 0, done: 0, live: LivePitch::default() });
    ON.store(true, std::sync::atomic::Ordering::Relaxed);
    crate::engine::log("tuner: listening to the mic");
    Ok(())
}

pub fn stop() {
    ON.store(false, std::sync::atomic::Ordering::Relaxed);
    if TUNER.lock().unwrap_or_else(|e| e.into_inner()).take().is_some() {
        unsafe { onkey_mic_meter(0) };
        crate::engine::log("tuner: stopped");
    }
}

/// The pitches since the last ask: `{on, level, notes}`. `notes` has one entry per 10 ms, oldest first: the MIDI note
/// number sung (69 = A4, with its fraction: 69.5 is A4 50 cents sharp) or null when no note was heard. `level` is the
/// mic's level now (dBFS).
pub fn read() -> Value {
    let mut guard = TUNER.lock().unwrap_or_else(|e| e.into_inner());
    let Some(t) = guard.as_mut() else { return json!({"on": false, "level": -120.0, "notes": []}) };
    let mut buf = vec![0f32; 48_000];
    let mut rate = 0f64;
    let n = unsafe { onkey_mic_samples(buf.as_mut_ptr(), buf.len() as i32, &mut rate) }.max(0) as usize;
    let level = unsafe { onkey_mic_level() };
    let notes = if n > 0 && rate >= 8000.0 { t.heard(&buf[..n], rate) } else { Vec::new() };
    json!({"on": true, "level": level, "notes": notes.iter().map(|m| if m.is_finite() { json!((*m as f64 * 100.0).round() / 100.0) } else { Value::Null }).collect::<Vec<_>>()})
}

impl Tuner {
    /// More of the mic's sound (at `rate`): the pitch of each new 10 ms frame (NaN: no note).
    fn heard(&mut self, x: &[f32], rate: f64) -> Vec<f32> {
        if self.rs.as_ref().is_none_or(|(r, _)| *r != rate) {
            self.rs = Some((rate, Resampler::new(rate, SR as f64)));
        }
        if let Some((_, rs)) = self.rs.as_mut() {
            rs.process(x, &mut self.x16);
        }
        let total = self.before + self.x16.len();
        if total < WIN {
            return Vec::new();
        }
        // (as a take does it: the newest full frame is centred half a window before the end)
        let newest = (total - WIN / 2) / HOP;
        let mut out = Vec::new();
        if newest > self.done {
            // (asked after a long gap, a sleeping window: only the latest second is worth showing)
            let k = (newest - self.done).min(100);
            let tail = &self.x16[self.x16.len().saturating_sub(SR)..];
            if let Ok(fs) = self.crepe.frames(tail, k) {
                out = self.live.push(&fs);
            }
            self.done = newest;
        }
        if self.x16.len() > 2 * SR {
            let cut = self.x16.len() - SR;
            self.x16.drain(..cut);
            self.before += cut;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sung-like tone (a few harmonics) at `midi`, `secs` long at `rate`.
    fn tone(midi: f32, secs: f32, rate: f64) -> Vec<f32> {
        let f = 440.0 * 2f32.powf((midi - 69.0) / 12.0);
        (0..(secs * rate as f32) as usize).map(|i| {
            let t = i as f32 / rate as f32;
            (1..=4).map(|h| (std::f32::consts::TAU * f * h as f32 * t).sin() / h as f32).sum::<f32>() * 0.2
        }).collect()
    }

    fn tuner() -> Tuner {
        Tuner { crepe: Crepe::tiny_with(|b| b.with_intra_threads(1)).unwrap(), rs: None, x16: Vec::new(), before: 0, done: 0, live: LivePitch::default() }
    }

    #[test]
    fn a_held_note_is_named_within_a_few_cents_at_any_mic_rate() {
        for (rate, midi) in [(48_000.0, 57.0f32), (44_100.0, 64.3), (96_000.0, 72.0)] {
            let mut t = tuner();
            let x = tone(midi, 1.5, rate);
            let mut notes = Vec::new();
            // (as the window asks: a little at a time)
            for block in x.chunks((rate / 30.0) as usize) {
                notes.extend(t.heard(block, rate));
            }
            let sung: Vec<f32> = notes.iter().copied().filter(|m| m.is_finite()).collect();
            assert!(notes.len() >= 130 && sung.len() * 10 >= notes.len() * 8, "{rate}: {} frames, {} sung", notes.len(), sung.len());
            let mut s = sung.clone();
            s.sort_by(|a, b| a.total_cmp(b));
            let median = s[s.len() / 2];
            assert!((median - midi).abs() < 0.15, "{rate} Hz mic, sang {midi}: heard {median}");
        }
    }

    #[test]
    fn silence_is_no_note_and_the_sound_kept_stays_small() {
        let mut t = tuner();
        let mut notes = Vec::new();
        for _ in 0..600 {
            notes.extend(t.heard(&vec![0.0; 1600], 48_000.0));
        }
        assert!(notes.len() > 1900 && notes.iter().all(|m| !m.is_finite()));
        assert!(t.x16.len() <= 2 * SR + 1600);
    }
}
