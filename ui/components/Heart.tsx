"use client";
// The favourite heart: an outline that fills (with a small pop) when a song is hearted. On cards it shows on hover
// or keyboard focus until it's filled (`subtle`); filled, it always shows. It never triggers what it sits in.
import { useState } from "react";
import { useT } from "@/lib/i18n";

export const HEART = "M12 20.3l-1.2-1.1C6.1 15 3 12.2 3 8.7 3 5.9 5.2 3.8 7.9 3.8c1.6 0 3.1.7 4.1 1.9 1-1.2 2.5-1.9 4.1-1.9 2.7 0 4.9 2.1 4.9 4.9 0 3.5-3.1 6.3-7.8 10.5L12 20.3z";

export default function Heart({ on, onToggle, size = 15, subtle = false, className = "" }:
    { on: boolean; onToggle: (on: boolean) => void; size?: number; subtle?: boolean; className?: string }) {
  const t = useT();
  const [popped, setPopped] = useState(0);
  const label = on ? t("heart.remove") : t("heart.add");
  return (
    <button type="button" aria-pressed={on} aria-label={label} title={label}
            onClick={e => { e.preventDefault(); e.stopPropagation(); if (!on) setPopped(p => p + 1); onToggle(!on); }}
            onKeyDown={e => e.stopPropagation()}
            className={`grid h-7 w-7 shrink-0 place-items-center rounded-full outline-none transition hover:bg-white/8 focus-visible:ring-2 focus-visible:ring-accent-400/60
                        ${on ? "text-accent-400 hover:text-accent-300" : `text-ink-400 hover:text-accent-300 ${subtle ? "opacity-0 group-hover:opacity-100 focus-visible:opacity-100" : ""}`} ${className}`}>
      <svg key={popped} width={size} height={size} viewBox="0 0 24 24" fill={on ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.8"
           strokeLinejoin="round" aria-hidden className={popped && on ? "animate-pop motion-reduce:animate-none" : ""}>
        <path d={HEART} />
      </svg>
    </button>
  );
}
