// npm test (node's own test runner; lib/dataSafety.ts is plain TypeScript, its types stripped)
import { test } from "node:test";
import assert from "node:assert/strict";
import { deleteEach, errorText, readList, runDelete } from "./dataSafety.ts";

const TRASH = "Couldn't move it to the Trash, so nothing was deleted.";

test("a failed delete comes back as its message, so the button is reset and shows it", async () => {
  assert.equal(await runDelete(async () => {}), null);
  assert.equal(await runDelete(() => {}), null);
  // invoke() rejects with the engine's Err string
  assert.equal(await runDelete(() => Promise.reject(TRASH)), TRASH);
  assert.equal(await runDelete(() => { throw new Error("disk full"); }), "disk full");
  assert.equal(await runDelete(() => Promise.reject(undefined)), "Couldn’t delete it.");
});

test("errorText uses the engine's words when there are some", () => {
  assert.equal(errorText("x", "y"), "x");
  assert.equal(errorText("  ", "y"), "y");
  assert.equal(errorText({}, "y"), "y");
});

test("deleting several stops at the first failure and drops only those deleted", async () => {
  const deleted = [];
  let gone = null;
  const del = async id => { if (id === "b") throw TRASH; deleted.push(id); };
  await assert.rejects(deleteEach(["a", "b", "c"], del, ids => { gone = ids; }), e => e === TRASH);
  assert.deepEqual(deleted, ["a"]);
  assert.deepEqual(gone, ["a"]);
  gone = null;
  await deleteEach(["a", "c"], del, ids => { gone = ids; });
  assert.deepEqual(gone, ["a", "c"]);
});

test("a read that fails is an error, never an empty list", async () => {
  assert.deepEqual(await readList(async () => [1, 2], "takes"), { items: [1, 2] });
  assert.deepEqual(await readList(async () => [], "takes"), { items: [] });
  assert.deepEqual(await readList(() => Promise.reject("couldn't read the takes folder (Permission denied)"), "takes"),
                   { error: "Couldn’t read your takes", detail: "couldn't read the takes folder (Permission denied)" });
  assert.deepEqual(await readList(async () => null, "songs"), { error: "Couldn’t read your songs", detail: "" });
});
