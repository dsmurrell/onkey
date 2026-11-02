"use client";
import { learnPct, learnStage, type LearnJob } from "@/lib/session";
import { useT } from "@/lib/i18n";

/** How far a song's learning has got: what it's doing and a thin bar. The bar moves only when the engine reports
 *  progress (a few times a second at most): nothing animates while nothing changes, and nothing slides with
 *  reduced motion. While the song still plays, or waits to be learned behind another song, there's no bar, just the words. */
export default function LearnBar({ job, className = "", showPct = true }: { job: LearnJob; className?: string; showPct?: boolean }) {
  const t = useT();
  const pct = learnPct(job);
  return (
    <div className={className} role="progressbar" aria-label={t("learnBar.label", { name: job.name })} aria-valuemin={0} aria-valuemax={100}
         aria-valuenow={pct ?? undefined} aria-valuetext={pct == null ? learnStage(job) : t("learnBar.valueText", { stage: learnStage(job), pct })}>
      <div className="mb-1 flex items-baseline justify-between gap-2 text-xs">
        <span className="truncate text-ink-300">{learnStage(job)}</span>
        {showPct && pct != null && <span className="tabular text-ink-400">{pct}%</span>}
      </div>
      <div className="h-1 overflow-hidden rounded-full bg-white/8">
        {pct != null && <div className="h-full rounded-full bg-accent-400 transition-[width] duration-500 ease-out motion-reduce:transition-none"
                             style={{ width: `${Math.max(2, pct)}%` }} />}
      </div>
    </div>
  );
}
