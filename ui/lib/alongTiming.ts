type Feed = { heard?: number | null; position: number; feed?: [number, number, number] | null; demo?: boolean };

const ms = (s: number | undefined | null) => (s == null || !Number.isFinite(s) ? "?" : `${Math.round(s * 1000)}`);

/** One short log line, diagnostics only: the song as heard and the app's own clock (+ ahead of it), the recording's
 *  target and where the native player has the voice (`off`: target - voice, s; voice null: not playing yet), Core
 *  Audio's output delay as allowed for, and listening's last feed (age, block length, audio fed behind the wall
 *  clock). All ms but the song times (s). */
export function alongTiming(n: Feed, song: number, target: number, voice: number | null, coreOutMs: number | undefined): string {
  const f = n.feed;
  const listen = n.demo ? "demo: OnKey's player" : f ? `feed ${ms(f[0])} ago, block ${ms(f[1])}, behind ${ms(f[2])}` : "no feed";
  const sign = (x: number) => `${x >= 0 ? "+" : ""}${x.toFixed(2)}`;
  const v = voice == null ? "voice ? off ?" : `voice ${voice.toFixed(2)} off ${sign(target - voice)}`;
  return `heard ${song.toFixed(2)} app ${sign(n.position - song)} · rec ${target.toFixed(2)} ${v} · core out ${coreOutMs ?? "?"} · ${listen}`;
}
