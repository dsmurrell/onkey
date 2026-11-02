// Which language to send a visitor of / to, from their browser's languages (Accept-Language) and the language
// switcher's cookie. Used by the Pages Function functions/_middleware.ts; no imports, so the tests run it as is.
// English needs no redirect, so English is null, as is anything we can't place: a search engine, no header, a
// language we don't have. Never by location: a Spanish speaker in the US still gets Spanish.

/** The cookie the language switcher sets; the Pages Function on / follows it before the browser's language. */
export const LANG_COOKIE = "lang";

export type Visit = { path: string; acceptLanguage?: string | null; cookie?: string | null; userAgent?: string | null };

/** Crawlers and link-preview fetchers: always the page that was asked for. */
const BOT = /bot\b|bot\/|crawl|spider|slurp|preview|facebookexternalhit|embedly|whatsapp|lighthouse|inspectiontool|headless/i;

/** The browser's languages, most wanted first (q=0 means "not this one"). */
export function parseAcceptLanguage(header: string): string[] {
  return header
    .split(",")
    .map((part, i) => {
      const [tag, ...params] = part.trim().split(";");
      const q = params.map((p) => p.trim()).find((p) => p.startsWith("q="));
      const weight = q ? Number(q.slice(2)) : 1;
      return { tag: tag.trim(), weight: Number.isFinite(weight) ? weight : 0, i };
    })
    .filter((l) => l.tag && l.tag !== "*" && l.weight > 0)
    .sort((a, b) => b.weight - a.weight || a.i - b.i)
    .map((l) => l.tag);
}

/** Our language for one of the browser's (`supported` includes "en"): the same tag, else Chinese by script or
 *  region, else the same base language (es-MX → es, pt-BR → pt). */
export function matchLanguage(tag: string, supported: string[]): string | null {
  const lower = tag.toLowerCase();
  const exact = supported.find((s) => s.toLowerCase() === lower);
  if (exact) return exact;
  const [base, ...rest] = lower.split("-");
  if (base === "zh") {
    const traditional = rest.includes("hant") || (!rest.includes("hans") && rest.some((r) => ["tw", "hk", "mo"].includes(r)));
    const want = traditional ? "zh-Hant" : "zh-Hans";
    return supported.includes(want) ? want : null;
  }
  return supported.find((s) => s.toLowerCase() === base) ?? null;
}

const cookieValue = (header: string, name: string) => {
  for (const part of header.split(";")) {
    const [k, ...v] = part.trim().split("=");
    if (k === name) { try { return decodeURIComponent(v.join("=")); } catch { return null; } }
  }
  return null;
};

/** The language to redirect `/` to, or null to serve English. `available`: the languages other than English
 *  that have pages. */
export function chooseLanguage(visit: Visit, available: string[], cookieName = "lang"): string | null {
  if (visit.path !== "/") return null;
  if (visit.userAgent && BOT.test(visit.userAgent)) return null;
  const chosen = visit.cookie ? cookieValue(visit.cookie, cookieName) : null;
  if (chosen === "en") return null;
  if (chosen && available.includes(chosen)) return chosen;
  if (!visit.acceptLanguage) return null;
  const supported = ["en", ...available];
  for (const tag of parseAcceptLanguage(visit.acceptLanguage)) {
    const match = matchLanguage(tag, supported);
    if (match) return match === "en" ? null : match;
  }
  return null;
}
