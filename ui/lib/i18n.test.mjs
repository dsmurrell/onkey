// npm test: the window's words (lib/i18n.ts, on use-intl) and the missing-key check over every catalog in messages/.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createTranslator } from "use-intl/core";
import { flatten, getLocale, locales, message, pickLocale, pseudoCatalog, PSEUDO, registerCatalog, setLanguageSetting, setLocale,
  setSystemLanguages, setSystemRegion, systemLanguages, systemRegion, t, tidy, unregisterCatalog } from "./i18n.ts";
import { catalogProblems, paramNames, parse, runs } from "./i18nCheck.ts";
import { TRANSLATIONS } from "./catalogs.ts";

/** One message formatted the way the window does (use-intl), in English. */
const tr = (msg, values) => createTranslator({ locale: "en", messages: { m: msg }, onError: e => { throw e; } })("m", values);
const trRich = (msg, values) => createTranslator({ locale: "en", messages: { m: msg }, onError: e => { throw e; } }).rich("m", values);

const dir = new URL("../messages/", import.meta.url);
const read = name => JSON.parse(readFileSync(new URL(name, dir), "utf8"));
const en = read("en.json");

test("the missing-key check: every translation in messages/ has English's keys and parameters, and no others", () => {
  const others = readdirSync(dir).filter(f => /^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*\.json$/.test(f) && f !== "en.json");   // not CREDITS.json
  for (const f of others) assert.deepEqual(catalogProblems(en, read(f), f.replace(/\.json$/, "")), [], `messages/${f}`);
});

test("the missing-key check fails a translation that lacks a key, has one English doesn't, or changes a parameter", () => {
  const broken = JSON.parse(readFileSync(new URL("test/xx-broken.json", dir), "utf8"));
  const english = { app: { title: "OnKey", note: "Learning {song}…", count: "{n, plural, one {# song} other {# songs}}" } };
  const problems = catalogProblems(english, { ...broken, app: { ...broken.app, count: "{n, plural, other {# 曲}}" } }, "xx");
  assert.deepEqual(problems.sort(), [
    'xx: "app.gone" isn\'t in English any more',
    'xx: "app.title" has {version} where English has {}',
    'xx: missing "app.note"',
  ]);
  // a translation may have fewer plural forms (Chinese has one), but not a different value
  assert.ok(catalogProblems(english, { app: { title: "x", note: "{name}…", count: "{n, plural, other {#}}" } })
    .some(p => p.includes("app.note") && p.includes("{name}")));
  assert.ok(catalogProblems(english, { app: { title: "x", note: "Learning {song", count: "{n, plural, other {#}}" } })
    .some(p => p.includes("isn't well formed")));
  // and against the real English catalog, the partial fixture misses keys
  const partial = read("test/xx.json");
  assert.ok(catalogProblems(en, partial, "xx").some(p => p.startsWith("xx: missing")));
});

test("every English message is well formed", () => {
  for (const [k, m] of flatten(en)) assert.doesNotThrow(() => parse(m), k);
});

test("every English message without values or tags shows exactly as written (no ICU quoting to trip on)", () => {
  for (const [k, m] of flatten(en)) {
    if (paramNames(m).length) continue;
    assert.equal(tr(m), m, k);
  }
});

test("values, plurals, selects and tags", () => {
  assert.equal(tr("Now: {name}.", { name: "MacBook Pro Microphone" }), "Now: MacBook Pro Microphone.");
  // {count} in the plural, not #: the number as the English always wrote it (1234), where # would say "1,234"
  const songs = "{count, plural, =0 {No songs} one {{count} song} other {{count} songs}}";
  assert.deepEqual([0, 1, 2, 1234].map(count => tr(songs, { count })), ["No songs", "1 song", "2 songs", "1234 songs"]);
  assert.equal(tr("{count, plural, other {# songs}}", { count: 1234 }), "1,234 songs");
  assert.equal(tr("{os, select, mac {System Settings} other {Windows Settings}} → Sound", { os: "mac" }), "System Settings → Sound");
  // text is taken as it is: curly quotes, apostrophes, dashes, ellipses, arrows, non-breaking spaces
  const s = "Don't — it’s “fine”… → ok – #1 the app's menu";
  assert.equal(tr(s), s);
  assert.deepEqual(runs(s), [s], "and the checks read it the same way");
  // ICU's quoting: what the checks see is what's shown
  assert.equal(tr("it''s '{x}'"), "it's {x}");
  assert.deepEqual(runs("it''s '{x}'"), ["it's {x}"]);
  assert.deepEqual(paramNames("<b>{a}</b> {n, plural, one {# {b}} other {#}}"), ["<b>", "a", "b", "n"]);
  assert.throws(() => parse("<b>open"));
  assert.throws(() => parse("{n, plural, one {x}}"));
});

