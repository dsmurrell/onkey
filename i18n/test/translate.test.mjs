// The translate tool against a fake Anthropic API: no network, no key needed.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { run, flatten, hash, LOCK_FILE, REVIEW_FILE } from "../translate.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixture = JSON.parse(fs.readFileSync(path.join(here, "fixtures/en.json"), "utf8"));

/** A temp catalog dir (with en.json) and an i18n dir (glossary + style guides). */
function setup() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "onkey-i18n-"));
  const dir = path.join(root, "messages");
  const i18nDir = path.join(root, "i18n");
  fs.mkdirSync(dir);
  fs.mkdirSync(path.join(i18nDir, "style"), { recursive: true });
  fs.writeFileSync(path.join(dir, "en.json"), JSON.stringify(fixture, null, 2));
  fs.writeFileSync(path.join(i18nDir, "glossary.md"), "OnKey: keep.\n");
  for (const l of ["es", "ja", "ru"]) fs.writeFileSync(path.join(i18nDir, "style", `${l}.md`), `Style for ${l}.\n`);
  return { root, dir, i18nDir };
}

const read = (file) => JSON.parse(fs.readFileSync(file, "utf8"));
const setEnglish = (dir, mutate) => {
  const en = read(path.join(dir, "en.json"));
  mutate(en);
  fs.writeFileSync(path.join(dir, "en.json"), JSON.stringify(en, null, 2));
};

/**
 * A fake fetch. `translate(key, en, lang, entry)` returns the message to send back for a key
 * (default: "[lang] " + English, which keeps ICU intact). Records every request.
 */
function fakeApi(translate = (key, en, lang) => `[${lang}] ${en}`) {
  const calls = [];
  const fetchImpl = async (url, init) => {
    const body = JSON.parse(init.body);
    const user = body.messages[0].content;
    const items = JSON.parse(user.slice(user.indexOf("{")));
    const lang = /locale code ([\w-]+)\)/.exec(body.system[0].text)[1];
    calls.push({ url, headers: init.headers, body, items, lang });
    const out = {};
    for (const [k, e] of Object.entries(items)) {
      const v = translate(k, e.en, lang, e, calls.length);
      if (v !== undefined) out[k] = v;
    }
    return {
      ok: true, status: 200, headers: { get: () => null },
      json: async () => ({ stop_reason: "end_turn", content: [{ type: "text", text: JSON.stringify(out) }], usage: {} }),
    };
  };
  return { fetchImpl, calls };
}

const base = (s, extra) => ({
  catalog: { dir: s.dir, name: "test", about: "a test catalog" },
  i18nDir: s.i18nDir,
  env: { ANTHROPIC_API_KEY: "test-key" },
  sleep: async () => {},
  log: () => {},
  langs: ["es"],
  ...extra,
});

const ESC = (en) => `[es] ${en}`;

test("missing keys are sent, written with nesting, and locked", async () => {
  const s = setup();
  const api = fakeApi();
  const { ok } = await run(base(s, { mode: "write", fetchImpl: api.fetchImpl }));
  assert.ok(ok);
  assert.equal(api.calls.length, 1);
  assert.deepEqual(Object.keys(api.calls[0].items).sort(), [...flatten(fixture).keys()].sort());
  assert.equal(api.calls[0].body.model, "claude-fable-5-1");
  assert.equal(api.calls[0].body.fallbacks, "default", "the default model gets the refusal fallback");
  assert.match(api.calls[0].headers["anthropic-beta"], /server-side-fallback/);
  assert.equal(api.calls[0].headers["x-api-key"], "test-key");
  assert.equal(api.calls[0].body.system[0].cache_control.type, "ephemeral");
  assert.match(api.calls[0].body.system[0].text, /Style for es\./);
  assert.match(api.calls[0].body.system[0].text, /OnKey: keep\./);

  const es = read(path.join(s.dir, "es.json"));
  assert.equal(es.nav.sing, ESC("Sing"));
  assert.deepEqual(Object.keys(es), Object.keys(fixture)); // English order and nesting
  const lock = read(path.join(s.dir, LOCK_FILE));
  assert.deepEqual(lock.es["nav.sing"], { en: hash("Sing"), out: hash(ESC("Sing")), human: false });
  assert.ok(!fs.existsSync(path.join(s.dir, REVIEW_FILE)));
});

