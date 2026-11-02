import { FaqList, firstQuestions, moreQuestions } from "@/components/Faq";
import { getT } from "@/lib/i18n";

export function faqTitle(locale: string) {
  return getT(locale, "faqPage").t("title");
}

export function FaqPage({ locale }: { locale: string }) {
  const { t } = getT(locale, "faqPage");
  return (
    <>
      <h1>{t("title")}</h1>
      <h2>{t("before")}</h2>
      <FaqList items={firstQuestions(locale)} />
      <h2>{t("using")}</h2>
      <FaqList items={moreQuestions(locale)} />
    </>
  );
}
