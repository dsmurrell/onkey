// OnKey's languages: codes, each one's name in itself, and its level. One list for the app, the site and the
// translate tool: i18n/languages.json, copied here as lib/languages.json (languages.test.mjs checks the copy is
// current: after changing the list, `cp i18n/languages.json ui/lib/languages.json`).
//   full:  the window, lyric timing and songs to try in that language
//   early: the window is translated; songs in that language show their lyrics a line at a time for now
import list from "./languages.json" with { type: "json" };

export type Level = "full" | "early";
export type Language = { code: string; name: string; native: string; level: Level };

/** English first (the source), then every other language as listed. */
export const LANGUAGES: Language[] = [
  { code: "en", name: "English", native: "English", level: "full" },
  ...(list.languages as Language[]),
];

export const languageOf = (code: string): Language | undefined => LANGUAGES.find(l => l.code === code);
/** A language's name in itself ("Deutsch"); the code for one not listed (a test's fixture). */
export const nativeName = (code: string): string => languageOf(code)?.native ?? code;
export const isEarly = (code: string): boolean => languageOf(code)?.level === "early";

/** Where a language's words are, for "Improve this translation" (CONTRIBUTING.md): its catalog on GitHub.
 *  The engine's open_link allows exactly these (lib.rs `is_translation_link`). */
export const translationUrl = (code: string): string => `https://github.com/dsmurrell/onkey/edit/main/ui/messages/${code}.json`;

/** Settings' language picker: the codes to offer, in the list's order (English first; a catalog not in the list,
 *  a test's or the pseudo-language, last), or null while there's only English (no picker then). */
export function pickerCodes(available: readonly string[]): string[] | null {
  if (available.length < 2) return null;
  const order = (c: string) => { const i = LANGUAGES.findIndex(l => l.code === c); return i < 0 ? LANGUAGES.length : i; };
  return [...available].sort((a, b) => order(a) - order(b) || a.localeCompare(b));
}

/** The language most spoken in a region (ISO country code, lower case, as platform::sys::region() gives it), when it's
 *  one OnKey ships: the Welcome step offers it second, after Automatic, so an English Mac set to Mexico sees
 *  Español (Latinoamérica) at once. Only regions with one clear majority language: Belgium is Dutch (Flemish, the
 *  larger part), Switzerland German; Canada and the United States are left out (English, which is Automatic). */
export const REGION_LANGUAGE: Record<string, string> = {
  // Spanish, Latin America
  mx: "es", ar: "es", co: "es", cl: "es", pe: "es", ve: "es", ec: "es", gt: "es", cu: "es", bo: "es", do: "es",
  hn: "es", py: "es", sv: "es", ni: "es", cr: "es", pa: "es", uy: "es", pr: "es", gq: "es",
  es: "es-ES",
  br: "pt", pt: "pt", ao: "pt", mz: "pt",
  it: "it", sm: "it",
  fr: "fr", mc: "fr", sn: "fr", ci: "fr",
  de: "de", at: "de", ch: "de", li: "de",
  jp: "ja",
  cn: "zh-Hans", sg: "zh-Hans",
  tw: "zh-Hant", hk: "zh-Hant", mo: "zh-Hant",
  kr: "ko",
  in: "hi",
  ru: "ru", by: "ru", kz: "ru",
  tr: "tr",
  pl: "pl",
  nl: "nl", be: "nl",
  id: "id",
  vn: "vi",
  th: "th",
};

/** The region's language, if OnKey ships it (`available`: the codes there are catalogs for); null otherwise. */
export function regionLanguage(region: string | null | undefined, available: readonly string[] = LANGUAGES.map(l => l.code)): string | null {
  const code = region ? REGION_LANGUAGE[region.trim().toLowerCase()] : undefined;
  return code && available.includes(code) ? code : null;
}

/** The Welcome step's list of languages (under Automatic, which the component adds): the region's language first
 *  when it isn't the one Automatic comes to (`auto`), then every language in the list's order (English, then Full,
 *  then Early; a catalog not in the list last). */
export function welcomeOrder(available: readonly string[], auto: string, region: string | null | undefined): string[] {
  const codes = pickerCodes(available) ?? [...available];
  const first = regionLanguage(region, codes);
  return first && first !== auto ? [first, ...codes.filter(c => c !== first)] : codes;
}

/** Translators' GitHub usernames by language (ui/messages/CREDITS.json; CONTRIBUTING.md "Credit"). */
export type Credits = Record<string, string[]>;

/** A GitHub username: letters, digits and single hyphens between them, at most 39 characters. */
export const GITHUB_USER = /^[A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}$/;

/** What's wrong with CREDITS.json (none: fine): each key a language there's a catalog for (`codes`, not English),
 *  each value a list of GitHub usernames, none twice. */
export function creditsProblems(credits: unknown, codes: readonly string[]): string[] {
  if (!credits || typeof credits !== "object" || Array.isArray(credits)) return ["CREDITS.json must be an object: language code → [GitHub usernames]"];
  const out: string[] = [];
  for (const [code, names] of Object.entries(credits as Record<string, unknown>)) {
    if (code === "en") out.push(`"en": English is the original; credit translators under their language`);
    else if (!codes.includes(code)) out.push(`"${code}": there's no messages/${code}.json`);
    if (!Array.isArray(names)) { out.push(`"${code}": must be a list of GitHub usernames`); continue; }
    const seen = new Set<string>();
    for (const n of names) {
      if (typeof n !== "string" || !GITHUB_USER.test(n)) out.push(`"${code}": ${JSON.stringify(n)} isn't a GitHub username`);
      else if (seen.has(n.toLowerCase())) out.push(`"${code}": ${n} is listed twice`);
      else seen.add(n.toLowerCase());
    }
  }
  return out;
}
