// npm test: no text in the window that skips the catalog. Every .tsx in app/ and components/ is read with the
// TypeScript compiler, and these fail: JSX text with a letter in it, and a string with a letter in it that's shown
// as it is: a title/aria-label/placeholder/alt attribute, or a string inside a JSX {…} (also through ?:, && and ||).
// Such text goes in messages/en.json and is shown with t("…"). Exceptions, each with why: untranslated.allow.json
// (dev-only tools, which stay English, and text that isn't words in any language).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const UI = fileURLToPath(new URL("..", import.meta.url));
const allow = JSON.parse(readFileSync(join(UI, "lib/untranslated.allow.json"), "utf8"));
const LETTER = /\p{L}/u;
const ATTRS = new Set(["title", "aria-label", "placeholder", "alt"]);

const walk = dir => readdirSync(dir).flatMap(n => {
  const p = join(dir, n);
  return statSync(p).isDirectory() ? walk(p) : p.endsWith(".tsx") && !/\.test\./.test(n) ? [p] : [];
});

/** The untranslated text in one file: [line, text]. */
export function untranslated(name, text) {
  const sf = ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const out = [];
  const say = (n, s) => out.push(`${sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1}: ${JSON.stringify(s)}`);
  // is this string shown as it is? (its value reaches a JSX child or one of ATTRS, through ?:, &&, ||, ?? and parens)
  const shown = n => {
    let p = n.parent;
    for (;;) {
      if (ts.isParenthesizedExpression(p)) { p = p.parent; continue; }
      if (ts.isConditionalExpression(p) && p.condition !== n) { n = p; p = p.parent; continue; }
      if (ts.isBinaryExpression(p) && p.right === n && [ts.SyntaxKind.AmpersandAmpersandToken, ts.SyntaxKind.BarBarToken, ts.SyntaxKind.QuestionQuestionToken].includes(p.operatorToken.kind)) { n = p; p = p.parent; continue; }
      break;
    }
    if (ts.isJsxAttribute(p)) return ATTRS.has(p.name.getText(sf));
    if (ts.isJsxExpression(p)) return ts.isJsxAttribute(p.parent) ? ATTRS.has(p.parent.name.getText(sf)) : true;
    return false;
  };
  const visit = n => {
    if (ts.isJsxText(n)) { const s = n.text.replace(/\s+/g, " ").trim(); if (LETTER.test(s)) say(n, s); }
    else if ((ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n) || ts.isTemplateExpression(n)) && shown(n)) {
      const s = ts.isTemplateExpression(n) ? n.getText(sf) : n.text;
      if (LETTER.test(ts.isTemplateExpression(n) ? [n.head.text, ...n.templateSpans.map(x => x.literal.text)].join("") : s)) say(n, s);
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}

test("no window text outside the catalog (messages/en.json)", () => {
  const problems = [];
  const used = new Set();
  for (const f of [...walk(join(UI, "app")), ...walk(join(UI, "components"))].sort()) {
    const rel = relative(UI, f);
    if (allow.files.some(a => a.file === rel)) { used.add(rel); continue; }
    for (const p of untranslated(rel, readFileSync(f, "utf8"))) {
      const text = JSON.parse(p.slice(p.indexOf(": ") + 2));
      const a = allow.text.find(a => (a.file === rel || a.file === "*") && a.text === text);
      if (a) { used.add(`${a.file} ${a.text}`); continue; }
      problems.push(`${rel}:${p}`);
    }
  }
  assert.deepEqual(problems, [], "text to move into messages/en.json (or, if it really isn't words, untranslated.allow.json with why)");
  const stale = [...allow.files.map(a => a.file), ...allow.text.map(a => `${a.file} ${a.text}`)].filter(k => !used.has(k));
  assert.deepEqual(stale, [], "allowlist entries that no longer match anything");
});

test("the check finds what it should", () => {
  const src = `const A = () => <p title="Hi" aria-label={x ? "On" : "Off"} className="text-sm">Hello {name}<b>{busy && "Saving…"}</b>{"·"}{t("k")}</p>;`;
  assert.deepEqual(untranslated("x.tsx", src).map(s => s.replace(/^\d+: /, "")), ['"Hi"', '"On"', '"Off"', '"Hello"', '"Saving…"']);
});
