// Pitch helpers shared by the lane, the take page and history. MIDI note numbers throughout (60 = C4).
import { CLOSE, GOOD, INK, OFF } from "./colors";
import { dateLocale, t } from "./i18n";
export const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
export const BLACK_KEYS = new Set([1, 3, 6, 8, 10]);

export const noteName = (m: number) => NOTE_NAMES[((Math.round(m) % 12) + 12) % 12] + (Math.floor(Math.round(m) / 12) - 1);

/** Signed cents error with octave errors forgiven, in [-600, 600). */
export const fold = (cents: number) => ((((cents + 600) % 1200) + 1200) % 1200) - 600;

/** Same rule as the engine's scoring at Relaxed: full credit within 50 cents, none beyond 150. */
export const credit = (err: number) => Math.max(0, Math.min(1, (150 - Math.abs(err)) / 100));

/** How strictly takes are scored: the engine's table (dsp.rs LEVELS). `full`/`zero`: cents for full and no credit;
 *  `centre`: frames your pitch's centre is judged over (vibrato averages out); 0: exact pitch. Its name and what it
 *  means are words (levelName, levelAbout), read when shown so they follow the window's language. */
export type Level = "relaxed" | "steady" | "pro";
export const LEVELS: Record<Level, { full: number; zero: number; centre: number }> = {
  relaxed: { full: 50, zero: 150, centre: 0 },
  steady: { full: 35, zero: 100, centre: 17 },
  pro: { full: 25, zero: 70, centre: 17 },
};
/** A level's name ("Relaxed"). */
export const levelName = (l: Level) => t(`music.levels.${l}.name`);
/** What a level means, in a sentence. */
export const levelAbout = (l: Level) => t(`music.levels.${l}.about`);
export const levelOf = (l: unknown): Level => (l === "steady" || l === "pro" ? l : "relaxed");
/** The best score of these takes, like with like: of the ones at the same level as the latest. */
export function bestAtLatestLevel<T extends { score: number; recorded: string; level?: string | null }>(takes: T[]): { best: number; level: string } | null {
  if (!takes.length) return null;
  const latest = takes.reduce((a, b) => (b.recorded > a.recorded ? b : a));
  const level = levelOf(latest.level);
  return { best: Math.max(...takes.filter(t => levelOf(t.level) === level).map(t => t.score)), level };
}
/** Credit for a cents error at a level (Relaxed: exactly `credit`). */
export const creditAt = (err: number, level: Level) => {
  if (level === "relaxed") return credit(err);
  const { full, zero } = LEVELS[level];
  return Math.max(0, Math.min(1, (zero - Math.abs(err)) / (zero - full)));
};
/** Your pitch's centre at frame f as the engine judges it at Steady and Pro (dsp.rs `centre`): the mean of the sung
 *  frames within `win` frames, leaving out those more than 150 cents from their median (the next note). */
export function centreAt(get: (f: number) => number, f: number, win: number) {
  const here = get(f);
  if (Number.isNaN(here) || win <= 1) return here;
  const h = Math.floor(win / 2), v: number[] = [];
  for (let k = f - h; k <= f + h; k++) { const x = get(k); if (!Number.isNaN(x)) v.push(x); }
  v.sort((a, b) => a - b);
  const n = v.length, m = n % 2 ? v[(n - 1) / 2] : (v[n / 2 - 1] + v[n / 2]) / 2;
  let s = 0, c = 0;
  for (const x of v) if (100 * Math.abs(x - m) < 150) { s += x; c++; }
  return c ? s / c : here;
}

export type Grade = "good" | "close" | "off" | "none";
export const grade = (err: number): Grade =>
  Number.isNaN(err) ? "none" : Math.abs(err) <= 50 ? "good" : Math.abs(err) <= 100 ? "close" : "off";

/** How a line (or a stretch) sang, from its accuracy (0-1) at the take's level: the bands drop as the level gets
 *  stricter, as its credit does, so a decent take at Pro still shows green. */
const LINE_BANDS: Record<Level, [number, number]> = { relaxed: [0.85, 0.65], steady: [0.75, 0.55], pro: [0.65, 0.45] };
export const lineGrade = (accuracy: number, level: Level): Grade => {
  const [good, close] = LINE_BANDS[level];
  return accuracy >= good ? "good" : accuracy >= close ? "close" : "off";
};

/** How far off, in plain words: "35¢ flat" under a semitone, then "about 4 semitones flat", "about an octave low". */
export function offBy(cents: number) {
  const c = Math.round(Math.abs(cents)), dir = cents > 0 ? "up" : "down";
  if (c < 100) return t("music.offBy.cents", { cents: c, dir });
  const semis = Math.round(c / 100);
  if (semis >= 11) return t("music.offBy.octaves", { count: Math.round(semis / 12), dir });
  return t("music.offBy.semitones", { count: semis, dir });
}

export const COLORS: Record<Grade, string> = { good: GOOD, close: CLOSE, off: OFF, none: INK[300] };

/** The app a track plays in, from its id. */
export const sourceName = (trackId: string) =>
  trackId.startsWith("demo:") ? "OnKey" : trackId.startsWith("music:") ? "Apple Music" : trackId.startsWith("youtube:") ? "YouTube" : "Spotify";

export const clock = (s: number) => {
  const t = Math.max(0, s);
  return `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;
};

/** Median of the defined values around frame f; steadies a pitch line without adding lag. */
export function smoothed(get: (f: number) => number, f: number, w = 3) {
  const v: number[] = [];
  for (let d = -w; d <= w; d++) {
    const x = get(f + d);
    if (!Number.isNaN(x)) v.push(x);
  }
  if (v.length < w) return NaN;
  v.sort((a, b) => a - b);
  return v[v.length >> 1];
}

export function scoreWord(score: number) {
  return t(`music.scoreWord.${score >= 90 ? "superb" : score >= 80 ? "great" : score >= 65 ? "good" : score >= 45 ? "gettingThere" : "keepAtIt"}`);
}

/** When a take was sung ("1 Nov, 14:05"), in the window's language (English: the computer's own date style). */
export function when(iso: string) {
  const d = new Date(iso), loc = dateLocale();
  return d.toLocaleDateString(loc, { day: "numeric", month: "short" }) + ", " +
    d.toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit" });
}
