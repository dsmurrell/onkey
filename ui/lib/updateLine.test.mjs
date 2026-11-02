// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { updateLine, updateFailed } from "./updateLine.ts";

// A translator that says which message it was asked for, and with what.
const t = (key, values) => values ? `${key} ${JSON.stringify(values)}` : key;
const status = (over) => ({ version: "0.2.0", supported: true, official: true, auto: true, state: "idle", waiting: false, checked: null, ...over });

test("a preview (CI's Windows and Linux test builds) says it doesn't update itself, on any platform", () => {
  const preview = status({ version: "0.2.0-preview.3", supported: false, preview: true, official: false });
  assert.equal(updateLine(t, preview, false), "settings.updates.unsupportedPc");
  assert.equal(updateLine(t, preview, true), "settings.updates.unsupportedPc");
});

test("a development build says so on the Mac; on Windows any unsupported build is a tester's", () => {
  const dev = status({ supported: false, preview: false });
  assert.equal(updateLine(t, dev, false), "settings.updates.unsupportedMac");
  assert.equal(updateLine(t, dev, true), "settings.updates.unsupportedPc");
  assert.equal(updateLine(t, status({ supported: false }), false), "settings.updates.unsupportedMac");
});

test("a supported build says what the updater is doing", () => {
  assert.equal(updateLine(t, status({ state: "current" }), false), "settings.updates.current");
  assert.equal(updateLine(t, status({ state: "current", official: false }), false), "settings.updates.currentSource");
  assert.equal(updateLine(t, status({ state: "ready", available: "0.3.0" }), false), 'settings.updates.ready {"version":"0.3.0"}');
  assert.equal(updateLine(t, status({ state: "ready", available: "0.3.0", waiting: true }), false), 'settings.updates.waiting {"version":"0.3.0"}');
  assert.equal(updateLine(t, status({ state: "idle" }), false), "");
});

test("errors: a failed check or a failed update, with the reason unless it only repeats the headline", () => {
  assert.equal(updateLine(t, status({ state: "error", code: "check_failed" }), false), "settings.updates.checkFailed");
  assert.equal(updateLine(t, status({ state: "error", code: "check_failed", error: "timed out" }), false), 'settings.updates.checkFailedWhy {"error":"timed out"}');
  assert.equal(updateLine(t, status({ state: "error", code: "install_failed", available: "0.3.0", error: "the update couldn't be installed" }), false), 'settings.updates.updateFailed {"version":"0.3.0"}');
  assert.equal(updateFailed({ available: "0.3.0" }), "update");
  assert.equal(updateFailed({}), "check");
});
