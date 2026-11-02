import Link from "next/link";
import { Logo } from "./Logo";
import { DISCORD_URL, GITHUB_URL } from "@/lib/site";
import { DownloadButton } from "./DownloadButton";
import { LanguageMenu } from "./LanguageMenu";
import { DEFAULT_LOCALE, localeHref } from "@/lib/languages";
import { downloadText, getT, otherLocales } from "@/lib/i18n";

export function Header({ locale }: { locale: string }) {
  const { t } = getT(locale, "header");
  const href = (path: string) => localeHref(locale, path);
  const others = otherLocales();
  return (
    <header className="sticky top-0 z-30 border-b border-line/70 bg-bg/80 backdrop-blur-md">
      <div className="mx-auto flex h-16 max-w-6xl items-center gap-3 px-4 sm:gap-6 sm:px-6">
        <Logo href={href("/")} label={t("home")} />
        <nav className="flex items-center gap-1 text-sm" aria-label={t("label")}>
          <Link href={href("/docs/")} className="inline-flex min-h-11 items-center rounded-full px-3 text-muted transition hover:text-fg">{t("docs")}</Link>
          <Link href={href("/docs/faq/")} className="inline-flex min-h-11 items-center rounded-full px-3 text-muted transition hover:text-fg">{t("faq")}</Link>
          <Link href={href("/changelog/")} className="hidden min-h-11 items-center rounded-full px-3 text-muted transition hover:text-fg sm:inline-flex">{t("whatsNew")}</Link>
          <a href={GITHUB_URL} data-track="GitHub" className="inline-flex min-h-11 items-center rounded-full px-3 text-muted transition hover:text-fg">{t("github")}</a>
          {DISCORD_URL && <a href={DISCORD_URL} data-track="Discord" className="inline-flex min-h-11 items-center rounded-full px-3 text-muted transition hover:text-fg">{t("discord")}</a>}
        </nav>
        <span className="ml-auto hidden items-center gap-2 sm:flex">
          {/* the language, from sm up (the globe alone until lg, where the name fits beside it); below sm, the footer's list */}
          {others.length > 0 && <LanguageMenu locales={[DEFAULT_LOCALE, ...others]} current={locale} label={getT(locale, "footer").t("language")} />}
          <DownloadButton where="header" note={false} text={downloadText(locale)} className="inline-block rounded-xl bg-accent px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-strong" />
        </span>
      </div>
    </header>
  );
}
