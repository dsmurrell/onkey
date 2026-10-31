// The demo song (src-tauri/src/demo.rs): one song OnKey has the rights to, shipped with its notes already learned,
// so a new user can sing and be scored at once ("Try it now"). Plain TypeScript, tested by demo.test.mjs.

/** What the engine says about the bundled demo (demo_info); null when this build has none. */
export type DemoSong = { id: string; name: string; artist: string; duration: number; credit: string; licence: string };

export const isDemo = (trackId: string | undefined | null) => !!trackId && trackId.startsWith("demo:");

/** Read the engine's answer: a demo only when it's a whole one. */
export function demoFrom(v: unknown): DemoSong | null {
  const d = v as Partial<DemoSong> | null;
  if (!d || typeof d !== "object" || !isDemo(d.id) || !d.name) return null;
  return { id: d.id!, name: d.name, artist: d.artist ?? "", duration: d.duration ?? 0, credit: d.credit ?? "", licence: d.licence ?? "" };
}

/** The home screen offers the demo while you have no songs of your own yet (the demo itself doesn't count). */
export const offerDemo = (library: { track: { id: string } }[]) => library.every(s => isDemo(s.track.id));

/** The small print under the offer: the song, and who made the recording. */
export function demoCredit(d: DemoSong) {
  return [d.artist ? `${d.name} (${d.artist})` : d.name, d.credit].filter(Boolean).join(". ");
}