test("rich text renders the same markup as the JSX it came from", () => {
  const html = node => renderToStaticMarkup(createElement("p", null, node));
  const jsx = createElement("p", null, "Now: ", createElement("b", null, "Mic"), ". Change it in ", createElement("a", { href: "/settings" }, "Settings"), ".");
  const rich = trRich("Now: <b>{name}</b>. Change it in <link>Settings</link>.", {
    name: "Mic", b: c => createElement("b", null, c), link: c => createElement("a", { href: "/settings" }, c),
  });
  assert.equal(html(rich), renderToStaticMarkup(jsx));
  assert.equal(html(trRich("Just text")), "<p>Just text</p>");
});

test("the language: Settings' choice, else the computer's, else English", () => {
  const have = ["en", "zh", "pt", "es-419"];
  assert.equal(pickLocale("auto", ["zh-CN", "en-GB"], have), "zh", "the computer's (zh-CN → zh)");
  assert.equal(pickLocale(undefined, ["de-DE", "pt-BR"], have), "pt", "the first there is one for");
  assert.equal(pickLocale("auto", ["es-MX"], have), "es-419", "a sibling region");
  assert.equal(pickLocale("es-419", ["zh"], have), "es-419", "the Settings choice first");
  assert.equal(pickLocale("fr", ["zh"], have), "zh", "a choice there's no catalog for: the computer's");
  assert.equal(pickLocale("auto", ["fr", "de"], have), "en", "else English");
  assert.equal(pickLocale("auto", [], have), "en");
  assert.equal(pickLocale(null, null, have), "en");
  // OnKey's own list: both Spanish variants, both Chinese scripts, Brazilian Portuguese as pt
  const ours = ["en", "es", "es-ES", "pt", "zh-Hans", "zh-Hant", "ja"];
  const pick = os => pickLocale("auto", [os], ours);
  assert.equal(pick("es-MX"), "es", "Mexico: Latin American");
  assert.equal(pick("es-419"), "es");
  assert.equal(pick("es-US"), "es");
  assert.equal(pick("es"), "es", "plain Spanish: Latin American");
  assert.equal(pick("es-ES"), "es-ES", "Spain: its own");
  assert.equal(pick("es_ES"), "es-ES", "an underscore");
  assert.equal(pick("zh-TW"), "zh-Hant");
  assert.equal(pick("zh-HK"), "zh-Hant");
  assert.equal(pick("zh-MO"), "zh-Hant");
  assert.equal(pick("zh-Hant-TW"), "zh-Hant", "macOS's form");
  assert.equal(pick("zh-Hant-CN"), "zh-Hant", "the script wins over the region");
  assert.equal(pick("zh-CN"), "zh-Hans");
  assert.equal(pick("zh-SG"), "zh-Hans");
  assert.equal(pick("zh-Hans-CN"), "zh-Hans");
  assert.equal(pick("zh-Hans-HK"), "zh-Hans");
  assert.equal(pick("zh"), "zh-Hans");
  assert.equal(pickLocale("auto", ["zh-TW"], ["en", "zh-Hans"]), "en", "never the other script");
  assert.equal(pick("pt-PT"), "pt", "Portugal: the Brazilian catalog");
  assert.equal(pick("pt-BR"), "pt");
  assert.equal(pick("ja-JP"), "ja");
  assert.equal(pick("sv-SE"), "en", "a language there's none for: English");
  assert.equal(pick("tlh"), "en");
  assert.equal(pickLocale("auto", ["sv-SE", "es-AR", "en-GB"], ours), "es", "the first one there is");
  assert.equal(pickLocale("es-ES", ["es-MX"], ours), "es-ES", "Settings' choice over the computer's");
  assert.equal(pickLocale("auto", ["es-MX"], ["en", "es-ES"]), "es-ES", "only one Spanish: that one");
  assert.deepEqual(locales(), ["en", ...Object.keys(TRANSLATIONS)], "English, then each translation in messages/");
  if (!Object.keys(TRANSLATIONS).length) assert.equal(pickLocale("auto", ["zh-CN"]), "en", "English alone: every way comes to English");
});

test("another catalog, once added, is shown and falls back to English for what it lacks", () => {
  const title = message("app.title", "en");
  try {
    registerCatalog("xx", read("test/xx.json"));
    setLanguageSetting("xx");
    assert.equal(getLocale(), "xx");
    assert.equal(t("app.title"), "OnKey (xx)");
    assert.equal(t("app.description"), message("app.description", "en"), "a key it lacks: English");
    setLanguageSetting("auto");
    assert.equal(getLocale(), "en", "auto here (no browser): English");
    setLocale("nope");
    assert.equal(getLocale(), "en");
  } finally {
    unregisterCatalog("xx");
    setLanguageSetting("auto");
  }
  assert.equal(t("app.title"), title);
});