test("model id comes from ONKEY_TRANSLATE_MODEL", async () => {
  const s = setup();
  const api = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api.fetchImpl, env: { ANTHROPIC_API_KEY: "k", ONKEY_TRANSLATE_MODEL: "claude-haiku-5-5" } }));
  assert.equal(api.calls[0].body.model, "claude-haiku-5-5");
  assert.equal(api.calls[0].body.fallbacks, undefined);
});

test("unchanged keys are skipped; nothing is sent the second time", async () => {
  const s = setup();
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  const api = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api.fetchImpl }));
  assert.equal(api.calls.length, 0);
});

test("English changed + machine-translated → sent again, with the previous translation", async () => {
  const s = setup();
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  setEnglish(s.dir, (en) => { en.nav.library = "Your library"; });
  const api = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api.fetchImpl }));
  assert.equal(api.calls.length, 1);
  assert.deepEqual(Object.keys(api.calls[0].items), ["nav.library"]);
  assert.equal(api.calls[0].items["nav.library"].previous, ESC("Library"));
  assert.equal(read(path.join(s.dir, "es.json")).nav.library, ESC("Your library"));
});

test("English changed + human-edited → not sent, kept, flagged for review", async () => {
  const s = setup();
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  // A contributor fixes the Spanish.
  const es = read(path.join(s.dir, "es.json"));
  es.nav.library = "Biblioteca";
  fs.writeFileSync(path.join(s.dir, "es.json"), JSON.stringify(es, null, 2));
  // Unchanged English: the edit is detected and protected, nothing is sent.
  const api1 = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api1.fetchImpl }));
  assert.equal(api1.calls.length, 0);
  assert.equal(read(path.join(s.dir, LOCK_FILE)).es["nav.library"].human, true);

  // Then English changes: still not sent, still the contributor's words, listed for review.
  setEnglish(s.dir, (en) => { en.nav.library = "Your library"; en.nav.sing = "Sing now"; });
  const api2 = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api2.fetchImpl }));
  assert.deepEqual(Object.keys(api2.calls[0].items), ["nav.sing"]); // machine one re-sent
  assert.equal(read(path.join(s.dir, "es.json")).nav.library, "Biblioteca");
  const report = fs.readFileSync(path.join(s.dir, REVIEW_FILE), "utf8");
  assert.match(report, /nav\.library/);
  assert.match(report, /Your library/);
  assert.doesNotMatch(report, /nav\.sing/);

  // A reviewer accepts it: the flag and report clear, and it stays protected.
  await run(base(s, { mode: "mark-reviewed", keys: ["nav.library"] }));
  assert.ok(!fs.existsSync(path.join(s.dir, REVIEW_FILE)));
  const api3 = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api3.fetchImpl }));
  assert.equal(api3.calls.length, 0);
});

test("translations the tool didn't write are adopted as human and never overwritten", async () => {
  const s = setup();
  fs.writeFileSync(path.join(s.dir, "es.json"), JSON.stringify({ nav: { sing: "Cantar" } }));
  const api = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api.fetchImpl }));
  assert.ok(!("nav.sing" in api.calls[0].items));
  assert.equal(read(path.join(s.dir, "es.json")).nav.sing, "Cantar");
  assert.equal(read(path.join(s.dir, LOCK_FILE)).es["nav.sing"].human, true);
});

