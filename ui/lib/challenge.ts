// "Challenge a friend": a link to singonkey.app/c/ that says "<name> scored 78 on Stand By Me. Can you beat it?".
//
// The link carries only the song's title and artist, the score, and (if typed) a first name, in its fragment
// (after the #), which a browser never sends to a server. Nothing else: no notes, lyrics or audio (only a title and
// a number, Daniel's rule), and no id of the take, the song or the person. ?ref=challenge is what the site's
// analytics see (Plausible counts it as the visit's source). The site's reader is site/lib/challenge.ts.
//
// The link is signed by the app (`sig`, src-tauri/src/challenge.rs), so a score changed in the address bar shows on
// the site as not checked. The signature is of the parts exactly as the link holds them (challengeParts).

export const CHALLENGE_BASE = "https://singonkey.app/c/?ref=challenge";

/** The longest each part may be (a title is cut, not refused). */
export const MAX = { song: 100, by: 80, name: 24 } as const;

// (a cut never leaves half of a two-unit character, an emoji say: it wouldn't survive the link)
const clean = (s: string, max: number) => s.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim().slice(0, max).replace(/[\uD800-\uDBFF]$/, "").trim();

export type ChallengeParts = { song: string; by: string; score: number; name: string };

/** What a link says, as it holds it (tidied, cut to length): what the app signs. */
export function challengeParts(song: string, artist: string, score: number, name = ""): ChallengeParts {
  return { song: clean(song, MAX.song) || "a song", by: clean(artist, MAX.by),
           score: Math.max(0, Math.min(100, Math.round(Number.isFinite(score) ? score : 0))), name: clean(name, MAX.name) };
}

/** The challenge link for a take's score. `name`: the first name typed (blank: none); `sig`: the app's signature of
 *  these parts (blank: an unsigned link, which the site shows as not checked). */
export function challengeLink(song: string, artist: string, score: number, name = "", sig = ""): string {
  const p = challengeParts(song, artist, score, name);
  const q = new URLSearchParams();
  q.set("song", p.song);
  if (p.by) q.set("by", p.by);
  q.set("score", String(p.score));
  if (p.name) q.set("name", p.name);
  if (/^[A-Za-z0-9_-]{86}$/.test(sig)) q.set("sig", sig);
  // %20 rather than +: reads better in a message, and either is read back the same
  return `${CHALLENGE_BASE}#${q.toString().replace(/\+/g, "%20")}`;
}
