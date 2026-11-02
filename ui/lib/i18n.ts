// The window's words, in the language chosen: every piece of text the window shows comes from a catalog,
// messages/<code>.json (English, messages/en.json, is the source of truth; the others are translations of it), by
// key: `t("settings.audio.title")`. Formatting is use-intl's (next-intl's core, without its routing): standard ICU
// messages, so any translation tool or translator knows them:
//   {name}                                   a value put in ("Now: {name}."); a number as it is (1234, no separators)
//   {count, plural, one {{count} song} other {{count} songs}}   plurals by the language's own rules. (# also works
//                                            but writes the number the language's way, "1,234": the catalog uses
//                                            {count} wherever the English said the bare number)
//   {os, select, mac {…} windows {…} other {…}}   a choice by a value
//   <b>…</b>                                 a part to style (bold, a link): `t.rich(key, { b: c => <b>{c}</b> })`
// ICU's one quirk: an apostrophe next to { or } quotes it, and '' is one apostrophe. The catalog writes ’ (curly)
// almost everywhere, and the checks (english-same.mjs) read every message with the same parser, so a quirk shows.
//
// Components: `const t = useT();` (use-intl's useTranslations, under the I18n provider in the layout). Code outside
// components (lib/: offBy, pcWords…): the module's `t`, which follows the same language.
//
// Which language: the Settings choice ("language": a code, or "auto"), else the computer's own languages (as the
// engine reads them from the system, `ui_locale`: WebView2 reports en-US whatever Windows says, so not
// navigator.languages, which is only the fallback outside the app), else English. A key a translation lacks is
// said in English. Adding a language: messages/<code>.json (scripts/catalogs.mjs picks it up into lib/catalogs.ts
// before every build; the missing-key check in i18n.test.mjs then holds it to English's keys and parameters) and
// its line in i18n/languages.json.
import { useMemo, useSyncExternalStore, type ReactNode } from "react";
import { createTranslator, type IntlError } from "use-intl/core";
import { useTranslations } from "use-intl/react";
import en from "../messages/en.json" with { type: "json" };
import { TRANSLATIONS } from "./catalogs";

export type Catalog = { [key: string]: string | Catalog };
type Leaves<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends string ? `${P}${K}` : Leaves<T[K], `${P}${K}.`>;
}[keyof T & string];
/** Every message's key, from English's catalog ("settings.audio.title"). */
export type MessageKey = Leaves<typeof en>;

/** The languages the window can show, by code. English first: the source of truth, and the fallback. */
const CATALOGS: Record<string, Catalog> = { en, ...TRANSLATIONS };

/** The pseudo-language: English with its letters accented and each message bracketed, values ({…}) and tags
 *  (<b>) left as they are. To see the window in "another language" before there's a translation: text that skips
 *  the catalog stays plain, and Settings shows the language picker. Only with NEXT_PUBLIC_ONKEY_PSEUDO=1 (e.g.
 *  `NEXT_PUBLIC_ONKEY_PSEUDO=1 npm run dev`); never in a release. */
export const PSEUDO = "en-XA";
const ACCENT: Record<string, string> = Object.fromEntries([..."aeiouyAEIOUYcnsCNS"].map((c, i) => [c, "áéíóúýÁÉÍÓÚÝçñšÇÑŠ"[i]]));
export function pseudoCatalog(catalog: Catalog): Catalog {
  const one = (text: string) => {
    let out = "", depth = 0;
    for (let i = 0; i < text.length; i++) {
      const c = text[i];
      if (c === "{") depth++;
      if (depth === 0 && c === "<") {
        const end = text.indexOf(">", i);
        if (end > i) { out += text.slice(i, end + 1); i = end; continue; }
      }
      out += depth === 0 ? ACCENT[c] ?? c : c;
      if (c === "}") depth--;
    }
    return `⟦${out}⟧`;
  };
  return Object.fromEntries(Object.entries(catalog).map(([k, v]) => [k, typeof v === "string" ? one(v) : pseudoCatalog(v)]));
}
if (typeof process !== "undefined" && process.env.NEXT_PUBLIC_ONKEY_PSEUDO === "1") CATALOGS[PSEUDO] = pseudoCatalog(en);

/** The codes there are catalogs for ("en"). */
export function locales(): string[] {
  return Object.keys(CATALOGS);
}

/** Add a catalog (tests, with a fixture; a new language goes in CATALOGS). */
export function registerCatalog(code: string, catalog: Catalog) {
  CATALOGS[code] = catalog;
  merged.delete(code);
  if (code === current) translator = make(code);
  emit();
}

/** Remove one (tests). English stays. */
export function unregisterCatalog(code: string) {
  if (code === "en") return;
  delete CATALOGS[code];
  merged.delete(code);
  if (current === code) setLocale("en");
  emit();
}

// ---- choosing the language ----

