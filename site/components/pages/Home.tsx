import Link from "next/link";
import { DownloadButton } from "@/components/DownloadButton";
import { LaneArt } from "@/components/LaneArt";
import { DemoVideo } from "@/components/DemoVideo";
import { GitHubStars } from "@/components/GitHubStars";
import { FaqList, firstQuestions } from "@/components/Faq";
import { EarlyNote } from "@/components/EarlyNote";
import { GITHUB_URL, LAUNCH, asset, DISCORD_URL } from "@/lib/site";
import { DEFAULT_LOCALE, languageName, localeHref } from "@/lib/languages";
import { downloadText, getT, otherLocales, pageLink, requirements } from "@/lib/i18n";

const FACTS = ["free", "macs", "local", "music"] as const;
const STEPS = ["play", "learn", "sing", "review"] as const;
const CARDS = ["network", "fair", "headphones"] as const;
/** The languages "Available in …" names, each in its own (without a region: "Español"); the rest are counted ("and 15
 *  more", a link to the footer's list). */
const NAMED_LANGUAGES = ["en", "es", "pt", "ja"];

const btn = "inline-flex min-h-12 items-center justify-center gap-2 rounded-xl px-5 font-medium transition";
const primary = `${btn} bg-accent text-white shadow-lg shadow-accent/25 hover:bg-accent-strong`;
const secondary = `${btn} border border-line text-fg hover:bg-panel`;

function GitHubMark() {
  return (
    <svg viewBox="0 0 16 16" width="18" height="18" aria-hidden="true" fill="currentColor">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
    </svg>
  );
}

function DiscordMark() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="currentColor">
      <path d="M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z" />
    </svg>
  );
}

