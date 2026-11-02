"use client";
// The filter and sort controls Library and History share: Sort (a menu), Difficulty (1 to 5), and the pill
// style the rest of each page's filters use.
import { useEffect, useRef, useState } from "react";
import { useT, type Translator } from "@/lib/i18n";

/** A song's difficulty, by its number (1 to 5): the keys of its words. */
const DIFFICULTY = ["easy", "moderate", "medium", "hard", "veryHard"] as const;
/** A difficulty's word (1 to 5: "Easy" … "Very hard"; "" for anything else). */
export const difficultyWord = (t: Translator, level: number) =>
  DIFFICULTY[level - 1] ? t(`listControls.difficulty.${DIFFICULTY[level - 1]}`) : "";

export const pill = (on: boolean) =>
  `rounded-full border border-white/8 px-3.5 py-2 text-sm outline-none transition focus-visible:ring-2 focus-visible:ring-accent-400/60 ${on ? "bg-white/10 text-white" : "bg-white/4 text-ink-300 hover:text-white"}`;

export function SortMenu<S extends string>({ sorts, sort, onSort }: { sorts: [S, string][]; sort: S; onSort: (s: S) => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("pointerdown", away); document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("pointerdown", away); document.removeEventListener("keydown", esc); };
  }, [open]);
  const label = sorts.find(([k]) => k === sort)?.[1] ?? sorts[0][1];
  return (
    <div ref={ref} className="relative ml-auto">
      <button onClick={() => setOpen(o => !o)} aria-haspopup="menu" aria-expanded={open}
              className="flex items-center gap-2 rounded-full border border-white/8 bg-white/4 py-2 pr-3 pl-4 text-sm text-ink-300 outline-none transition hover:text-white focus-visible:ring-2 focus-visible:ring-accent-400/60">
        <span className="text-ink-400">{t("listControls.sort")}</span><span className="text-ink-100">{label}</span>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" aria-hidden
             className={`text-ink-400 transition ${open ? "rotate-180" : ""}`}><path d="M6 9l6 6 6-6" /></svg>
      </button>
      {open && (
        <div role="menu" className="absolute right-0 z-20 mt-2 w-48 overflow-hidden rounded-2xl border border-white/10 bg-ink-900/98 p-1 shadow-xl shadow-black/50">
          {sorts.map(([k, l]) => (
            <button key={k} role="menuitemradio" aria-checked={sort === k} onClick={() => { onSort(k); setOpen(false); }}
                    className={`flex w-full items-center justify-between rounded-xl px-3 py-2 text-left text-sm transition hover:bg-white/6 ${sort === k ? "text-white" : "text-ink-300"}`}>
              {l}{sort === k && <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden className="text-accent-400"><path d="M5 12l5 5 9-10" /></svg>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** Difficulty 1 to 5, any number of them on (none: every song). `counts[n - 1]`: how many songs are that hard. */
export function DifficultyPicker({ levels, onToggle, onClear, counts }: {
  levels: number[]; onToggle: (n: number) => void; onClear: () => void; counts: number[];
}) {
  const t = useT();
  return (
    <div role="group" aria-label={t("listControls.difficultyLabel")} className="inline-flex items-center gap-1 rounded-full border border-white/8 bg-white/4 p-1 text-sm">
      <span className="px-2 text-xs text-ink-400">{t("listControls.difficultyLabel")}</span>
      {[1, 2, 3, 4, 5].map(n => (
        <button key={n} aria-pressed={levels.includes(n)} onClick={() => onToggle(n)}
                title={t("listControls.levelTitle", { level: difficultyWord(t, n), count: counts[n - 1] })}
                aria-label={t("listControls.levelLabel", { level: difficultyWord(t, n), n })}
                className={`tabular h-8 w-8 rounded-full font-medium outline-none transition focus-visible:ring-2 focus-visible:ring-accent-400/60
                            ${levels.includes(n) ? "bg-white/12 text-white" : "text-ink-300 hover:bg-white/5 hover:text-ink-100"}`}>
          {n}
        </button>
      ))}
      {levels.length > 0 && <button onClick={onClear} className="px-2 text-xs text-ink-400 transition hover:text-white">{t("listControls.clear")}</button>}
    </div>
  );
}
