// "Beat my score": the challenge link OnKey copies (ui/lib/challenge.ts in the app) opens /c/ with the challenge in
// the fragment: /c/?ref=challenge#song=Stand%20By%20Me&by=Ben%20E.%20King&score=78&name=Sam
// A browser never sends the fragment to a server, and the page takes it out of the address before any analytics
// script loads (STASH_SCRIPT), so Plausible and Flow Myna see only /c/?ref=challenge. Only a title and a number.

//
// The app signs what the link says (`sig`; src-tauri/src/challenge.rs in the app), and the page checks it here in the
// browser (checkChallenge): a score changed in the address bar, or a link with no signature, is shown as not checked.

export type Challenge = { song: string; by: string; score: number; name: string; sig: string };

const MAX = { song: 100, by: 80, name: 24 };
const clean = (s: string | null, max: number) =>
  (s ?? "").replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim().slice(0, max).replace(/[\uD800-\uDBFF]$/, "").trim();

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
  const sig = (q.get("sig") ?? "").trim();
  return { song, by: clean(q.get("by"), MAX.by), score, name: clean(q.get("name"), MAX.name), sig: /^[A-Za-z0-9_-]{86}$/.test(sig) ? sig : "" };
}

/** The public halves of the keys OnKey signs challenge links with (P-256 points, hex; the private halves are given to
 *  official builds and are in no repository). A new key is added here before the app starts using it. */
export const CHALLENGE_KEYS = [
  "041ffb28a24c41e0b48913a6345f7a125c29042403591d5cc2b6ab4694ce7d50b0a9cc6608db9c906bdc5cb2a830143a69e3a34cabc3701c9ecc3b2d58efc62f08",
];

/** What the app signed: the link's parts, one to a line (challenge.rs `message`). */
export const challengeMessage = (c: Challenge) => `onkey-challenge-v1\n${c.song}\n${c.by}\n${c.score}\n${c.name}`;

const unhex = (h: string) => Uint8Array.from(h.match(/../g) ?? [], b => parseInt(b, 16));
const unbase64url = (s: string) => Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/")), c => c.charCodeAt(0));

/** Whether the link's signature fits what it says: true only for a link OnKey made that hasn't been changed since.
 *  False for a link without a signature (older versions of OnKey, builds from source), and in a browser too old to
 *  check one. */
export async function checkChallenge(c: Challenge, keys: string[] = CHALLENGE_KEYS): Promise<boolean> {
  if (!c.sig) return false;
  try {
    const sig = unbase64url(c.sig), msg = new TextEncoder().encode(challengeMessage(c));
    for (const k of keys) {
      const key = await crypto.subtle.importKey("raw", unhex(k), { name: "ECDSA", namedCurve: "P-256" }, false, ["verify"]);
      if (await crypto.subtle.verify({ name: "ECDSA", hash: "SHA-256" }, key, sig, msg)) return true;
    }
  } catch {}
  return false;
}

/** Where the page keeps the fragment once it's out of the address (this tab only, so a reload still shows it). */
export const STASH_KEY = "onkey.challenge";

/** Runs as the page's HTML is read, before React and before any analytics script: keeps the fragment in memory
 *  (and this tab's sessionStorage) and takes it out of the address. A fixed string, never built from the link. */
export const STASH_SCRIPT = `(function(){try{var h=location.hash.slice(1);if(!h)return;window.__onkeyChallenge=h;try{sessionStorage.setItem(${JSON.stringify(STASH_KEY)},h)}catch(e){}history.replaceState(history.state,"",location.pathname+location.search)}catch(e){}})();`;
