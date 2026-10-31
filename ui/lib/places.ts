// Where a take's recording sits in the song. A take is placed in stretches: a jump in the song (a skip, or the
// music app landing a little off after one) starts a new stretch with its own place (engine Take::stretches, kept
// in the take's meta.json as `places`: [from, to) take frames and that stretch's song offset in frames).
// A take kept without them has one place throughout: its song offset.

/** [from, to) take frames and the stretch's song offset, in frames */
export type Place = [number, number, number];

const FRAME_S = 0.01;

/** The song offset (s) of the stretch holding take time `t` (s). */
export function offsetAt(t: number, places: Place[] | null | undefined, songOffset: number): number {
  const f = t / FRAME_S;
  const p = places?.find(([a, b]) => f >= a && f < b);
  return p ? p[2] * FRAME_S : songOffset;
}

/** Song time (s) at take time `t` (s). */
export function songAtTake(t: number, places: Place[] | null | undefined, songOffset: number): number {
  return t + offsetAt(t, places, songOffset);
}

/** Take time (s) at song time `song` (s): in the first stretch that holds it; outside them all, by the song
 *  offset (the place at the end of the take). */
export function takeAtSong(song: number, places: Place[] | null | undefined, songOffset: number): number {
  for (const [a, b, k] of places ?? []) {
    const t = song - k * FRAME_S;
    if (t / FRAME_S >= a && t / FRAME_S < b) return t;
  }
  return song - songOffset;
}
