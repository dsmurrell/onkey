// npm test (node's own test runner; lib/guideFade.ts is plain TypeScript, its types stripped)
import { test } from "node:test";
import assert from "node:assert/strict";
import { FADE_LEVELS, fadeLevel, pitchAlpha, songScores, songSkill } from "./guideFade.ts";

const song = { id: "sp:1", name: "Yellow", artist: "Coldplay" };
const take = (score, recorded, extra = {}) => ({ track: { ...song }, recorded, score, coverage: 0.8, ...extra });

test("a song's scores: this song's takes, newest first, sung ones only", () => {
  const takes = [
    take(70, "2026-10-20T10:00:00"),
    take(90, "2026-10-22T10:00:00"),
    take(99, "2026-10-21T10:00:00", { coverage: 0.05 }),                    // hardly sung: not counted
    take(null, "2026-10-23T10:00:00"),                                       // no score
    take(85, "2026-10-24T10:00:00", { track: { id: "music:9", name: "Clocks", artist: "Coldplay" } }),
    take(60, "2026-10-25T10:00:00", { track: { id: "isrc:GB1", name: " yellow", artist: "COLDPLAY " } }),  // same song, other id
  ];
  assert.deepEqual(songScores(takes, song), [60, 90, 70]);
  assert.deepEqual(songScores([], song), []);
});

test("skill: the best, or the last two's lower score plus 5", () => {
  assert.equal(songSkill([]), null);
  assert.equal(songSkill([72]), 72);
  assert.equal(songSkill([76, 75, 40]), 80);       // last two 75+: as good as a best of 80
  assert.equal(songSkill([60, 85]), 85);
});

test("no takes, or not sung well yet: the full guide", () => {
  assert.equal(fadeLevel([]), 0);
  assert.equal(fadeLevel([79]), 0);
  assert.equal(fadeLevel([74, 78]), 0);
});

test("the better you've done, the more it fades", () => {
  assert.equal(fadeLevel([80]), 1);
  assert.equal(fadeLevel([75, 76]), 1);
  assert.equal(fadeLevel([87]), 2);
  assert.equal(fadeLevel([82, 84]), 2);
  assert.equal(fadeLevel([93]), 3);
  assert.equal(fadeLevel([88, 90]), 3);
  // never less faded for a better record
  let prev = 0;
  for (let s = 0; s <= 100; s++) { const l = fadeLevel([s, s]); assert.ok(l >= prev); prev = l; }
});

test("a struggle last time brings the guide back a step", () => {
  assert.equal(fadeLevel([70, 93]), 2);            // best 93, but the last take 23 under it
  assert.equal(fadeLevel([64, 80]), 0);
  assert.equal(fadeLevel([79, 93]), 3);            // 14 under: an off day, not a struggle
});

test("faded, your line shows in full only where you're more than a semitone off", () => {
  assert.equal(pitchAlpha(0, 10), 1);
  assert.equal(pitchAlpha(0, 300), 1);
  for (const l of [1, 2, 3]) {
    assert.equal(pitchAlpha(l, 0), FADE_LEVELS[l].onPitch);
    assert.equal(pitchAlpha(l, -100), FADE_LEVELS[l].onPitch);
    assert.equal(pitchAlpha(l, NaN), FADE_LEVELS[l].onPitch);         // no note to judge against
    assert.equal(pitchAlpha(l, 101), 1);
    assert.equal(pitchAlpha(l, -250), 1);
  }
  assert.ok(FADE_LEVELS[1].onPitch > FADE_LEVELS[2].onPitch && FADE_LEVELS[2].onPitch > FADE_LEVELS[3].onPitch);
  assert.equal(FADE_LEVELS[3].onPitch, 0);
});
