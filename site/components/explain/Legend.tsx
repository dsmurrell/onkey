import { getT } from "@/lib/i18n";

/** The four colours every diagram uses, each with one meaning (named in docs.diagrams.legend). */
const ROLES = [
  { c: "var(--music)", key: "music" },
  { c: "var(--singer)", key: "singer" },
  { c: "var(--you)", key: "you" },
  { c: "var(--score)", key: "score" },
] as const;

export function Legend({ locale, compact = false }: { locale: string; compact?: boolean }) {
  const { t } = getT(locale, "docs.diagrams.legend");
  return (
    <ul className={`wide not-prose !mt-6 flex list-none flex-wrap gap-x-5 gap-y-2 !pl-0 text-[13px] ${compact ? "" : "rounded-xl border border-line bg-bg-raised px-4 py-3"}`} aria-label={t("label")}>
      {ROLES.map((r) => (
        <li key={r.key} className="!mt-0 flex items-center gap-2 text-muted">
          <span className="inline-block h-2.5 w-5 rounded-full" style={{ background: r.c }} aria-hidden="true" />
          <span><strong className="font-medium text-fg">{t(r.key)}</strong>{compact ? "" : <span className="hidden sm:inline">: {t(`${r.key}Note`)}</span>}</span>
        </li>
      ))}
    </ul>
  );
}
