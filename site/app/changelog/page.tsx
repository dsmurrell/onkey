import type { Metadata } from "next";
import { ENTRIES, RELEASES, type Entry } from "@/lib/changelog";
import { GITHUB_URL } from "@/lib/site";

export const metadata: Metadata = {
  title: "What's new",
  description: "Every OnKey feature and fix, newest first, with the release that brought it.",
};

const KIND = {
  new: { label: "New", cls: "text-accent-soft bg-accent-wash", dot: "bg-accent" },
  better: { label: "Better", cls: "text-music bg-[color-mix(in_srgb,var(--music)_14%,transparent)]", dot: "bg-music" },
  fix: { label: "Fixed", cls: "text-good bg-[color-mix(in_srgb,var(--good)_14%,transparent)]", dot: "bg-good" },
} as const;

const day = (iso: string) =>
  new Date(`${iso}T12:00:00Z`).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });

const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "");

export default function Changelog() {
  return (
    <div className="mx-auto max-w-4xl px-4 py-12 sm:px-6 md:py-16">
      <header className="max-w-2xl">
        <p className="text-xs font-medium uppercase tracking-wider text-accent-soft">Changelog</p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-fg sm:text-4xl">What&rsquo;s new in OnKey</h1>
        <p className="mt-3 text-lg text-muted">
          Every feature and fix, newest first, and the release that brought it. OnKey updates itself, so these reach
          you on their own.
        </p>
      </header>

      <div className="mt-12">
        {RELEASES.map((r, i) => {
          const entries = ENTRIES.filter((e) => e.version === r.version);
          return (
            <section key={r.version} id={`v${slug(r.version)}`} aria-labelledby={`h-v${slug(r.version)}`} className="relative">
              {/* the release: a marker on the timeline above the features it brought */}
              <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 pb-6 sm:grid sm:grid-cols-[120px_1fr] sm:gap-x-8">
                <span className="hidden sm:block" />
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <h2 id={`h-v${slug(r.version)}`} className="text-xl font-semibold tracking-tight text-fg">
                    <a href={`${GITHUB_URL}/releases/tag/v${r.version}`} className="hover:underline underline-offset-4">OnKey {r.version}</a>
                  </h2>
                  {i === 0 && <span className="rounded-full bg-accent px-2 py-0.5 text-[11px] font-medium text-white">Latest</span>}
                  <span className="tabular text-sm text-muted">{day(r.date)}</span>
                  <span className="w-full text-sm text-faint">
                    {r.note}
                    {r.link && <> · <a href={r.link.href} target="_blank" rel="noopener noreferrer" data-track={r.link.label} className="text-accent-soft underline underline-offset-4">{r.link.label}</a></>}
                  </span>
                </div>
              </div>
              <ol className="relative pb-12 sm:pl-0">
                {/* the timeline's rail */}
                <span aria-hidden className="absolute bottom-0 left-[5px] top-2 w-px bg-line sm:left-[calc(120px+2rem-1.5rem+5px)]" />
                {entries.map((e) => <Item key={e.title} e={e} />)}
              </ol>
            </section>
          );
        })}
      </div>

      <p className="mt-2 text-sm text-muted">
        Every change is on <a href={GITHUB_URL} data-track="GitHub" className="text-accent-soft underline underline-offset-4">GitHub</a>,
        where OnKey is open source.
      </p>
    </div>
  );
}

function Item({ e }: { e: Entry }) {
  const k = KIND[e.kind ?? "new"];
  return (
    <li id={slug(e.title)} className="relative pb-8 pl-7 last:pb-0 sm:grid sm:grid-cols-[120px_1fr] sm:gap-x-8 sm:pl-0">
      <time dateTime={e.date} className="tabular block pb-1 pt-0.5 text-sm text-faint sm:pb-0 sm:text-right">{day(e.date)}</time>
      <div className="relative">
        <span aria-hidden className={`absolute -left-7 top-2 h-[11px] w-[11px] rounded-full ring-4 ring-bg sm:-left-6 ${k.dot}`} />
        <div className="rounded-2xl border border-line bg-panel p-5">
          <div className="flex flex-wrap items-center gap-2">
            <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${k.cls}`}>{k.label}</span>
            <h3 className="text-[17px] font-semibold tracking-tight text-fg">{e.title}</h3>
          </div>
          <p className="mt-2 leading-relaxed text-fg-soft">{e.summary}</p>
          {e.points && (
            <ul className="mt-3 grid gap-1.5 text-[15px] text-muted">
              {e.points.map((p) => (
                <li key={p} className="flex gap-2.5"><span aria-hidden className="mt-[0.6em] h-1 w-1 shrink-0 rounded-full bg-faint" />{p}</li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </li>
  );
}
