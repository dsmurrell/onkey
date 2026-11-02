#!/usr/bin/env node
// Translate one of OnKey's message catalogs (ui, site or native) from en.json into other languages.
//
//   node i18n/translate.mjs ui --langs es,ja            # dry run: what would be sent, and why
//   node i18n/translate.mjs ui --langs full --write     # translate and write es/pt/it/fr/de.json
//   node i18n/translate.mjs site --check                # every language has every key, all valid ICU
//   node i18n/translate.mjs ui --langs es --mark-reviewed --keys sing.title
//   node i18n/translate.mjs ui --langs es --adopt --as machine   # record es.json as written by the tool
//
// Only keys that are missing, or whose English changed since the tool translated them, are sent.
// A translation a person changed is never overwritten: if its English changes later, it is listed
// in needs-review.md instead. The lock file .sources.json (next to en.json) remembers, per language
// and key, the hash of the English it was translated from and of the text the tool wrote.
// See i18n/README.md.

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { compare, pluralCategories, requiredPluralCategories, parserName } from "./icu.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.join(here, "..");

export const LOCK_FILE = ".sources.json";
export const REVIEW_FILE = "needs-review.md";
export const DEFAULT_MODEL = "claude-fable-5-1";
const API_URL = "https://api.anthropic.com/v1/messages";
// Models that accept the server-side refusal fallback ("default" routing).
const FALLBACK_MODELS = new Set(["claude-sonnet-5-5", "claude-opus-5-5", "claude-opus-5", "claude-fable-5-1"]);

export const CATALOGS = {
  ui: { dir: "ui/messages", about: "the OnKey desktop app's window (buttons, settings, status lines, guides)" },
  site: { dir: "site/messages", about: "OnKey's website, singonkey.app (home, download, the docs: how it works, scoring, privacy, FAQ, troubleshooting)" },
  changelog: { dir: "site/messages/changelog", about: "the website's What's new page (singonkey.app/changelog): each release's features and fixes, written for people who sing" },
  native: { dir: "src-tauri/messages", about: "OnKey's macOS/Windows menu items and system permission prompts (very short, platform conventions apply)" },
};

export const LANGUAGES = JSON.parse(fs.readFileSync(path.join(here, "languages.json"), "utf8")).languages;

// ---------------------------------------------------------------------------------------------
// Small helpers.

export const hash = (s) => crypto.createHash("sha256").update(String(s)).digest("hex").slice(0, 16);

/** {a:{b:"x"}} → Map("a.b" → "x"), in file order. */
export function flatten(obj, prefix = "", out = new Map()) {
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) flatten(v, key, out);
    else if (typeof v === "string") out.set(key, v);
    else throw new Error(`${key}: messages must be strings or nested objects, got ${JSON.stringify(v)}`);
  }
  return out;
}

/** Rebuild nesting, ordered like the English file so the two read side by side. */
export function unflatten(values, englishOrder) {
  const out = {};
  for (const key of englishOrder) {
    if (!values.has(key)) continue;
    const parts = key.split(".");
    let node = out;
    for (const p of parts.slice(0, -1)) node = node[p] ??= {};
    node[parts.at(-1)] = values.get(key);
  }
  return out;
}

const readJson = (file, fallback) => {
  if (!fs.existsSync(file)) return fallback;
  return JSON.parse(fs.readFileSync(file, "utf8"));
};
const writeJson = (file, obj) => fs.writeFileSync(file, JSON.stringify(obj, null, 2) + "\n");

function sortObject(obj) {
  return Object.fromEntries(Object.keys(obj).sort().map((k) => [k, obj[k]]));
}

export function resolveCatalog(arg, root = repoRoot) {
  if (CATALOGS[arg]) return { dir: path.join(root, CATALOGS[arg].dir), about: CATALOGS[arg].about, name: arg };
  const dir = path.resolve(arg);
  const name = Object.keys(CATALOGS).find((k) => dir.endsWith(CATALOGS[k].dir)) ?? path.basename(dir);
  return { dir, about: CATALOGS[name]?.about ?? "OnKey's user interface", name };
}

