// "Challenge a friend": a link to singonkey.app/c/ that says "<name> scored 78 on Stand By Me. Can you beat it?".
//
// The link carries only the song's title and artist, the score, and (if typed) a first name, in its fragment
// (after the #), which a browser never sends to a server. Nothing else: no notes, lyrics or audio (only a title and
// a number, Daniel's rule), and no id of the take, the song or the person. ?ref=challenge is what the site's
// analytics see (Plausible counts it as the visit's source). The site's reader is site/lib/challenge.ts.

export const CHALLENGE_BASE = "https://singonkey.app/c/?ref=challenge";

/** The longest each part may be (a title is cut, not refused). */
export const MAX = { song: 100, by: 80, name: 24 } as const;

const clean = (s: string, max: number) => s.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim().slice(0, max).trim();

/** The challenge link for a take's score. `name`: the first name typed (blank: none). */
export function challengeLink(song: string, artist: string, score: number, name = ""): string {
  const q = new URLSearchParams();
  q.set("song", clean(song, MAX.song) || "a song");
  const by = clean(artist, MAX.by);
  if (by) q.set("by", by);
  q.set("score", String(Math.max(0, Math.min(100, Math.round(Number.isFinite(score) ? score : 0)))));
  const n = clean(name, MAX.name);
  if (n) q.set("name", n);
  // %20 rather than +: reads better in a message, and either is read back the same
  return `${CHALLENGE_BASE}#${q.toString().replace(/\+/g, "%20")}`;
}
