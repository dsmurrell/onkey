// Score trends, kept apart from the pages so they can be tested.

const mean = (v: number[]) => v.reduce((s, x) => s + x, 0) / v.length;

/** The rolling average of each point over it and the ones before it, up to `n` (fewer at the start). */
export function rolling(v: number[], n = 10): number[] {
  let sum = 0;
  return v.map((x, i) => {
    sum += x;
    if (i >= n) sum -= v[i - n];
    return sum / Math.min(i + 1, n);
  });
}

/** Your last `n` scores against the `n` before them (newest first), as rounded averages: `now` and the change.
 *  null with fewer than 2n scores. */
export function vsBefore(newestFirst: number[], n = 10): { now: number; change: number } | null {
  if (newestFirst.length < 2 * n) return null;
  const now = Math.round(mean(newestFirst.slice(0, n))), then = Math.round(mean(newestFirst.slice(n, 2 * n)));
  return { now, change: now - then };
}

/** In words: "up 4 on the 10 before", "down 3 …", "same as …". */
export const changeWords = (change: number, n = 10) =>
  change > 0 ? `up ${change} on the ${n} before` : change < 0 ? `down ${-change} on the ${n} before` : `same as the ${n} before`;

/** A song's plays (oldest first): the average of the last `n` against the first `n`, rounded, once there are 2n;
 *  before that the latest against the first. `both`: averages were used. null with no plays. */
export function lastVsFirst(oldestFirst: number[], n = 3): { from: number; to: number; change: number; both: boolean } | null {
  if (!oldestFirst.length) return null;
  const both = oldestFirst.length >= 2 * n;
  const from = both ? Math.round(mean(oldestFirst.slice(0, n))) : oldestFirst[0];
  const to = both ? Math.round(mean(oldestFirst.slice(-n))) : oldestFirst[oldestFirst.length - 1];
  return { from, to, change: to - from, both };
}