/** The language to show: the Settings choice if it's a code there's a catalog for (not "auto"), else the first of
 *  the computer's languages there is one for, else English. A computer language matches its own code ("es-ES"),
 *  else its base language ("es-MX", "es-419" → "es"; "pt-PT" → "pt"), else another region of it ("es-MX" →
 *  "es-ES" when there's no plain "es"). Chinese goes by script, never to the other one: "zh-Hant…", zh-TW, zh-HK
 *  and zh-MO → "zh-Hant"; any other zh → "zh-Hans" (or a plain "zh"). */
export function pickLocale(setting: string | null | undefined, preferred: readonly string[] | null | undefined,
                           available: readonly string[] = locales()): string {
  const find = (code: string) => available.find(a => a.toLowerCase() === code);
  const match = (code: string): string | null => {
    const want = code.trim().replace(/_/g, "-").toLowerCase();
    if (!want) return null;
    const exact = find(want);
    if (exact) return exact;
    const [base, ...rest] = want.split("-");
    if (base === "zh") {
      const traditional = rest.includes("hant") || (!rest.includes("hans") && rest.some(r => r === "tw" || r === "hk" || r === "mo"));
      return find(traditional ? "zh-hant" : "zh-hans") ?? find("zh") ?? null;
    }
    return find(base) ?? available.find(a => a.toLowerCase().startsWith(base + "-")) ?? null;
  };
  if (setting && setting !== "auto") {
    const m = match(setting);
    if (m) return m;
  }
  for (const p of preferred ?? []) {
    const m = match(p);
    if (m) return m;
  }
  return "en";
}

let osLanguages: string[] | null = null;

/** The computer's languages as the engine read them (`ui_locale`), most preferred first. Re-picks the language
 *  shown when Settings says "auto". */
export function setSystemLanguages(list: readonly string[] | null | undefined) {
  osLanguages = list?.length ? [...list] : null;
  setLocale(pickLocale(setting, systemLanguages()));
}

/** The computer's languages, most preferred first: the engine's list once it's given, else the browser's (none
 *  while the page is built). */
export function systemLanguages(): string[] {
  if (osLanguages) return osLanguages;
  if (typeof navigator === "undefined") return [];
  const list = navigator.languages?.length ? navigator.languages : navigator.language ? [navigator.language] : [];
  return [...list];
}

let osRegion: string | null = null;

/** The computer's region as the engine read it (`ui_region`: "mx"), for the Welcome step's list. */
export function setSystemRegion(region: string | null | undefined) {
  osRegion = region ? region.toLowerCase() : null;
  emit();
}

/** The computer's region, lower case ("mx"): the engine's once it's given, else the region of the browser's first
 *  language ("es-MX" → "mx"; none while the page is built or when the language names no region). */
export function systemRegion(): string | null {
  if (osRegion) return osRegion;
  const first = systemLanguages()[0];
  const m = first?.replace(/_/g, "-").match(/^[a-z]{2,3}(?:-[a-z]{4})?-([a-z]{2})(?:-|$)/i);
  return m ? m[1].toLowerCase() : null;
}

let setting: string = "auto";
let current = "en";
const listeners = new Set<() => void>();
function emit() {
  for (const l of listeners) l();
}

/** Show this language (a code there's a catalog for; anything else: English). Sets <html lang> too, which picks
 *  the right glyphs for Chinese and Japanese. */
export function setLocale(code: string) {
  const next = CATALOGS[code] ? code : "en";
  if (next === current) return;
  current = next;
  translator = make(next);
  if (typeof document !== "undefined") document.documentElement.lang = next;
  emit();
}

/** The Settings choice ("auto" or a code) changed, or was read at launch: show the language it comes to. */
export function setLanguageSetting(choice: string | null | undefined) {
  const was = setting;
  setting = choice || "auto";
  const before = current;
  setLocale(pickLocale(setting, systemLanguages()));
  // the choice can change without the language shown changing (Automatic on a computer set to it): say so anyway,
  // so the language menus tick the right row
  if (setting !== was && current === before) emit();
}

/** The Settings choice as last given ("auto" until Settings is read). */
export function languageSetting(): string {
  return setting;
}

/** The Settings choice, kept current: it arrives a moment after the window opens (Settings is read then). */
export function useLanguageSetting(): string {
  return useSyncExternalStore(subscribe, languageSetting, () => "auto");
}

/** The language shown now. */
export function getLocale(): string {
  return current;
}

const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };

/** The language shown, for components: English while the page is put together (the built page is English), then
 *  the real one; a change re-renders. */
export function useLocale(): string {
  return useSyncExternalStore(subscribe, getLocale, () => "en");
}

/** `systemRegion()` for components: null while the page is put together, then the real one once the engine says. */
export function useSystemRegion(): string | null {
  return useSyncExternalStore(subscribe, systemRegion, () => null);
}

// ---- the messages ----

/** A catalog as key → message ("settings.audio.title" → "Audio"). */
export function flatten(catalog: Catalog, prefix = "", out = new Map<string, string>()): Map<string, string> {
  for (const [k, v] of Object.entries(catalog)) {
    if (typeof v === "string") out.set(prefix + k, v);
    else flatten(v, prefix + k + ".", out);
  }
  return out;
}

