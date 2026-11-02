// ICU MessageFormat checks for OnKey's catalogs.
//
// parse() uses @formatjs/icu-messageformat-parser (the parser use-intl itself runs, via
// intl-messageformat) when it can be found: i18n/node_modules first, then ui/node_modules.
// Without it, a small built-in parser handles the subset OnKey uses: {arg}, {n, number},
// {d, date}, {t, time}, plural / selectordinal / select, #, rich-text tags and apostrophe
// quoting. Both produce the same AST shape (formatjs's TYPE numbers), so compare() is shared.

import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const TYPE = { literal: 0, argument: 1, number: 2, date: 3, time: 4, select: 5, plural: 6, pound: 7, tag: 8 };

const here = path.dirname(fileURLToPath(import.meta.url));

function loadFormatjs() {
  if (process.env.ONKEY_I18N_PARSER === "builtin") return null;
  for (const base of [here, path.join(here, "..", "ui")]) {
    try {
      const req = createRequire(path.join(base, "package.json"));
      const mod = req("@formatjs/icu-messageformat-parser");
      if (typeof mod.parse === "function") return mod;
    } catch { /* try the next place */ }
  }
  return null;
}

const formatjs = loadFormatjs();
export const parserName = formatjs ? "@formatjs/icu-messageformat-parser" : "built-in";

/** Parse an ICU message. Throws an Error with a readable message when it isn't valid. */
export function parse(msg, { builtin = false } = {}) {
  if (typeof msg !== "string") throw new Error(`not a string (${typeof msg})`);
  if (formatjs && !builtin) {
    try {
      return formatjs.parse(msg, { requiresOtherClause: true });
    } catch (e) {
      throw new Error(`ICU syntax: ${e.message}`);
    }
  }
  return builtinParse(msg);
}

// ---------------------------------------------------------------------------------------------
// Built-in fallback parser.

