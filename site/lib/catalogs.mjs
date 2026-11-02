// Which languages the site is built in: one catalog per language in site/messages/ (en.json is the source).
// Plain JavaScript so next.config.ts, the build's scripts/languages.mjs and the tests share it.
import { readdirSync } from "node:fs";
import { join } from "node:path";

/** The pseudo-language: English with every message's letters accented and bracketed, built from en.json.
 *  Only with SITE_PSEUDO=1, to see on /en-XA/ which text still isn't in the catalog. Never shipped. */
export const PSEUDO = "en-XA";

export const messagesDir = () => join(process.cwd(), "messages");

/** The languages other than English that have a catalog in `dir` (the pages' catalog unless given: the changelog's
 *  is messages/changelog/), sorted, plus the pseudo-language when asked for. */
export function catalogLocales(pseudo = Boolean(process.env.SITE_PSEUDO), dir = messagesDir()) {
  const found = readdirSync(dir)
    .filter((f) => /^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*\.json$/.test(f))
    .map((f) => f.slice(0, -".json".length))
    .filter((l) => l !== "en")
    .sort();
  return pseudo ? [...found, PSEUDO] : found;
}

/** Every message as `{ "a.b.c": text }`. */
export function flatten(messages, prefix = "") {
  const out = {};
  for (const [k, v] of Object.entries(messages)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object") Object.assign(out, flatten(v, key));
    else out[key] = v;
  }
  return out;
}

/** `base` with every message `over` has put in its place (a catalog missing a key falls back to English). */
export function merge(base, over) {
  const out = { ...base };
  for (const [k, v] of Object.entries(over ?? {})) {
    out[k] = v && typeof v === "object" && base[k] && typeof base[k] === "object" ? merge(base[k], v) : v;
  }
  return out;
}

const ACCENT = Object.fromEntries([..."aeiouyAEIOUYcnsCNS"].map((c, i) => [c, "áéíóúýÁÉÍÓÚÝçñšÇÑŠ"[i]]));

/** One message in the pseudo-language: letters accented and the whole bracketed, with ICU arguments ({…}) and
 *  tags (<link>, </link>) left as they are. */
export function pseudoMessage(text) {
  let out = "", depth = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "{") depth++;
    if (depth === 0 && c === "<") {
      const end = text.indexOf(">", i);
      if (end > i) { out += text.slice(i, end + 1); i = end; continue; }
    }
    out += depth === 0 ? (ACCENT[c] ?? c) : c;
    if (c === "}") depth--;
  }
  return `⟦${out}⟧`;
}

/** The pseudo-language's catalog, made from English. */
export function pseudoMessages(messages) {
  return Object.fromEntries(Object.entries(messages).map(([k, v]) => [k, typeof v === "string" ? pseudoMessage(v) : pseudoMessages(v)]));
}

/** A message's ICU arguments ({n}, {language}, the {n} of a plural or select) and tags (<link>), which a
 *  translation must keep. Walks the message: the braces of a plural's or select's options hold text, not
 *  arguments ("{n, plural, one {GitHub पर # स्टार} …}" has the one argument n), and '…' is quoted text. */
export function placeholders(text) {
  const args = new Set(), tags = [];
  let i = 0;
  const skipQuoted = () => { i++; while (i < text.length && text[i] !== "'") i++; i++; };
  const readText = () => {                                   // up to an unmatched "}" (or the end)
    while (i < text.length && text[i] !== "}") {
      if (text[i] === "'" && text[i + 1] && "{}#'<".includes(text[i + 1])) { if (text[i + 1] === "'") i += 2; else skipQuoted(); }
      else if (text[i] === "{") readArg();
      else if (text[i] === "<") { const m = /^<([A-Za-z0-9_]+)>/.exec(text.slice(i)); if (m) tags.push(m[1]); i++; }
      else i++;
    }
  };
  const readArg = () => {                                    // at "{": the argument, through its "}"
    i++;
    const m = /^\s*([A-Za-z0-9_]+)\s*/.exec(text.slice(i));
    if (!m) return;
    args.add(m[1]);
    i += m[0].length;
    if (text[i] === "}") { i++; return; }
    if (text[i] !== ",") return;
    const type = /^,\s*([A-Za-z]+)\s*/.exec(text.slice(i));
    if (!type) return;
    i += type[0].length;
    if (["plural", "select", "selectordinal"].includes(type[1])) {
      if (text[i] === ",") i++;
      while (i < text.length && text[i] !== "}") {              // options: key {text} key {text} … (offset:1 too)
        const key = /^\s*[^\s{}]+\s*/.exec(text.slice(i));
        if (!key) break;
        i += key[0].length;
        if (text[i] === "{") { i++; readText(); i++; }
      }
      i++;
    } else {                                                   // number/date/time with a style: through its "}"
      let depth = 1;
      while (i < text.length && depth) { if (text[i] === "{") depth++; else if (text[i] === "}") depth--; i++; }
    }
  };
  readText();
  return [...args].sort().join(",") + "|" + tags.sort().join(",");
}

/** What's wrong with a catalog against English: missing keys, keys English doesn't have, and messages whose
 *  arguments or tags differ. Empty when it's complete. */
export function checkCatalog(en, catalog) {
  const want = flatten(en), have = flatten(catalog), problems = [];
  for (const k of Object.keys(want)) {
    if (!(k in have)) problems.push(`missing ${k}`);
    else if (typeof have[k] !== "string") problems.push(`not text: ${k}`);
    else if (placeholders(have[k]) !== placeholders(want[k])) problems.push(`arguments or tags differ: ${k}`);
  }
  for (const k of Object.keys(have)) if (!(k in want)) problems.push(`not in English: ${k}`);
  return problems;
}