/** English with a translation laid over it: what a translation lacks is said in English. */
function over(base: Catalog, top: Catalog): Catalog {
  const out: Catalog = { ...base };
  for (const [k, v] of Object.entries(top)) {
    const b = base[k];
    out[k] = typeof v === "string" || typeof b !== "object" ? v : over(b, v);
  }
  return out;
}
const merged = new Map<string, Catalog>();
/** The messages for a language, English filling what it lacks (the provider and `t` both use these). */
export function messagesFor(code: string): Catalog {
  let m = merged.get(code);
  if (!m) {
    m = code === "en" || !CATALOGS[code] ? en : over(en, CATALOGS[code]);
    merged.set(code, m);
  }
  return m;
}

/** A message as written in the language shown (English if that language lacks it; the key if English does too). */
export function message(key: string, locale = current): string {
  return flatten(messagesFor(locale)).get(key) ?? key;
}

/** A formatting mistake (a value not given, a message that isn't well formed) is logged, never thrown: the window
 *  goes on with the key in its place. (The tests and english-same.mjs catch them before a release.) */
export function onIntlError(e: IntlError) {
  console.error(`[i18n] ${e.message}`);
}
export const getMessageFallback = ({ namespace, key }: { namespace?: string; key: string }) =>
  namespace ? `${namespace}.${key}` : key;

/** The computer's time zone (use-intl asks for one; the window formats its own dates, lib/music.ts `when`). */
export const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;

const make = (locale: string) =>
  createTranslator({ locale, messages: messagesFor(locale) as typeof en, onError: onIntlError, getMessageFallback, timeZone });
let translator = make("en");

/** The translator for the language shown: `t("library.count", { count: 3 })`, `t.rich(…)`, `t.has(…)`. */
export type Translator = ReturnType<typeof make>;

// ---- Chinese spacing ----

/** Chinese puts a space between a Han character and a Latin word or number ("在 Spotify 中打开", i18n/style/zh-*.md),
 *  never between two Han characters. A message written around a brand name ("在 {app} 中打开") sometimes gets
 *  Chinese put in instead (the fallback "浏览器" or "你的音乐应用", a folder's own name), and then the spaces around it
 *  are wrong ("在 浏览器 中打开"). So in Chinese, a space between two Han characters is taken out of every formatted
 *  message. Only Chinese: Japanese writes {app}で with no space, Korean and Thai space their own words. */
const HAN_SPACE = new RegExp("(?<=\\p{Script=Han}) (?=\\p{Script=Han})", "gu");
export function tidy(locale: string, text: string): string {
  return locale.startsWith("zh") ? text.replace(HAN_SPACE, "") : text;
}

/** A translator whose every result (`t`, `rich`, `markup`) is tidied for the language shown; `raw` and `has` as
 *  they are. `tr` and `locale` are read at each call, so the module's `t` follows a change of language. */
function tidied(tr: () => Translator, locale: () => string): Translator {
  const nodes = (n: ReactNode): ReactNode => typeof n === "string" ? tidy(locale(), n) : Array.isArray(n) ? n.map(nodes) : n;
  const tags = (v: Record<string, unknown> | undefined) => v && Object.fromEntries(Object.entries(v).map(([k, x]) =>
    [k, typeof x === "function" ? (c: ReactNode) => (x as (c: ReactNode) => ReactNode)(nodes(c)) : x]));
  return Object.assign(
    ((key: Parameters<Translator>[0], values?: Parameters<Translator>[1]) => tidy(locale(), tr()(key, values))) as Translator,
    {
      rich: ((key: Parameters<Translator["rich"]>[0], values?: Parameters<Translator["rich"]>[1]) =>
        nodes(tr().rich(key, tags(values as Record<string, unknown> | undefined) as Parameters<Translator["rich"]>[1]))) as Translator["rich"],
      markup: ((...a: Parameters<Translator["markup"]>) => tidy(locale(), tr().markup(...a))) as Translator["markup"],
      raw: ((...a: Parameters<Translator["raw"]>) => tr().raw(...a)) as Translator["raw"],
      has: ((...a: Parameters<Translator["has"]>) => tr().has(...a)) as Translator["has"],
    },
  );
}

/** A message by key, in the language shown, with its values put in. For code outside components; a component uses
 *  `useT()`, which re-renders when the language changes. */
export const t: Translator = tidied(() => translator, () => current);

/** `t`, for components: use-intl's useTranslations, under the I18n provider (components/I18n.tsx), so a language
 *  change re-renders. */
export function useT(): Translator {
  const tr = useTranslations() as unknown as Translator;
  const locale = useLocale();
  return useMemo(() => locale.startsWith("zh") ? tidied(() => tr, () => locale) : tr, [tr, locale]);
}

/** A date in the language shown ("1 Nov" in English on a British Mac): the window's language picks the words and
 *  order; the computer's own region still decides for English, as before (`undefined`). */
export function dateLocale(): string | undefined {
  return current === "en" ? undefined : current;
}
