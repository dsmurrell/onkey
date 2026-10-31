"use client";
// A guide with Reduce Motion on: one still frame at a time, the size of the animation, with its caption, and
// step dots plus Next to go through it. Nothing moves by itself.
import { useState, type ReactNode } from "react";

export default function StillSteps({ count, frame, caption, label }: {
  count: number;
  /** the still picture for step i */
  frame: (i: number) => ReactNode;
  caption: (i: number) => ReactNode;
  /** what the whole guide shows, for screen readers */
  label: string;
}) {
  const [i, setI] = useState(0);
  const next = () => setI(n => (n + 1) % count);
  return (
    <div className="w-full rounded-2xl bg-[#0b0d13] text-[#e6e9f0]">
      <div role="img" aria-label={label}>
        <button type="button" onClick={next} aria-hidden tabIndex={-1} className="block w-full cursor-pointer">{frame(i)}</button>
        <p className="min-h-[2.7em] px-4 pt-1 text-center text-[13px] leading-snug [text-wrap:balance]">{caption(i)}</p>
      </div>
      <div className="flex items-center justify-center gap-1 pb-2">
        <div role="group" aria-label="Steps" className="flex">
          {Array.from({ length: count }, (_, k) => (
            <button key={k} type="button" onClick={() => setI(k)} aria-label={`Step ${k + 1} of ${count}`}
                    aria-current={k === i ? "step" : undefined}
                    className="grid h-6 w-6 place-items-center rounded-full focus-visible:outline focus-visible:outline-1 focus-visible:outline-[#b5a8ff]">
              <span className={`block h-1.5 rounded-full ${k === i ? "w-4 bg-[#7c6cf6]" : "w-1.5 bg-[#2a3142] hover:bg-[#8b94ab]"}`} />
            </button>
          ))}
        </div>
        <button type="button" onClick={next} className="ml-1 rounded-md px-2 py-0.5 text-xs text-[#8b94ab] transition hover:text-[#e6e9f0]">
          {i === count - 1 ? "Again" : "Next"}
        </button>
      </div>
    </div>
  );
}
