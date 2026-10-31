//! Music and mic into the engine's [music L, music R, mic] frames on Windows (windows/audio.rs `pump`), where they
//! come from two WASAPI clients that aren't sample-aligned the way macOS's aggregate device makes them. The mic's
//! frames set the pace; the music is held back by a fixed cushion so a loopback packet a little late (Windows'
//! timer, scheduling) is still there when it's due, and the music's offset from the mic stays the same all take
//! (the engine learns it once, as an output's latency). Plain arithmetic, so tested on macOS too.
use std::collections::VecDeque;

pub struct Interleave {
    /// the music captured and not yet given: L, R, L, R…
    pub music: VecDeque<f32>,
    /// frames of music held back before it's given (the cushion), and how far over it it may run before the
    /// excess is cut (the music's clock running fast)
    cushion: usize,
    slack: usize,
    /// the cushion is there: music is given (else silence, while it fills)
    primed: bool,
}

/// What a `take` had to do to the music: cut some that ran ahead, or ran short (a gap of silence, then the
/// cushion fills again).
#[derive(Debug, Default, PartialEq)]
pub struct Took {
    pub cut: bool,
    pub short: bool,
}

impl Interleave {
    /// At `rate` Hz: a 50 ms cushion, cut back to it past 100 ms.
    pub fn new(rate: u32) -> Interleave {
        let cushion = rate as usize / 20;
        Interleave { music: VecDeque::new(), cushion, slack: cushion, primed: false }
    }

    /// `n` frames onto `out`, the mic's from `mic` (silence where it has none) and the music's from `self.music`.
    pub fn take(&mut self, n: usize, mic: &mut VecDeque<f32>, out: &mut Vec<f32>) -> Took {
        let mut took = Took::default();
        if n == 0 {
            return took;
        }
        let have = self.music.len() / 2;
        if !self.primed && have >= self.cushion {
            self.primed = true;
        }
        let give = if !self.primed {
            0
        } else if have < n {
            // late (or stopped): what there is, then silence, and wait for the cushion again
            took.short = have > 0;
            self.primed = false;
            have
        } else {
            let over = have - n;
            if over > self.cushion + self.slack {
                self.music.drain(..(over - self.cushion) * 2);
                took.cut = true;
            }
            n
        };
        for i in 0..n {
            let (l, r) = if i < give { (self.music.pop_front().unwrap_or(0.0), self.music.pop_front().unwrap_or(0.0)) } else { (0.0, 0.0) };
            out.extend_from_slice(&[l, r, mic.pop_front().unwrap_or(0.0)]);
        }
        took
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const TICK: usize = 480;      // 10 ms

    /// Music frames numbered 1, 2, 3… (L = R = the number), so where each lands in the output says its offset.
    fn music(it: &mut Interleave, from: &mut usize, frames: usize) {
        for _ in 0..frames {
            *from += 1;
            it.music.extend([*from as f32, *from as f32]);
        }
    }

    /// The offset (frames) of the music in the output: output frame index minus the music frame's own number.
    fn offsets(out: &[f32], start: usize) -> Vec<i64> {
        out.chunks_exact(3).enumerate().filter(|(_, f)| f[0] > 0.0).map(|(i, f)| (start + i) as i64 - f[0] as i64).collect()
    }

    #[test]
    fn late_packets_neither_gap_the_music_nor_move_it() {
        let mut it = Interleave::new(RATE);
        let (mut next, mut given, mut all) = (0usize, 0usize, Vec::new());
        let mut mic = VecDeque::new();
        for tick in 0..300 {
            // the loopback's packets: on time, except every 7th tick's arrives a tick late, and every 13th two ticks
            let late = tick % 7 == 3 || tick % 13 == 5;
            if !late {
                let behind = (tick + 1) * TICK - next;
                music(&mut it, &mut next, behind);
            }
            let mut out = Vec::new();
            let took = it.take(TICK, &mut mic, &mut out);
            assert!(!took.cut, "tick {tick}");
            if tick > 10 {
                assert!(!took.short, "tick {tick}: a late packet left a gap");
            }
            all.extend(offsets(&out, given));
            given += TICK;
        }
        let first = all[0];
        assert!(all.iter().all(|&o| o == first), "the music's offset from the mic moved");
        assert!((first as usize) <= TICK * 6, "held back {first} frames");
    }

    #[test]
    fn music_running_fast_is_cut_back_to_the_cushion() {
        let mut it = Interleave::new(RATE);
        let (mut next, mut mic) = (0usize, VecDeque::new());
        let mut cuts = 0;
        for _ in 0..200 {
            music(&mut it, &mut next, TICK + 24);       // 5% fast
            let mut out = Vec::new();
            cuts += it.take(TICK, &mut mic, &mut out).cut as u32;
            assert!(it.music.len() / 2 <= RATE as usize / 10 + TICK, "backlog {}", it.music.len() / 2);
        }
        assert!(cuts > 0);
    }

    #[test]
    fn music_that_stops_and_starts_again_keeps_its_offset() {
        let mut it = Interleave::new(RATE);
        let (mut next, mut given, mut mic) = (0usize, 0usize, VecDeque::new());
        let mut offs = Vec::new();
        for tick in 0..120 {
            // nothing plays for ticks 40..70 (the loopback sends nothing); the music's numbering skips that time
            if (40..70).contains(&tick) {
                next += TICK;
            } else {
                music(&mut it, &mut next, TICK);
            }
            let mut out = Vec::new();
            it.take(TICK, &mut mic, &mut out);
            offs.extend(offsets(&out, given).into_iter().map(|o| (tick, o)));
            given += TICK;
        }
        let before = offs.iter().find(|(t, _)| *t < 40).unwrap().1;
        let after = offs.iter().rev().find(|(t, _)| *t >= 80).unwrap().1;
        assert_eq!(before, after);
    }

    #[test]
    fn the_mic_is_given_as_it_came_with_silence_for_any_missing() {
        let mut it = Interleave::new(RATE);
        let mut mic: VecDeque<f32> = [0.5, 0.25].into();
        let mut out = Vec::new();
        it.take(3, &mut mic, &mut out);
        assert_eq!(out, vec![0.0, 0.0, 0.5, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0]);
        assert_eq!(it.take(0, &mut mic, &mut out), Took::default());
    }
}
