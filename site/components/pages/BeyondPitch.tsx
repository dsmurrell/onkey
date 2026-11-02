import { GITHUB_URL } from "@/lib/site";
import { getT, pageLink } from "@/lib/i18n";

export function beyondPitchTitle(locale: string) {
  return getT(locale, "docs.beyondPitch").t("title");
}

/** Beyond pitch: what OnKey can't hear, and where to learn the rest. */
export function BeyondPitchPage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "docs.beyondPitch");
  const external = (href: string) => (c: React.ReactNode) => <a href={href}>{c}</a>;
  return (
    <>
      <h1>{t("title")}</h1>
      <p className="lede">
        {t("lede")}
      </p>

      <h2>{t("hear.title")}</h2>
      <p>
        {t("hear.body")}
      </p>

      <h2>{t("learn.title")}</h2>
      <ul>
        <li>{rich("learn.breath")}</li>
        <li>{rich("learn.tension")}</li>
        <li>{rich("learn.registers")}</li>
        <li>{rich("learn.vowels")}</li>
        <li>{rich("learn.phrasing")}</li>
      </ul>

      <h2>{t("teachers.title")}</h2>
      <ul>
        <li>
          {rich("teachers.callaway", { link: external("https://www.youtube.com/@dancallawaystudio") })}
        </li>
        <li>
          {rich("teachers.rolka", { link: external("https://www.youtube.com/@JeffRolka") })}
        </li>
      </ul>
      <p>
        {rich("teachers.suggested", { link: external(`${GITHUB_URL}/issues`) })}
      </p>

      <h2>{t("lessons.title")}</h2>
      <p>
        {t("lessons.body")}
      </p>

      <p className="text-sm">
        {rich("voice", { link: pageLink(locale, "/docs/your-voice/") })}
      </p>
    </>
  );
}
