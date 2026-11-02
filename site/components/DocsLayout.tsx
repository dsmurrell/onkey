import { DocsNav } from "@/components/DocsNav";
import { DEFAULT_LOCALE, TRANSLATED_PAGES, localeHref } from "@/lib/languages";
import { getT } from "@/lib/i18n";

/** The docs pages in order, by their English path and their title's key in messages (docsNav.pages). */
export const DOCS = [
  { href: "/docs/", key: "start" },
  { href: "/docs/how-it-works/", key: "howItWorks" },
  { href: "/docs/scoring/", key: "scoring" },
  { href: "/docs/privacy/", key: "privacy" },
  { href: "/docs/faq/", key: "faq" },
  { href: "/docs/your-voice/", key: "yourVoice" },
  { href: "/docs/beyond-pitch/", key: "beyondPitch" },
  { href: "/docs/troubleshooting/", key: "troubleshooting" },
  { href: "/docs/credits/", key: "credits" },
] as const;

/** The docs' frame: the list of pages beside the page. In another language, a page that's only in English
 *  says so in the list. */
export function DocsLayout({ locale, children }: { locale: string; children: React.ReactNode }) {
  const { t } = getT(locale, "docsNav");
  const items = DOCS.map((d) => {
    const title = t(`pages.${d.key}`);
    const english = locale !== DEFAULT_LOCALE && !TRANSLATED_PAGES.includes(d.href);
    return { href: localeHref(locale, d.href), title: english ? t("inEnglish", { title }) : title };
  });
  return (
    <div className="mx-auto grid max-w-6xl gap-8 px-4 py-10 sm:px-6 md:grid-cols-[200px_minmax(0,1fr)] md:gap-12 md:py-14">
      <aside className="md:sticky md:top-24 md:self-start">
        <DocsNav items={items} label={t("label")} heading={t("heading")} />
      </aside>
      <article className="prose docs-wide min-w-0">{children}</article>
    </div>
  );
}
