// Links and facts used across the site.
//
// LAUNCH PLACEHOLDERS: everything Daniel still has to supply lives in LAUNCH below, and nowhere else in the site.
// Each one is marked FILL IN. Until it's filled, the site falls back to something safe (noted per item), and
// `npm run build` prints which ones are still empty.
export const LAUNCH = {
  /** The GitHub repository. */
  githubUrl: "https://github.com/dsmurrell/onkey",
  /** FILL IN: the notarised OnKey.dmg (a release asset URL). Empty: the repo's latest-release page. */
  downloadUrl: "https://github.com/dsmurrell/onkey/releases/latest/download/OnKey.dmg",
  /** FILL IN: the "Want Windows?" issue. Empty: the repo's issue list. */
  windowsIssueUrl: "https://github.com/dsmurrell/onkey/issues/1",
  /** The contact address (Cloudflare Email Routing forwards it). Empty: "(address to come)". */
  contactEmail: "hello@singonkey.app",
  /** The site's own address (for link previews). Empty: relative links only. */
  siteUrl: "https://singonkey.app",
  /** Plausible: the site as added in Plausible (cookieless visit counts, the website only, never the app). Empty: none. */
  plausibleDomain: "singonkey.app",
  /** Flow Myna website tracker id ("fm_…", from a Website Tracker key with singonkey.app allowed): visit
   *  paths, cookieless, the website only. Empty: no Flow Myna script, nothing sent. */
  flowMynaTrackerId: "fm_XtWTFHG5Mpk7",
  /** The Discord server's invite (never expires). Empty: no Discord links. */
  discordUrl: "https://discord.gg/afHvqTNS6r",
  /** The demo video's address (hosted outside the repo, e.g. "https://media.singonkey.app/demo.mp4": too big for git, and kept off GitHub). Empty: the lane illustration. */
  demoVideo: "https://media.singonkey.app/demo.mp4",
  /** Optional: a still shown before the demo clip plays, e.g. "/demo-poster.jpg" in site/public/. */
  demoPoster: "/demo-poster.jpg",
  /** True: the downloadable build is notarised by Apple (every DMG is, since 19 Oct). False: install steps explain "Open Anyway". */
  notarised: true,
};

/** Where the repository lives. */
export const GITHUB_URL = LAUNCH.githubUrl;

/** The notarised download. */
export const DOWNLOAD_URL = LAUNCH.downloadUrl || `${GITHUB_URL}/releases/latest`;

/** The "Want Windows?" issue. */
export const WINDOWS_ISSUE_URL = LAUNCH.windowsIssueUrl || `${GITHUB_URL}/issues`;

/** Plausible's site name, or null (no analytics script). */
export const PLAUSIBLE_DOMAIN: string | null = LAUNCH.plausibleDomain || null;

/** Flow Myna's website tracker id, or null (no script, no events). */
export const FLOWMYNA_TRACKER_ID: string | null = LAUNCH.flowMynaTrackerId || null;

/** The Discord server, or null. */
export const DISCORD_URL: string | null = LAUNCH.discordUrl || null;

/** Rights-holder and general contact, or null until there is one. */
export const CONTACT_EMAIL: string | null = LAUNCH.contactEmail || null;

/** The site's own address, or null until there is one. */
export const SITE_URL: string | null = LAUNCH.siteUrl || null;

/** Which placeholders are still empty. Printed once during the build so none is forgotten. */
export const UNFILLED = Object.entries(LAUNCH).filter(([k, v]) => !v && k !== "demoPoster").map(([k]) => k);

/** Where OnKey keeps its files on the Mac. */
export const DATA_DIR = "~/Library/Application Support/audio.onkey.app/";
const g = globalThis as { __onkeyWarned?: boolean };
if (typeof window === "undefined" && UNFILLED.length && !g.__onkeyWarned) {
  g.__onkeyWarned = true;
  console.warn(`[site] Launch placeholders still empty in site/lib/site.ts: ${UNFILLED.join(", ")}`);
}

export const REQUIREMENTS = ["A Mac with Apple Silicon (M1 or later)", "macOS 14.2 or later"];

/** Prefix for files in public/ when the site is served under a sub-path (GitHub Pages project sites). */
export const asset = (path: string) => /^https?:\/\//.test(path) ? path : `${process.env.NEXT_PUBLIC_BASE_PATH ?? ""}${path}`;

/** A file in the repository (on its default branch), for "Under the hood" links. */
export const codeUrl = (path: string) => `${GITHUB_URL}/blob/HEAD/${path}`;

/** The card shown when a link to the site is shared (1280x640). Hosted with the demo video, outside the repo,
 *  since it shows a song's notes. */
export const SHARE_IMAGE = "https://media.singonkey.app/social-preview.png";
