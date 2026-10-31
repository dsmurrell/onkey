// npm test (node's own test runner; lib/alongTiming.ts is plain TypeScript, its types stripped)
import { test } from "node:test";
import assert from "node:assert/strict";
import { alongTiming } from "./alongTiming.ts";

test("the timing line says every figure, short", () => {
  const line = alongTiming({ heard: 62.41, position: 62.95, feed: [0.023, 0.03, 0.012] }, 62.41, 61.03, 61.05, 180);
  assert.equal(line, "heard 62.41 app +0.54 · rec 61.03 voice 61.05 off -0.02 · core out 180 · feed 23 ago, block 30, behind 12");
});

test("missing figures are ?, never a throw", () => {
  assert.match(alongTiming({ heard: 1, position: 1, feed: null }, 1, 0.5, null, undefined), /voice \? off \? · core out \? · no feed$/);
  assert.match(alongTiming({ heard: 1, position: 1, demo: true }, 1, 0.5, 0.5, 0), /demo: OnKey's player$/);
});
