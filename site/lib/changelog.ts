// What's new in OnKey: the site's /changelog/ page (the app's present button opens it). Features newest first,
// each with the day it was built and the release that brought it to you. Written for people who sing, not
// developers; CHANGELOG.md in the repo is the short record.
//
// The words are in site/messages/changelog/en.json (and one file per language beside it, made with
// `node i18n/translate.mjs changelog`): `releases.<version, dots as dashes>.note` (and `.link`, the link's label),
// and for each entry `entries.<id>.title`, `.summary` and `.p1`, `.p2`… for its points. This file holds what isn't
// words: the order, dates, versions and kinds. To add a release: a RELEASES row and its note, then an ENTRIES row
// and its words per feature. `npm test` checks the two agree.

export type Kind = "new" | "better" | "fix";

export type Entry = {
  /** The key of its words in the changelog catalog, and its anchor on the page (#id): the English title in lower
   *  case with dashes between words. Fixed once written, as translations and links hang off it. */
  id: string;
  /** ISO date it was built */
  date: string;
  /** the release it's in */
  version: string;
  /** something new, something better, or a fix to something that went wrong */
  kind: Kind;
};

export type Release = {
  version: string;
  date: string;
  /** a page about the release (its label is `releases.<version>.link` in the catalog; `track` names the click for analytics) */
  link?: { href: string; track: string };
};

/** A version as a key: "0.2.0" → "0-2-0" (keys can't hold dots), and an entry's title as its id. */
export const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "");

/** Each release, newest first: a divider in the timeline above the features it brought. */
export const RELEASES: Release[] = [
  { version: "0.3.0", date: "2026-11-02" },
  { version: "0.2.0", date: "2026-10-31" },
  { version: "0.1.0", date: "2026-10-22", link: { href: "https://news.ycombinator.com/item?id=50195488", track: "Launched on Hacker News" } },
];

export const ENTRIES: Entry[] = [
  { id: "onkey-in-your-language", date: "2026-11-01", version: "0.3.0", kind: "new" },
  { id: "lyrics-keep-time-in-more-languages", date: "2026-11-01", version: "0.3.0", kind: "new" },
  { id: "songs-to-try-in-your-language", date: "2026-11-01", version: "0.3.0", kind: "new" },
  { id: "when-a-song-has-no-lyrics", date: "2026-11-01", version: "0.3.0", kind: "new" },
  { id: "smoother-switching-in-spotify", date: "2026-11-02", version: "0.3.0", kind: "better" },
  { id: "a-clearer-settings-page", date: "2026-11-01", version: "0.3.0", kind: "better" },
  { id: "small-fixes-0-3-0", date: "2026-11-01", version: "0.3.0", kind: "fix" },
  { id: "pause-listening", date: "2026-10-30", version: "0.2.0", kind: "new" },
  { id: "see-when-a-paused-take-will-be-scored", date: "2026-10-30", version: "0.2.0", kind: "new" },
  { id: "audio-interfaces-and-your-microphone-list", date: "2026-10-30", version: "0.2.0", kind: "better" },
  { id: "listening-back-stays-in-step-after-a-skip", date: "2026-10-30", version: "0.2.0", kind: "fix" },
  { id: "reliability-fixes", date: "2026-10-30", version: "0.2.0", kind: "fix" },
  { id: "songs-start-cleanly", date: "2026-10-29", version: "0.2.0", kind: "fix" },
  { id: "guides-and-help-and-beyond-pitch", date: "2026-10-29", version: "0.2.0", kind: "new" },
  { id: "smoother-setup-and-youtube", date: "2026-10-29", version: "0.2.0", kind: "better" },
  { id: "learn-a-song-again", date: "2026-10-28", version: "0.2.0", kind: "new" },
  { id: "a-tidier-app-page-by-page", date: "2026-10-28", version: "0.2.0", kind: "better" },
  { id: "scoring-levels-relaxed-steady-and-pro", date: "2026-10-26", version: "0.2.0", kind: "new" },
  { id: "leave-out-what-isn-t-the-melody", date: "2026-10-26", version: "0.2.0", kind: "new" },
  { id: "your-vocal-range", date: "2026-10-26", version: "0.2.0", kind: "new" },
  { id: "challenge-a-friend-and-share-your-best-takes", date: "2026-10-26", version: "0.2.0", kind: "new" },
  { id: "fade-the-guide", date: "2026-10-26", version: "0.2.0", kind: "new" },
  { id: "lyrics-line-up-better", date: "2026-10-26", version: "0.2.0", kind: "better" },
  { id: "onkey-s-first-release", date: "2026-10-22", version: "0.1.0", kind: "new" },
];