export function resolveLangs(spec) {
  if (!spec || spec === "all") return LANGUAGES.map((l) => l.code);
  if (spec === "full" || spec === "early") return LANGUAGES.filter((l) => l.level === spec).map((l) => l.code);
  const codes = spec.split(",").map((s) => s.trim()).filter(Boolean);
  for (const c of codes) if (!LANGUAGES.some((l) => l.code === c)) throw new Error(`unknown language "${c}" (see i18n/languages.json)`);
  return codes;
}

// ---------------------------------------------------------------------------------------------
// Planning: what each language needs.

/**
 * Work out, for one language, which keys to send and which need a person.
 * Mutates `lockLang` only to record newly detected human edits and adopt untracked translations.
 */
export function plan(english, target, lockLang, { retranslate = null } = {}) {
  const send = []; // { key, reason: "missing" | "changed" | "retranslate", previous? }
  const needsReview = []; // { key, english, current }
  const humanDetected = [];
  const adopted = [];
  let unchanged = 0;

  for (const [key, en] of english) {
    const enHash = hash(en);
    const current = target.get(key);
    let entry = lockLang[key];

    if (current === undefined) { send.push({ key, reason: "missing" }); continue; }
    if (!entry) {
      // A translation the tool didn't write (a contributor's, or one imported from a fork):
      // a person's work, assumed to match today's English.
      entry = lockLang[key] = { en: enHash, out: hash(current), human: true };
      adopted.push(key);
    } else if (!entry.human && hash(current) !== entry.out) {
      entry.human = true;
      humanDetected.push(key);
    }

    if (retranslate?.has(key)) { send.push({ key, reason: "retranslate", previous: current }); continue; }
    if (entry.en === enHash) { unchanged++; continue; }
    if (entry.human) needsReview.push({ key, english: en, current });
    else send.push({ key, reason: "changed", previous: current });
  }

  const removed = [...target.keys()].filter((k) => !english.has(k));
  return { send, needsReview, humanDetected, adopted, removed, unchanged };
}

// ---------------------------------------------------------------------------------------------
// Prompt.

export function systemPrompt({ lang, catalogAbout, glossary, style }) {
  const info = LANGUAGES.find((l) => l.code === lang);
  const cats = (type) => {
    const all = pluralCategories(lang, type), req = requiredPluralCategories(lang, type);
    const opt = all.filter((c) => !req.includes(c));
    return req.join(", ") + (opt.length ? ` (optional, only for very large numbers: ${opt.join(", ")})` : "");
  };
  const card = cats("cardinal");
  const ord = cats("ordinal");
  return `You translate the interface text of OnKey from English into ${info.name} (${info.native}, locale code ${lang}).
OnKey is a free desktop singing trainer: it listens to the song playing in Spotify, Apple Music or YouTube, learns the melody, and shows the notes and lyrics so the person can sing along, then scores how on key they were. These messages are for ${catalogAbout}.

<rules>
- Translate meaning and tone, not word by word. OnKey's voice is friendly, plain and short: everyday words, short sentences, no jargon, no marketing hype. Follow the style guide below for register and punctuation.
- Messages are ICU MessageFormat. Keep every placeholder exactly as written: {name}, {count, number}, {when, date}. Never translate, rename, add or drop a placeholder name.
- Keep plural/select/selectordinal structure. The argument name, the keyword (plural/select/selectordinal) and any =0/=1 exact cases stay as in English.
  - plural: write the CLDR categories ${lang} uses, no others: ${card}. Keep # where English uses it in "other".
  - selectordinal: ${lang} uses: ${ord}.
  - select: keep every option key exactly as in English (they are code values); translate only the text inside.
- Keep rich-text tags exactly: <b>…</b>, <link>…</link>, <kbd>…</kbd> etc. Same tag names, same count; move them to wrap the matching words in the translation.
- Write apostrophes as ’ (typographic), as the English does: a plain ' is ICU's quote character and can hide text. Don't add { } < > characters of your own.
- Keep product and service names as the glossary says (OnKey, Spotify, Apple Music, YouTube, Shazam, LRCLIB, GitHub, Discord, macOS, Windows, Linux).
- Keep keyboard shortcuts (⌘⇧P, Space, Esc) and email addresses or URLs unchanged.
- Length: these strings sit on buttons, tabs and small cards. Stay within about 30% of the English length where the language allows; prefer the shorter natural phrasing.
- Use the punctuation of ${info.name}, not English habits (quote marks, spacing, ellipsis, full stops). Don't add exclamation marks the English doesn't have.
- Keys are given for context (e.g. "settings.lyrics.sub" is the small line under the "Show lyrics" setting). "previous" is the old translation of a string whose English changed: keep its wording where the meaning didn't change.
- If a "problem" is given, your earlier answer for that key broke a rule; fix exactly that.
</rules>

<glossary>
${glossary.trim()}
</glossary>

<style_guide lang="${lang}">
${style.trim()}
</style_guide>

Answer with only a JSON object mapping each key to its translated message string, with no commentary and no code fences.`;
}

