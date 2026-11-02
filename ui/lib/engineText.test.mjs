// npm test: the engine's kinds and codes (src-tauri/messages/KINDS.md) said from the catalog (lib/engineText.ts),
// in English exactly as the engine says them: every row of KINDS.md's tables, with sample values, rendered by the
// window and compared with the engine's English from the same row.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { t } from "./i18n.ts";
import { appText, EngineError, engineErrorText, fitText, lineText, modelsError, reportWhat, shareReason, timingError, updateWhy, youtubeSetting } from "./engineText.ts";

const kinds = readFileSync(new URL("../../src-tauri/messages/KINDS.md", import.meta.url), "utf8");

/** The rows of the table under a "## " heading starting with `heading`: cells, without the header rows. */
function table(heading) {
  const section = kinds.split(/^## /m).find(s => s.startsWith(heading));
  assert.ok(section, `KINDS.md has "## ${heading}"`);
  return section.split("\n").filter(l => l.startsWith("| `"))
    .map(l => l.split("|").slice(1, -1).map(c => c.trim()));
}
const code = cell => /^`([^`]+)`/.exec(cell)[1];
const SAMPLE = { song: "Yellow", app: "Spotify", score: "72", over: "80" };
/** KINDS.md's English with the values put in ("{points} point(s)": 1 point, 12 points). */
const fill = (english, params) => english.replace(/\{(\w+)\}( point\(s\))?/g, (_, k, pts) =>
  pts ? `${params[k]} point${params[k] === "1" ? "" : "s"}` : params[k]);

test("the status line: each kind's English as the engine's, with its values", () => {
  const rows = table("The status line");
  assert.equal(rows.length, 9);
  for (const [kind, params, english] of rows) {
    const p = params === "none" ? {} : Object.fromEntries([...params.matchAll(/`(\w+)`/g)].map(m => [m[1], SAMPLE[m[1]]]));
    assert.equal(lineText(t, { kind: code(kind), params: p, text: "(the engine's)" }), fill(english, p), code(kind));
  }
});

test("the share ask: each kind's English, plural points included", () => {
  const rows = table("The share ask");
  assert.equal(rows.length, 4);
  for (const [kind, params, english] of rows)
    for (const points of ["1", "12"]) {
      const p = Object.fromEntries([...params.matchAll(/`(\w+)`/g)].map(m => [m[1], m[1] === "points" ? points : SAMPLE[m[1]]]));
      assert.equal(shareReason(t, { kind: code(kind), params: p, reason: "(the engine's)" }), fill(english, p), `${code(kind)} ${points}`);
    }
  // a song named with digits stays text
  assert.equal(shareReason(t, { kind: "first", params: { song: "1999", score: "85" }, reason: "x" }), "85 on your first go at 1999");
});

test("a refused timing check: each code's English; \"other\" and no code show the engine's words", () => {
  const rows = table("The output timing check").filter(([c]) => code(c) !== "other");
  assert.equal(rows.length, 7);
  for (const [c, english] of rows) assert.equal(timingError(t, { code: code(c), error: "(the engine's)" }), english, code(c));
  assert.equal(timingError(t, { code: "other", error: "Something new." }), "Something new.");
  assert.equal(timingError(t, { error: "Couldn’t play the clicks." }), "Couldn’t play the clicks.");
});

test("a setting that couldn't be saved (setting_unsaved), with the system's reason", () => {
  const english = /"(Couldn’t save the setting \(\{OS error\}\)\. It holds until OnKey quits\.)"/.exec(kinds.replace(/\s+/g, " "))?.[1];
  assert.ok(english, "KINDS.md gives the English");
  const said = english.replace("{OS error}", "No space left on device (os error 28)");
  assert.equal(engineErrorText(t, new EngineError("setting_unsaved", said), "fallback"), said);
  assert.equal(engineErrorText(t, new EngineError("setting_unsaved", "something else"), "fallback"), "something else", "English it doesn't recognise: as sent");
  assert.equal(engineErrorText(t, "a plain rejection", "fallback"), "a plain rejection");
  assert.equal(engineErrorText(t, null, "fallback"), "fallback");
});

test("a kind the catalog doesn't know, or a value missing: the engine's English", () => {
  const quiet = console.error;
  console.error = () => {};
  try {
    assert.equal(lineText(t, { kind: "brand_new", params: {}, text: "Something new" }), "Something new");
    assert.equal(lineText(t, { kind: "stopped", params: {}, text: "Not learning Yellow" }), "Not learning Yellow");
    assert.equal(shareReason(t, { kind: "best", reason: "Your best on Yellow by 12 points" }), "Your best on Yellow by 12 points", "an older engine: no params");
  } finally {
    console.error = quiet;
  }
});

/** A Rust source file of the engine, for checking the catalog's English against the words it sends. */
const rust = f => readFileSync(new URL(`../../src-tauri/src/${f}`, import.meta.url), "utf8");

test("a song's fit with your range: each key's English as range.rs says it", () => {
  const rows = table("A song's fit");
  assert.equal(rows.length, 8);
  const labels = rust("range.rs");
  for (const [key, english] of rows) {
    assert.equal(fitText(t, { fit: code(key), fit_label: "(the engine's)" }), english, code(key));
    assert.ok(labels.includes(`=> ${JSON.stringify(english)}`), `range.rs Fit::label says ${english}`);
  }
  assert.equal(fitText(t, { fit: "brand_new", fit_label: "Something new" }), "Something new", "a key the catalog doesn't know");
  assert.equal(fitText(t, { fit_label: "Older engine" }), "Older engine");
  assert.equal(fitText(t, {}), null);
});

test("a problem report's files: each kind's English, on a Mac and on a PC", () => {
  const rows = table("A problem report's files");
  assert.equal(rows.length, 6);
  const params = { artist: "The Fray", name: "How to Save a Life" };
  for (const [kind, , english] of rows)
    for (const [os, machine] of [["mac", "Mac"], ["pc", "PC"]]) {
      // (Linux's summary says "computer"; the window has no "linux" os, so a PC's summary says PC)
      const want = fill(english.replace("{machine}", code(kind) === "summary" && os === "mac" ? "computer" : machine), params);
      assert.equal(reportWhat(t, { what: "(the engine's)", what_kind: code(kind), what_params: params }, os), want, `${code(kind)} ${os}`);
    }
  const report = rust("report.rs");
  assert.ok(report.includes(`"OnKey's log for that day"`) && report.includes(`a play's summary: {artist} - {song}`), "report.rs says the same");
  assert.equal(reportWhat(t, { what: "the engine's words" }, "mac"), "the engine's words", "an older engine: no kind");
});