test("--retranslate --keys re-sends a human-edited key and returns it to the tool", async () => {
  const s = setup();
  fs.writeFileSync(path.join(s.dir, "es.json"), JSON.stringify({ nav: { sing: "Cantar" } }));
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  const api = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api.fetchImpl, keys: ["nav.sing"], retranslate: true }));
  assert.deepEqual(Object.keys(api.calls[0].items), ["nav.sing"]);
  assert.equal(read(path.join(s.dir, LOCK_FILE)).es["nav.sing"].human, false);
});

test("keys English dropped are removed from translations and the lock", async () => {
  const s = setup();
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  setEnglish(s.dir, (en) => { delete en.library.learned; });
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  assert.equal(read(path.join(s.dir, "es.json")).library.learned, undefined);
  assert.equal(read(path.join(s.dir, LOCK_FILE)).es["library.learned"], undefined);
});

test("dry run sends nothing and writes nothing", async () => {
  const s = setup();
  const lines = [];
  const api = fakeApi();
  await run(base(s, { mode: "dry-run", fetchImpl: api.fetchImpl, log: (l) => lines.push(l) }));
  assert.equal(api.calls.length, 0);
  assert.ok(!fs.existsSync(path.join(s.dir, "es.json")));
  assert.ok(!fs.existsSync(path.join(s.dir, LOCK_FILE)));
  assert.match(lines.join("\n"), /es: 6 to send \(6 new/);
  assert.match(lines.join("\n"), /send \[missing\] take\.scored/);
});

test("a broken reply is retried once with the problem, then accepted", async () => {
  const s = setup();
  const api = fakeApi((key, en, lang, entry, call) => {
    if (key === "take.scored" && call === 1) return "Has sacado {score} en {song}"; // lost <b>
    if (key === "library.count" && call === 1) return "{count} canciones"; // broke the plural
    return `[${lang}] ${en}`;
  });
  const { ok } = await run(base(s, { mode: "write", fetchImpl: api.fetchImpl }));
  assert.ok(ok);
  assert.equal(api.calls.length, 2);
  assert.deepEqual(Object.keys(api.calls[1].items).sort(), ["library.count", "take.scored"]);
  assert.match(api.calls[1].items["take.scored"].problem, /lost <b>/);
  assert.match(api.calls[1].items["library.count"].problem, /plural/);
  assert.equal(read(path.join(s.dir, "es.json")).take.scored, ESC("You scored <b>{score}</b> on {song}"));
});

test("a reply that fails twice is reported, not written", async () => {
  const s = setup();
  const api = fakeApi((key, en, lang) => (key === "take.scored" ? "Has sacado {puntos}" : key === "nav.sing" ? undefined : `[${lang}] ${en}`));
  const { ok, results } = await run(base(s, { mode: "write", fetchImpl: api.fetchImpl }));
  assert.equal(ok, false);
  assert.equal(api.calls.length, 2);
  const failed = results.get("es").failed.map((f) => f.key).sort();
  assert.deepEqual(failed, ["nav.sing", "take.scored"]);
  const es = read(path.join(s.dir, "es.json"));
  assert.equal(es.take?.scored, undefined);
  assert.equal(es.nav?.sing, undefined);
  assert.equal(read(path.join(s.dir, LOCK_FILE)).es["take.scored"], undefined);
  assert.match(fs.readFileSync(path.join(s.dir, REVIEW_FILE), "utf8"), /take\.scored.*lost \{score\}/);
});

test("Russian and Japanese plurals are checked against their own categories", async () => {
  const s = setup();
  const forms = {
    ru: "{count, plural, =0 {Пока нет песен} one {# песня} few {# песни} many {# песен} other {# песни}}",
    ja: "{count, plural, =0 {まだ曲がありません} other {#曲}}",
  };
  const api = fakeApi((key, en, lang) => (key === "library.count" ? forms[lang] : `[${lang}] ${en}`));
  const { ok } = await run(base(s, { mode: "write", fetchImpl: api.fetchImpl, langs: ["ru", "ja"] }));
  assert.ok(ok);
  assert.equal(api.calls.length, 2);
  assert.match(api.calls.find((c) => c.lang === "ru").body.system[0].text, /one, few, many, other/);
});

test("HTTP 429 and 500 are retried; a bad reply body counts as a failed attempt", async () => {
  const s = setup();
  const ok200 = fakeApi();
  let n = 0;
  const fetchImpl = async (url, init) => {
    n++;
    if (n === 1) return { ok: false, status: 429, headers: { get: () => "1" }, json: async () => ({}) };
    if (n === 2) return { ok: false, status: 529, headers: { get: () => null }, json: async () => ({}) };
    if (n === 3) return { ok: true, status: 200, headers: { get: () => null }, json: async () => ({ stop_reason: "end_turn", content: [{ type: "text", text: "Sorry, here you go: not json" }] }) };
    return ok200.fetchImpl(url, init);
  };
  const { ok } = await run(base(s, { mode: "write", fetchImpl }));
  assert.ok(ok);
  assert.equal(n, 4);
  assert.equal(read(path.join(s.dir, "es.json")).nav.sing, ESC("Sing"));
});

test("--check finds missing, extra and invalid messages", async () => {
  const s = setup();
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  assert.equal((await run(base(s, { mode: "check" }))).ok, true);
  const es = read(path.join(s.dir, "es.json"));
  delete es.nav.sing;
  es.extra = "x";
  es.take.scored = "Has sacado {score}";
  fs.writeFileSync(path.join(s.dir, "es.json"), JSON.stringify(es));
  const { ok, results } = await run(base(s, { mode: "check" }));
  assert.equal(ok, false);
  const r = results.get("es");
  assert.deepEqual(r.missing, ["nav.sing"]);
  assert.deepEqual(r.extra, ["extra"]);
  assert.equal(r.invalid[0].key, "take.scored");
});

// --adopt: translations written outside the tool (by an agent, or a person) recorded in the lock without the API.
const writeEs = (s, es) => fs.writeFileSync(path.join(s.dir, "es.json"), JSON.stringify(es, null, 2));
const ES_ALL = {
  nav: { sing: "Cantar", library: "Biblioteca" },
  library: { count: "{count, plural, =0 {Aún no hay canciones} one {# canción} other {# canciones}}", learned: "Aprendida · lista para cantar" },
  take: { scored: "Sacaste <b>{score}</b> en {song}", who: "{singer, select, me {Tu toma} other {La toma de {singer}}}" },
};

test("--adopt --as machine: written outside the tool, recorded as the tool's, so a change of English refreshes it", async () => {
  const s = setup();
  writeEs(s, ES_ALL);
  const r = await run(base(s, { mode: "adopt", as: "machine" }));
  assert.ok(r.ok);
  const lock = read(path.join(s.dir, LOCK_FILE)).es;
  assert.equal(Object.keys(lock).length, 6);
  assert.ok(Object.values(lock).every((e) => e.human === false));
  assert.deepEqual(lock["nav.sing"], { en: hash("Sing"), out: hash("Cantar"), human: false });
  // nothing to send now
  const api1 = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api1.fetchImpl }));
  assert.equal(api1.calls.length, 0);
  // English changes: the key is sent again (not kept as a person's, not listed for review)
  setEnglish(s.dir, (en) => { en.nav.library = "Your library"; });
  const api2 = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api2.fetchImpl }));
  assert.deepEqual(Object.keys(api2.calls[0].items), ["nav.library"]);
  assert.equal(api2.calls[0].items["nav.library"].previous, "Biblioteca");
  assert.ok(!fs.existsSync(path.join(s.dir, REVIEW_FILE)));
});

