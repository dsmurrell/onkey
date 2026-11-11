// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { challengeLink, challengeParts, CHALLENGE_BASE } from "./challenge.ts";

const parts = (link) => Object.fromEntries(new URLSearchParams(link.split("#")[1]));

test("a link holds the title, artist, score and name, nothing else", () => {
  const l = challengeLink("Stand By Me", "Ben E. King", 78.4, "Sam");
  assert.ok(l.startsWith(`${CHALLENGE_BASE}#`));
  assert.deepEqual(parts(l), { song: "Stand By Me", by: "Ben E. King", score: "78", name: "Sam" });
  assert.ok(!l.includes("+"));
});

test("no name, no artist: left out", () => {
  assert.deepEqual(parts(challengeLink("Yellow", "", 91, "  ")), { song: "Yellow", score: "91" });
});

test("odd input is tidied: score clamped, long text cut, symbols survive", () => {
  const p = parts(challengeLink("A & B #1 ?=%", "X", 140, "Zoë-Anne the very long first name indeed"));
  assert.equal(p.song, "A & B #1 ?=%");
  assert.equal(p.score, "100");
  assert.ok(p.name.length <= 24);
  assert.equal(parts(challengeLink("", "", NaN)).score, "0");
  assert.equal(parts(challengeLink("x".repeat(300), "", 5)).song.length, 100);
});

test("a signature goes in the link only when it is one, and the parts signed are the parts in the link", () => {
  const sig = "F9tUas_rFTv-shV8UOEw3cT9aS1l9wK0_Re0rKqdkWjOi7iSx6gmwMXmC7Px8v2UED2_2kUILb53QfOxuwIKTQ";
  const l = challengeLink("Everybody's Changing", "Keane", 81, "Daniel", sig);
  assert.deepEqual(parts(l), { song: "Everybody's Changing", by: "Keane", score: "81", name: "Daniel", sig });
  assert.equal(parts(challengeLink("Yellow", "", 91, "", "nope")).sig, undefined);
  const p = challengeParts("  A   long\ttitle ", "X", 80.6, " Sam ");
  assert.deepEqual(p, { song: "A long title", by: "X", score: 81, name: "Sam" });
  assert.deepEqual(parts(challengeLink("  A   long\ttitle ", "X", 80.6, " Sam ")), { song: p.song, by: p.by, score: "81", name: p.name });
  // a cut never leaves half an emoji (it wouldn't survive the link, and the signature wouldn't fit)
  assert.equal(challengeParts("x", "", 1, "a".repeat(23) + "\u{1F3A4}").name, "a".repeat(23));
});