test("the computer's region: the engine's (ui_region) once given, else the region of its first language", () => {
  try {
    setSystemLanguages(["es-MX", "en-GB"]);
    assert.equal(systemRegion(), "mx");
    setSystemLanguages(["zh-Hant-TW"]);
    assert.equal(systemRegion(), "tw", "a script tag between");
    setSystemLanguages(["de"]);
    assert.equal(systemRegion(), null, "no region in the language");
    setSystemRegion("BR");
    assert.equal(systemRegion(), "br", "the engine's, lower case, over the language's");
    setSystemRegion(null);
    assert.equal(systemRegion(), null);
  } finally {
    setSystemRegion(null);
    setSystemLanguages(null);
  }
});

test("the computer's languages come from the engine (ui_locale) once given, not the browser's", () => {
  try {
    registerCatalog("es-ES", read("test/xx.json"));
    registerCatalog("es", read("test/xx.json"));
    setLanguageSetting("auto");
    setSystemLanguages(["es-ES", "en-GB"]);
    assert.deepEqual(systemLanguages(), ["es-ES", "en-GB"]);
    assert.equal(getLocale(), "es-ES", "auto: the engine's first");
    setLanguageSetting("es");
    assert.equal(getLocale(), "es", "Settings' choice first");
    setLanguageSetting("auto");
    setSystemLanguages(["sv-SE"]);
    assert.equal(getLocale(), "en");
  } finally {
    setSystemLanguages(null);
    unregisterCatalog("es-ES");
    unregisterCatalog("es");
    setLanguageSetting("auto");
  }
});

test("the pseudo-language keeps every key, value and tag, and changes the words", () => {
  const pseudo = pseudoCatalog(en);
  assert.deepEqual(catalogProblems(en, pseudo, PSEUDO), []);
  assert.equal(flatten(pseudo).get("settings.language.title"), "⟦Láñgúágé⟧");
  try {
    registerCatalog(PSEUDO, pseudo);
    setLanguageSetting(PSEUDO);
    assert.equal(t("engine.line.stopped", { song: "Yellow" }), "⟦Ñót léárñíñg Yellow⟧");
  } finally {
    unregisterCatalog(PSEUDO);
    setLanguageSetting("auto");
  }
});

test("Chinese: a space between two Han characters is taken out (a Chinese fragment where a brand name was expected), Latin keeps its spaces", () => {
  const zh = { starting: { yourApp: "你的音乐应用", theSong: "这首歌", going: { title: "正在 {app} 中开始“{song}”…" } },
               report: { savedPc: "已保存到{folder}，文件名 <file>{name}</file>，并在{files}中显示。" },
               controlPrompt: { yourBrowser: "浏览器", openIn: "在 {app} 中打开" } };
  try {
    registerCatalog("zh-Hans", zh);
    setLanguageSetting("zh-Hans");
    assert.equal(getLocale(), "zh-Hans");
    assert.equal(t("controlPrompt.openIn", { app: "Spotify" }), "在 Spotify 中打开", "a brand name: the spaces stay");
    assert.equal(t("controlPrompt.openIn", { app: t("controlPrompt.yourBrowser") }), "在浏览器中打开", "the fallback: no stray spaces");
    assert.equal(t("starting.going.title", { app: t("starting.yourApp"), song: t("starting.theSong") }), "正在你的音乐应用中开始“这首歌”…");
    assert.equal(t("starting.going.title", { app: "Apple Music", song: "晴天" }), "正在 Apple Music 中开始“晴天”…");
    const rich = t.rich("report.savedPc", { folder: "文件夹“Bureau”", name: "r.zip", files: "文件 资源管理器", file: c => createElement("b", null, c) });
    assert.equal(renderToStaticMarkup(createElement("p", null, rich)), "<p>已保存到文件夹“Bureau”，文件名 <b>r.zip</b>，并在文件资源管理器中显示。</p>");
  } finally {
    unregisterCatalog("zh-Hans");
    setLanguageSetting("auto");
  }
  assert.equal(tidy("en", "a b"), "a b");
  assert.equal(tidy("zh-Hant", "在 瀏覽器 中打開 Spotify 的歌"), "在瀏覽器中打開 Spotify 的歌");
  assert.equal(tidy("ja", "東京 大阪"), "東京 大阪", "Japanese is left alone");
});
