// npm test: the What's new present's dot (lib/whatsNew.ts): after an update, never on a fresh install.
import { test } from "node:test";
import assert from "node:assert/strict";
import { SEEN_KEY, compareReleases, markFreshInstall, markSeen, releasePart, seenRelease, showWhatsNewDot } from "./whatsNew.ts";

test("a version's release part: the preview or build suffix dropped", () => {
  assert.equal(releasePart("0.2.0"), "0.2.0");
  assert.equal(releasePart("0.3.0-preview.3"), "0.3.0");
  assert.equal(releasePart("0.3.0+build.7"), "0.3.0");
  assert.equal(releasePart(" v0.2.1 "), "0.2.1");
});

test("releases compare as numbers, by their release part", () => {
  assert.ok(compareReleases("0.3.0", "0.2.0") > 0);
  assert.ok(compareReleases("0.2.0", "0.3.0") < 0);
  assert.equal(compareReleases("0.2.0", "0.2.0"), 0);
  assert.ok(compareReleases("0.10.0", "0.9.1") > 0, "10 after 9, not by text");
  assert.equal(compareReleases("0.3.0-preview.3", "0.3.0"), 0, "a preview of 0.3.0 is 0.3.0");
  assert.ok(compareReleases("0.3.0-preview.1", "0.2.0") > 0);
  assert.ok(compareReleases("1.0", "0.9.9") > 0);
  assert.equal(compareReleases("1.0", "1.0.0"), 0);
});

test("the dot: a fresh install no, an update yes, the same release seen no, nothing ever seen yes", () => {
  assert.equal(showWhatsNewDot("0.3.0", "0.3.0"), false, "fresh install: setup remembered this release");
  assert.equal(showWhatsNewDot("0.3.0", "0.2.0"), true, "updated from 0.2.0, not opened since");
  assert.equal(showWhatsNewDot("0.3.0", null), true, "an existing user from before (0.2.0, never opened)");
  assert.equal(showWhatsNewDot("0.2.0", "0.2.0"), false, "opened on this release");
  assert.equal(showWhatsNewDot("0.3.0-preview.3", "0.2.0"), true, "a preview of the next release counts as it");
  assert.equal(showWhatsNewDot("0.3.0-preview.3", "0.3.0"), false, "…and is seen once 0.3.0 is");
  assert.equal(showWhatsNewDot("0.2.0", "0.3.0"), false, "a downgrade: nothing new");
  assert.equal(showWhatsNewDot(null, null), false, "the version not known yet (or outside the app): no dot");
  assert.equal(showWhatsNewDot("", "0.2.0"), false);
});

test("what's remembered: the release part; first-run setup remembers only when nothing is remembered yet", () => {
  const store = new Map();
  globalThis.localStorage = { getItem: k => store.has(k) ? store.get(k) : null, setItem: (k, v) => store.set(k, String(v)) };
  try {
    assert.equal(seenRelease(), null);
    markFreshInstall("0.3.0-preview.2");
    assert.equal(seenRelease(), "0.3.0", "a fresh install on a preview: its release");
    assert.equal(store.get(SEEN_KEY), "0.3.0", "the key 0.2.0 wrote, so what it stored still counts");
    markFreshInstall("0.4.0");
    assert.equal(seenRelease(), "0.3.0", "setup run again: what's remembered stays");
    markSeen("0.4.0-preview.1");
    assert.equal(seenRelease(), "0.4.0", "opened: this release");
    assert.equal(showWhatsNewDot("0.4.0", seenRelease()), false);
  } finally {
    delete globalThis.localStorage;
  }
  // no storage at all (a private window, storage blocked): nothing thrown, nothing remembered
  assert.equal(seenRelease(), null);
  assert.doesNotThrow(() => markSeen("0.4.0"));
});
