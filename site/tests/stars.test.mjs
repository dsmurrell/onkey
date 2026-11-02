// The GitHub star count's label: `common.stars` is an ICU plural in every language, and formats for the counts
// that tell the plural forms apart (Polish and Russian have few/many).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { catalogLocales } from "../lib/catalogs.mjs";
import { starsLabel } from "../lib/format.ts";

const stars = (l) => JSON.parse(readFileSync(new URL(`../messages/${l}.json`, import.meta.url), "utf8")).common.stars;

test("English: one star, many stars", () => {
  assert.equal(starsLabel("en", stars("en"), 1), "1 star on GitHub");
  assert.equal(starsLabel("en", stars("en"), 2), "2 stars on GitHub");
  assert.equal(starsLabel("en", stars("en"), 1234), "1,234 stars on GitHub");
});

test("Polish and Russian: one, few, many", () => {
  assert.equal(starsLabel("pl", stars("pl"), 1), "1 gwiazdka na GitHub");
  assert.equal(starsLabel("pl", stars("pl"), 3), "3 gwiazdki na GitHub");
  assert.equal(starsLabel("pl", stars("pl"), 5), "5 gwiazdek na GitHub");
  assert.equal(starsLabel("pl", stars("pl"), 22), "22 gwiazdki na GitHub");
  assert.equal(starsLabel("ru", stars("ru"), 1), "1 звезда на GitHub");
  assert.equal(starsLabel("ru", stars("ru"), 2), "2 звезды на GitHub");
  assert.equal(starsLabel("ru", stars("ru"), 5), "5 звезд на GitHub");
  assert.equal(starsLabel("ru", stars("ru"), 21), "21 звезда на GitHub");
});

test("every language formats 1, 2, 5 and 22 without error, with the number in the text", () => {
  for (const l of ["en", ...catalogLocales(false)]) {
    for (const n of [1, 2, 5, 22]) assert.match(starsLabel(l, stars(l), n), new RegExp(String(n)), `${l} ${n}`);
  }
});
