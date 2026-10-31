// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { rolling, vsBefore, changeWords, lastVsFirst } from "./trend.ts";

test("rolling average: fewer at the start, then the last n", () => {
  assert.deepEqual(rolling([10, 20, 30, 40], 2), [10, 15, 25, 35]);
  assert.deepEqual(rolling([], 10), []);
  const v = Array.from({ length: 25 }, (_, i) => i);
  const r = rolling(v);
  assert.equal(r[0], 0);
  assert.equal(r[4], 2);
  assert.equal(r[24], 19.5);   // 15..24
});

test("last 10 against the 10 before, only with 20", () => {
  assert.equal(vsBefore(Array(19).fill(50)), null);
  assert.deepEqual(vsBefore([...Array(10).fill(74), ...Array(10).fill(70)]), { now: 74, change: 4 });
  assert.deepEqual(vsBefore([...Array(10).fill(60), ...Array(10).fill(63), 1, 1]), { now: 60, change: -3 });
  assert.equal(changeWords(4), "up 4 on the 10 before");
  assert.equal(changeWords(-3), "down 3 on the 10 before");
  assert.equal(changeWords(0), "same as the 10 before");
});

test("a song: last 3 against first 3 from 6 plays, else latest against first", () => {
  assert.equal(lastVsFirst([]), null);
  assert.deepEqual(lastVsFirst([40]), { from: 40, to: 40, change: 0, both: false });
  assert.deepEqual(lastVsFirst([40, 90, 50, 60, 70]), { from: 40, to: 70, change: 30, both: false });
  assert.deepEqual(lastVsFirst([40, 50, 60, 0, 70, 80, 90]), { from: 50, to: 80, change: 30, both: true });
});
