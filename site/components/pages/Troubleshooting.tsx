import { GITHUB_URL, DISCORD_URL } from "@/lib/site";
import { getT, pageLink } from "@/lib/i18n";
import { EarlyNote } from "@/components/EarlyNote";

export function troubleshootingTitle(locale: string) {
  return getT(locale, "troubleshooting").t("title");
}

export function TroubleshootingPage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "troubleshooting");
  return (
    <>
      <h1>{t("title")}</h1>
      <p className="lede">{t("lede")}</p>

      <h2>{t("starting.title")}</h2>
      <p>
        {rich("starting.body", { link: (c) => <a href="#report">{c}</a> })}
      </p>

      <h2>{t("permissions.title")}</h2>
      <p>{t("permissions.intro")}</p>
      <ul>
        <li>{rich("permissions.mic")}</li>
        <li>
          {rich("permissions.audio")}
        </li>
        <li>
          {rich("permissions.control")}
        </li>
      </ul>
      <p>
        {t("permissions.buttons")}
      </p>

      <h2>{t("naming.title")}</h2>
      <p>
        {t("naming.body")}
      </p>

      <h2>{t("noNotes.title")}</h2>
      <p>
        {t("noNotes.body")}
      </p>

      <h2>{t("dim.title")}</h2>
      <p>
        {t("dim.body")}
      </p>

      <h2>{t("wholeLine.title")}</h2>
      <p>
        {t("wholeLine.body")}
      </p>

      <h2 id="timing">{t("timing.title")}</h2>
      <p>
        {t("timing.latency")}
      </p>
      <p>
        {rich("timing.still", { link: (c) => <a href={`${GITHUB_URL}/issues`} data-track="GitHub">{c}</a> })}
      </p>

      <h2>{t("lyrics.title")}</h2>
      <p>
        {rich("lyrics.body", { link: (c) => <a href="https://lrclib.net">{c}</a> })}
      </p>
      <EarlyNote locale={locale} />

      <h2>{t("busy.title")}</h2>
      <p>
        {t("busy.intro")}
      </p>
      <ul>
        <li>{rich("busy.battery")}</li>
        <li>{rich("busy.models")}</li>
      </ul>

      <h2>{t("logs.title")}</h2>
      <p>{rich("logs.where")}</p>
      <pre><code>open $TMPDIR/onkey.log</code></pre>
      <p>
        {rich("logs.records")}
      </p>

      <h2 id="report">{t("report.title")}</h2>
      <ol>
        <li>
          {rich("report.choose")}
        </li>
        <li>
          {rich("report.zip")}
        </li>
        <li>
          {rich("report.issue", { link: (c) => <a href={`${GITHUB_URL}/issues`}>{c}</a> })}
        </li>
      </ol>
      <p>
        {rich("more", { link: pageLink(locale, "/docs/faq/") })}{DISCORD_URL && <> {rich("discord", { link: (c) => <a href={DISCORD_URL!} data-track="Discord">{c}</a> })}</>}
      </p>
    </>
  );
}
