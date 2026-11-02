// The What's new present's dot (components/Nav.tsx): shown to someone who updated and hasn't opened What's new since,
// never on a fresh install. What's remembered (localStorage "onkey-whats-new-seen") is the release last opened, or
// the release first-run setup finished on; the dot shows while this OnKey's release is newer than that, or nothing
// was ever remembered (someone from before the dot was tied to the version: 0.2.0 users who never opened it).
// Releases compare by their release part: a preview of 0.3.0 ("0.3.0-preview.3") counts as 0.3.0.

export const SEEN_KEY = "onkey-whats-new-seen";

/** A version's release part: "0.2.0-preview.3" → "0.2.0", "0.3.0+build.7" → "0.3.0"; trimmed, a leading "v" dropped. */
export function releasePart(version: string): string {
  return version.trim().replace(/^v/i, "").split(/[-+]/)[0];
}

/** Compare two releases part by part as numbers ("0.10.0" after "0.9.1"); their release parts only. Negative when
 *  `a` is older, 0 when the same, positive when newer. A part that isn't a number counts as 0. */
export function compareReleases(a: string, b: string): number {
  const pa = releasePart(a).split("."), pb = releasePart(b).split(".");
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (parseInt(pa[i] ?? "0", 10) || 0) - (parseInt(pb[i] ?? "0", 10) || 0);
    if (d) return d;
  }
  return 0;
}

/** Show the dot? `current`: this OnKey's version (null until known, or outside the app: no dot). `seen`: the release
 *  remembered (null: nothing remembered, so an existing user from before: the dot). */
export function showWhatsNewDot(current: string | null | undefined, seen: string | null | undefined): boolean {
  if (!current) return false;
  if (!seen) return true;
  return compareReleases(current, seen) > 0;
}

/** The release remembered as seen, or null (none, or no storage). */
export function seenRelease(): string | null {
  try { return localStorage.getItem(SEEN_KEY); } catch { return null; }
}

/** Remember this version's release as seen: What's new was opened, or first-run setup finished on it. */
export function markSeen(version: string) {
  try { localStorage.setItem(SEEN_KEY, releasePart(version)); } catch {}
}

/** First-run setup finished on this version: nothing to catch up on, so no dot, unless an older release is
 *  remembered already (someone who updated and reran setup before opening What's new keeps the dot). */
export function markFreshInstall(version: string) {
  if (!seenRelease()) markSeen(version);
}
