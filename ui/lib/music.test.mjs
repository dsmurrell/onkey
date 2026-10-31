// npm test: the scoring levels as the window applies them (the line verdicts, the take page's strip, the best)
import { test } from "node:test";
import assert from "node:assert/strict";
import { bestAtLatestLevel, centreAt, credit, creditAt, lineGrade, offBy } from "./music.ts";

test("Relaxed credit is the rule as it was; stricter levels give less off the note", () => {
  for (let e = -300; e <= 300; e += 0.7) assert.equal(creditAt(e, "relaxed"), credit(e));
  assert.equal(creditAt(35, "steady"), 1); assert.equal(creditAt(100, "steady"), 0);
  assert.equal(creditAt(25, "pro"), 1); assert.equal(creditAt(70, "pro"), 0);
  for (const e of [30, 40, 60, 90]) assert.ok(creditAt(e, "relaxed") >= creditAt(e, "steady") && creditAt(e, "steady") >= creditAt(e, "pro"));
});

test("the centre evens out a 6 Hz vibrato and keeps a held wrong note wrong", () => {
  const vib = f => 60 + 0.8 * Math.sin(2 * Math.PI * 6 * f * 0.01);
  for (let f = 20; f < 280; f++) assert.ok(Math.abs(100 * (centreAt(vib, f, 17) - 60)) < 25, `frame ${f}`);
  assert.ok(Math.abs(centreAt(() => 60.4, 50, 17) - 60.4) < 1e-9);
  assert.ok(Number.isNaN(centreAt(() => NaN, 5, 17)));
});

test("the best is like with like: of the takes at the latest take's level", () => {
  const t = (score, recorded, level) => ({ score, recorded, level });
  assert.deepEqual(bestAtLatestLevel([t(90, "2026-10-01", undefined), t(70, "2026-10-02", "pro"), t(75, "2026-10-03", "pro")]), { best: 75, level: "pro" });
  assert.deepEqual(bestAtLatestLevel([t(70, "2026-10-01", "pro"), t(80, "2026-10-02", "relaxed")]), { best: 80, level: "relaxed" });
  assert.equal(bestAtLatestLevel([]), null);
});

test("how far off reads in cents under a semitone, then semitones, then octaves", () => {
  assert.equal(offBy(-35), "35¢ flat"); assert.equal(offBy(14.4), "14¢ sharp"); assert.equal(offBy(-99.4), "99¢ flat");
  assert.equal(offBy(100), "about a semitone sharp"); assert.equal(offBy(-481), "about 5 semitones flat");
  assert.equal(offBy(-1180), "about an octave low"); assert.equal(offBy(1250), "about an octave high");
  assert.equal(offBy(2400), "about 2 octaves high");
});

test("line colours ease as the level gets stricter", () => {
  assert.equal(lineGrade(0.7, "relaxed"), "close"); assert.equal(lineGrade(0.7, "pro"), "good");
  assert.equal(lineGrade(0.5, "relaxed"), "off"); assert.equal(lineGrade(0.5, "pro"), "close");
  for (const a of [0.3, 0.5, 0.6, 0.7, 0.8, 0.9]) {
    const rank = l => ["off", "close", "good"].indexOf(lineGrade(a, l));
    assert.ok(rank("relaxed") <= rank("steady") && rank("steady") <= rank("pro"), `accuracy ${a}`);
  }
});
