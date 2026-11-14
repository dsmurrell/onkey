// The tuner page's sums (app/tuner/page.tsx): what a pitch is called, how far off it is, whether a note is being
// held on a target, and which notes the picture shows. The pitches come from the engine (src-tauri/src/tuner.rs):
// one every 10 ms, a MIDI note number with its fraction (69 = A4, 69.5 = A4 and 50 cents), or null for no note.

export const FRAME_MS = 10;
const NAMES = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];

/** A note number's name: 60 is "C4" (middle C), 69 "A4". */
export const noteName = (midi: number) => NAMES[((Math.round(midi) % 12) + 12) % 12] + (Math.floor(Math.round(midi) / 12) - 1);
export const isBlackKey = (midi: number) => [1, 3, 6, 8, 10].includes(((Math.round(midi) % 12) + 12) % 12);
export const hzOf = (midi: number) => 440 * Math.pow(2, (midi - 69) / 12);

export type Reading = { note: number; name: string; cents: number; hz: number };

/** What a pitch is: its nearest note, and how many cents above (+) or below (-) that note it is (-50..50). */
export function reading(midi: number): Reading {
  const note = Math.round(midi);
  return { note, name: noteName(note), cents: Math.round((midi - note) * 100), hz: hzOf(midi) };
}

/** The pitch to show as "now": the middle one of the last 80 ms of notes sung (a single stray frame doesn't flick
 *  the name), or null when fewer than three of those frames had a note. */
export function steady(notes: (number | null)[]): number | null {
  const last = notes.slice(-8).filter((m): m is number => m != null);
  if (last.length < 3) return null;
  const s = [...last].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

/** Holding a target note: fed each new frame, it counts the time sung within `tolerance` cents of `target`. A slip
 *  or a breath of up to 0.3 s is forgiven; longer, and the count starts again. */
export class Hold {
  heldMs = 0;
  private awayMs = 0;
  private sum = 0;
  private n = 0;
  readonly target: number;
  readonly tolerance: number;
  readonly needMs: number;
  constructor(target: number, tolerance: number, needMs: number) {
    this.target = target; this.tolerance = tolerance; this.needMs = needMs;
  }
  get done() { return this.heldMs >= this.needMs; }
  /** 0..1 of the way there. */
  get progress() { return Math.min(1, this.heldMs / this.needMs); }
  /** The average of the cents off while on the note (+ sharp, - flat), once anything has been held. */
  get average() { return this.n ? Math.round(this.sum / this.n) : 0; }
  push(midi: number | null) {
    if (this.done) return;
    const cents = midi == null ? null : (midi - this.target) * 100;
    if (cents != null && Math.abs(cents) <= this.tolerance) {
      this.heldMs += FRAME_MS;
      this.awayMs = 0;
      this.sum += cents;
      this.n += 1;
    } else {
      this.awayMs += FRAME_MS;
      if (this.awayMs > 300) { this.heldMs = 0; this.sum = 0; this.n = 0; }
    }
  }
}

export type View = { lo: number; hi: number };

/** The notes the picture shows (`span` semitones), moved just enough to keep `midi` at least 3 semitones inside it
 *  and never by more than `step` at once, so the picture glides after a singer who moves up or down. */
export function follow(view: View, midi: number | null, step = 0.5): View {
  if (midi == null) return view;
  const margin = 3;
  let d = 0;
  if (midi > view.hi - margin) d = midi - (view.hi - margin);
  else if (midi < view.lo + margin) d = midi - (view.lo + margin);
  d = Math.max(-step, Math.min(step, d));
  return { lo: view.lo + d, hi: view.hi + d };
}

/** A note to try: a white key within `lo..hi` (your comfortable range when it's known), not the one just given. */
export function pickNote(lo: number, hi: number, not: number | null, random: () => number = Math.random): number {
  const keys: number[] = [];
  for (let m = Math.ceil(lo); m <= Math.floor(hi); m++) if (!isBlackKey(m) && m !== not) keys.push(m);
  if (!keys.length) return Math.round((lo + hi) / 2);
  return keys[Math.floor(random() * keys.length) % keys.length];
}

/** Short patterns to sing, as steps in semitones from the note they start on (major: do re mi fa so and back; the
 *  chord's notes up to the octave and back; a jump of an octave). */
export const PATTERNS = {
  five: [0, 2, 4, 5, 7, 5, 4, 2, 0],
  arpeggio: [0, 4, 7, 12, 7, 4, 0],
  octave: [0, 12, 0],
} as const;
export type Pattern = keyof typeof PATTERNS;

/** The notes of a pattern from the note it starts on. */
export const patternFrom = (pattern: Pattern, start: number) => PATTERNS[pattern].map(s => Math.round(start) + s);

/** The notes of a pattern placed in `lo..hi` (your comfortable range): started on a white key that keeps all of it
 *  in range when there is one, else as low as fits, else from `lo` (a range narrower than the pattern). */
export function patternNotes(pattern: Pattern, lo: number, hi: number, random: () => number = Math.random): number[] {
  const steps = PATTERNS[pattern], top = Math.max(...steps);
  const first = Math.ceil(lo), last = Math.floor(hi) - top;
  const starts: number[] = [];
  for (let m = first; m <= last; m++) if (!isBlackKey(m)) starts.push(m);
  const start = starts.length ? starts[Math.floor(random() * starts.length) % starts.length] : first;
  return steps.map(s => start + s);
}

/** Singing a pattern: each note in turn must be held on (within `tolerance` cents, for `needMs`) before the next
 *  comes up. `results` has a note's average cents off once it's done, or null for one skipped. */
export class Sequence {
  readonly notes: number[];
  readonly tolerance: number;
  readonly needMs: number;
  at = 0;
  results: (number | null)[] = [];
  private hold: Hold | null;
  constructor(notes: number[], tolerance: number, needMs = 800) {
    this.notes = notes; this.tolerance = tolerance; this.needMs = needMs;
    this.hold = notes.length ? new Hold(notes[0], tolerance, needMs) : null;
  }
  get done() { return this.at >= this.notes.length; }
  /** The note to sing now (null once done). */
  get target() { return this.done ? null : this.notes[this.at]; }
  /** 0..1 of the way through holding the note to sing now. */
  get progress() { return this.hold?.progress ?? 0; }
  get hit() { return this.results.filter(r => r != null).length; }
  private next(result: number | null) {
    this.results.push(result);
    this.at += 1;
    this.hold = this.done ? null : new Hold(this.notes[this.at], this.tolerance, this.needMs);
  }
  /** A new frame of your pitch; true when it finished a note. */
  push(midi: number | null): boolean {
    const h = this.hold;
    if (!h) return false;
    h.push(midi);
    if (!h.done) return false;
    this.next(h.average);
    return true;
  }
  /** Give up on the note to sing now and go on to the next. */
  skip() { if (!this.done) this.next(null); }
}