export function userMessage(items, english) {
  const obj = {};
  for (const it of items) {
    const e = { en: english.get(it.key) };
    if (it.previous !== undefined) e.previous = it.previous;
    if (it.problem) e.problem = it.problem;
    obj[it.key] = e;
  }
  return `Translate these ${items.length} message${items.length === 1 ? "" : "s"}:\n${JSON.stringify(obj, null, 2)}`;
}

export function batches(items, english, { maxKeys = 40, maxChars = 6000 } = {}) {
  const out = [];
  let cur = [], chars = 0;
  for (const it of items) {
    const len = english.get(it.key).length + (it.previous?.length ?? 0);
    if (cur.length && (cur.length >= maxKeys || chars + len > maxChars)) { out.push(cur); cur = []; chars = 0; }
    cur.push(it); chars += len;
  }
  if (cur.length) out.push(cur);
  return out;
}

/** Pull the JSON object out of the model's reply (tolerates code fences or a stray line). */
export function parseReply(text) {
  const t = text.replace(/^\s*```(?:json)?\s*/i, "").replace(/\s*```\s*$/, "");
  const a = t.indexOf("{"), b = t.lastIndexOf("}");
  if (a < 0 || b < a) throw new Error("the reply had no JSON object");
  const obj = JSON.parse(t.slice(a, b + 1));
  if (!obj || typeof obj !== "object" || Array.isArray(obj)) throw new Error("the reply wasn't a JSON object");
  return obj;
}

// ---------------------------------------------------------------------------------------------
// The API call (plain fetch, injectable for tests).

export async function callClaude({ system, user, fetchImpl, env, sleep, log }) {
  const key = env.ANTHROPIC_API_KEY;
  if (!key) throw new Error("ANTHROPIC_API_KEY is not set");
  const model = env.ONKEY_TRANSLATE_MODEL || DEFAULT_MODEL;
  const headers = { "content-type": "application/json", "x-api-key": key, "anthropic-version": "2023-06-01" };
  const body = {
    model,
    max_tokens: 16000,
    // The system prompt (rules + glossary + style guide) is the same for every batch of a language.
    system: [{ type: "text", text: system, cache_control: { type: "ephemeral" } }],
    messages: [{ role: "user", content: user }],
  };
  if (FALLBACK_MODELS.has(model) && env.ONKEY_TRANSLATE_FALLBACK !== "0") {
    headers["anthropic-beta"] = "server-side-fallback-2026-07-01";
    body.fallbacks = "default";
  }
  const url = (env.ANTHROPIC_BASE_URL ? env.ANTHROPIC_BASE_URL.replace(/\/$/, "") + "/v1/messages" : API_URL);

  let lastErr;
  for (let attempt = 0; attempt < 4; attempt++) {
    let res;
    try {
      res = await fetchImpl(url, { method: "POST", headers, body: JSON.stringify(body) });
    } catch (e) {
      lastErr = new Error(`network: ${e.message}`);
      await sleep(2000 * 2 ** attempt);
      continue;
    }
    if (res.status === 429 || res.status >= 500) {
      const ra = Number(res.headers?.get?.("retry-after"));
      lastErr = new Error(`HTTP ${res.status}`);
      log(`  (API busy, HTTP ${res.status}; retrying)`);
      await sleep(Number.isFinite(ra) && ra > 0 ? ra * 1000 : 2000 * 2 ** attempt);
      continue;
    }
    const data = await res.json();
    if (!res.ok) throw new Error(`HTTP ${res.status}: ${data?.error?.message ?? JSON.stringify(data)}`);
    if (data.stop_reason === "refusal") throw new Error(`the model declined (${data.stop_details?.category ?? "no category"})`);
    if (data.stop_reason === "max_tokens") throw new Error("the reply was cut off (max_tokens); use a smaller --batch");
    const text = (data.content ?? []).filter((b) => b.type === "text").map((b) => b.text).join("");
    return { text, usage: data.usage };
  }
  throw lastErr;
}

