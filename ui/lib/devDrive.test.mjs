// npm test (node's own test runner; lib/devDrive.ts is plain TypeScript, its types stripped)
import { test } from "node:test";
import assert from "node:assert/strict";
import { driveGraphAt, driveHref } from "./devDrive.ts";

test("a drive request opens its take with where to start", () => {
  assert.equal(driveHref({ take: "20261026-140201-fun-We-Are-Young", song: 61.5, n: 7 }), "/take?id=20261026-140201-fun-We-Are-Young&along=song%3A61.5&n=7");
  assert.equal(driveHref({ take: "a b", t: 12, n: 1 }), "/take?id=a%20b&along=t%3A12&n=1");
  assert.equal(driveHref({ take: "x", n: 1 }), "/take?id=x&along=t%3A0&n=1");
  assert.equal(driveHref({ stop: true }), null);
  assert.equal(driveHref({}), null);
});

test("song time is moved onto the take's graph", () => {
  assert.equal(driveGraphAt("song:61.5", 40), 21.5);
  assert.equal(driveGraphAt("song:10", 40), 0);
  assert.equal(driveGraphAt("t:12.25", 40), 12.25);
  assert.equal(driveGraphAt("t:3", null), 3);
  assert.equal(driveGraphAt("song:3", null), null);
  assert.equal(driveGraphAt("song:-3", 0), null);
  assert.equal(driveGraphAt("nonsense", 0), null);
  assert.equal(driveGraphAt(null, 0), null);
});
