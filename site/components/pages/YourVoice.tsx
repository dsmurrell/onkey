import { getT, pageLink } from "@/lib/i18n";

export function yourVoiceTitle(locale: string) {
  return getT(locale, "docs.yourVoice").t("title");
}

/** Singing comfortably: looking after your voice. */
export function YourVoicePage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "docs.yourVoice");
  return (
    <>
      <h1>{t("title")}</h1>
      <p className="lede">{t("lede")}</p>

      <h2>{t("volume.title")}</h2>
      <p>
        {t("volume.body")}
      </p>

      <h2>{t("octave.title")}</h2>
      <p>
        {t("octave.body")}
      </p>

      <h2>{t("warmUp.title")}</h2>
      <ul>
        <li>{t("warmUp.easy")}</li>
        <li>{t("warmUp.water")}</li>
        <li>
          {rich("warmUp.rest")}
        </li>
        <li>{t("warmUp.gradually")}</li>
      </ul>

      <h2>{t("pain.title")}</h2>
      <p>
        {t("pain.body")}
      </p>

      <p>
        {rich("technique", { link: pageLink(locale, "/docs/beyond-pitch/") })}
      </p>

      <p className="text-sm">{t("disclaimer")}</p>
    </>
  );
}
