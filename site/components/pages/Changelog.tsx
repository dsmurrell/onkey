import type { Metadata } from "next";
import { ENTRIES, RELEASES, slug, type Entry } from "@/lib/changelog";
import { GITHUB_URL } from "@/lib/site";
import { formatDay, getChangelogT } from "@/lib/i18n";

const KIND = {
  new: { cls: "text-accent-soft bg-accent-wash", dot: "bg-accent" },
  better: { cls: "text-music bg-[color-mix(in_srgb,var(--music)_14%,transparent)]", dot: "bg-music" },
  fix: { cls: "text-good bg-[color-mix(in_srgb,var(--good)_14%,transparent)]", dot: "bg-good" },
} as const;

/** The page's title and description in `locale`. */
export function changelogMetadata(locale: string): Metadata {
  const { t } = getChangelogT(locale);
  return { title: t("page.title"), description: t("page.description") };
}

/** What's new: every release, newest first, and the features and fixes it brought. The words come from the
 *  changelog's own catalog (lib/changelog.ts says how), dates from Intl in the page's language. */
export function ChangelogPage({ locale }: { locale: string }) {
  const { t, rich } = getChangelogT(locale);
  type Key = Parameters<typeof t>[0];
  const key = (k: string) => k as Key;   // a release's or entry's keys are made from its version or id
  const day = (iso: string) => formatDay(locale, iso);
  return (
    <div className="mx-auto max-w-4xl px-4 py-12 sm:px-6 md:py-16">
      <header className="max-w-2xl">
        <p className="text-xs font-medium uppercase tracking-wider text-accent-soft">{t("page.kicker")}</p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-fg sm:text-4xl">{t("page.heading")}</h1>
        <p className="mt-3 text-lg text-muted">
          {t("page.intro")}
        </p>
      </header>

      <div className="mt-12">
        {RELEASES.map((r, i) => {
          const entries = ENTRIES.filter((e) => e.version === r.version);
          const v = slug(r.version);
          return (
            <section key={r.version} id={`v${v}`} aria-labelledby={`h-v${v}`} className="relative">
              {/* the release: a marker on the timeline above the features it brought */}
              <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 pb-6 sm:grid sm:grid-cols-[120px_1fr] sm:gap-x-8">
                <span className="hidden sm:block" />
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <h2 id={`h-v${v}`} className="text-xl font-semibold tracking-tight text-fg">
                    <a href={`${GITHUB_URL}/releases/tag/v${r.version}`} className="hover:underline underline-offset-4">{t("page.release", { version: r.version })}</a>
                  </h2>
                  {i === 0 && <span className="rounded-full bg-accent px-2 py-0.5 text-[11px] font-medium text-white">{t("page.latest")}</span>}
                  <span className="tabular text-sm text-muted">{day(r.date)}</span>
                  <span className="w-full text-sm text-faint">
                    {t(key(`releases.${v}.note`))}
                    {r.link && <> · <a href={r.link.href} target="_blank" rel="noopener noreferrer" data-track={r.link.track} className="text-accent-soft underline underline-offset-4">{t(key(`releases.${v}.link`))}</a></>}
                  </span>
                </div>
              </div>
              <ol className="relative pb-12 sm:pl-0">
                {/* the timeline's rail */}
                <span aria-hidden className="absolute bottom-0 left-[5px] top-2 w-px bg-line sm:left-[calc(120px+2rem-1.5rem+5px)]" />
                {entries.map((e) => <Item key={e.id} e={e} locale={locale} />)}
              </ol>
            </section>
          );
        })}
      </div>

      <p className="mt-2 text-sm text-muted">
        {rich("page.github", { link: (c) => <a href={GITHUB_URL} data-track="GitHub" className="text-accent-soft underline underline-offset-4">{c}</a> })}
      </p>
    </div>
  );
}

function Item({ e, locale }: { e: Entry; locale: string }) {
  const { t } = getChangelogT(locale);
  type Key = Parameters<typeof t>[0];
  const key = (k: string) => `entries.${e.id}.${k}` as Key;
  const k = KIND[e.kind];
  const points: string[] = [];
  for (let i = 1; t.has(key(`p${i}`)); i++) points.push(t(key(`p${i}`)));
  return (
    <li id={e.id} className="relative pb-8 pl-7 last:pb-0 sm:grid sm:grid-cols-[120px_1fr] sm:gap-x-8 sm:pl-0">
      <time dateTime={e.date} className="tabular block pb-1 pt-0.5 text-sm text-faint sm:pb-0 sm:text-right">{formatDay(locale, e.date)}</time>
      <div className="relative">
        <span aria-hidden className={`absolute -left-7 top-2 h-[11px] w-[11px] rounded-full ring-4 ring-bg sm:-left-6 ${k.dot}`} />
        <div className="rounded-2xl border border-line bg-panel p-5">
          <div className="flex flex-wrap items-center gap-2">
            <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${k.cls}`}>{t(`page.kind.${e.kind}`)}</span>
            <h3 className="text-[17px] font-semibold tracking-tight text-fg">{t(key("title"))}</h3>
          </div>
          <p className="mt-2 leading-relaxed text-fg-soft">{t(key("summary"))}</p>
          {points.length > 0 && (
            <ul className="mt-3 grid gap-1.5 text-[15px] text-muted">
              {points.map((p) => (
                <li key={p} className="flex gap-2.5"><span aria-hidden className="mt-[0.6em] h-1 w-1 shrink-0 rounded-full bg-faint" />{p}</li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </li>
  );
}
