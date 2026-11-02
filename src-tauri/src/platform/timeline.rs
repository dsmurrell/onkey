//! Where a song is from what a Windows media session's timeline says (windows/media.rs): its position as of when
//! it was last updated, run on by the time since, while it plays. An app updates the timeline only now and then
//! (Spotify: on play, pause and seek), so a song played through keeps the update time of its start; that's normal.
//! What isn't, and is then not run on (and not taken as exact: listening's alignment places the song instead):
//! - no update time at all;
//! - running on would put it past the song's end while it still plays (the update time is stale);
//! - the app moves the position without moving its update time (it doesn't keep that up at all).
//! Plain arithmetic, so tested on macOS too.

/// Windows' DateTime ticks a second.
const TICKS: f64 = 1e7;
/// How far past the end a run-on position may go (the song's last poll) before the update time is called stale.
const PAST_END_S: f64 = 2.0;

/// One app's timeline as last read.
#[derive(Debug, Default)]
pub struct Timeline {
    last: Option<(i64, i64)>,
    /// it moved its position without its update time once: its update time isn't used again
    ignores_time: bool,
}

impl Timeline {
    /// The timeline's `position` (ticks from its start) as of `updated` (DateTime, 0 when not said), read at `now`;
    /// `duration` in s (0 when unknown): (position s, exact).
    pub fn read(&mut self, position: i64, updated: i64, now: i64, playing: bool, duration: f64) -> (f64, bool) {
        if let Some((p, u)) = self.last {
            if p != position && u == updated && updated != 0 {
                self.ignores_time = true;
            }
        }
        self.last = Some((position, updated));
        let end = if duration > 0.0 { duration } else { f64::MAX };
        let raw = (position.max(0) as f64 / TICKS).min(end);
        if updated == 0 || self.ignores_time {
            return (raw, false);
        }
        if !playing {
            return (raw, true);
        }
        let run = position.max(0) as f64 / TICKS + (now - updated).max(0) as f64 / TICKS;
        if run > end + PAST_END_S {
            return (raw, false);
        }
        (run.min(end), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = 10_000_000;
    const T0: i64 = 134_000_000_000_000_000;

    #[test]
    fn a_song_played_through_runs_on_from_its_start() {
        let mut t = Timeline::default();
        // Spotify: updated when the song began (position 0), read 200 s later, still playing
        assert_eq!(t.read(0, T0, T0 + 200 * S, true, 240.0), (200.0, true));
        assert_eq!(t.read(0, T0, T0 + 201 * S, true, 240.0), (201.0, true));
        // paused at 201 s: the position as said
        assert_eq!(t.read(201 * S, T0 + 201 * S, T0 + 230 * S, false, 240.0), (201.0, true));
    }

    #[test]
    fn no_update_time_is_not_run_on() {
        let mut t = Timeline::default();
        assert_eq!(t.read(30 * S, 0, T0, true, 240.0), (30.0, false));
    }

    #[test]
    fn a_stale_update_time_does_not_pin_the_song_at_its_end() {
        let mut t = Timeline::default();
        // updated an hour ago, 30 s in: running on would be far past the end
        assert_eq!(t.read(30 * S, T0 - 3600 * S, T0, true, 240.0), (30.0, false));
        // (and just past the end, the last poll of a song, is still the end)
        let mut t = Timeline::default();
        assert_eq!(t.read(0, T0, T0 + 241 * S, true, 240.0), (240.0, true));
    }

    #[test]
    fn an_app_moving_its_position_but_not_its_update_time_is_taken_as_it_says() {
        let mut t = Timeline::default();
        assert_eq!(t.read(10 * S, T0, T0 + 10 * S, true, 240.0), (20.0, true));
        // the next poll: the position moved on, its update time didn't (it would read 10 + 10 + 0.4 s)
        assert_eq!(t.read(10 * S + 4 * S / 10, T0, T0 + 10 * S + 4 * S / 10, true, 240.0), (10.4, false));
        assert_eq!(t.read(11 * S, T0, T0 + 11 * S, true, 240.0), (11.0, false));
    }

    #[test]
    fn a_seek_moves_both_and_is_run_on() {
        let mut t = Timeline::default();
        t.read(0, T0, T0 + 5 * S, true, 240.0);
        assert_eq!(t.read(100 * S, T0 + 6 * S, T0 + 8 * S, true, 240.0), (102.0, true));
    }
}
