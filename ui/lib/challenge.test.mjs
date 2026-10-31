// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { challengeLink, CHALLENGE_BASE } from "./challenge.ts";

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
