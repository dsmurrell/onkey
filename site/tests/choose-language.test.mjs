// The Pages Function's choice for / (lib/choose-language.ts, used by functions/_middleware.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { chooseLanguage, matchLanguage, parseAcceptLanguage } from "../lib/choose-language.ts";

const AVAILABLE = ["es", "fr", "pt", "zh-Hans", "zh-Hant"];
const CHROME = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const choose = (visit) => chooseLanguage({ path: "/", userAgent: CHROME, ...visit }, AVAILABLE);

test("Accept-Language: most wanted first, q=0 and * dropped, ties keep their order", () => {
  assert.deepEqual(parseAcceptLanguage("fr-CH, fr;q=0.9, en;q=0.8, de;q=0.7, *;q=0.5"), ["fr-CH", "fr", "en", "de"]);
  assert.deepEqual(parseAcceptLanguage("en;q=0.5, es"), ["es", "en"]);
  assert.deepEqual(parseAcceptLanguage("es;q=0, pt"), ["pt"]);
  assert.deepEqual(parseAcceptLanguage(""), []);
});

test("matching: exact, base language, Chinese by script or region", () => {
  const s = ["en", ...AVAILABLE];
  assert.equal(matchLanguage("es-MX", s), "es");
  assert.equal(matchLanguage("pt-BR", s), "pt");
  assert.equal(matchLanguage("ZH-hant", s), "zh-Hant");
  assert.equal(matchLanguage("zh-TW", s), "zh-Hant");
  assert.equal(matchLanguage("zh-HK", s), "zh-Hant");
  assert.equal(matchLanguage("zh-CN", s), "zh-Hans");
  assert.equal(matchLanguage("zh", s), "zh-Hans");
  assert.equal(matchLanguage("zh-Hans-HK", s), "zh-Hans");
  assert.equal(matchLanguage("de-DE", s), null);
});

test("a browser that prefers a language we have goes there", () => {
  assert.equal(choose({ acceptLanguage: "es-ES,es;q=0.9,en;q=0.8" }), "es");
  assert.equal(choose({ acceptLanguage: "zh-TW,zh;q=0.9" }), "zh-Hant");
  assert.equal(choose({ acceptLanguage: "de-DE,de;q=0.9,fr;q=0.8,en;q=0.7" }), "fr");   // no German yet: the next one we have
});

test("English first, or nothing we have, stays on English", () => {
  assert.equal(choose({ acceptLanguage: "en-US,en;q=0.9,es;q=0.8" }), null);
  assert.equal(choose({ acceptLanguage: "en-GB" }), null);
  assert.equal(choose({ acceptLanguage: "de-DE,de;q=0.9,ja;q=0.8" }), null);
});

test("no Accept-Language (most search engines) or a crawler: English", () => {
  assert.equal(choose({}), null);
  assert.equal(choose({ acceptLanguage: "" }), null);
  for (const ua of [
    "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm) Chrome/116.0.1938.76 Safari/537.36",
    "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
    "Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)",
    "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)",
  ]) assert.equal(chooseLanguage({ path: "/", acceptLanguage: "es", userAgent: ua }, AVAILABLE), null, ua);
});

test("the switcher's cookie wins over the browser", () => {
  assert.equal(choose({ acceptLanguage: "es", cookie: "lang=en" }), null);
  assert.equal(choose({ acceptLanguage: "en-US", cookie: "a=1; lang=fr; b=2" }), "fr");
  assert.equal(choose({ acceptLanguage: "es", cookie: "lang=xx" }), "es");     // a language we don't have: ignored
  assert.equal(choose({ acceptLanguage: "es", cookie: "lang=%E0%A4%A" }), "es"); // unreadable: ignored
});

test("only / is ever redirected", () => {
  for (const path of ["/docs/", "/docs/faq/", "/changelog/", "/es/", "/c/", "/index.html", "/_next/static/x.js"]) {
    assert.equal(chooseLanguage({ path, acceptLanguage: "es", cookie: "lang=fr", userAgent: CHROME }, AVAILABLE), null, path);
  }
});

test("with only English, nobody is redirected", () => {
  assert.equal(chooseLanguage({ path: "/", acceptLanguage: "es", cookie: "lang=es", userAgent: CHROME }, []), null);
});
