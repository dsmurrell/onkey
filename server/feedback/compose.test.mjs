import { test } from "node:test";
import assert from "node:assert/strict";
import { check, mime, subject, text } from "./compose.js";

const base = { version: "0.3.1", os: "macos", os_version: "15.5", language: "es", from: "take" };

test("a thumb or words, nothing else unknown", () => {
  assert.equal(check({ ...base, mood: null, text: "  " }).ok, false);
  assert.equal(check({ ...base, mood: "sideways" }).ok, false);
  assert.equal(check({ ...base, mood: "up" }).ok, true);
  assert.equal(check({ ...base, text: "x".repeat(4001) }).ok, false);
  assert.equal(check(null).ok, false);
});

test("a reply address only when it looks like one; no header can be split", () => {
  assert.equal(check({ ...base, mood: "up", email: "a@b.co" }).f.email, "a@b.co");
  assert.equal(check({ ...base, mood: "up", email: "nope" }).f.email, "");
  const f = check({ ...base, mood: "up", email: "a@b.co\r\nBcc: x@y.z", version: "1\r\nX: y" }).f;
  assert.equal(f.email, "");
  const m = mime(f, "feedback@singonkey.app", "me@example.com", "id1");
  const head = m.split("\r\n\r\n")[0];
  assert.ok(!/^Bcc:/m.test(head) && !/^X:/m.test(head));
});

test("the subject and text say what came", () => {
  const f = check({ ...base, mood: "down", text: "lyrics late", email: "a@b.co" }).f;
  assert.equal(subject(f), "OnKey feedback: 👎 (macos 15.5, 0.3.1, es)");
  const t = text(f, new Date("2026-11-03T12:00:00Z"));
  assert.match(t, /lyrics late/);
  assert.match(t, /Reply to: a@b.co/);
  assert.match(t, /the card on a take's page/);
  assert.match(text(check({ ...base, mood: "up", from: "bar" }).f), /the top bar's speech bubble/);
  assert.match(text(check({ ...base, mood: "up", from: "elsewhere" }).f), /Settings → Send feedback/);
  const m = mime(f, "feedback@singonkey.app", "me@example.com", "id1", new Date("2026-11-03T12:00:00Z"));
  assert.match(m, /^Reply-To: <a@b.co>$/m);
  const body = m.split("\r\n\r\n")[1].replace(/\r\n/g, "");
  assert.match(Buffer.from(body, "base64").toString("utf8"), /lyrics late/);
});
