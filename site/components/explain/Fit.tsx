"use client";
import { useLayoutEffect, useRef, useState } from "react";

/**
 * SVG text that shrinks (never grows) to fit `max` units wide: a translation often runs longer than the English the
 * diagram was drawn around. Measured after the fonts load; no smaller than `floor` of its size.
 */
export function Fit({ max, fontSize, floor = 0.65, children, ...rest }: Omit<React.SVGProps<SVGTextElement>, "fontSize" | "children"> & {
  max: number; fontSize: number; floor?: number; children: string;
}) {
  const ref = useRef<SVGTextElement>(null);
  const [size, setSize] = useState(fontSize);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let live = true;
    const fit = () => {
      if (!live) return;
      el.setAttribute("font-size", String(fontSize));
      const w = el.getComputedTextLength();
      const s = w > max ? Math.max((fontSize * max) / w, fontSize * floor) : fontSize;
      el.setAttribute("font-size", String(s));
      setSize(s);
    };
    fit();
    // again once the web font is in: `ready` alone misses a font whose load starts after this ran
    document.fonts?.ready.then(fit);
    document.fonts?.addEventListener("loadingdone", fit);
    return () => { live = false; document.fonts?.removeEventListener("loadingdone", fit); };
  }, [children, max, fontSize, floor]);
  return <text ref={ref} fontSize={size} {...rest}>{children}</text>;
}
