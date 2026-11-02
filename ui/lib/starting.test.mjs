// npm test (node's own test runner; lib/starting.ts is plain TypeScript, its types stripped)
import { test } from "node:test";
import assert from "node:assert/strict";
import { startingView, STARTING_MS } from "./starting.ts";

const s = (more = {}) => ({ id: "isrc:X", name: "Yellow", app: "Apple Music", at: 1000, before: 7, ...more });

test("the new song shows as starting at once, not the song before", () => {
  const v = startingView(s(), 1000, 7);
  assert.equal(v.title, "Starting Yellow in Apple Music…");
  assert.equal(v.failed, false);
  // nothing was shown before, and still nothing
  assert.equal(startingView(s({ before: null }), 1500, null).title, "Starting Yellow in Apple Music…");
});

test("it goes once a song shows", () => {
  assert.equal(startingView(s(), 2000, 8), null);
  assert.equal(startingView(s({ before: null }), 2000, 1), null);
  assert.equal(startingView(null, 2000, 7), null);
});

test("an app that can't start it says so, with the app named", () => {
  const no = startingView(s({ cant: "no_id" }), 1200, 7);
  assert.equal(no.failed, true);
  assert.equal(no.title, "OnKey can't start Yellow in Apple Music");
  const failed = startingView(s({ cant: "failed" }), 1200, 7);
  assert.equal(failed.title, "Couldn’t start Yellow in Apple Music");
  assert.match(failed.sub, /Apple Music library/);
  assert.equal(startingView(s({ app: "Spotify", cant: "failed" }), 1200, 7).sub, "Open it in Spotify and press play there: OnKey follows it.");
  // a failure stays until a song shows (or it's dismissed)
  assert.equal(startingView(s({ cant: "failed" }), 1000 + 10 * STARTING_MS, 7).failed, true);
});

test("too long without the song: said not to have started, unless OnKey is naming what it hears", () => {
  assert.equal(startingView(s(), 1000 + STARTING_MS - 1, 7).failed, false);
  assert.equal(startingView(s(), 1000 + STARTING_MS + 1, 7).failed, true);
  assert.equal(startingView(s(), 1000 + STARTING_MS + 1, 7, true), null);
  assert.equal(startingView(s(), 1000, 7, true).failed, false);       // naming, early: still "Starting"
});

test("no name or app known: the fragments, never at the start of a title (they're lowercase)", () => {
  assert.equal(startingView(s({ name: "", app: "" }), 1000, 7).title, "Starting the song in your music app…");
  assert.equal(startingView(s({ name: "", app: "", cant: "failed" }), 1000, 7).title, "Couldn’t start the song in your music app");
  assert.equal(startingView(s({ name: "", app: "", cant: "no_id" }), 1000, 7).title, "OnKey can't start the song in your music app");
});

test("the song shown already, started over, shows as starting for a moment and then goes, never as a failure", () => {
  const same = s({ endsAt: 3500 });
  assert.equal(startingView(same, 1200, 7).title, "Starting Yellow in Apple Music…");
  assert.equal(startingView(same, 2000, 8), null, "its new take shows");
  assert.equal(startingView(same, 3500, 7), null, "no new take followed: the song as it is");
  assert.equal(startingView(same, 1000 + STARTING_MS + 1, 7), null);
});
