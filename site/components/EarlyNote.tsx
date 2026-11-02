import { isEarly, languageName } from "@/lib/languages";
import { getT } from "@/lib/i18n";

/** Where the site describes lyrics: in an early language, songs in that language show their lyrics a line at a
 *  time, without word timing, for now (lab/later/languages.md, decisions of 1 Nov 2026). Nothing otherwise. */
export function EarlyNote({ locale, className }: { locale: string; className?: string }) {
  if (!isEarly(locale)) return null;
  const { t } = getT(locale, "common");
  return <p className={className}>{t("early", { language: languageName(locale) })}</p>;
}
