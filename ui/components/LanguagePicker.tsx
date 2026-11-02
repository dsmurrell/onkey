"use client";
// Settings → Language: the list (components/LanguageSelect.tsx, shared with first-run setup: Automatic, the
// computer's language named, or a language there's a catalog for, each in its own name), shown only once there's a
// translation (lib/languages.ts `pickerCodes`). An Early language says what's not there yet and links to its catalog
// on GitHub; under the picker, who translated the language shown (ui/messages/CREDITS.json). Kept across restarts by
// settings.rs ("language").
import LanguageSelect from "@/components/LanguageSelect";
import { locales, useLocale, useT } from "@/lib/i18n";
import { isEarly, nativeName, pickerCodes, translationUrl, type Credits } from "@/lib/languages";
import { openLink } from "@/lib/session";
import credits from "@/messages/CREDITS.json" with { type: "json" };

/** A language's name in the language shown ("Japanese" in English, "Japonés" in Spanish); its own name if the
 *  system can't say. */
function nameIn(locale: string, code: string): string {
  try { return new Intl.DisplayNames([locale], { type: "language" }).of(code) ?? nativeName(code); } catch { return nativeName(code); }
}

export default function LanguagePicker({ choice, onChange }: { choice: string; onChange: (code: string) => void }) {
  const t = useT();
  const shown = useLocale();
  if (!pickerCodes(locales())) return null;
  const by = (credits as Credits)[shown] ?? [];
  let names = by.join(", ");
  try { names = new Intl.ListFormat(shown, { type: "conjunction" }).format(by); } catch {}
  return (
    <>
      <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.language.title")}</h2>
      <section className="glass overflow-hidden rounded-3xl">
        <div className="flex items-start justify-between gap-6 px-5 py-4">
          <div className="min-w-0">
            <label htmlFor="set-language" className="flex items-center gap-2 text-sm font-medium text-ink-100">
              {t("settings.language.label")}
              {isEarly(shown) && <span className="rounded-full bg-white/6 px-1.5 py-px text-[10px] font-normal text-ink-300">{t("settings.language.early")}</span>}
            </label>
            <p className="mt-1 text-xs text-ink-400">{t("settings.language.sub")}</p>
            {isEarly(shown) && (
              <p className="mt-2 text-xs text-ink-400">
                {t.rich("settings.language.earlyNote", {
                  language: nameIn(shown, shown),
                  link: c => <a href={translationUrl(shown)} onClick={e => { e.preventDefault(); void openLink(translationUrl(shown)); }}
                                className="text-accent-300 underline-offset-2 hover:underline">{c}</a>,
                })}
              </p>
            )}
            {shown !== "en" && by.length > 0 && <p className="mt-2 text-xs text-ink-500">{t("settings.language.credits", { names })}</p>}
          </div>
          <LanguageSelect id="set-language" choice={choice} onChange={onChange} />
        </div>
      </section>
    </>
  );
}