export function HomePage({ locale }: { locale: string }) {
  const { t, rich } = getT(locale, "home");
  const common = getT(locale, "common").t;
  const stars: string = common.raw("stars");   // an ICU plural on {n}: formatted in the browser once the count is known
  const download = downloadText(locale);
  const href = (path: string) => localeHref(locale, path);
  const shipped = [DEFAULT_LOCALE, ...otherLocales()];
  const named = NAMED_LANGUAGES.filter((l) => shipped.includes(l));
  return (
    <>
      <section className="relative overflow-hidden">
        <div aria-hidden="true" className="pointer-events-none absolute inset-x-0 -top-40 h-[480px] bg-[radial-gradient(700px_300px_at_50%_30%,var(--accent-wash),transparent)]" />
        <div className="relative mx-auto grid max-w-6xl gap-12 px-4 pb-16 pt-10 sm:px-6 md:pt-20 lg:grid-cols-[1.05fr_1fr] lg:items-center">
          <div className="rise grid gap-6">
            <p className="text-sm font-medium text-accent-soft">{t("kicker")}</p>
            <h1 className="text-[2.15rem] font-semibold leading-[1.1] tracking-tight text-fg sm:text-5xl sm:leading-[1.08]">
              {t("title")}
            </h1>
            <p className="-mt-3 text-balance text-base italic text-muted">{common("joke")}</p>
            <p className="max-w-[54ch] text-lg leading-relaxed text-muted">
              {t("intro")}
            </p>
            {shipped.length > named.length && (
              <p className="-mt-2 flex items-start gap-2 text-sm text-muted">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className="mt-0.5 shrink-0">
                  <circle cx="12" cy="12" r="9" />
                  <path d="M3 12h18M12 3c3 3.3 3 14.7 0 18M12 3c-3 3.3-3 14.7 0 18" />
                </svg>
                <span>
                  {rich("languages", {
                    names: named.map((l) => languageName(l).replace(/\s*\(.*\)$/, "")).join(/^(ja|zh)/.test(locale) ? "、" : ", "),
                    n: shipped.length - named.length,
                    more: (c) => <a href="#languages" className="text-fg underline underline-offset-4 decoration-line hover:decoration-fg">{c}</a>,
                  })}
                </span>
              </p>
            )}
            <div className="flex flex-wrap items-center gap-3">
              <DownloadButton where="top" text={download} className={primary} />
              <a href={GITHUB_URL} data-track="GitHub" className={secondary}><GitHubMark />{common("sourceOnGitHub")}<GitHubStars label={stars} locale={locale} /></a>
              <Link href={href("/docs/how-it-works/")} className="inline-flex min-h-12 items-center px-2 font-medium text-accent-soft underline-offset-4 hover:underline">
                {t("howLink")}
              </Link>
            </div>
            <ul className="grid gap-x-6 gap-y-1.5 text-sm text-muted sm:grid-cols-2" aria-label={t("factsLabel")}>
              {FACTS.map((f) => (
                <li key={f} className="flex items-start gap-2">
                  <span aria-hidden="true" className="mt-[0.45rem] h-1.5 w-1.5 shrink-0 rounded-full bg-good" />
                  {t(`facts.${f}`)}
                </li>
              ))}
            </ul>
          </div>

          <figure className="rise grid gap-3" style={{ animationDelay: "120ms" }}>
            {LAUNCH.demoVideo ? (
              // the video is the app's own window: no mock window around it
              <div className="overflow-hidden rounded-2xl border border-line bg-black shadow-2xl shadow-black/30">
                <DemoVideo src={asset(LAUNCH.demoVideo)} poster={LAUNCH.demoPoster ? asset(LAUNCH.demoPoster) : undefined} text={{ label: t("video.label"), video: t("video.video"), play: t("video.play"), dialog: t("video.dialog"), close: t("video.close") }} />
              </div>
            ) : (
              <div className="overflow-hidden rounded-2xl border border-line bg-panel shadow-2xl shadow-black/20">
                <div className="flex items-center gap-2 border-b border-line px-4 py-2.5">
                  <span className="h-2.5 w-2.5 rounded-full bg-[#ff5f57]" /><span className="h-2.5 w-2.5 rounded-full bg-[#febc2e]" /><span className="h-2.5 w-2.5 rounded-full bg-[#28c840]" />
                  <span className="ml-3 truncate text-xs text-muted">{t("laneTitle")}</span>
                </div>
                <LaneArt />
              </div>
            )}
            <figcaption className="text-center text-xs text-muted">
              {LAUNCH.demoVideo ? t("videoCaption") : t("artCaption")}
            </figcaption>
          </figure>
        </div>
      </section>

      <section className="border-t border-line bg-bg-raised" aria-labelledby="how">
        <div className="mx-auto max-w-6xl px-4 py-16 sm:px-6">
          <h2 id="how" className="text-2xl font-semibold tracking-tight text-fg">{t("how.title")}</h2>
          <ol className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {STEPS.map((s, i) => (
              <li key={s} className="grid content-start gap-2 rounded-2xl border border-line bg-panel p-5">
                <span className="tabular text-base font-medium text-accent-soft">{t("how.step", { n: i + 1 })}</span>
                <h3 className="text-lg font-semibold text-fg">{t(`how.steps.${s}.title`)}</h3>
                <p className="text-base leading-relaxed text-muted">{t(`how.steps.${s}.body`)}</p>
              </li>
            ))}
          </ol>
          <p className="mt-6 text-base text-muted">
            {rich("how.more", { link: pageLink(locale, "/docs/how-it-works/", "text-accent-soft underline underline-offset-4") })}
          </p>
          <EarlyNote locale={locale} className="mt-3 text-sm text-muted" />
        </div>
      </section>

      <section className="border-t border-line" aria-labelledby="privacy">
        <div className="mx-auto grid max-w-6xl gap-10 px-4 py-16 sm:px-6 md:grid-cols-2">
          <div className="grid content-start gap-4">
            <h2 id="privacy" className="text-2xl font-semibold tracking-tight text-fg">{t("privacy.title")}</h2>
            <p className="leading-relaxed text-muted">
              {t("privacy.body")}
            </p>
            <Link href={href("/docs/privacy/")} className="text-sm text-accent-soft underline underline-offset-4">{t("privacy.link")}</Link>
          </div>
          <ul className="grid content-start gap-3">
            {CARDS.map((c) => (
              <li key={c} className="rounded-2xl border border-line bg-panel p-4">
                <p className="font-medium text-fg">{t(`privacy.cards.${c}.title`)}</p>
                <p className="mt-1 text-sm leading-relaxed text-muted">{t(`privacy.cards.${c}.body`)}</p>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section className="border-t border-line bg-bg-raised" aria-labelledby="faq">
        <div className="mx-auto grid max-w-6xl gap-8 px-4 py-16 sm:px-6 md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
          <div className="grid content-start gap-3">
            <h2 id="faq" className="text-2xl font-semibold tracking-tight text-fg">{t("questions.title")}</h2>
            <p className="text-muted">{rich("questions.body", { link: pageLink(locale, "/docs/faq/", "text-accent-soft underline underline-offset-4") })}</p>
          </div>
          <FaqList items={firstQuestions(locale)} />
        </div>
      </section>

      <section className="border-t border-line" aria-labelledby="get">
        <div className="mx-auto grid max-w-6xl gap-6 px-4 py-16 sm:px-6 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center lg:gap-10">
          <div className="grid gap-2">
            <h2 id="get" className="text-2xl font-semibold tracking-tight text-fg">{t("get.title")}</h2>
            <p className="text-muted">{t("get.body", { requirements: requirements(locale).join(", ") })}</p>
          </div>
          {/* on a phone the three buttons stack at one width; side by side from sm up */}
          <div className="grid gap-3 sm:flex sm:flex-wrap">
            <DownloadButton where="bottom" text={download} className={primary} />
            <a href={GITHUB_URL} data-track="GitHub" className={secondary}><GitHubMark />{common("sourceOnGitHub")}<GitHubStars label={stars} locale={locale} /></a>
            {DISCORD_URL && <a href={DISCORD_URL} data-track="Discord" className={secondary}><DiscordMark />{t("get.discord")}</a>}
          </div>
        </div>
      </section>
    </>
  );
}
