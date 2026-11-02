// npm test: the one list of languages (i18n/languages.json, copied to lib/languages.json), Settings' language
// picker appearing once there's a translation, translators' credits (messages/CREDITS.json), lib/catalogs.ts kept
// current, and the songs to try per language.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { locales, registerCatalog, unregisterCatalog, flatten, pseudoCatalog, PSEUDO } from "./i18n.ts";
import { LANGUAGES, REGION_LANGUAGE, creditsProblems, isEarly, nativeName, pickerCodes, regionLanguage, translationUrl, welcomeOrder } from "./languages.ts";
import { STARTERS, startersFor } from "./starterSongs.ts";
import { catalogsSource, translationCodes } from "../scripts/catalogs.mjs";
import { TRANSLATIONS } from "./catalogs.ts";

const read = url => JSON.parse(readFileSync(new URL(url, import.meta.url), "utf8"));
const en = read("../messages/en.json");

test("lib/languages.json is i18n/languages.json (after changing the list: cp i18n/languages.json ui/lib/languages.json)", () => {
  assert.deepEqual(read("./languages.json"), read("../../i18n/languages.json"));
  const codes = LANGUAGES.map(l => l.code);
  assert.equal(codes[0], "en");
  assert.equal(new Set(codes).size, codes.length);
  assert.ok(codes.includes("es") && codes.includes("es-ES"));
  assert.notEqual(nativeName("es"), nativeName("es-ES"), "the two Spanish variants are told apart");
  assert.ok(isEarly("ja") && !isEarly("de") && !isEarly("en"));
});

test("lib/catalogs.ts lists every translation in messages/ (npm run build rewrites it; node scripts/catalogs.mjs)", () => {
  assert.equal(readFileSync(new URL("./catalogs.ts", import.meta.url), "utf8"), catalogsSource(translationCodes()));
  assert.ok(!translationCodes().includes("CREDITS"));
  assert.match(catalogsSource(["es", "zh-Hans"]), /import m_zh_Hans from "\.\.\/messages\/zh-Hans\.json" with \{ type: "json" \};/);
});

test("the language picker: none with English alone; once a translation exists, it's offered, in the list's order", () => {
  assert.equal(pickerCodes(["en"]), null, "English alone: no picker");
  const before = locales();
  for (const c of before) if (c !== "en") unregisterCatalog(c);   // as if there were none yet
  assert.equal(pickerCodes(locales()), null);
  try {
    registerCatalog("ja", pseudoCatalog(en));
    registerCatalog("de", pseudoCatalog(en));
    assert.deepEqual(pickerCodes(locales()), ["en", "de", "ja"]);
    registerCatalog(PSEUDO, pseudoCatalog(en));
    assert.deepEqual(pickerCodes(locales()), ["en", "de", "ja", PSEUDO], "one not in the list: last");
  } finally {
    for (const c of ["ja", "de", PSEUDO]) unregisterCatalog(c);
    for (const c of before) if (c !== "en") registerCatalog(c, TRANSLATIONS[c]);
  }
  assert.deepEqual(locales(), before);
  assert.equal(translationUrl("zh-Hant"), "https://github.com/dsmurrell/onkey/edit/main/ui/messages/zh-Hant.json");
});

