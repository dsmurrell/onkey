"use client";
// Score per take over time: a line with a dot per take, a hover tooltip, a faint 0-100 grid. With many takes the
// line is the rolling average (of 10) and each take a faint small dot, so the trend shows through the noise.
import { useEffect, useMemo, useRef, useState } from "react";
import type { TakeSummary } from "@/lib/session";
import { INK } from "@/lib/colors";
import { when } from "@/lib/music";
import { useT } from "@/lib/i18n";
import { rolling } from "@/lib/trend";

const TREND = "#8a78ff";   // validated for the dark surface (dataviz lightness/contrast checks)
const TIP_W = 180, TIP_GAP = 14;
/** The tooltip sits beside the hover line, right of it when it fits, else left, so the take under the cursor stays visible. */
export const tipLeft = (at: number, w: number) => at + TIP_GAP + TIP_W <= w ? at + TIP_GAP : Math.max(0, at - TIP_GAP - TIP_W);
const FEW = 8, AVG = 10;   // up to FEW takes: a plain line through each one

export default function Trend({ takes, unit = "take" }: { takes: TakeSummary[]; unit?: "take" | "play" }) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(800);
  const [hover, setHover] = useState<number | null>(null);
  useEffect(() => {
    const ro = new ResizeObserver(e => setW(e[0].contentRect.width));
    if (ref.current) ro.observe(ref.current);
    return () => ro.disconnect();
  }, []);
  const H = 220, P = { l: 34, r: 12, t: 12, b: 26 };
  const n = takes.length;
  const smooth = n > FEW;
  const line = useMemo(() => smooth ? rolling(takes.map(tk => tk.score), AVG) : takes.map(tk => tk.score), [takes, smooth]);
  const x = (i: number) => P.l + (n <= 1 ? (w - P.l - P.r) / 2 : (i / (n - 1)) * (w - P.l - P.r));
  const y = (v: number) => P.t + (1 - v / 100) * (H - P.t - P.b);
  const path = line.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const hv = hover != null ? takes[hover] : null;
  return (
    <div ref={ref} className="relative">
      <svg width={w} height={H} className="block"
           onMouseMove={e => {
             const r = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
             const i = Math.round(((e.clientX - r.left - P.l) / (w - P.l - P.r)) * (n - 1));
             setHover(Math.max(0, Math.min(n - 1, i)));
           }}
           onMouseLeave={() => setHover(null)}>
        {[0, 25, 50, 75, 100].map(v => (
          <g key={v}>
            <line x1={P.l} x2={w - P.r} y1={y(v)} y2={y(v)} stroke="rgba(255,255,255,.06)" />
            <text x={P.l - 8} y={y(v)} textAnchor="end" dominantBaseline="middle" className="fill-ink-400 text-[10px]">{v}</text>
          </g>
        ))}
        {smooth && takes.map((tk, i) => (
          <circle key={tk.id} cx={x(i)} cy={y(tk.score)} r={2} fill={TREND} opacity={0.35} />
        ))}
        <path d={path} fill="none" stroke={TREND} strokeWidth={smooth ? 2.5 : 2} strokeLinejoin="round" strokeLinecap="round" />
        {!smooth && takes.map((tk, i) => (
          <circle key={tk.id} cx={x(i)} cy={y(tk.score)} r={hover === i ? 5.5 : 4} fill={TREND} stroke={INK[850]} strokeWidth={2} />
        ))}
        {hover != null && <line x1={x(hover)} x2={x(hover)} y1={P.t} y2={H - P.b} stroke="rgba(255,255,255,.18)" />}
        {smooth && hv && <circle cx={x(hover!)} cy={y(hv.score)} r={4.5} fill={TREND} stroke={INK[850]} strokeWidth={2} />}
      </svg>
      {smooth && <div className="mt-1 text-right text-[11px] text-ink-400">{t("trend.legend", { unit, avg: AVG })}</div>}
      {hv && (
        <div className="pointer-events-none absolute top-2 rounded-xl border border-white/10 bg-ink-800/95 px-3 py-2 text-xs shadow-xl"
             style={{ left: tipLeft(x(hover!), w), width: TIP_W }}>
          <div className="truncate font-medium text-ink-100">{hv.track.name}</div>
          <div className="text-ink-400">{when(hv.recorded)}</div>
          <div className="mt-1 text-ink-200">{t.rich("trend.tipScore", { score: hv.score, pct: Math.round(100 * hv.coverage), n: c => <span className="tabular font-semibold text-white">{c}</span> })}</div>
          {smooth && <div className="text-ink-400">{t.rich("trend.tipAverage", { avg: Math.round(line[hover!]), n: c => <span className="tabular text-ink-200">{c}</span> })}</div>}
        </div>
      )}
    </div>
  );
}
