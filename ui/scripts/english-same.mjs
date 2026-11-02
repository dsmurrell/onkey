// English stays word for word as it was when the window's text moved into messages/en.json ([languages]).
//
//   npm run english-same [-- [base] [file…]]    (from ui/; base: the commit before the move, default the merge-base
//                                                with develop; files: only these, as ui/-relative paths, for work in
//                                                progress; the unused-key check then covers only their keys)
//
// For every source file (app/, components/, lib/; not tests), the text it had at `base` and has now are compared:
// every JSX text (whitespace collapsed as React does), string literal and template literal part. What a file lost
// is the text that moved; each piece of it must appear, exactly, in a message the file now uses (t("key"),
// t.rich("key"), or a key prefix made at run time: t(`page.lineSub.${kind}`)). And the other way: each run of text
// in a message the file uses must come from text the file had (nothing reworded or made up). Exceptions, each with
// its reason, are in scripts/english-same.allow.json: also `moved` (a file's text that moved, as it was, into another
// source file: a data table) and `newText` (messages whose English is new on purpose, by key prefix, each with the
// test that proves its words: the engine's lines, new Settings text). Exits 1 on any difference.
import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { flatten } from "../lib/i18n.ts";
import { runs } from "../lib/i18nCheck.ts";

const UI = join(dirname(fileURLToPath(import.meta.url)), "..");
const git = (...a) => execFileSync("git", a, { cwd: UI, encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "ignore"] });
const args = process.argv.slice(2);
const base = args[0] && !/\.(ts|tsx)$/.test(args[0]) ? args.shift() : git("merge-base", "HEAD", "develop").trim();
const only = args.length ? new Set(args) : null;
const ROOTS = ["app", "components", "lib"];
const source = f => /\.(ts|tsx)$/.test(f) && !/\.test\./.test(f) && !f.startsWith("lib/i18n");

const filesAt = rev => git("ls-tree", "-r", "--name-only", rev, "--", ...ROOTS).split("\n").filter(source);
const filesNow = () => git("ls-files", "--cached", "--others", "--exclude-standard", "--", ...ROOTS).split("\n").filter(source).filter(f => existsSync(join(UI, f)));
const textAt = (rev, f) => { try { return git("show", `${rev}:ui/${f}`); } catch { return null; } };

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: "\"", apos: "'", nbsp: "\u00a0", rsquo: "’", lsquo: "‘", ldquo: "“", rdquo: "”", hellip: "…", mdash: "—", ndash: "–", rarr: "→", larr: "←", middot: "·", times: "×" };
const decode = s => s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e) =>
  e[0] === "#" ? String.fromCodePoint(e[1] === "x" || e[1] === "X" ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10)) : ENTITIES[e] ?? m);

/** JSX text as React renders it (Babel's cleanJSXElementLiteralChild). */
function jsxText(raw) {
  const lines = raw.split(/\r\n|\n|\r/);
  let last = 0;
  lines.forEach((l, i) => { if (/[^ \t]/.test(l)) last = i; });
  let out = "";
  lines.forEach((line, i) => {
    let s = line.replace(/\t/g, " ");
    if (i !== 0) s = s.replace(/^[ ]+/, "");
    if (i !== lines.length - 1) s = s.replace(/[ ]+$/, "");
    if (s) { if (i !== last) s += " "; out += s; }
  });
  return decode(out);
}

/** A file's text pieces, and the message keys it uses (exact, and prefixes made at run time). */
function scan(name, text) {
  const sf = ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true, name.endsWith("x") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const pieces = [];
  const keys = new Set();
  const prefixes = new Set();
  const visit = n => {
    if (ts.isJsxText(n)) { const s = jsxText(n.text); if (s) pieces.push(s); }
    else if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) {
      if (!ts.isImportDeclaration(n.parent) && !ts.isExportDeclaration(n.parent) && !ts.isExternalModuleReference(n.parent))
        pieces.push(ts.isJsxAttribute(n.parent) ? decode(n.text) : n.text);
    }
    else if (ts.isTemplateHead(n) || ts.isTemplateMiddle(n) || ts.isTemplateTail(n)) pieces.push(n.text);
    if (ts.isCallExpression(n)) {
      const callee = n.expression.getText(sf);
      if (/^(t|t\.rich|t\.markup|t\.raw|t\.has)$/.test(callee) && n.arguments.length) {
        const a = n.arguments[0];
        if (ts.isStringLiteral(a) || ts.isNoSubstitutionTemplateLiteral(a)) keys.add(a.text);
        else if (ts.isTemplateExpression(a)) prefixes.add(a.head.text);
        else prefixes.add("?");
      }
    }
    // keys named in the file otherwise (a table of them: { kind: "page.lineSub.kind" }, `as MessageKey`)
    if ((ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) && /^[a-z][\w]*(\.[\w]+)+$/i.test(n.text)) keys.add(n.text);
    // en.json read directly where t() can't be (the layout's metadata, built on the server): en.app.title
    if (ts.isPropertyAccessExpression(n) && !ts.isPropertyAccessExpression(n.parent) && /^en(\.\w+)+$/.test(n.getText(sf))) keys.add(n.getText(sf).slice(3));
    if (ts.isTemplateExpression(n) && /^[a-z][\w]*(\.[\w]+)*\.$/i.test(n.head.text)) prefixes.add(n.head.text);
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return { pieces, keys, prefixes };
}