test("CREDITS.json: languages there's a catalog for (not English), GitHub usernames, none twice", () => {
  const codes = translationCodes();
  assert.deepEqual(creditsProblems(read("../messages/CREDITS.json"), codes), []);
  assert.deepEqual(creditsProblems({ es: ["octocat", "hubot"] }, ["es"]), []);
  assert.deepEqual(creditsProblems({ en: ["octocat"] }, ["es"]).length, 1);
  assert.match(creditsProblems({ fr: ["octocat"] }, ["es"])[0], /no messages\/fr\.json/);
  assert.match(creditsProblems({ es: ["octo cat"] }, ["es"])[0], /isn't a GitHub username/);
  assert.match(creditsProblems({ es: ["-octocat"] }, ["es"])[0], /isn't a GitHub username/);
  assert.match(creditsProblems({ es: ["octo--cat"] }, ["es"])[0], /isn't a GitHub username/);
  assert.match(creditsProblems({ es: ["a".repeat(40)] }, ["es"])[0], /isn't a GitHub username/);
  assert.match(creditsProblems({ es: ["octocat", "Octocat"] }, ["es"])[0], /listed twice/);
  assert.match(creditsProblems({ es: "octocat" }, ["es"])[0], /must be a list/);
  assert.equal(creditsProblems([], ["es"]).length, 1);
});

test("songs to try: Full languages with a list get theirs (es-ES: es's), others English; every reason is in the catalog", () => {
  assert.equal(startersFor("en").list, "en");
  assert.equal(startersFor("es").list, "es");
  assert.equal(startersFor("es-ES").list, "es");
  for (const c of ["pt", "it", "fr", "de"]) assert.equal(startersFor(c).list, c);
  for (const c of ["ja", "zh-Hans", "ru", "xx", PSEUDO]) assert.equal(startersFor(c).list, "en", c);
  assert.ok(startersFor("en").playlist, "English keeps its playlist");
  for (const c of ["es", "pt", "it", "fr", "de"]) assert.ok(!startersFor(c).playlist && !startersFor(c).playlistApple, `${c}: no playlist yet`);
  const keys = flatten(en);
  for (const [list, { songs }] of Object.entries(STARTERS)) {
    assert.equal(songs.length, 8, list);
    for (const s of songs) assert.ok(keys.has(`starters.why.${list}.${s.why}`), `starters.why.${list}.${s.why}`);
    if (list !== "en") for (const s of songs) assert.match(s.spotify, /^[A-Za-z0-9]{22}$/, `${list} ${s.title}`);
  }
  // and no reason without its song
  for (const k of keys.keys()) if (k.startsWith("starters.why.")) {
    const [, , list, why] = k.split(".");
    assert.ok(STARTERS[list]?.songs.some(s => s.why === why), k);
  }
});

test("a region's language: every entry a language OnKey ships; the regions the plan names; none for a region with no clear one", () => {
  const codes = LANGUAGES.map(l => l.code);
  for (const [region, code] of Object.entries(REGION_LANGUAGE)) {
    assert.match(region, /^[a-z]{2}$/, region);
    assert.ok(codes.includes(code) && code !== "en", `${region} → ${code}`);
  }
  const want = { mx: "es", ar: "es", es: "es-ES", br: "pt", pt: "pt", cn: "zh-Hans", sg: "zh-Hans", tw: "zh-Hant", hk: "zh-Hant",
    jp: "ja", kr: "ko", in: "hi", ru: "ru", tr: "tr", pl: "pl", id: "id", vn: "vi", th: "th", nl: "nl", be: "nl", fr: "fr",
    ch: "de", de: "de", at: "de", it: "it" };
  for (const [region, code] of Object.entries(want)) assert.equal(regionLanguage(region), code, region);
  assert.equal(regionLanguage("MX"), "es", "any case");
  assert.equal(regionLanguage(" br "), "pt", "trimmed");
  for (const r of ["us", "gb", "ca", "au", "za", "", null, undefined, "xx"]) assert.equal(regionLanguage(r), null, String(r));
  assert.equal(regionLanguage("mx", ["en", "pt"]), null, "not shipped: none");
});

test("first-run setup's list: the region's language first when Automatic comes to another, then the list's order", () => {
  const all = LANGUAGES.map(l => l.code);
  // an English Mac set to Mexico: Spanish (Latin America) first, then English and the rest in order
  assert.deepEqual(welcomeOrder(all, "en", "mx").slice(0, 3), ["es", "en", "es-ES"]);
  assert.deepEqual(welcomeOrder(all, "en", "br").slice(0, 3), ["pt", "en", "es"]);
  assert.deepEqual(welcomeOrder(all, "en", "tw").slice(0, 2), ["zh-Hant", "en"]);
  // a Spanish Mac in Mexico: Automatic is already Spanish, so no second Spanish line; the list as it is
  assert.deepEqual(welcomeOrder(all, "es", "mx"), all);
  // a British Mac, a region with no language of its own, or none known: the list as it is
  assert.deepEqual(welcomeOrder(all, "en", "gb"), all);
  assert.deepEqual(welcomeOrder(all, "en", null), all);
  // the order itself: English, then the Full languages, then the Early ones
  const levels = welcomeOrder(all, "en", null).map(c => c === "en" ? "en" : isEarly(c) ? "early" : "full");
  assert.deepEqual(levels, ["en", ...Array(levels.filter(l => l === "full").length).fill("full"), ...Array(levels.filter(l => l === "early").length).fill("early")]);
  // only the languages there are catalogs for
  assert.deepEqual(welcomeOrder(["en", "de", "ja"], "en", "jp"), ["ja", "en", "de"]);
  assert.deepEqual(welcomeOrder(["en", "de"], "en", "jp"), ["en", "de"]);
  // the pseudo-language or a test's fixture: last
  assert.deepEqual(welcomeOrder(["en", PSEUDO, "de"], "en", null), ["en", "de", PSEUDO]);
});
