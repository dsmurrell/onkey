import Link from "next/link";
import { Privacy as PrivacyDiagram } from "@/components/diagrams/Privacy";
import { diagramText } from "@/components/diagrams/text";
import { CONTACT_EMAIL, DATA_DIR, FLOWMYNA_TRACKER_ID, codeUrl } from "@/lib/site";
import { DEFAULT_LOCALE } from "@/lib/languages";
import { getT } from "@/lib/i18n";

export function privacyTitle(locale: string) {
  return getT(locale, "docs.privacy").t("title");
}

/** Privacy & legal: everything OnKey keeps and everything it sends, and the legal notes. A translation opens
 *  with a note that the English page applies where they differ. */
export function PrivacyPage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "docs.privacy");
  const d = diagramText(locale);
  return (
    <>
      <h1>{t("title")}</h1>
      {locale !== DEFAULT_LOCALE && (
        <p className="callout">{rich("translationNote", { link: (c) => <Link href="/docs/privacy/">{c}</Link> })}</p>
      )}
      <p className="lede">
        {t("lede")}
      </p>

      <figure className="wide -mx-4 overflow-hidden border-y border-line sm:mx-0 sm:rounded-2xl sm:border" style={{ maxWidth: "40rem" }}>
        <PrivacyDiagram all text={d.privacy} />
      </figure>

      <h2>{t("keeps.title")}</h2>
      <p>{rich("keeps.intro", { dataDir: DATA_DIR })}</p>
      <ul>
        <li>{rich("keeps.songs")}</li>
        <li>{rich("keeps.lyrics")}</li>
        <li>
          {rich("keeps.takes")}
        </li>
        <li>
          {rich("keeps.records")}
        </li>
        <li>{t("keeps.settings")}</li>
      </ul>
      <p>
        {t("keeps.remove")}
      </p>

      <h2>{t("never.title")}</h2>
      <p>
        {t("never.body")}
      </p>

      <h2>{t("leaves.title")}</h2>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t("leaves.sentTo")}</th><th>{t("leaves.what")}</th></tr></thead>
          <tbody>
            <tr>
              <td>{rich("leaves.shazam")}</td>
              <td>{rich("leaves.shazamWhat")}</td>
            </tr>
            <tr>
              <td><strong><a href="https://lrclib.net">LRCLIB</a></strong></td>
              <td>{t("leaves.lrclibWhat")}</td>
            </tr>
            <tr>
              <td>{rich("leaves.itunes")}</td>
              <td>{t("leaves.itunesWhat")}</td>
            </tr>
            <tr>
              <td>{rich("leaves.covers")}</td>
              <td>{t("leaves.coversWhat")}</td>
            </tr>
            <tr>
              <td><strong>GitHub</strong></td>
              <td>{rich("leaves.githubWhat")}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <p>
        {t("leaves.nothingElse")}
      </p>
      <p>
        {t("leaves.website")}
        {FLOWMYNA_TRACKER_ID && <> {t("leaves.flowMyna")}</>} {t("leaves.stars")}
      </p>

      <h2>{t("streaming.title")}</h2>
      <p>
        {t("streaming.noMusic")}
      </p>
      <p>
        {t("streaming.control")}
      </p>
      <p>
        {t("streaming.terms")}
      </p>

      <h2>{t("rights.title")}</h2>
      <p>
        {rich("rights.body", {
          address: CONTACT_EMAIL ?? t("rights.noAddress"),
          email: (c) => CONTACT_EMAIL ? <a href={`mailto:${CONTACT_EMAIL}`} data-track="Email">{c}</a> : <strong>{c}</strong>,
          legal: () => <a href={codeUrl("LEGAL.md")}>LEGAL.md</a>,
        })}
      </p>

      <h2>{t("trademarks.title")}</h2>
      <p>{t("trademarks.body")}</p>

      <h2>{t("licence.title")}</h2>
      <p>{t("licence.body")}</p>
    </>
  );
}
