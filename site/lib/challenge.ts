// "Beat my score": the challenge link OnKey copies (ui/lib/challenge.ts in the app) opens /c/ with the challenge in
// the fragment: /c/?ref=challenge#song=Stand%20By%20Me&by=Ben%20E.%20King&score=78&name=Sam
// A browser never sends the fragment to a server, and the page takes it out of the address before any analytics
// script loads (STASH_SCRIPT), so Plausible and Flow Myna see only /c/?ref=challenge. Only a title and a number.

export type Challenge = { song: string; by: string; score: number; name: string };

const MAX = { song: 100, by: 80, name: 24 };
const clean = (s: string | null, max: number) =>
  (s ?? "").replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim().slice(0, max).trim();

/** The challenge in a fragment (with or without its #), or null when it's missing or doesn't make sense. */
export function readChallenge(fragment: string | null | undefined): Challenge | null {
  if (!fragment) return null;
  let q: URLSearchParams;
  try { q = new URLSearchParams(fragment.replace(/^#/, "")); } catch { return null; }
  const song = clean(q.get("song"), MAX.song);
  const raw = (q.get("score") ?? "").trim();
  if (!song || !/^\d{1,3}$/.test(raw)) return null;
  const score = Number(raw);
  if (score > 100) return null;
  return { song, by: clean(q.get("by"), MAX.by), score, name: clean(q.get("name"), MAX.name) };
}

/** Where the page keeps the fragment once it's out of the address (this tab only, so a reload still shows it). */
export const STASH_KEY = "onkey.challenge";

/** Runs as the page's HTML is read, before React and before any analytics script: keeps the fragment in memory
 *  (and this tab's sessionStorage) and takes it out of the address. A fixed string, never built from the link. */
export const STASH_SCRIPT = `(function(){try{var h=location.hash.slice(1);if(!h)return;window.__onkeyChallenge=h;try{sessionStorage.setItem(${JSON.stringify(STASH_KEY)},h)}catch(e){}history.replaceState(history.state,"",location.pathname+location.search)}catch(e){}})();`;
