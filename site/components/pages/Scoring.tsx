import { CoveragePlayground, CreditPlayground } from "@/components/diagrams/Scoring";
import { diagramText } from "@/components/diagrams/text";
import { getT } from "@/lib/i18n";

/** The scoring levels: full credit within (cents), none from (cents), timing slack (ms). Numbers, kept out of the
 *  translations (README: keep them in step with the latest measurements), like the results below. */
const LEVELS = [["relaxed", 50, 150, 80], ["steady", 35, 100, 60], ["pro", 25, 70, 50]] as const;
const REPORT = ["score", "accuracy", "notesHit", "sangAlong", "tendency", "latency"] as const;
const CHECKS = [["covered", "98%"], ["rightNote", "92%"], ["starts", "73%"], ["noNote", "3%"], ["lyricLines", "91–94%"]] as const;
const EXPERTS = [["expertsEachOther", "0.88"], ["notesHit", "0.77"], ["score", "0.61"]] as const;

export function scoringTitle(locale: string) {
  return getT(locale, "docs.scoring").t("title");
}

/** Scoring explained: the rules, the levels, the report's numbers, how they're checked, and the limits. */
export function ScoringPage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "docs.scoring");
  const d = diagramText(locale);
  return (
    <>
      <h1>{t("title")}</h1>
      <p className="lede">{t("lede")}</p>

      <h2>{t("rules.title")}</h2>
      <ul>
        <li>{rich("rules.every")}</li>
        <li>{rich("rules.credit")}</li>
        <li>{rich("rules.octaves")}</li>
        <li>{rich("rules.slack")}</li>
        <li>{rich("rules.delay")}</li>
      </ul>
      <p>
        {rich("rules.relaxed")}
      </p>

      <h2>{t("levels.title")}</h2>
      <p>
        {t("levels.intro")}
      </p>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t("levels.level")}</th><th className="num">{t("levels.fullWithin")}</th><th className="num">{t("levels.noneFrom")}</th><th className="num">{t("levels.slack")}</th></tr></thead>
          <tbody>
            {LEVELS.map(([key, full, none, slack]) => (
              <tr key={key}><td>{rich(`levels.${key}`)}</td><td className="num">{t("levels.cents", { n: full })}</td><td className="num">{t("levels.cents", { n: none })}</td><td className="num">{t("levels.ms", { n: slack })}</td></tr>
            ))}
          </tbody>
        </table>
      </div>
      <p>
        {rich("levels.vibrato")}
      </p>

      <CreditPlayground locale={locale} text={d.credit} />

      <h2>{t("report.title")}</h2>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t("report.number")}</th><th>{t("report.meaning")}</th></tr></thead>
          <tbody>
            {REPORT.map((key) => (
              <tr key={key}><td>{rich(`report.${key}`)}</td><td>{t(`report.${key}Meaning`)}</td></tr>
            ))}
          </tbody>
        </table>
      </div>

      <h3>{t("report.sangTitle")}</h3>
      <p>
        {t("report.sangBody")}
      </p>
      <CoveragePlayground locale={locale} text={d.coverage} />

      <h2>{t("checks.title")}</h2>
      <p>
        {t("checks.intro")}
      </p>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t("checks.check")}</th><th className="num">{t("checks.result")}</th></tr></thead>
          <tbody>
            {CHECKS.map(([key, result]) => (
              <tr key={key}><td>{t(`checks.${key}`)}</td><td className="num">{result}</td></tr>
            ))}
          </tbody>
        </table>
      </div>

      <h3>{t("checks.expertsTitle")}</h3>
      <p>
        {t("checks.expertsIntro")}
      </p>
      <div className="table-wrap">
        <table>
          <thead><tr><th>{t("checks.measure")}</th><th className="num">{t("checks.agreement")}</th></tr></thead>
          <tbody>
            {EXPERTS.map(([key, result]) => (
              <tr key={key}><td>{t(`checks.${key}`)}</td><td className="num">{result}</td></tr>
            ))}
          </tbody>
        </table>
      </div>
      <p>
        {t("checks.expertsOutro")}
      </p>

      <h2>{t("limits.title")}</h2>
      <ul>
        <li>{rich("limits.rap")}</li>
        <li>{rich("limits.harmonies")}</li>
        <li>{rich("limits.speakers")}</li>
        <li>{rich("limits.firstPlay")}</li>
      </ul>
    </>
  );
}
