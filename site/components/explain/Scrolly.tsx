"use client";
import { cloneElement, useEffect, useRef, useState } from "react";
import { formatMessage } from "@/lib/format";

/**
 * Scrollytelling: short steps of text beside one diagram that stays in view and changes as each step reaches the
 * middle of the screen. On a phone the diagram sticks under the header and the text scrolls beneath it.
 * `diagram` is the diagram's element; it's drawn again with `step` set for each step (with motion off it is
 * simply redrawn per step). `stepOf`: "Step {n} of {total}" as written in the page's language (raw ICU).
 */
export function Scrolly({ steps, diagram, label, locale, stepOf }: {
  steps: React.ReactNode[];
  diagram: React.ReactElement<{ step?: number }>;
  label: string;
  locale: string;
  stepOf: string;
}) {
  const [step, setStep] = useState(0);
  const refs = useRef<(HTMLDivElement | null)[]>([]);
  useEffect(() => {
    // root: document so rootMargin applies to the viewport even inside the page's scroll container
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) if (e.isIntersecting) setStep(Number((e.target as HTMLElement).dataset.step));
      },
      { root: document, rootMargin: "-45% 0px -45% 0px", threshold: 0 },
    );
    refs.current.forEach((el) => el && io.observe(el));
    return () => io.disconnect();
  }, [steps.length]);

  return (
    <div className="wide grid gap-6 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] lg:gap-10">
      <div className="sticky top-16 z-10 -mx-4 self-start border-b border-line bg-bg/90 px-0 pb-3 pt-3 backdrop-blur-md sm:-mx-6 sm:px-6 lg:order-2 lg:top-24 lg:mx-0 lg:border-0 lg:bg-transparent lg:p-0 lg:backdrop-blur-none">
        <figure aria-label={label} className="diagram overflow-hidden border-y border-line bg-bg-raised sm:rounded-2xl sm:border">
          {cloneElement(diagram, { step })}
        </figure>
        <p className="mt-2 text-center text-xs text-faint lg:hidden" aria-hidden="true">{formatMessage(locale, stepOf, { n: step + 1, total: steps.length })}</p>
      </div>
      <div className="lg:order-1">
        {steps.map((s, i) => (
          <div
            key={i}
            ref={(el) => { refs.current[i] = el; }}
            data-step={i}
            data-active={step === i}
            className="scrolly-step flex min-h-[38vh] flex-col justify-center py-6 text-[16px] leading-[1.7] first:pt-2 last:min-h-[30vh] lg:min-h-[52vh]"
          >
            <div className="max-w-[65ch]">{s}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