const bag = xs => { const m = new Map(); for (const x of xs) m.set(x, (m.get(x) ?? 0) + 1); return m; };
const en = flatten(JSON.parse(readFileSync(join(UI, "messages/en.json"), "utf8")));
const allowPath = join(UI, "scripts/english-same.allow.json");
const allow = existsSync(allowPath) ? JSON.parse(readFileSync(allowPath, "utf8")) : { removed: {}, added: {} };
const allowed = (kind, file, s) => (allow[kind]?.[file] ?? []).some(a => a.text === s) || (allow[kind]?.["*"] ?? []).some(a => a.text === s);
const usedAllow = new Set();

const problems = [];
const used = new Set();
let moved = 0;
const files = new Set([...filesAt(base), ...filesNow()].filter(f => !only || only.has(f)));
for (const f of [...files].sort()) {
  const before = textAt(base, f);
  const after = existsSync(join(UI, f)) ? readFileSync(join(UI, f), "utf8") : null;
  const old = before ? scan(f, before) : { pieces: [], keys: new Set(), prefixes: new Set() };
  const now = after ? scan(f, after) : { pieces: [], keys: new Set(), prefixes: new Set() };
  // the messages this file uses now
  const mine = [...en.keys()].filter(k => now.keys.has(k) || [...now.prefixes].some(p => p !== "?" && k.startsWith(p)));
  mine.forEach(k => used.add(k));
  const texts = mine.map(k => en.get(k));
  // what the file lost must be in them
  const left = bag(now.pieces);
  for (const [s, n] of bag(old.pieces)) {
    const gone = n - (left.get(s) ?? 0);
    if (gone <= 0 || !s.trim()) continue;
    if (/^#[0-9a-f]{3,8}$/i.test(s)) continue;   // a colour, not text (hex now comes from lib/colors.ts)
    moved += gone;
    if (texts.some(m => m.includes(s))) continue;
    if (now.prefixes.has("?") && [...en.values()].some(m => m.includes(s))) continue;   // a key chosen at run time
    if (allowed("removed", f, s)) { usedAllow.add(`removed ${f} ${s}`); continue; }
    const to = (allow.moved ?? []).find(m => m.from === f && existsSync(join(UI, m.to)));
    if (to && scan(to.to, readFileSync(join(UI, to.to), "utf8")).pieces.includes(s)) continue;   // moved as it was
    const elsewhere = [...en.entries()].find(([, m]) => m.includes(s));
    problems.push(`${f}: lost ${JSON.stringify(s)}${elsewhere ? ` (it's in "${elsewhere[0]}", which this file doesn't use)` : " (in no message)"}`);
  }
  // and every run of text in them must come from the file's own text
  if (!mine.length) continue;
  for (const k of mine) for (const r of runs(en.get(k))) {
    const s = r.trim();
    if (!s || old.pieces.some(p => p.includes(s))) continue;
    if (allowed("added", k, s) || allowed("added", f, s)) { usedAllow.add(`added ${k} ${s}`); continue; }
    if ((allow.newText ?? []).some(a => k.startsWith(a.prefix))) continue;
    problems.push(`${f}: "${k}" says ${JSON.stringify(s)}, which ${f} never said`);
  }
}
// messages no file uses (a key spelled wrong, or text left behind)
if (!only) for (const k of en.keys()) if (!used.has(k) && !(allow.unused ?? []).some(a => a.key === k)) problems.push(`messages/en.json: "${k}" isn't used by any file`);

console.log(`English check against ${base.slice(0, 10)}: ${files.size} files, ${moved} pieces of text moved, ${en.size} messages.`);
if (problems.length) {
  console.log(`${problems.length} difference(s):\n  ` + problems.join("\n  "));
  process.exit(1);
}
console.log("Every piece of English is word for word as it was.");
