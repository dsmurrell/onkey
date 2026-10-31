// npm test (node's own test runner; lib/keys.ts is plain TypeScript, its types stripped)
import { test } from "node:test";
import assert from "node:assert/strict";
import { typing } from "./keys.ts";

// an element whose `closest` matches when it is one of `kinds` (a stand-in for the DOM's)
const el = (...kinds) => ({ closest: sel => kinds.some(k => sel.includes(k)) ? {} : null });

test("one-key shortcuts are left alone while typing or while a dialog is open", () => {
  const withDialog = open => { globalThis.document = { querySelector: sel => open && sel.includes("dialog") ? {} : null }; };
  withDialog(false);
  assert.equal(typing({ target: el() }), false, "the page itself: shortcuts work");
  assert.equal(typing({ target: el("button") }), false, "a button on the page (Space still plays/pauses)");
  assert.equal(typing({ target: el("input") }), true);
  assert.equal(typing({ target: el("textarea") }), true);
  assert.equal(typing({ target: el("[role=dialog]") }), true, "a button inside a dialog");
  withDialog(true);
  assert.equal(typing({ target: el() }), true, "Setup open, focus elsewhere: M doesn't switch to Mini");
  delete globalThis.document;
  assert.equal(typing({ target: null }), false);
});
