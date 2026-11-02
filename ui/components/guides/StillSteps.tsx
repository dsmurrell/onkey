"use client";
// A guide with Reduce Motion on: one still frame at a time, the size of the animation, with its caption, and
// step dots plus Next to go through it. Nothing moves by itself.
import { useState, type ReactNode } from "react";
import { useT } from "@/lib/i18n";

export default function StillSteps({ count, frame, caption, label }: {
  count: number;
  /** the still picture for step i */
  frame: (i: number) => ReactNode;
  caption: (i: number) => ReactNode;
  /** what the whole guide shows, for screen readers */
  label: string;
}) {
  const t = useT();
  const [i, setI] = useState(0);
  const next = () => setI(n => (n + 1) % count);
  return (
    <div className="w-full rounded-2xl bg-ink-900 text-ink-100">
      <div role="img" aria-label={label}>
        <button type="button" onClick={next} aria-hidden tabIndex={-1} className="block w-full cursor-pointer">{frame(i)}</button>
        <p className="min-h-[2.7em] px-4 pt-1 text-center text-[13px] leading-snug [text-wrap:balance]">{caption(i)}</p>
      </div>
      <div className="flex items-center justify-center gap-1 pb-2">
        <div role="group" aria-label={t("guides.stillSteps.dots")} className="flex">
          {Array.from({ length: count }, (_, k) => (
            <button key={k} type="button" onClick={() => setI(k)} aria-label={t("guides.stillSteps.stepOf", { step: k + 1, count })}
                    aria-current={k === i ? "step" : undefined}
                    className="grid h-6 w-6 place-items-center rounded-full focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent-300">
              <span className={`block h-1.5 rounded-full ${k === i ? "w-4 bg-accent-500" : "w-1.5 bg-ink-600 hover:bg-ink-300"}`} />
            </button>
          ))}
        </div>
        <button type="button" onClick={next} className="ml-1 rounded-md px-2 py-0.5 text-xs text-ink-300 transition hover:text-ink-100">
          {i === count - 1 ? t("guides.stillSteps.again") : t("guides.stillSteps.next")}
        </button>
      </div>
    </div>
  );
}