// ---------------------------------------------------------------------------------------------
// Translating one language.

async function translateLanguage({ lang, items, english, system, opts }) {
  const { fetchImpl, env, sleep, log, batchSize } = opts;
  const done = new Map();
  const failed = [];

  for (const batch of batches(items, english, { maxKeys: batchSize })) {
    let todo = batch;
    for (let attempt = 0; attempt < 2 && todo.length; attempt++) {
      let reply, requestError = null;
      try {
        const { text } = await callClaude({ system, user: userMessage(todo, english), fetchImpl, env, sleep, log });
        reply = parseReply(text);
      } catch (e) {
        requestError = e.message;
      }
      const retry = [];
      for (const it of todo) {
        let problems;
        if (requestError) problems = [requestError];
        else if (typeof reply[it.key] !== "string") problems = ["no translation was returned for this key"];
        else problems = compare(english.get(it.key), reply[it.key], lang);
        if (!problems.length) done.set(it.key, reply[it.key]);
        else if (attempt === 0) retry.push({ ...it, problem: problems.join("; ") });
        else failed.push({ key: it.key, problems });
      }
      if (retry.length) log(`  ${lang}: ${retry.length} message${retry.length === 1 ? "" : "s"} failed the checks; asking once more`);
      todo = retry;
    }
  }
  return { done, failed };
}

// ---------------------------------------------------------------------------------------------
// needs-review.md

function reviewReport(catalogName, perLang) {
  const sections = [];
  for (const [lang, r] of perLang) {
    if (!r.needsReview.length && !r.failed.length) continue;
    const lines = [`## ${lang}`];
    if (r.needsReview.length) {
      lines.push("", "English changed after a person edited these. Check each still says the same as the English, edit it if not, then run `--mark-reviewed` (or `--retranslate`) for the key.", "");
      for (const n of r.needsReview) lines.push(`- \`${n.key}\``, `  - English now: ${JSON.stringify(n.english)}`, `  - ${lang} now: ${JSON.stringify(n.current)}`);
    }
    if (r.failed.length) {
      lines.push("", "Couldn't be translated (failed the checks twice); still missing:", "");
      for (const f of r.failed) lines.push(`- \`${f.key}\`: ${f.problems.join("; ")}`);
    }
    sections.push(lines.join("\n"));
  }
  if (!sections.length) return null;
  return `# Translations to review: ${catalogName}\n\nWritten by \`node i18n/translate.mjs ${catalogName} --write\`. Regenerated on every run; don't edit.\n\n${sections.join("\n\n")}\n`;
}

// ---------------------------------------------------------------------------------------------
// Main entry (used by the CLI and the tests).

