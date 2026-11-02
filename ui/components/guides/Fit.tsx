"use client";
import { useLayoutEffect, useRef, useState } from "react";

/**
 * SVG text that shrinks (never grows) to fit `max` units wide: a translation often runs longer than the English the
 * guide was drawn around. Measured after the fonts load; no smaller than `floor` of its size. With `ellipsis`, text
 * still too long at the floor is cut short with "…", as macOS does in its own sidebars. (The site's diagrams have
 * the same: site/components/explain/Fit.tsx.)
 */
export function Fit({ max, fontSize, floor = 0.65, ellipsis = false, children, ...rest }: Omit<React.SVGProps<SVGTextElement>, "fontSize" | "children"> & {
  max: number; fontSize: number; floor?: number; ellipsis?: boolean; children: string;
}) {
  const ref = useRef<SVGTextElement>(null);
  const [size, setSize] = useState(fontSize);
  const [shown, setShown] = useState(children);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let live = true;
    const fit = () => {
      if (!live) return;
      el.textContent = children;
      el.setAttribute("font-size", String(fontSize));
      const w = el.getComputedTextLength();
      const s = w > max ? Math.max((fontSize * max) / w, fontSize * floor) : fontSize;
      el.setAttribute("font-size", String(s));
      let text = children;
      if (ellipsis && el.getComputedTextLength() > max) {
        const chars = Array.from(children);
        for (let n = chars.length - 1; n > 1; n--) {
          text = chars.slice(0, n).join("").trimEnd() + "…";
          el.textContent = text;
          if (el.getComputedTextLength() <= max) break;
        }
      }
      setSize(s);
      setShown(text);
    };
    fit();
    // again once the web font is in: `ready` alone misses a font whose load starts after this ran
    document.fonts?.ready.then(fit);
    document.fonts?.addEventListener("loadingdone", fit);
    return () => { live = false; document.fonts?.removeEventListener("loadingdone", fit); };
  }, [children, max, fontSize, floor, ellipsis]);
  return <text ref={ref} fontSize={size} {...rest}>{shown}</text>;
}
