"use client";
import { usePathname } from "next/navigation";
import { LANG_COOKIE, TRANSLATED_PAGES, languageName, localeHref, splitLocale } from "@/lib/languages";
import { asset } from "@/lib/site";

/** Where each language's version of this page is (or that language's home page when this page is English only),
 *  and how a choice is remembered: a cookie, which the Pages Function on / follows (functions/_middleware.ts). The
 *  footer's list and the header's menu (LanguageMenu.tsx) both use these, so they go to the same places. */
export function useLanguageLinks(locales: string[]) {
  const pathname = usePathname();
  const norm = pathname.endsWith("/") ? pathname : `${pathname}/`;
  const { path } = splitLocale(norm, locales);
  const href = (l: string) => asset(TRANSLATED_PAGES.includes(path) ? localeHref(l, path) : localeHref(l, "/"));
  const remember = (l: string) => {
    document.cookie = `${LANG_COOKIE}=${encodeURIComponent(l)}; path=/; max-age=31536000; samesite=lax`;
  };
  return { href, remember };
}

/** The footer's list: links to this page in each language, each in its own name (`hreflang` for search engines).
 *  `#languages` is where the home page's "and N more" lands. */
export function LanguageSwitcher({ locales, current, label }: { locales: string[]; current: string; label: string }) {
  const { href, remember } = useLanguageLinks(locales);
  return (
    <nav id="languages" aria-label={label} className="flex flex-wrap gap-x-3 gap-y-1 text-xs md:max-w-2xl md:justify-end">
      {locales.map((l) => (
        <a
          key={l}
          href={href(l)}
          hrefLang={l}
          lang={l}
          aria-current={l === current ? "true" : undefined}
          onClick={() => remember(l)}
          className={`inline-block py-1 ${l === current ? "font-medium text-fg" : "hover:text-fg"}`}
        >
          {languageName(l)}
        </a>
      ))}
    </nav>
  );
}
