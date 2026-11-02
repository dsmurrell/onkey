import { test } from "node:test";
import assert from "node:assert/strict";
import { compare, parse, parserName } from "../icu.mjs";

// Run each check with the formatjs parser (when installed) and with the built-in fallback.
const parsers = [["default (" + parserName + ")", "default"], ["built-in", "builtin"]];

for (const [label, which] of parsers) {
  const check = async (en, tr, lang) => {
    if (which === "builtin") process.env.ONKEY_I18N_PARSER = "builtin";
    // icu.mjs decides its parser at import time, so load a fresh copy for the built-in run.
    const mod = which === "builtin" ? await import(`../icu.mjs?builtin=${Math.random()}`) : { compare };
    delete process.env.ONKEY_I18N_PARSER;
    return mod.compare(en, tr, lang);
  };

  test(`[${label}] a faithful translation passes`, async () => {
    assert.deepEqual(await check("You scored <b>{score}</b> on {song}", "Has sacado <b>{score}</b> en {song}", "es"), []);
    assert.deepEqual(await check("{count, plural, =0 {No songs} one {# song} other {# songs}}", "{count, plural, =0 {Нет песен} one {# песня} few {# песни} many {# песен} other {# песни}}", "ru"), []);
    assert.deepEqual(await check("{count, plural, one {# song} other {# songs}}", "{count, plural, other {#曲}}", "ja"), []);
    assert.deepEqual(await check("It’s a song: learn it", "C’est une chanson : apprends-la", "fr"), []);
  });

  test(`[${label}] rejects a lost {name}`, async () => {
    const p = await check("Listening to {player}.", "Escuchando.", "es");
    assert.ok(p.some((x) => x.includes("lost {player}")), p.join("; "));
  });

  test(`[${label}] rejects a renamed {name}`, async () => {
    const p = await check("Listening to {player}.", "Escuchando {reproductor}.", "es");
    assert.ok(p.some((x) => x.includes("lost {player}")));
    assert.ok(p.some((x) => x.includes("added {reproductor}")));
  });

  test(`[${label}] rejects a lost <b>`, async () => {
    const p = await check("You scored <b>{score}</b>", "Has sacado {score}", "es");
    assert.ok(p.some((x) => x.includes("lost <b>")), p.join("; "));
  });

  test(`[${label}] rejects a broken plural`, async () => {
    // syntax: unclosed option
    assert.ok((await check("{n, plural, one {# song} other {# songs}}", "{n, plural, one {# canción} other {# canciones}", "es")).length);
    // no "other"
    assert.ok((await check("{n, plural, one {# song} other {# songs}}", "{n, plural, one {# canción}}", "es")).length);
    // plural turned into a plain argument
    const p = await check("{n, plural, one {# song} other {# songs}}", "{n} canciones", "es");
    assert.ok(p.some((x) => x.includes("{n} is argument but English has plural")), p.join("; "));
    // Russian needs few and many
    const r = await check("{n, plural, one {# song} other {# songs}}", "{n, plural, one {# песня} other {# песни}}", "ru");
    assert.ok(r.some((x) => x.includes('"few"')) && r.some((x) => x.includes('"many"')), r.join("; "));
    // a category the language doesn't have
    const j = await check("{n, plural, one {# song} other {# songs}}", "{n, plural, one {#曲} other {#曲}}", "ja");
    assert.ok(j.some((x) => x.includes('"one"')), j.join("; "));
    // dropped the number
    const d = await check("{n, plural, one {# song} other {# songs}}", "{n, plural, one {una canción} other {canciones}}", "es");
    assert.ok(d.some((x) => x.includes("dropped the #")), d.join("; "));
    // lost the =0 case
    const z = await check("{n, plural, =0 {None} one {# song} other {# songs}}", "{n, plural, one {# canción} other {# canciones}}", "es");
    assert.ok(z.some((x) => x.includes("=0")), z.join("; "));
  });

  test(`[${label}] select keys must stay as in English`, async () => {
    const p = await check("{who, select, me {Your take} other {Their take}}", "{who, select, yo {Tu toma} other {Su toma}}", "es");
    assert.ok(p.some((x) => x.includes("select")), p.join("; "));
  });

  test(`[${label}] Spanish may leave out "many" (only for millions)`, async () => {
    assert.deepEqual(await check("{n, plural, one {# song} other {# songs}}", "{n, plural, one {# canción} other {# canciones}}", "es"), []);
  });
}

test("parse throws on unbalanced tags", () => {
  assert.throws(() => parse("<b>hi"));
});
