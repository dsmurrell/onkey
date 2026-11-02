"use client";
// "Pick up where you left off": the last few songs you sang, each with its last score and a Sing button (as
// History's "Sing it again"). Shows nothing until you've sung something.
import { useEffect, useState } from "react";
import Cover from "@/components/Cover";
import { useT } from "@/lib/i18n";
import { getTakes, learnLabel, playTrack, useLearn, type TakeSummary } from "@/lib/session";

export default function RecentSongs({ count = 3, className = "" }: { count?: number; className?: string }) {
  const t = useT();
  const [recent, setRecent] = useState<TakeSummary[]>([]);
  useEffect(() => {
    let gone = false;
    getTakes().then(ts => {
      if (gone) return;
      // each song once, by its latest take
      const seen = new Set<string>(), out: TakeSummary[] = [];
      for (const tk of [...ts].sort((a, b) => b.recorded.localeCompare(a.recorded))) {
        if (!tk.track?.id || seen.has(tk.track.id)) continue;
        seen.add(tk.track.id); out.push(tk);
        if (out.length === count) break;
      }
      setRecent(out);
    }).catch(() => {});
    return () => { gone = true; };
  }, [count]);
  if (!recent.length) return null;
  return (
    <section aria-label={t("recent.title")} className={`w-full text-left ${className}`}>
      <div className="mb-2 px-1 text-xs font-medium text-ink-300">{t("recent.title")}</div>
      <ul className="grid gap-2 sm:grid-cols-3">
        {recent.map(tk => <Recent key={tk.track.id} take={tk} />)}
      </ul>
    </section>
  );
}

function Recent({ take: tk }: { take: TakeSummary }) {
  const t = useT();
  const job = useLearn(tk.track);     // still being learned: it can be sung again once that's done
  const [busy, setBusy] = useState(false);
  const sing = async () => {
    setBusy(true);
    try { await playTrack(tk.track.id, true, tk.track.name); } catch {} finally { setBusy(false); }
  };
  return (
    <li className="glass flex items-center gap-3 rounded-2xl p-3">
      <Cover name={tk.track.name} art={tk.track.art} link={tk.track.link} size={44} />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm font-medium text-ink-100" title={tk.track.name}>{tk.track.name}</span>
        <span className="truncate text-xs text-ink-400" title={tk.track.artist}>{tk.track.artist}</span>
        <span className="mt-0.5 text-[11px] text-ink-500">{t.rich("recent.last", { score: tk.score, n: c => <span className="tabular font-semibold text-ink-200">{c}</span> })}</span>
      </span>
      <button onClick={sing} disabled={!!job || busy} title={job ? t("recent.whenLearned") : t("recent.singNamed", { name: tk.track.name })}
              className="flex shrink-0 items-center gap-1 rounded-full bg-white/6 px-3 py-1.5 text-xs font-medium text-ink-100 transition hover:bg-accent-500/25 hover:text-white
                         disabled:cursor-default disabled:text-ink-400 disabled:hover:bg-white/6">
        {job ? learnLabel(job) : busy ? t("recent.starting") : <>{t("recent.sing")}<svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor" aria-hidden><path d="M8 5.5v13l11-6.5z" /></svg></>}
      </button>
    </li>
  );
}
