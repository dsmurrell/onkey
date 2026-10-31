// "Fade the guide" (Settings, off by default): on a song you've sung well before, your own pitch line and ball on
// the Sing screen fade, so you listen to yourself more and lean on the picture less. The target bars stay. The
// better you've done on the song, the less is shown; where you're more than about a semitone off, your line still
// shows in full, so a wrong note is never hidden. Pure (no React, no engine), so lib/guideFade.test.mjs can check
// it with `npm test`.

/** What a past take of the song gives us (TakeSummary's fields). `recorded`: local time, 2026-10-13T09:15:00. */
export type PastTake = { track: { id: string; name: string; artist: string }; recorded: string; score: number | null;
                         coverage?: number | null };

/** 0: the full guide. 1-3: fainter and fainter (see FADE_LEVELS). */
export type FadeLevel = 0 | 1 | 2 | 3;

/** How each level shows your pitch while you're within a semitone of the note: the line's and ball's opacity (1:
 *  as usual), and whether the "✓ On pitch" pill shows. More than a semitone off, everything shows as usual. */
export const FADE_LEVELS: Record<FadeLevel, { onPitch: number; okPill: boolean }> = {
  0: { onPitch: 1, okPill: true },
  1: { onPitch: 0.45, okPill: false },
  2: { onPitch: 0.18, okPill: false },
  3: { onPitch: 0, okPill: false },
};

/** Past this far off the note (cents), your line and ball show in full however faded the guide is. */
export const OFF_CENTS = 100;

/** A take with less singing heard than this (of the song's notes) doesn't count, as for the Library's best. */
const MIN_COVERAGE = 0.1;

const sameSong = (a: { name: string; artist: string }, b: { name: string; artist: string }) =>
  a.name.trim().toLowerCase() === b.name.trim().toLowerCase() && a.artist.trim().toLowerCase() === b.artist.trim().toLowerCase();

/** Scores of the takes sung of this song (by its id, or the same artist and title: a take may be under Spotify's
 *  id, the song kept by its ISRC), newest first. Takes with hardly any singing, or no score, are left out. */
export function songScores(takes: PastTake[], song: { id: string; name: string; artist: string }): number[] {
  return takes
    .filter(t => (t.track.id === song.id || sameSong(t.track, song)) && t.score != null && Number.isFinite(t.score)
                 && (t.coverage == null || t.coverage >= MIN_COVERAGE))
    .sort((a, b) => (a.recorded < b.recorded ? 1 : a.recorded > b.recorded ? -1 : 0))
    .map(t => t.score as number);
}

/** How well you know a song, from its scores (newest first): your best, or your last two takes' lower score plus 5,
 *  whichever is higher (so "best 80" and "last two 75+" both give 80). Null with no takes. */
export function songSkill(scores: number[]): number | null {
  if (!scores.length) return null;
  const best = Math.max(...scores);
  const lastTwo = scores.length >= 2 ? Math.min(scores[0], scores[1]) + 5 : -Infinity;
  return Math.max(best, lastTwo);
}

/** The fade for a song, from its scores (newest first): none under 80, then 1 from 80, 2 from 87, 3 from 93.
 *  If your last take was a struggle (15 or more under your best), it comes back one step: the guide returns
 *  when you need it. */
export function fadeLevel(scores: number[]): FadeLevel {
  const skill = songSkill(scores);
  if (skill == null) return 0;
  let level = skill >= 93 ? 3 : skill >= 87 ? 2 : skill >= 80 ? 1 : 0;
  if (level > 0 && scores[0] <= Math.max(...scores) - 15) level--;
  return level as FadeLevel;
}

/** How opaque your line or ball is at a moment, `errCents` off the note there (NaN: no note to judge against,
 *  which counts as on it: there's nothing to correct). */
export function pitchAlpha(level: FadeLevel, errCents: number): number {
  if (level === 0) return 1;
  if (!Number.isNaN(errCents) && Math.abs(errCents) > OFF_CENTS) return 1;
  return FADE_LEVELS[level].onPitch;
}

/** The fade in force on the Sing screen now (the Lane and the hint read it; the Sing page sets it per take). */
export const guide: { level: FadeLevel } = { level: 0 };
const heard = new Set<() => void>();
export function setGuideLevel(level: FadeLevel) {
  if (level === guide.level) return;
  guide.level = level;
  for (const f of heard) f();
}
/** Told when the fade changes (for useSyncExternalStore). */
export function onGuideLevel(f: () => void) { heard.add(f); return () => { heard.delete(f); }; }