test("the learning models' trouble: unloadable from the catalog, technical detail as sent", () => {
  assert.equal(modelsError(t, { code: "unloadable", error: "they couldn't be loaded" }), "they couldn't be loaded");
  assert.ok(rust("engine.rs").includes(`"code": "unloadable", "error": "they couldn't be loaded"`), "engine.rs says the same");
  assert.equal(modelsError(t, { code: "files_bad", error: "crepe.onnx: 0 bytes" }), "crepe.onnx: 0 bytes");
  assert.equal(modelsError(t, { code: "pitch_model" }), "");
});

test("the browser setting YouTube needs: each kind's English as platform/mac/youtube.rs says it", () => {
  const rows = table("The browser setting");
  assert.equal(rows.length, 2);
  const src = rust("platform/mac/youtube.rs");
  for (const [kind, english] of rows) {
    assert.equal(youtubeSetting(t, { setting: "(the engine's)", setting_kind: code(kind) }), english, code(kind));
    assert.ok(src.includes(JSON.stringify(english)), `youtube.rs js_setting says ${english}`);
  }
  assert.equal(youtubeSetting(t, { setting: "Somewhere ▸ New" }), "Somewhere ▸ New", "an older engine: no kind");
});

test("an app with no name of its own, and the update's reason", () => {
  assert.equal(appText(t, "Spotify"), "Spotify");
  assert.equal(appText(t, null), "your music app");
  assert.equal(appText(t, null, "browser"), "your browser");
  assert.equal(appText(t, null, "music_app"), "your music app");
  assert.equal(appText(t, "Apple Music", "app"), "Apple Music");
  assert.equal(updateWhy({ code: "install_failed", error: "the update couldn't be installed" }), null);
  assert.equal(updateWhy({ code: "download_failed", error: "HTTP 503" }), "HTTP 503");
  assert.equal(updateWhy({ code: "check_failed", error: "" }), null);
});