test("--adopt --as machine after English changed: the rewritten value becomes the tool's again", async () => {
  const s = setup();
  writeEs(s, ES_ALL);
  await run(base(s, { mode: "adopt", as: "machine" }));
  setEnglish(s.dir, (en) => { en.nav.library = "Your library"; });
  writeEs(s, { ...ES_ALL, nav: { ...ES_ALL.nav, library: "Tu biblioteca" } });   // an agent's update
  await run(base(s, { mode: "adopt", as: "machine" }));
  assert.deepEqual(read(path.join(s.dir, LOCK_FILE)).es["nav.library"], { en: hash("Your library"), out: hash("Tu biblioteca"), human: false });
});

test("--adopt --as machine never takes a person's translation unless --keys names it", async () => {
  const s = setup();
  writeEs(s, ES_ALL);
  await run(base(s, { mode: "adopt", as: "machine" }));
  // someone fixes a current translation, and another key is a person's already
  writeEs(s, { ...ES_ALL, nav: { ...ES_ALL.nav, sing: "¡Canta!" } });
  const lock0 = read(path.join(s.dir, LOCK_FILE));
  lock0.es["library.learned"].human = true;
  fs.writeFileSync(path.join(s.dir, LOCK_FILE), JSON.stringify(lock0));
  await run(base(s, { mode: "adopt", as: "machine" }));
  let lock = read(path.join(s.dir, LOCK_FILE)).es;
  assert.equal(lock["nav.sing"].out, hash("Cantar"), "the fix isn't recorded as the tool's");
  assert.equal(lock["library.learned"].human, true);
  // and the next write marks the fix as a person's, as for any edit
  await run(base(s, { mode: "write", fetchImpl: fakeApi().fetchImpl }));
  assert.equal(read(path.join(s.dir, LOCK_FILE)).es["nav.sing"].human, true);
  // named: handed to the tool
  await run(base(s, { mode: "adopt", as: "machine", keys: ["library.learned"] }));
  lock = read(path.join(s.dir, LOCK_FILE)).es;
  assert.equal(lock["library.learned"].human, false);
  assert.equal(lock["nav.sing"].human, true);
});

