"use client";
import { useEffect, useState } from "react";
import { getVoiceRange, type VoiceRange as Range } from "@/lib/session";
import { BLACK_KEYS } from "@/lib/music";

/** Your range as the engine has it (range.rs), reloaded when `refresh` changes (a take finished). */
export function useVoiceRange(refresh: unknown = 0): Range | null {
  const [r, setR] = useState<Range | null>(null);
  useEffect(() => { getVoiceRange().then(setR).catch(() => setR(null)); }, [refresh]);
  return r;
}

const pretty = (name: string) => name.replace("#", "♯");

/** "Your range": low to high from your takes, the comfortable middle marked, on a strip of keys C2-C6 (wider
 *  when you are). Nothing outside the app. */
export default function VoiceRange({ refresh }: { refresh?: unknown }) {
  const v = useVoiceRange(refresh);
  if (!v) return null;
  const r = v.range;
  if (!r) {
    return (
      <section className="glass mt-4 rounded-3xl px-5 py-4 text-sm">
        <h2 className="font-medium text-ink-200">Your range</h2>
        <p className="mt-1 text-ink-400">{v.pending > 0 ? "Working it out from your takes…" : "Sing a few songs and OnKey shows your vocal range here."}</p>
      </section>
    );
  }
  const lo = Math.min(36, r.low - 2), hi = Math.max(84, r.high + 2);
  const keys = Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
  const pct = (m: number) => `${(100 * (m - lo)) / (hi - lo + 1)}%`;
  const span = (a: number, b: number) => ({ left: pct(a), width: `${(100 * (b - a + 1)) / (hi - lo + 1)}%` });
  return (
    <section className="glass mt-4 rounded-3xl p-5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h2 className="text-sm font-medium text-ink-200">Your range</h2>
        <span className="text-xs text-ink-400">
          from {v.takes} take{v.takes === 1 ? "" : "s"}{v.pending > 0 ? `; ${v.pending} older still being measured` : ""}
        </span>
      </div>
      <p className="mt-2 text-lg">
        <span className="font-semibold text-white">{pretty(r.low_name)}–{pretty(r.high_name)}</span>
        <span className="text-ink-300">, most comfortable around </span>
        <span className="font-medium text-accent-300">{pretty(r.mid_lo_name)}–{pretty(r.mid_hi_name)}</span>
      </p>
      <div className="relative mt-3 h-7 overflow-hidden rounded-lg" role="img"
           aria-label={`Your range ${r.low_name} to ${r.high_name}, most comfortable ${r.mid_lo_name} to ${r.mid_hi_name}`}>
        <div className="absolute inset-0 flex">
          {keys.map(m => <span key={m} className={`h-full flex-1 border-r border-black/30 ${BLACK_KEYS.has(((m % 12) + 12) % 12) ? "bg-ink-800" : "bg-ink-600"}`} />)}
        </div>
        <div className="absolute inset-y-0 rounded-md bg-accent-500/30 ring-1 ring-accent-400/40" style={span(r.low, r.high)} />
        <div className="absolute inset-y-1 rounded bg-accent-400/70" style={span(r.mid_lo, r.mid_hi)} />
      </div>
      <div className="relative mt-1 h-4 text-[10px] text-ink-500">
        {keys.filter(m => m % 12 === 0).map(m => <span key={m} className="absolute -translate-x-1/2" style={{ left: pct(m + 0.5) }}>C{m / 12 - 1}</span>)}
      </div>
      <p className="mt-2 text-xs text-ink-500">
        The notes you’ve sung most, from your takes (the lowest and highest 5% left out). It grows as you sing more songs.
      </p>
    </section>
  );
}