function builtinParse(src) {
  let i = 0;
  const fail = (why) => { throw new Error(`ICU syntax: ${why} at offset ${i}`); };

  function message(depth, inPlural, closeTag) {
    const out = [];
    let text = "";
    const flush = () => { if (text) { out.push({ type: TYPE.literal, value: text }); text = ""; } };
    while (i < src.length) {
      const c = src[i];
      if (c === "'") {
        // '' is a literal apostrophe; 'x… quotes up to the next lone ' when x is special.
        if (src[i + 1] === "'") { text += "'"; i += 2; continue; }
        const nx = src[i + 1];
        if (nx === "{" || nx === "}" || nx === "<" || (inPlural && nx === "#")) {
          i++;
          while (i < src.length) {
            if (src[i] === "'" && src[i + 1] === "'") { text += "'"; i += 2; continue; }
            if (src[i] === "'") { i++; break; }
            text += src[i++];
          }
          continue;
        }
        text += c; i++; continue;
      }
      if (c === "{") { flush(); out.push(argument(depth, inPlural)); continue; }
      if (c === "}") { if (depth > 0) break; fail("unmatched }"); }
      if (c === "#" && inPlural) { flush(); out.push({ type: TYPE.pound }); i++; continue; }
      if (c === "<") {
        if (src[i + 1] === "/") {
          const m = /^<\/([A-Za-z][\w-]*)>/.exec(src.slice(i));
          if (!m) fail("bad closing tag");
          if (m[1] !== closeTag) fail(`closing </${m[1]}> without matching opening tag`);
          break;
        }
        const m = /^<([A-Za-z][\w-]*)>/.exec(src.slice(i));
        if (m) {
          flush();
          i += m[0].length;
          const children = message(depth, inPlural, m[1]);
          const close = `</${m[1]}>`;
          if (src.slice(i, i + close.length) !== close) fail(`<${m[1]}> is not closed`);
          i += close.length;
          out.push({ type: TYPE.tag, value: m[1], children });
          continue;
        }
        text += c; i++; continue;
      }
      text += c; i++;
    }
    if (closeTag && !src.startsWith(`</${closeTag}>`, i)) fail(`<${closeTag}> is not closed`);
    flush();
    return out;
  }

  const ws = () => { while (i < src.length && /\s/.test(src[i])) i++; };
  const ident = () => {
    const m = /^[^\s{}#,<>'=]+/u.exec(src.slice(i));
    if (!m) fail("expected a name");
    i += m[0].length;
    return m[0];
  };

  function argument(depth, inPlural) {
    i++; // {
    ws();
    const name = ident();
    ws();
    if (src[i] === "}") { i++; return { type: TYPE.argument, value: name }; }
    if (src[i] !== ",") fail(`expected , or } after {${name}`);
    i++; ws();
    const kind = ident();
    ws();
    if (kind === "number" || kind === "date" || kind === "time") {
      let style = null;
      if (src[i] === ",") {
        i++;
        const end = src.indexOf("}", i);
        if (end < 0) fail("unclosed argument");
        style = src.slice(i, end).trim();
        i = end;
      }
      if (src[i] !== "}") fail(`expected } to close {${name}, ${kind}}`);
      i++;
      return { type: TYPE[kind], value: name, style };
    }
    if (kind === "plural" || kind === "selectordinal" || kind === "select") {
      if (src[i] !== ",") fail(`expected , after ${kind}`);
      i++; ws();
      let offset = 0;
      if (kind !== "select" && src.startsWith("offset:", i)) {
        i += 7; ws();
        const m = /^\d+/.exec(src.slice(i));
        if (!m) fail("bad offset");
        offset = Number(m[0]); i += m[0].length; ws();
      }
      const options = {};
      while (i < src.length && src[i] !== "}") {
        let key;
        if (src[i] === "=") { i++; const m = /^\d+(\.\d+)?/.exec(src.slice(i)); if (!m) fail("bad =n selector"); key = "=" + m[0]; i += m[0].length; }
        else key = ident();
        if (key in options) fail(`duplicate option ${key} in {${name}}`);
        ws();
        if (src[i] !== "{") fail(`expected { after option ${key}`);
        i++;
        const value = message(depth + 1, kind !== "select" || inPlural, null);
        if (src[i] !== "}") fail(`option ${key} is not closed`);
        i++;
        options[key] = { value };
        ws();
      }
      if (src[i] !== "}") fail(`{${name}, ${kind}} is not closed`);
      i++;
      if (!("other" in options)) fail(`{${name}, ${kind}} has no "other" option`);
      if (kind === "select") return { type: TYPE.select, value: name, options };
      return { type: TYPE.plural, value: name, options, offset, pluralType: kind === "plural" ? "cardinal" : "ordinal" };
    }
    fail(`unknown argument type "${kind}"`);
  }

  const ast = message(0, false, null);
  if (i < src.length) fail("unexpected text");
  return ast;
}

// ---------------------------------------------------------------------------------------------
// Comparing a translation with its English source.

const KIND = { 1: "argument", 2: "number", 3: "date", 4: "time", 5: "select", 6: "plural" };

/** Everything a translation must keep: arguments (name → kind), tag counts, plural/select args. */
function summarize(ast) {
  const args = new Map(); // name -> kind
  const tags = new Map(); // name -> count
  const plurals = new Map(); // name -> { pluralType, keys:Set, pound:bool (in "other") }
  const selects = new Map(); // name -> Set(keys)
  const problems = [];
  const walk = (nodes) => {
    for (const n of nodes) {
      if (n.type === TYPE.argument || n.type === TYPE.number || n.type === TYPE.date || n.type === TYPE.time) {
        const kind = KIND[n.type];
        if (args.has(n.value) && args.get(n.value) !== kind && args.get(n.value) !== "argument" && kind !== "argument") {
          problems.push(`{${n.value}} is used as both ${args.get(n.value)} and ${kind}`);
        }
        if (!args.has(n.value) || args.get(n.value) === "argument") args.set(n.value, kind);
      } else if (n.type === TYPE.tag) {
        tags.set(n.value, (tags.get(n.value) || 0) + 1);
        walk(n.children);
      } else if (n.type === TYPE.plural) {
        args.set(n.value, "plural");
        const other = n.options.other ? n.options.other.value : [];
        plurals.set(n.value, {
          pluralType: n.pluralType,
          keys: new Set(Object.keys(n.options)),
          pound: hasPound(other),
        });
        for (const o of Object.values(n.options)) walk(o.value);
      } else if (n.type === TYPE.select) {
        args.set(n.value, "select");
        selects.set(n.value, new Set(Object.keys(n.options)));
        for (const o of Object.values(n.options)) walk(o.value);
      }
    }
  };
  walk(ast);
  return { args, tags, plurals, selects, problems };
}

function hasPound(nodes) {
  for (const n of nodes) {
    if (n.type === TYPE.pound) return true;
    if (n.type === TYPE.tag && hasPound(n.children)) return true;
  }
  return false;
}

const pluralCache = new Map();
/** CLDR plural categories a language uses ("cardinal" or "ordinal"). */
export function pluralCategories(lang, type = "cardinal") {
  const k = `${lang}|${type}`;
  if (!pluralCache.has(k)) {
    const order = ["zero", "one", "two", "few", "many", "other"];
    const cats = new Intl.PluralRules(lang, { type }).resolvedOptions().pluralCategories;
    pluralCache.set(k, [...cats].sort((a, b) => order.indexOf(a) - order.indexOf(b)));
  }
  return pluralCache.get(k);
}

/**
 * The categories a translation must write: those whole numbers 0–200 actually reach, plus "other".
 * (Spanish, French, Italian and Portuguese also have "many", but only for numbers like 1 000 000 or
 * compact forms; leaving it out falls back to "other", so it's allowed, not required.)
 */
export function requiredPluralCategories(lang, type = "cardinal") {
  const k = `req|${lang}|${type}`;
  if (!pluralCache.has(k)) {
    const r = new Intl.PluralRules(lang, { type });
    const s = new Set(["other"]);
    for (let n = 0; n <= 200; n++) s.add(r.select(n));
    pluralCache.set(k, pluralCategories(lang, type).filter((c) => s.has(c)));
  }
  return pluralCache.get(k);
}

/**
 * Check a translated message against its English source.
 * Returns a list of problems (empty = fine). `lang` decides which plural categories are required.
 */
export function compare(english, translated, lang) {
  let enAst, trAst;
  try { enAst = parse(english); } catch (e) { return [`the English itself doesn't parse: ${e.message}`]; }
  try { trAst = parse(translated); } catch (e) { return [e.message]; }
  const en = summarize(enAst);
  const tr = summarize(trAst);
  const problems = [...tr.problems];

  for (const [name, kind] of en.args) {
    if (!tr.args.has(name)) problems.push(`lost {${name}}`);
    else if (tr.args.get(name) !== kind) problems.push(`{${name}} is ${tr.args.get(name)} but English has ${kind}`);
  }
  for (const name of tr.args.keys()) if (!en.args.has(name)) problems.push(`added {${name}}, which English doesn't have`);

  for (const [name, count] of en.tags) {
    const got = tr.tags.get(name) || 0;
    if (got === 0) problems.push(`lost <${name}>`);
    else if (got !== count) problems.push(`<${name}> appears ${got}× but ${count}× in English`);
  }
  for (const name of tr.tags.keys()) if (!en.tags.has(name)) problems.push(`added <${name}>, which English doesn't have`);

  for (const [name, p] of en.plurals) {
    const t = tr.plurals.get(name);
    if (!t) continue; // already reported as lost / wrong kind
    if (t.pluralType !== p.pluralType) problems.push(`{${name}} is ${t.pluralType === "ordinal" ? "selectordinal" : "plural"} but English has ${p.pluralType === "ordinal" ? "selectordinal" : "plural"}`);
    const cats = pluralCategories(lang, p.pluralType);
    const exact = [...p.keys].filter((k) => k.startsWith("="));
    for (const k of exact) if (!t.keys.has(k)) problems.push(`{${name}} lost the ${k} case`);
    for (const k of t.keys) {
      if (k.startsWith("=")) { if (!p.keys.has(k)) problems.push(`{${name}} added a ${k} case English doesn't have`); continue; }
      if (!cats.includes(k)) problems.push(`{${name}} has "${k}", which ${lang} doesn't use (it uses: ${cats.join(", ")})`);
    }
    for (const c of requiredPluralCategories(lang, p.pluralType)) if (!t.keys.has(c)) problems.push(`{${name}} is missing the "${c}" form ${lang} needs`);
    if (p.pound && !t.pound) problems.push(`{${name}}'s "other" form dropped the # (the number)`);
  }

  for (const [name, keys] of en.selects) {
    const t = tr.selects.get(name);
    if (!t) continue;
    const want = [...keys].sort().join(", ");
    const got = [...t].sort().join(", ");
    if (want !== got) problems.push(`{${name}, select} has options [${got}] but English has [${want}]`);
  }
  return problems;
}
