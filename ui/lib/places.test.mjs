// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { songAtTake, takeAtSong } from "./places.ts";

const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `${a} vs ${b}`);

// Uptown Girl, 30 Oct: placed +0.15 s, then a skip at 83.2 s that Spotify landed 0.65 s early (-0.50 s after it)
const places = [[0, 8322, 15], [8322, 9223372036854775807, -50]];

test("before a skip the recording plays at its own place, not the place after the skip", () => {
  near(takeAtSong(10.4, places, -0.5), 10.25);
  near(songAtTake(10.25, places, -0.5), 10.4);
});

test("after a skip the recording plays at the place after it", () => {
  near(takeAtSong(120, places, -0.5), 120.5);
  near(songAtTake(120.5, places, -0.5), 120);
});

test("a take kept without places has its song offset throughout", () => {
  near(takeAtSong(10.4, undefined, -0.5), 10.9);
  near(songAtTake(10.9, [], -0.5), 10.4);
});
