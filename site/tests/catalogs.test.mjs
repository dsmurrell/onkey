// The catalogs: every language has every English key, with the same arguments and tags; the build's language
// list is current; English messages parse.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createTranslator } from "use-intl/core";
import { fileURLToPath } from "node:url";
import { catalogLocales, checkCatalog, flatten, merge, pseudoMessage, pseudoMessages } from "../lib/catalogs.mjs";
import { ENTRIES, RELEASES, slug } from "../lib/changelog.ts";

const load = (l, dir = "") => JSON.parse(readFileSync(new URL(`../messages/${dir}${l}.json`, import.meta.url), "utf8"));
const en = load("en");
const changelogDir = fileURLToPath(new URL("../messages/changelog/", import.meta.url));
const changelogEn = load("en", "changelog/");

// A key a catalog doesn't have yet is shown in English (lib/i18n.tsx messagesFor), so it's noted, not a failure:
// new English keys land before their translations. `node i18n/translate.mjs site --check` is the strict check.
function sameKeys(english, dir, locales) {
  for (const l of locales) {
    const problems = checkCatalog(english, load(l, dir));
    const missing = problems.filter((p) => p.startsWith("missing "));
    if (missing.length) console.log(`messages/${dir}${l}.json: ${missing.length} keys not translated yet (shown in English)`);
    assert.deepEqual(problems.filter((p) => !p.startsWith("missing ")), [], `messages/${dir}${l}.json`);
  }
}

test("every catalog's keys are English's, with the same arguments and tags", () => {
  sameKeys(en, "", catalogLocales(false));
});

test("the changelog's catalogs too (messages/changelog/)", () => {
  sameKeys(changelogEn, "changelog/", catalogLocales(false, changelogDir));
});

test("the changelog's entries and releases have their words, and every word has its entry", () => {
  const ids = ENTRIES.map((e) => e.id);
  assert.deepEqual([...new Set(ids)], ids, "an id is used twice");
  for (const e of ENTRIES) {
    assert.equal(slug(e.id), e.id, `${e.id}: an id is lower case, words joined with dashes`);
    const words = changelogEn.entries[e.id];
    assert.ok(words?.title && words?.summary, `entries.${e.id}.title and .summary in messages/changelog/en.json`);
    const points = Object.keys(words).filter((k) => k.startsWith("p"));
    assert.deepEqual(points, points.map((_, i) => `p${i + 1}`), `entries.${e.id}: points are p1, p2, … in order`);
  }
  assert.deepEqual(Object.keys(changelogEn.entries).sort(), [...ids].sort(), "every entry in en.json is in ENTRIES");
  for (const r of RELEASES) {
    const words = changelogEn.releases[slug(r.version)];
    assert.ok(words?.note, `releases.${slug(r.version)}.note`);
    assert.equal(Boolean(words?.link), Boolean(r.link), `releases.${slug(r.version)}.link goes with the release's link`);
  }
  assert.deepEqual(Object.keys(changelogEn.releases).sort(), RELEASES.map((r) => slug(r.version)).sort());
});

test("the check names a missing key, an extra key, and a lost argument or tag", () => {
  const broken = structuredClone(en);
  delete broken.header.docs;
  broken.header.extra = "Extra";
  broken.home.how.step = "Paso";                        // lost {n}
  broken.footer.madeBy = "Hecho por Daniel Murrell";    // lost <link>
  assert.deepEqual(checkCatalog(en, broken).sort(), [
    "arguments or tags differ: footer.madeBy",
    "arguments or tags differ: home.how.step",
    "missing header.docs",
    "not in English: header.extra",
  ]);
});

test("the pseudo-language keeps every argument and tag, and changes every message", () => {
  const pseudo = pseudoMessages(en);
  assert.deepEqual(checkCatalog(en, pseudo), []);
  assert.equal(pseudoMessage("Step {n}"), "⟦Štép {n}⟧");
  assert.equal(pseudoMessage("Made by <link>Daniel</link>."), "⟦Mádé bý <link>Dáñíél</link>.⟧");
});

test("a catalog missing a key falls back to English", () => {
  const m = merge(en, { header: { docs: "Documentos" } });
  assert.equal(m.header.docs, "Documentos");
  assert.equal(m.header.faq, en.header.faq);
});

test("lib/shipped-languages.json lists the catalogs (npm run build rewrites it: commit it)", () => {
  const shipped = JSON.parse(readFileSync(new URL("../lib/shipped-languages.json", import.meta.url), "utf8"));
  assert.deepEqual(shipped, catalogLocales(false));
});

test("every English message parses and formats", () => {
  const values = { n: 1, language: "x", names: "x", requirements: "x", title: "x", dataDir: "x", address: "x", total: 1, p: 1, dir: "flat", s: 1, version: "x" };
  const tag = (c) => c;
  for (const messages of [en, changelogEn]) {
    const t = createTranslator({ locale: "en", messages, timeZone: "UTC", onError: (e) => { throw e; } });
    for (const [k, v] of Object.entries(flatten(messages))) {
      const tags = Object.fromEntries([...v.matchAll(/<(\w+)>/g)].map((m) => [m[1], tag]));
      assert.doesNotThrow(() => t.rich(k, { ...values, ...tags }), k);
    }
  }
});
