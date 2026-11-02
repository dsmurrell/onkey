// The site's languages and which pages are translated. No file access here, so client components can use it.
//
// English is the source and stays at the site's root (/, /docs/faq/, …) with its URLs unchanged. Every other
// language with a catalog in site/messages/ gets the translated pages under its code: /es/, /es/docs/faq/, …
// Pages that aren't translated yet link to the English page.
import list from "./languages.json" with { type: "json" };

/** "full": lyric timing, suggested songs and the site's claims hold in that language. "early": the words are
 *  translated, but songs in that language show their lyrics a line at a time (the note under lyric claims). */
export type Level = "full" | "early";

/** Every language OnKey plans for: its name in itself and its level. The list is OnKey's one list,
 *  i18n/languages.json (the app and the translate tool read it too), copied to lib/languages.json (a test checks
 *  the copy is current: `cp i18n/languages.json site/lib/languages.json`). Only those with a catalog are built. No
 *  right-to-left languages: the lanes and timelines run left to right. */
export const LANGUAGES: Record<string, { name: string; level: Level }> = {
  en: { name: "English", level: "full" },
  ...Object.fromEntries(list.languages.map((l) => [l.code, { name: l.native, level: l.level as Level }])),
  "en-XA": { name: "Pseudo", level: "early" },
};

export const DEFAULT_LOCALE = "en";

/** The cookie the language switcher sets; the Pages Function on / follows it before the browser's language
 *  (choose-language.ts has the same, for the Function, which doesn't import this file and its JSON; a test checks
 *  they match). */
export const LANG_COOKIE = "lang";

/** The pages that exist in every language: the home page, every docs page and the changelog. A page whose words a
 *  catalog doesn't have yet shows them in English (lib/i18n.tsx messagesFor). */
export const TRANSLATED_PAGES = [
  "/", "/docs/", "/docs/how-it-works/", "/docs/scoring/", "/docs/privacy/", "/docs/faq/", "/docs/your-voice/",
  "/docs/beyond-pitch/", "/docs/troubleshooting/", "/docs/credits/", "/changelog/",
];

/** `path` (an English path like "/docs/faq/" or "/docs/faq/#x") in `locale`: under /<locale>/ when that page is
 *  translated, else the English page. */
export function localeHref(locale: string, path: string): string {
  if (locale === DEFAULT_LOCALE) return path;
  const [page] = path.split("#");
  return TRANSLATED_PAGES.includes(page) ? `/${locale}${path}` : path;
}

/** The English path of a page, and its locale, from its URL path (without the base path). */
export function splitLocale(pathname: string, locales: string[]): { locale: string; path: string } {
  const m = pathname.match(/^\/([^/]+)(\/.*)?$/);
  if (m && locales.includes(m[1])) return { locale: m[1], path: m[2] || "/" };
  return { locale: DEFAULT_LOCALE, path: pathname };
}

export const languageName = (locale: string) => LANGUAGES[locale]?.name ?? locale;
export const isEarly = (locale: string) => LANGUAGES[locale]?.level === "early";
