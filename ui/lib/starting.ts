// What the Sing page shows while a song OnKey was asked to start (the Library's Play, History's and a take's Sing it
// again) hasn't started yet. Plain TypeScript (no React), so node's test runner can check it (starting.test.mjs).
import { t } from "./i18n";

/** A song OnKey was asked to start: its id and name, the app it starts in, when (performance.now() ms), the version
 *  of the song shown then (null: none), and why the app couldn't start it, once known (src-tauri/src/lib.rs
 *  play_track: "no_id", OnKey knows no id for it there; "failed", the app didn't play it). */
export type Starting = { id: string; name: string; app: string; at: number; before: number | null; cant?: "no_id" | "failed" | null;
  /** the song shown already, started over: shown as starting until its new take shows, or until this time (no new take
   *  may follow, e.g. it was paused a few seconds in), never said to have failed */
  endsAt?: number };

/** What the Sing page says: a title, a line under it, and whether it's a failure (with a button to open the song in
 *  its app). */
export type StartingView = { id: string; app: string; title: string; sub: string; failed: boolean };

/** How long a start may take before it's said not to have worked (Apple Music can take a few seconds to open). */
export const STARTING_MS = 15000;

/** The song shown already, started over: how long its starting screen waits for its new take (`Starting.endsAt`). */
export const SAME_SONG_MS = 2500;

/** What to show for a start at `now` while the song shown is `songVersion` (null: none). Null once another song
 *  shows (it started; or another one did) and, after STARTING_MS, while OnKey is naming what it hears (`naming`:
 *  listening mode, the song may well be playing: the usual "Getting ready…" says so). Until then the new song is
 *  shown as starting, never the song before (its lane, or its score once it ended). */
export function startingView(s: Starting | null, now: number, songVersion: number | null, naming = false): StartingView | null {
  if (!s) return null;
  if (songVersion != null && songVersion !== s.before) return null;
  if (s.endsAt != null && now >= s.endsAt) return null;
  const app = s.app || t("starting.yourApp");
  const song = s.name || t("starting.theSong");
  const base = { id: s.id, app };
  if (s.cant === "no_id")
    return { ...base, failed: true, title: t("starting.noId.title", { song, app }), sub: t("starting.noId.sub", { app }) };
  const late = now - s.at > STARTING_MS;
  if (late && naming && !s.cant) return null;
  if (s.cant === "failed" || late)
    return { ...base, failed: true, title: t("starting.failed.title", { app, song }),
             sub: app === "Apple Music" ? t("starting.failed.subMusic") : t("starting.failed.sub", { app }) };
  return { ...base, failed: false, title: t("starting.going.title", { song, app }), sub: t("starting.going.sub") };
}
