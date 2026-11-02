import { DOWNLOAD_URL, LAUNCH } from "@/lib/site";
import { getT, pageLink, requirements } from "@/lib/i18n";
import { EarlyNote } from "@/components/EarlyNote";

export function startTitle(locale: string) {
  return getT(locale, "start").t("title");
}

/** Getting started: what's needed, the download and install, permissions, and the first song. */
export function GettingStartedPage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "start");
  const link = (path: string) => pageLink(locale, path);
  return (
    <>
      <h1>{t("title")}</h1>
      <p className="lede">{t("lede")}</p>

      <h2>{t("need.title")}</h2>
      <ul>
        {requirements(locale).map((r) => <li key={r}>{r}</li>)}
        <li>{t("need.music")}</li>
        <li>{t("need.mic")}</li>
      </ul>
      <p>
        {t("need.why")}
      </p>

      <h2>{t("install.title")}</h2>
      <ol>
        <li>
          {rich("install.download", { link: (c) => <a href={DOWNLOAD_URL} data-track="Download for Mac">{c}</a> })}
        </li>
        <li>{t("install.drag")}</li>
        {LAUNCH.notarised ? (
          <li>{t("install.open")}</li>
        ) : (
          <li>
            {rich("install.openAnyway")}
          </li>
        )}
      </ol>
      <p>
        {rich("install.updates")}
      </p>

      <h2>{t("permissions.title")}</h2>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t("permissions.permission")}</th><th>{t("permissions.why")}</th></tr></thead>
          <tbody>
            <tr><td>{rich("permissions.mic")}</td><td>{t("permissions.micWhy")}</td></tr>
            <tr><td>{rich("permissions.audio")}</td><td>{t("permissions.audioWhy")}</td></tr>
            <tr><td>{rich("permissions.control")}</td><td>{t("permissions.controlWhy")}</td></tr>
            <tr><td>{rich("permissions.browser")}</td><td>{t("permissions.browserWhy")}</td></tr>
          </tbody>
        </table>
      </div>
      <p>
        {rich("permissions.after")}
      </p>

      <h2>{t("first.title")}</h2>
      <ol>
        <li>
          {t("first.play")}
        </li>
        <li>
          {t("first.lyrics")}
        </li>
        <li>
          {rich("first.learns")}
        </li>
        <li>
          {t("first.again")}
        </li>
        <li>
          {t("first.score")}
        </li>
      </ol>
      <EarlyNote locale={locale} className="callout" />
      <p className="callout">
        {t("first.learned")}
      </p>
      <p>
        {rich("first.rest", { link: link("/docs/your-voice/") })}
      </p>
      <p>
        {rich("first.next", { how: link("/docs/how-it-works/"), scoring: link("/docs/scoring/"), troubleshooting: link("/docs/troubleshooting/") })}
      </p>
    </>
  );
}
