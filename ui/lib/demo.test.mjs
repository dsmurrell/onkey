// npm test (node's own test runner; lib/demo.ts is plain TypeScript, its types stripped)
import { test } from "node:test";
import assert from "node:assert/strict";
import { demoCredit, demoFrom, isDemo, offerDemo } from "./demo.ts";

const demo = { id: "demo:wellerman", name: "Soon May the Wellerman Come", artist: "Traditional", duration: 150, credit: "Recording made for OnKey", licence: "CC0" };

test("the demo is offered only when the build has a whole one", () => {
  assert.deepEqual(demoFrom(demo), demo);
  assert.equal(demoFrom(null), null);                                   // no demo bundled: no "Try it now"
  assert.equal(demoFrom({ ...demo, id: "spotify:track:1" }), null);
  assert.equal(demoFrom({ ...demo, name: "" }), null);
  assert.deepEqual(demoFrom({ id: "demo:x", name: "X" }), { id: "demo:x", name: "X", artist: "", duration: 0, credit: "", licence: "" });
});

test("a demo id is told from an app's", () => {
  assert.ok(isDemo("demo:wellerman"));
  assert.ok(!isDemo("spotify:track:54OBgO0Xwu20Jak9TMXbR7") && !isDemo("isrc:demo:") && !isDemo(undefined) && !isDemo(""));
});

test("home offers the demo until you have songs of your own", () => {
  const song = id => ({ track: { id } });
  assert.ok(offerDemo([]));
  assert.ok(offerDemo([song("demo:wellerman")]), "the demo itself doesn't count");
  assert.ok(!offerDemo([song("demo:wellerman"), song("spotify:track:1")]));
});

test("the credit says the song and who made the recording", () => {
  assert.equal(demoCredit(demo), "Soon May the Wellerman Come (Traditional). Recording made for OnKey");
  assert.equal(demoCredit({ ...demo, artist: "", credit: "" }), "Soon May the Wellerman Come");
});
