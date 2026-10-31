/** Development tools only: driving "Play with the song" without a click (the mirror's POST /dev/along, which sends
 *  the window an "onkey-dev-along" event; see src-tauri/src/mirror.rs `drive`). Plain TypeScript, tested by npm test. */
export type DriveEvent = { stop?: boolean; take?: string; song?: number | null; t?: number | null; n?: number };

/** The take page to open for a drive request, with where to start ("along": song:<s> or t:<s on the graph>) and a
 *  nonce ("n") so the same request twice starts twice. Null for a stop or a request with no take. */
export function driveHref(ev: DriveEvent): string | null {
  if (ev.stop || !ev.take) return null;
  const at = ev.song != null ? `song:${ev.song}` : `t:${ev.t ?? 0}`;
  return `/take?id=${encodeURIComponent(ev.take)}&along=${encodeURIComponent(at)}&n=${ev.n ?? Date.now()}`;
}

/** Where on the take's graph (s) a drive request starts: song:<s> is that far into the song (the take starts
 *  `songOffset` into it), t:<s> is the graph's own time. Null when it can't be read. */
export function driveGraphAt(spec: string | null, songOffset: number | null | undefined): number | null {
  const m = /^(song|t):(\d+(?:\.\d+)?)$/.exec(spec ?? "");
  if (!m) return null;
  const x = Number(m[2]);
  if (m[1] === "t") return x;
  return songOffset == null ? null : Math.max(0, x - songOffset);
}
