// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { noteName, reading, steady, Hold, follow, pickNote, isBlackKey, hzOf, patternNotes, patternFrom, Sequence, PATTERNS } from "./tuner.ts";

test("notes are named with middle C as C4", () => {
  assert.equal(noteName(60), "C4");
  assert.equal(noteName(69), "A4");
  assert.equal(noteName(58), "A♯3");
  assert.equal(noteName(59.6), "C4");
  assert.ok(isBlackKey(61) && !isBlackKey(64));
  assert.ok(Math.abs(hzOf(69) - 440) < 1e-9 && Math.abs(hzOf(57) - 220) < 1e-9);
});

test("a pitch reads as its nearest note and the cents off it", () => {
  assert.deepEqual({ ...reading(69.2), hz: 0 }, { note: 69, name: "A4", cents: 20, hz: 0 });
  assert.equal(reading(68.6).name, "A4");
  assert.equal(reading(68.6).cents, -40);
});

test("the note shown ignores a stray frame and needs a few frames of singing", () => {
  assert.equal(steady([null, null, 60, null]), null);
  assert.equal(steady([60.1, 60.0, 72.3, 60.2, 60.1]), 60.1);
  assert.equal(steady([60, 60, 60, null, null, null, null, null, null, null, null]), null);
});

test("holding a note: time on it counts, a short slip is forgiven, a long one starts again", () => {
  const h = new Hold(60, 25, 1000);
  for (let i = 0; i < 50; i++) h.push(60.1);          // 0.5 s on it, 10 cents sharp
  assert.equal(h.heldMs, 500);
  for (let i = 0; i < 20; i++) h.push(null);          // a breath of 0.2 s
  for (let i = 0; i < 5; i++) h.push(60.4);           // 40 cents sharp: off it, but not for long
  assert.equal(h.heldMs, 500);
  for (let i = 0; i < 50; i++) h.push(59.9);
  assert.ok(h.done && h.progress === 1);
  assert.equal(h.average, 0);                          // +10 for half, -10 for half
  const g = new Hold(60, 25, 1000);
  for (let i = 0; i < 50; i++) g.push(60);
  for (let i = 0; i < 40; i++) g.push(62);            // another note for 0.4 s
  assert.equal(g.heldMs, 0);
});

test("the picture follows the singer, gently", () => {
  const v = { lo: 55, hi: 69 };
  assert.deepEqual(follow(v, 60), v);
  assert.deepEqual(follow(v, null), v);
  assert.deepEqual(follow(v, 70), { lo: 55.5, hi: 69.5 });
  assert.deepEqual(follow(v, 56, 1), { lo: 54, hi: 68 });       // 2 semitones too low: one step at a time
  assert.deepEqual(follow(v, 57, 2), { lo: 54, hi: 68 });
});

test("a note to try is a white key in range, not the same one twice", () => {
  for (let i = 0; i < 50; i++) {
    const m = pickNote(57, 67, 60);
    assert.ok(m >= 57 && m <= 67 && !isBlackKey(m) && m !== 60);
  }
  assert.equal(pickNote(60.2, 60.8, null), 61);
});

test("a pattern is placed inside your range, on a white key when one fits", () => {
  for (let i = 0; i < 40; i++) {
    const n = patternNotes("five", 55, 67);
    assert.equal(n.length, PATTERNS.five.length);
    assert.ok(Math.min(...n) >= 55 && Math.max(...n) <= 67 && !isBlackKey(n[0]), String(n));
    assert.deepEqual(n.map(m => m - n[0]), [...PATTERNS.five]);
  }
  // from a note of your choosing
  assert.deepEqual(patternFrom("octave", 62), [62, 74, 62]);
  assert.deepEqual(patternFrom("five", 58)[4], 65);
  // an octave needs 12 semitones: in a range of exactly that, it starts at the bottom
  assert.deepEqual(patternNotes("octave", 57, 69), [57, 69, 57]);
  // a range too narrow for it: from the bottom anyway
  assert.deepEqual(patternNotes("arpeggio", 60, 65), [60, 64, 67, 72, 67, 64, 60]);
});

test("singing a pattern: each note held in turn, a skipped one counted as missed", () => {
  const q = new Sequence([60, 62, 64], 25, 500);
  assert.equal(q.target, 60);
  let finished = 0;
  for (let i = 0; i < 49; i++) finished += q.push(60.1) ? 1 : 0;
  assert.equal(finished, 0);
  assert.ok(q.progress > 0.9);
  assert.equal(q.push(60.1), true);                    // half a second on the first note
  assert.equal(q.target, 62);
  for (let i = 0; i < 80; i++) q.push(60.1);           // still singing the first note: the second isn't done
  assert.equal(q.target, 62);
  q.skip();
  for (let i = 0; i < 50; i++) q.push(63.85);          // the third, 15 cents flat
  assert.ok(q.done && q.target === null);
  assert.deepEqual(q.results, [10, null, -15]);
  assert.equal(q.hit, 2);
  assert.equal(q.push(64), false);
});