export async function run(options) {
  const opts = {
    mode: "dry-run",
    langs: LANGUAGES.map((l) => l.code),
    i18nDir: here,
    fetchImpl: globalThis.fetch,
    env: process.env,
    sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
    log: console.log,
    batchSize: 40,
    concurrency: 4,
    keys: null,
    retranslate: false,
    showPrompt: false,
    ...options,
  };
  const { dir } = opts.catalog;
  const enFile = path.join(dir, "en.json");
  if (!fs.existsSync(enFile)) throw new Error(`${enFile} doesn't exist`);
  const english = flatten(readJson(enFile));
  const englishOrder = [...english.keys()];
  const lockFile = path.join(dir, LOCK_FILE);
  const lock = readJson(lockFile, {});
  const glossary = fs.readFileSync(path.join(opts.i18nDir, "glossary.md"), "utf8");
  const keyFilter = opts.keys ? new Set(opts.keys) : null;
  const results = new Map();

  if (opts.retranslate && !keyFilter) throw new Error("--retranslate needs --keys");
  if (opts.mode === "adopt" && !["machine", "human"].includes(opts.as)) throw new Error("--adopt needs --as machine or --as human");

  // --check: every language file complete and valid. No lock changes, no API.
  if (opts.mode === "check") {
    let ok = true;
    for (const lang of opts.langs) {
      const file = path.join(dir, `${lang}.json`);
      const target = flatten(readJson(file, {}));
      const missing = englishOrder.filter((k) => !target.has(k));
      const extra = [...target.keys()].filter((k) => !english.has(k));
      const invalid = [];
      for (const [k, v] of target) if (english.has(k)) { const p = compare(english.get(k), v, lang); if (p.length) invalid.push({ key: k, problems: p }); }
      results.set(lang, { missing, extra, invalid });
      if (missing.length || extra.length || invalid.length) {
        ok = false;
        opts.log(`${lang}: ${missing.length} missing, ${extra.length} not in English, ${invalid.length} invalid`);
        for (const k of missing.slice(0, 10)) opts.log(`  missing ${k}`);
        for (const k of extra.slice(0, 10)) opts.log(`  extra ${k}`);
        for (const x of invalid.slice(0, 10)) opts.log(`  ${x.key}: ${x.problems.join("; ")}`);
      } else opts.log(`${lang}: complete (${target.size} keys)`);
    }
    return { ok, results };
  }

  const work = [];
  for (const lang of opts.langs) {
    const file = path.join(dir, `${lang}.json`);
    const target = flatten(readJson(file, {}));
    const lockLang = (lock[lang] ??= {});
    if (opts.mode === "adopt") {
      // Record what's in <lang>.json now as written by the tool ("machine": later runs may refresh it when its
      // English changes) or by a person ("human": never overwritten). For translations written outside the
      // tool, e.g. by an agent editing <lang>.json. Each value must pass the same ICU checks as the tool's own.
      // "machine" takes keys with no lock entry (just written) and the tool's own keys, but never a person's (a
      // lock entry marked human, or a tool translation of today's English that was edited since: someone's fix)
      // unless --keys names it. Run it right after writing, before taking anyone else's edits.
      const adoptedNow = [], skipped = [], invalid = [];
      for (const k of Object.keys(lockLang)) if (!english.has(k)) delete lockLang[k];
      for (const [key, current] of target) {
        if (!english.has(key) || (keyFilter && !keyFilter.has(key))) continue;
        const entry = lockLang[key];
        const persons = entry && (entry.human || (entry.en === hash(english.get(key)) && hash(current) !== entry.out));
        if (opts.as === "machine" && persons && !keyFilter) { skipped.push(key); continue; }
        const problems = compare(english.get(key), current, lang);
        if (problems.length) { invalid.push({ key, problems }); continue; }
        lockLang[key] = { en: hash(english.get(key)), out: hash(current), human: opts.as === "human" };
        adoptedNow.push(key);
      }
      opts.log(`${lang}: recorded ${adoptedNow.length} as ${opts.as === "human" ? "a person's" : "the tool's"}${skipped.length ? `, ${skipped.length} kept as a person's (name them with --keys to change)` : ""}${invalid.length ? `, ${invalid.length} invalid (not recorded)` : ""}`);
      for (const x of invalid.slice(0, 10)) opts.log(`  ${x.key}: ${x.problems.join("; ")}`);
      results.set(lang, { ...plan(english, target, structuredClone(lockLang)), adoptedNow, skipped, failed: [], invalid, written: [] });
      continue;
    }

    const p = plan(english, target, lockLang, { retranslate: opts.retranslate ? keyFilter : null });
    if (keyFilter) p.send = p.send.filter((s) => keyFilter.has(s.key));
    for (const k of Object.keys(lockLang)) if (!english.has(k)) delete lockLang[k];

    if (opts.mode === "mark-reviewed") {
      const marked = [];
      for (const n of p.needsReview) {
        if (keyFilter && !keyFilter.has(n.key)) continue;
        lockLang[n.key] = { en: hash(english.get(n.key)), out: hash(n.current), human: true };
        marked.push(n.key);
      }
      p.needsReview = p.needsReview.filter((n) => !marked.includes(n.key));
      opts.log(`${lang}: marked ${marked.length} reviewed`);
      results.set(lang, { ...p, marked, failed: [], written: [] });
      continue;
    }

    const style = path.join(opts.i18nDir, "style", `${lang}.md`);
    if (!fs.existsSync(style)) throw new Error(`no style guide for ${lang}: ${style}`);
    const system = systemPrompt({ lang, catalogAbout: opts.catalog.about, glossary, style: fs.readFileSync(style, "utf8") });
    const counts = { missing: 0, changed: 0, retranslate: 0 };
    for (const s of p.send) counts[s.reason]++;
    opts.log(`${lang}: ${p.send.length} to send (${counts.missing} new, ${counts.changed} English changed${counts.retranslate ? `, ${counts.retranslate} retranslate` : ""}), ${p.unchanged} unchanged, ${p.needsReview.length} need review, ${p.removed.length} no longer in English`);
    if (p.humanDetected.length) opts.log(`  person-edited since last run (now protected): ${p.humanDetected.join(", ")}`);
    if (p.adopted.length) opts.log(`  translations not written by this tool (kept, protected): ${p.adopted.length}`);
    for (const n of p.needsReview) opts.log(`  needs review: ${n.key}`);
    if (opts.mode === "dry-run") {
      for (const s of p.send) opts.log(`  send [${s.reason}] ${s.key}: ${JSON.stringify(english.get(s.key))}`);
      const nb = batches(p.send, english, { maxKeys: opts.batchSize }).length;
      if (p.send.length) opts.log(`  ${nb} request${nb === 1 ? "" : "s"}; system prompt ${system.length} characters`);
      if (opts.showPrompt) opts.log(`\n----- system prompt (${lang}) -----\n${system}\n----- first request -----\n${p.send.length ? userMessage(batches(p.send, english, { maxKeys: opts.batchSize })[0], english) : "(none)"}\n`);
    }
    results.set(lang, { ...p, failed: [], written: [] });
    work.push({ lang, file, target, lockLang, p, system });
  }

  if (opts.mode === "write") {
    if (work.some((w) => w.p.send.length) && !opts.env.ANTHROPIC_API_KEY) throw new Error("ANTHROPIC_API_KEY is not set (try --dry-run)");
    // A few languages at a time; batches inside a language run one after another.
    const queue = work.filter((w) => w.p.send.length || w.p.removed.length);
    const runOne = async (w) => {
      const { done, failed } = w.p.send.length
        ? await translateLanguage({ lang: w.lang, items: w.p.send, english, system: w.system, opts })
        : { done: new Map(), failed: [] };
      for (const [k, v] of done) {
        w.target.set(k, v);
        w.lockLang[k] = { en: hash(english.get(k)), out: hash(v), human: false };
      }
      for (const k of w.p.removed) w.target.delete(k);
      if (done.size || w.p.removed.length) writeJson(w.file, unflatten(w.target, englishOrder));
      const r = results.get(w.lang);
      r.failed = failed;
      r.written = [...done.keys()];
      opts.log(`${w.lang}: wrote ${done.size}${failed.length ? `, ${failed.length} failed (see ${REVIEW_FILE})` : ""}${w.p.removed.length ? `, removed ${w.p.removed.length}` : ""}`);
    };
    const pool = [];
    for (let n = 0; n < Math.max(1, opts.concurrency); n++) {
      pool.push((async () => { while (queue.length) await runOne(queue.shift()); })());
    }
    await Promise.all(pool);
  }

  if (opts.mode === "write" || opts.mode === "mark-reviewed" || opts.mode === "adopt") {
    const sortedLock = {};
    for (const lang of Object.keys(lock).sort()) sortedLock[lang] = sortObject(lock[lang]);
    writeJson(lockFile, sortedLock);
    // The report covers every language with an entry in the lock, not only those run now.
    const all = new Map();
    for (const lang of Object.keys(sortedLock)) {
      if (results.has(lang)) { all.set(lang, results.get(lang)); continue; }
      const target = flatten(readJson(path.join(dir, `${lang}.json`), {}));
      const p = plan(english, target, structuredClone(sortedLock[lang]));
      all.set(lang, { ...p, failed: [] });
    }
    const report = reviewReport(opts.catalog.name, all);
    const reportFile = path.join(dir, REVIEW_FILE);
    if (report) fs.writeFileSync(reportFile, report);
    else if (fs.existsSync(reportFile)) fs.rmSync(reportFile);
  }

  const ok = [...results.values()].every((r) => !r.failed.length && !r.invalid?.length);
  return { ok, results };
}

// ---------------------------------------------------------------------------------------------
// CLI.

function parseArgs(argv) {
  const a = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const s = argv[i];
    if (!s.startsWith("--")) { a._.push(s); continue; }
    const [k, v] = s.slice(2).split("=", 2);
    if (["dry-run", "write", "check", "mark-reviewed", "adopt", "retranslate", "show-prompt", "help"].includes(k)) a[k] = true;
    else a[k] = v ?? argv[++i];
  }
  return a;
}