test("--adopt --as human: recorded as a person's, never overwritten; English changing lists it for review", async () => {
  const s = setup();
  writeEs(s, ES_ALL);
  await run(base(s, { mode: "adopt", as: "human" }));
  assert.ok(Object.values(read(path.join(s.dir, LOCK_FILE)).es).every((e) => e.human === true));
  setEnglish(s.dir, (en) => { en.nav.library = "Your library"; });
  const api = fakeApi();
  await run(base(s, { mode: "write", fetchImpl: api.fetchImpl }));
  assert.equal(api.calls.length, 0);
  assert.equal(read(path.join(s.dir, "es.json")).nav.library, "Biblioteca");
  assert.match(fs.readFileSync(path.join(s.dir, REVIEW_FILE), "utf8"), /nav\.library/);
});

test("--adopt checks each value like the tool's own: a broken one isn't recorded, and the run fails", async () => {
  const s = setup();
  writeEs(s, { ...ES_ALL, take: { ...ES_ALL.take, scored: "Sacaste {puntos} en {song}" } });
  const r = await run(base(s, { mode: "adopt", as: "machine" }));
  assert.equal(r.ok, false);
  assert.deepEqual(r.results.get("es").invalid.map((x) => x.key), ["take.scored"]);
  const lock = read(path.join(s.dir, LOCK_FILE)).es;
  assert.ok(!("take.scored" in lock));
  assert.equal(lock["nav.sing"].human, false);
  await assert.rejects(run(base(s, { mode: "adopt" })), /--as machine or --as human/);
});

test("languages.json: unique codes, a level each, and a style guide for every language", async () => {
  const { LANGUAGES } = await import("../translate.mjs");
  const codes = LANGUAGES.map((l) => l.code);
  assert.equal(new Set(codes).size, codes.length);
  assert.ok(codes.includes("es") && codes.includes("es-ES"), "both Spanish variants");
  for (const l of LANGUAGES) {
    assert.ok(["full", "early"].includes(l.level), l.code);
    assert.ok(l.name && l.native, l.code);
    assert.ok(fs.existsSync(path.join(here, "..", "style", `${l.code}.md`)), `style/${l.code}.md`);
  }
});
