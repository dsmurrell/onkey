// The site's languages are OnKey's one list (i18n/languages.json), copied to lib/languages.json.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { LANGUAGES, LANG_COOKIE, isEarly, languageName } from "../lib/languages.ts";
import { LANG_COOKIE as FUNCTION_COOKIE } from "../lib/choose-language.ts";

const read = (url) => JSON.parse(readFileSync(new URL(url, import.meta.url), "utf8"));

test("lib/languages.json is i18n/languages.json (after changing the list: cp i18n/languages.json site/lib/languages.json)", () => {
  assert.deepEqual(read("../lib/languages.json"), read("../../i18n/languages.json"));
});

test("every language in the list, by its own name and level, plus English and the pseudo-language", () => {
  const list = read("../../i18n/languages.json").languages;
  assert.deepEqual(Object.keys(LANGUAGES), ["en", ...list.map((l) => l.code), "en-XA"]);
  for (const l of list) {
    assert.equal(languageName(l.code), l.native);
    assert.equal(isEarly(l.code), l.level === "early");
  }
  assert.notEqual(languageName("es"), languageName("es-ES"));
});

test("the switcher's cookie is the one the Pages Function reads", () => {
  assert.equal(LANG_COOKIE, FUNCTION_COOKIE);
});
