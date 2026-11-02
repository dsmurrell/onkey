import Link from "next/link";
import { LogoMark } from "./Logo";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { GITHUB_URL, DISCORD_URL } from "@/lib/site";
import { DEFAULT_LOCALE, localeHref } from "@/lib/languages";
import { getT, otherLocales } from "@/lib/i18n";

export function Footer({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "footer");
  const others = otherLocales();
  const href = (path: string) => localeHref(locale, path);
  return (
    <footer className="border-t border-line">
      <div className="mx-auto grid max-w-6xl gap-8 px-4 py-10 text-sm text-muted sm:px-6 md:grid-cols-[1fr_auto]">
        <div className="grid max-w-xl gap-3">
          <div className="flex items-center gap-2.5 text-fg">
            <LogoMark size={24} />
            <span className="font-semibold tracking-tight">OnKey</span>
            <span className="text-muted">{t("license")}</span>
          </div>
          <p className="text-xs italic text-muted">{getT(locale, "common").t("joke")}</p>
          <p className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted">
            <img src="/dsmurrell.png" alt="" width={16} height={16} className="rounded-sm" />
            {rich("madeBy", { link: (c) => <a href="https://dsmurrell.com" data-track="Daniel's site" className="text-fg underline-offset-2 hover:underline">{c}</a> })}
          </p>
          <p className="text-xs leading-relaxed text-faint">{t("notAffiliated")}</p>
          {locale !== DEFAULT_LOCALE && (
            <p className="text-xs"><a href={`${GITHUB_URL}/edit/main/site/messages/${locale}.json`} target="_blank" rel="noopener" data-track="GitHub" className="underline underline-offset-2 hover:text-fg">{t("improve")}</a></p>
          )}
        </div>
        <div className="flex flex-col gap-4 md:items-end md:justify-between">
        <nav className="flex flex-wrap gap-x-6 gap-y-2 md:justify-end" aria-label={t("label")}>
          <Link href={href("/docs/")} className="inline-block py-1.5 hover:text-fg">{t("gettingStarted")}</Link>
          <Link href={href("/docs/how-it-works/")} className="inline-block py-1.5 hover:text-fg">{t("howItWorks")}</Link>
          <Link href={href("/docs/privacy/")} className="inline-block py-1.5 hover:text-fg">{t("privacy")}</Link>
          <Link href={href("/docs/faq/")} className="inline-block py-1.5 hover:text-fg">{t("faq")}</Link>
          <Link href={href("/changelog/")} className="inline-block py-1.5 hover:text-fg">{t("whatsNew")}</Link>
          <a href={GITHUB_URL} data-track="GitHub" className="inline-block py-1.5 hover:text-fg">{t("github")}</a>
          {DISCORD_URL && <a href={DISCORD_URL} data-track="Discord" className="inline-block py-1.5 hover:text-fg">{t("discord")}</a>}
        </nav>
        {/* the languages, at the bottom right, level with the left column's last line: two lines at desktop width */}
        {others.length > 0 && <LanguageSwitcher locales={[DEFAULT_LOCALE, ...others]} current={locale} label={t("language")} />}
        </div>
      </div>
    </footer>
  );
}