const USAGE = `usage: node i18n/translate.mjs <ui|site|native|path/to/messages> [options]
  --langs es,pt | full | early | all   which languages (default all)
  --dry-run          show what would be sent (default)
  --write            translate, validate and write <lang>.json, .sources.json, needs-review.md
  --check            every language has every key and valid ICU; exit 1 if not (no API)
  --mark-reviewed    accept person-edited translations whose English changed (with --keys to limit)
  --adopt --as machine|human   record what's in <lang>.json now in the lock (no API): "machine" = as if the
                     tool wrote it (refreshed when English changes; never takes a person's key unless --keys names
                     it), "human" = a person's (protected). For translations written outside the tool
  --retranslate      with --keys: translate these again even if a person edited them
  --keys a.b,c.d     limit to these keys
  --batch 40         keys per request
  --concurrency 4    languages translated at once
  --show-prompt      (dry run) print the full system prompt and first request
env: ANTHROPIC_API_KEY, ONKEY_TRANSLATE_MODEL (default ${DEFAULT_MODEL}), ONKEY_TRANSLATE_FALLBACK=0 to skip the refusal fallback
ICU parser: ${parserName}`;

async function main() {
  const a = parseArgs(process.argv.slice(2));
  if (a.help || !a._[0]) { console.log(USAGE); process.exit(a.help ? 0 : 2); }
  const mode = a.write ? "write" : a.check ? "check" : a["mark-reviewed"] ? "mark-reviewed" : a.adopt ? "adopt" : "dry-run";
  const { ok } = await run({
    catalog: resolveCatalog(a._[0]),
    langs: resolveLangs(a.langs),
    mode,
    keys: a.keys ? a.keys.split(",").map((s) => s.trim()) : null,
    retranslate: !!a.retranslate,
    as: a.as,
    batchSize: a.batch ? Number(a.batch) : 40,
    concurrency: a.concurrency ? Number(a.concurrency) : 4,
    showPrompt: !!a["show-prompt"],
  });
  process.exit(ok ? 0 : 1);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => { console.error(`translate: ${e.message}`); process.exit(2); });
}
