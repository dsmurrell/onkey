import { Legend } from "@/components/explain/Legend";
import { HowSections } from "@/components/docs/HowSections";
import { getT } from "@/lib/i18n";

export function howItWorksTitle(locale: string) {
  return getT(locale, "docs.howItWorks").t("title");
}

/** How OnKey works: seven sections of diagrams and plain explanation, then the exact numbers. */
export function HowItWorksPage({ locale }: { locale: string }) {
  const { t } = getT(locale, "docs.howItWorks");
  return (
    <>
      <h1>{t("title")}</h1>
      <p className="lede">
        {t("lede")}
      </p>
      <Legend locale={locale} />
      <HowSections locale={locale} />
    </>
  );
}
