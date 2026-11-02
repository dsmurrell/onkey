import { DATA_DIR, GITHUB_URL, WINDOWS_ISSUE_URL } from "@/lib/site";
import { getT, pageLink } from "@/lib/i18n";

export type QA = { id: string; q: string; a: React.ReactNode };

/** The questions a first-time reader asks before downloading. Shown on the home page and at the top of the FAQ. */
export function firstQuestions(locale: string): QA[] {
  const { t, rich } = getT(locale, "faq");
  const link = (path: string) => ({ link: pageLink(locale, path) });
  return [
    {
      id: "network",
      q: t("network.q"),
      a: (
        <>
          <p>{t("network.intro")}</p>
          <ul>
            <li>{rich("network.shazam")}</li>
            <li>{rich("network.lrclib")}</li>
            <li>{rich("network.apple")}</li>
            <li>{rich("network.github")}</li>
          </ul>
          <p>{rich("network.outro", link("/docs/privacy/"))}</p>
        </>
      ),
    },
    {
      id: "music",
      q: t("music.q"),
      a: <p>{t("music.a")}</p>,
    },
    {
      id: "kept",
      q: t("kept.q"),
      a: <p>{rich("kept.a", { dataDir: DATA_DIR })}</p>,
    },
    {
      id: "permissions",
      q: t("permissions.q"),
      a: (
        <ul>
          <li>{rich("permissions.mic")}</li>
          <li>{rich("permissions.audio")}</li>
          <li>{rich("permissions.control")}</li>
          <li>{rich("permissions.browser")}</li>
        </ul>
      ),
    },
    {
      id: "macs",
      q: t("macs.q"),
      a: <p>{t("macs.a")}</p>,
    },
    {
      id: "windows",
      q: t("windows.q"),
      a: <p>{rich("windows.a", { link: (c) => <a href={WINDOWS_ISSUE_URL} data-track="Windows issue">{c}</a> })}</p>,
    },
    {
      id: "browser",
      q: t("browser.q"),
      a: (
        <>
          <p>{t("browser.compare")}</p>
          <p>{t("browser.web")}</p>
        </>
      ),
    },
    {
      id: "scoring",
      q: t("scoring.q"),
      a: <p>{rich("scoring.a", link("/docs/scoring/"))}</p>,
    },
    {
      id: "licence",
      q: t("licence.q"),
      a: <p>{rich("licence.a", link("/docs/credits/"))}</p>,
    },
  ];
}

/** Everyday questions, after the first ones, on the FAQ page. */
export function moreQuestions(locale: string): QA[] {
  const { t, rich } = getT(locale, "faq");
  const plain = (id: string, key: "firstPlay" | "learning" | "starters" | "busy" | "headphones" | "octave" | "apps" | "shazam" | "lyricsOff" | "updates") =>
    ({ id, q: t(`${key}.q`), a: <p>{t(`${key}.a`)}</p> });
  return [
    plain("first-play", "firstPlay"),
    plain("learning", "learning"),
    plain("starters", "starters"),
    plain("busy", "busy"),
    plain("headphones", "headphones"),
    plain("octave", "octave"),
    plain("apps", "apps"),
    plain("shazam", "shazam"),
    plain("lyrics-off", "lyricsOff"),
    {
      id: "odd-scores",
      q: t("oddScores.q"),
      a: <p>{rich("oddScores.a", { link: pageLink(locale, "/docs/scoring/") })}</p>,
    },
    {
      id: "report",
      q: t("report.q"),
      a: (
        <p>
          {rich("report.a", { github: (c) => <a href={`${GITHUB_URL}/issues`}>{c}</a>, link: pageLink(locale, "/docs/troubleshooting/") })}
        </p>
      ),
    },
    plain("updates", "updates"),
  ];
}

/** A list of questions as open/close rows. */
export function FaqList({ items }: { items: QA[] }) {
  return (
    <div className="grid gap-3">
      {items.map(({ id, q, a }) => (
        <details key={id} id={`faq-${id}`} className="group scroll-mt-24 rounded-xl border border-line bg-panel [&_summary::-webkit-details-marker]:hidden">
          <summary className="flex min-h-12 cursor-pointer list-none items-center justify-between gap-4 px-4 py-3 font-medium text-fg">
            {q}
            <span aria-hidden="true" className="text-lg leading-none text-faint transition group-open:rotate-45 motion-reduce:transition-none">+</span>
          </summary>
          <div className="faq-answer px-4 pb-4 text-[15px] leading-relaxed text-fg-soft">{a}</div>
        </details>
      ))}
    </div>
  );
}
