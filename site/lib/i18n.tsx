// The site's words, from one catalog per language in site/messages/ (use-intl: ICU messages, typed keys).
// Server only (it reads the catalogs while the static pages are built).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Metadata } from "next";
import Link from "next/link";
import { createTranslator, type NamespaceKeys, type NestedKeyOf } from "use-intl/core";
import en from "@/messages/en.json";
import changelogEn from "@/messages/changelog/en.json";
import { PSEUDO, catalogLocales, merge, messagesDir, pseudoMessages } from "./catalogs.mjs";
import { DEFAULT_LOCALE, localeHref } from "./languages";
import { WINDOWS_ISSUE_URL } from "./site";
import type { DownloadText } from "@/components/DownloadButton";

export type Messages = typeof en;
type Namespace = NamespaceKeys<Messages, NestedKeyOf<Messages>>;
/** The changelog's own catalog, site/messages/changelog/: a release adds entries there, not to the pages' catalog. */
export type ChangelogMessages = typeof changelogEn;

const loaded = new Map<string, Messages | ChangelogMessages>();

/** A language's messages from `english` and its file `<dir>/<locale>.json`; any key the file doesn't have yet
 *  (or the whole file, for the changelog) is the English one. */
function load<M extends object>(locale: string, english: M, dir: string): M {
  const id = `${dir}/${locale}`;
  let m = loaded.get(id) as M | undefined;
  if (!m) {
    const file = join(dir, `${locale}.json`);
    if (locale === DEFAULT_LOCALE) m = english;
    else if (locale === PSEUDO) m = pseudoMessages(english) as M;
    else m = merge(english, existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : {}) as M;
    loaded.set(id, m as Messages);
  }
  return m;
}

/** A language's messages; any key its catalog doesn't have yet is the English one. */
export const messagesFor = (locale: string): Messages => load(locale, en, messagesDir());
export const changelogMessagesFor = (locale: string): ChangelogMessages => load(locale, changelogEn, join(messagesDir(), "changelog"));

/** Tags any message may use, rendered the same everywhere. */
const TAGS = {
  strong: (c: React.ReactNode) => <strong>{c}</strong>,
  code: (c: React.ReactNode) => <code>{c}</code>,
};
type Tag = (chunks: React.ReactNode) => React.ReactNode;

type Values = Record<string, Tag | string | number>;
type Rich<Key> = (key: Key, values: Record<string, unknown>) => React.ReactNode;

const options = (locale: string) => ({ locale: locale === PSEUDO ? DEFAULT_LOCALE : locale, timeZone: "UTC", onError: (e: Error) => { throw e; } });

/** `t(key, values)` for plain text, `rich(key, tags)` for text with tags (<strong> and <code> built in).
 *  A message that's missing or doesn't parse fails the build. */
export function getT<NS extends Namespace>(locale: string, namespace: NS) {
  const t = createTranslator({ ...options(locale), messages: messagesFor(locale), namespace });
  type Key = Parameters<typeof t.rich>[0];
  const rich = (key: Key, values?: Values) => (t.rich as unknown as Rich<Key>)(key, { ...TAGS, ...values });
  return { t, rich };
}

/** The same for the changelog's catalog (site/messages/changelog/). */
export function getChangelogT(locale: string) {
  const t = createTranslator({ ...options(locale), messages: changelogMessagesFor(locale) });
  type Key = Parameters<typeof t.rich>[0];
  const rich = (key: Key, values?: Values) => (t.rich as unknown as Rich<Key>)(key, { ...TAGS, ...values });
  return { t, rich };
}

/** A date as the page's language writes it ("31 Oct 2026" in English). */
export const formatDay = (locale: string, iso: string) =>
  new Date(`${iso}T12:00:00Z`).toLocaleDateString(locale === DEFAULT_LOCALE || locale === PSEUDO ? "en-GB" : locale, { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });

/** A tag that's a link to a page of the site, in `locale` when that page is translated. */
export const pageLink = (locale: string, path: string, className?: string): Tag =>
  (c) => <Link href={localeHref(locale, path)} className={className}>{c}</Link>;

/** The languages other than English the site is built in. */
export const otherLocales = () => catalogLocales();

/** hreflang links for a translated page (`path` is its English path), once there's a second language. */
export function alternates(path: string): Pick<Metadata, "alternates"> {
  const others = otherLocales();
  if (!others.length) return {};
  return {
    alternates: {
      languages: { [DEFAULT_LOCALE]: path, ...Object.fromEntries(others.map((l) => [l, `/${l}${path}`])), "x-default": path },
    },
  };
}

/** The download button's words (it runs in the browser, so it gets them from the page). */
export function downloadText(locale: string): DownloadText {
  const { t, rich } = getT(locale, "download");
  return {
    mac: t("mac"),
    macsOnly: rich("macsOnly", { link: (c) => <a href={WINDOWS_ISSUE_URL} data-track="Windows issue" className="underline underline-offset-2">{c}</a> }),
    send: t("send"),
    sent: t("sent"),
    phones: t("phones"),
    shareText: t("shareText"),
    mailSubject: t("mailSubject"),
  };
}

/** What OnKey needs, in the page's language (lib/site.ts REQUIREMENTS in English). */
export function requirements(locale: string): string[] {
  const { t } = getT(locale, "common");
  return [t("requirements.silicon"), t("requirements.macos")];
}
