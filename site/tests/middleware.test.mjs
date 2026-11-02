// The Pages Function itself (functions/_middleware.ts), run in Node with its imports compiled by TypeScript and
// Cloudflare's context stubbed: { request, next }, as in
// https://developers.cloudflare.com/pages/functions/middleware/. Workers' Request/Response are the same as Node's.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

/** The function and the modules it imports, as plain JavaScript, with the language list set to `shipped`. */
async function load(shipped) {
  const dir = mkdtempSync(join(tmpdir(), "onkey-mw-"));
  mkdirSync(join(dir, "functions")); mkdirSync(join(dir, "lib"));
  const compile = (from, to) => {
    const src = readFileSync(new URL(`../${from}`, import.meta.url), "utf8");
    const js = ts.transpileModule(src, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
      .replace(/from "(\.\.\/lib\/[\w-]+)"/g, 'from "$1.mjs"')
      .replace(/import shipped from "\.\.\/lib\/shipped-languages\.json";/, `const shipped = ${JSON.stringify(shipped)};`);
    writeFileSync(join(dir, to), js);
  };
  compile("functions/_middleware.ts", "functions/_middleware.mjs");
  compile("lib/choose-language.ts", "lib/choose-language.mjs");
  return import(join(dir, "functions/_middleware.mjs"));
}

const page = () => new Response("<html>English</html>", { status: 200, headers: { "Content-Type": "text/html" } });
const call = (mw, path, headers = {}) => {
  let nexted = false;
  const next = async () => { nexted = true; return page(); };
  return mw.onRequest({ request: new Request(`https://singonkey.app${path}`, { headers }), next }).then((r) => ({ r, nexted }));
};

test("/ in Spanish: a 302 to /es/, keeping the query, not cached for others", async () => {
  const mw = await load(["es"]);
  const { r, nexted } = await call(mw, "/?ref=hn", { "Accept-Language": "es-ES,es;q=0.9" });
  assert.equal(r.status, 302);
  assert.equal(r.headers.get("Location"), "/es/?ref=hn");
  assert.match(r.headers.get("Vary"), /Accept-Language/);
  assert.equal(r.headers.get("Cache-Control"), "private, no-store");
  assert.equal(nexted, false);
});

test("/ in English, or with the cookie set to English: the static page, with Vary", async () => {
  const mw = await load(["es"]);
  for (const headers of [{ "Accept-Language": "en-US" }, { "Accept-Language": "es", Cookie: "lang=en" }, {}]) {
    const { r, nexted } = await call(mw, "/", headers);
    assert.equal(r.status, 200);
    assert.equal(nexted, true);
    assert.equal(await r.text(), "<html>English</html>");
    assert.match(r.headers.get("Vary"), /Accept-Language, Cookie/);
  }
});

test("any other path passes straight through, untouched", async () => {
  const mw = await load(["es"]);
  const { r, nexted } = await call(mw, "/docs/faq/", { "Accept-Language": "es", Cookie: "lang=es" });
  assert.equal(r.status, 200);
  assert.equal(nexted, true);
  assert.equal(r.headers.get("Vary"), null);
});

test("with no other language built, nobody is redirected", async () => {
  const mw = await load([]);
  const { r } = await call(mw, "/", { "Accept-Language": "es" });
  assert.equal(r.status, 200);
});
