// What's new in OnKey: the site's /changelog/ page (the app's present button opens it). Features newest first,
// each with the day it was built and the release that brought it to you. Written for people who sing, not
// developers; CHANGELOG.md in the repo is the short record.

export type Entry = {
  /** ISO date it was built */
  date: string;
  /** the release it's in */
  version: string;
  title: string;
  summary: string;
  points?: string[];
  /** a fix to something that went wrong, rather than something new */
  kind?: "new" | "better" | "fix";
};

export type Release = { version: string; date: string; note: string; link?: { label: string; href: string } };

/** Each release, newest first: a divider in the timeline above the features it brought. */
export const RELEASES: Release[] = [
  { version: "0.2.0", date: "2026-10-31", note: "Downloads and installs itself on Macs already running OnKey." },
  { version: "0.1.0", date: "2026-10-22", note: "The first public release.",
    link: { label: "Launched on Hacker News", href: "https://news.ycombinator.com/item?id=50195488" } },
];

export const ENTRIES: Entry[] = [
  {
    date: "2026-10-30",
    version: "0.2.0",
    kind: "new",
    title: "Pause listening",
    summary:
      "Watching a stream or a podcast? Pause OnKey's listening, and it stops hearing anything until you resume. " +
      "Nothing is followed, named or learned in the meantime.",
    points: [
      "Press ⌘⇧P, click the Live chip on the Sing page, or choose OnKey → Pause Listening",
      "A small \"Listening paused · Resume\" pill stays at the top of every page, so you don't forget",
      "It stays paused after a restart, until you resume",
    ],
  },
  {
    date: "2026-10-30",
    version: "0.2.0",
    kind: "new",
    title: "See when a paused take will be scored",
    summary:
      "Pause mid-song and OnKey waits a minute in case you carry on. It now tells you, and lets you skip the wait.",
    points: [
      "The Sing page counts down: \"Paused · scored in 0:45 · Finish now\"",
      "History shows the song you're singing as its first card, so a play you just paused never looks lost",
      "Every song card in History draws your scores from the second play",
    ],
  },
  {
    date: "2026-10-30",
    version: "0.2.0",
    kind: "better",
    title: "Audio interfaces and your microphone list",
    summary:
      "OnKey now takes your voice from the input it's actually on, so a mic on input 2 of a Focusrite (or any " +
      "interface with several inputs) just works.",
    points: [
      "No setting needed: OnKey follows the input your voice is on",
      "Virtual devices other apps add (Krisp, Loopback, BlackHole, Zoom) are listed apart from your real mics",
    ],
  },
  {
    date: "2026-10-30",
    version: "0.2.0",
    kind: "fix",
    title: "Listening back stays in step after a skip",
    summary:
      "If you skipped ahead while singing, \"Play with the song\" could play the part before the skip out of time. " +
      "Every part of the take now plays in step with the song.",
  },
  {
    date: "2026-10-30",
    version: "0.2.0",
    kind: "fix",
    title: "Reliability fixes",
    summary: "Things that could go wrong mid-song, now fixed.",
    points: [
      "Finish no longer throws a take away if pressed while OnKey checks the player",
      "A song can no longer stay at \"Waiting\" or \"Scoring\" for good after a rare failure",
      "Library's Delete all no longer sticks, and \"Stop learning and delete\" only deletes the song you picked",
      "Keyboard shortcuts no longer reach the music behind dialogs",
      "The score arrives straight away when a song ends paused",
      "A radio stream or hours-long mix stops at 20 minutes rather than running on as one take",
      "Quitting is instant: OnKey disappears at once and finishes saving what it learned out of sight",
    ],
  },
  {
    date: "2026-10-29",
    version: "0.2.0",
    kind: "fix",
    title: "Songs start cleanly",
    summary:
      "Pressing Sing on a card could start the music twice, or play a blip of the song before it restarted. " +
      "One press now means one clean start.",
  },
  {
    date: "2026-10-29",
    version: "0.2.0",
    kind: "new",
    title: "Guides and help, and Beyond pitch",
    summary:
      "Settings now links to these docs. A new guide, Beyond pitch, covers what OnKey can't hear (tone, breath, " +
      "style) and free teachers to learn it from.",
  },
  {
    date: "2026-10-29",
    version: "0.2.0",
    kind: "better",
    title: "Smoother setup and YouTube",
    summary: "Fixes for the first things our testers ran into.",
    points: [
      "Setup's music apps step goes straight on when there's nothing for macOS to ask",
      "A YouTube tab that Chrome put to sleep no longer holds OnKey up",
      "A music app you don't have installed shows as off, rather than on",
    ],
  },
  {
    date: "2026-10-28",
    version: "0.2.0",
    kind: "new",
    title: "Learn a song again",
    summary:
      "If a song's lyrics or notes slip, have OnKey learn it afresh: its next play learns it, and replaces the old " +
      "copy once it's heard enough.",
    points: [
      "Choose Learn again from the Library's ⋯ menu or the song's page",
      "OnKey suggests it when a song's words drift the same way on two plays",
    ],
  },
  {
    date: "2026-10-22",
    version: "0.1.0",
    kind: "new",
    title: "OnKey's first release",
    summary: "Free, open source, and everything on your Mac.",
    points: [
      "Learns the melody of any song you play in Spotify, Apple Music or YouTube, the first time you play it",
      "A live pitch line over the singer's notes, with synced lyrics and a score for every take",
      "Takes to hear back, and a short video of a take to share: your voice, no music",
      "Starter songs, a library of what you've learned, and updates that install themselves",
    ],
  },
];
