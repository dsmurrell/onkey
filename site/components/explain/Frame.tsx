import { Fit } from "./Fit";

/** Every diagram draws into the same 480×300 space, with a caption naming what this frame shows. */
export const W = 480, H = 300;

export function Frame({ label, caption, children }: { label: string; caption?: string; children: React.ReactNode }) {
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="dg mx-auto block h-auto max-h-[40vh] w-full lg:max-h-none" role="img" aria-label={caption ? `${label}: ${caption}` : label}>
      <rect width={W} height={H} fill="var(--bg-raised)" />
      {children}
      {caption && (
        <Fit max={W - 24} x={W / 2} y={H - 14} textAnchor="middle" fontSize={13} fill="var(--muted)">{caption}</Fit>
      )}
    </svg>
  );
}

/** A labelled box: dimmed unless it's the part the current step talks about. */
export function Node({ x, y, w = 120, h = 52, colour, title, sub, on = true, icon }: {
  x: number; y: number; w?: number; h?: number; colour: string; title: string; sub?: string; on?: boolean; icon?: React.ReactNode;
}) {
  return (
    <g className="hl" opacity={on ? 1 : 0.32}>
      <rect x={x - w / 2} y={y - h / 2} width={w} height={h} rx="12" fill="var(--panel)" stroke={colour} strokeWidth={on ? 2 : 1} />
      {icon && <g transform={`translate(${x - w / 2 + 14} ${y - 8})`}>{icon}</g>}
      <Fit max={w - (icon ? 44 : 16)} x={icon ? x + 10 : x} y={sub ? y - 3 : y + 5} textAnchor="middle" fontSize={13.5} fontWeight="600" fill="var(--fg)">{title}</Fit>
      {sub && <Fit max={w - (icon ? 44 : 16)} x={icon ? x + 10 : x} y={y + 14} textAnchor="middle" fontSize={11} fill="var(--muted)">{sub}</Fit>}
    </g>
  );
}

/** A path with dots flowing along it (the dash offset moves with `t`). Static when `t` stops. */
export function Flow({ d, colour, t, on = true, width = 2 }: { d: string; colour: string; t: number; on?: boolean; width?: number }) {
  return (
    <g className="hl" opacity={on ? 1 : 0.18}>
      <path d={d} fill="none" stroke={colour} strokeOpacity="0.25" strokeWidth={width} />
      <path d={d} fill="none" stroke={colour} strokeWidth={width + 1} strokeLinecap="round" strokeDasharray="2 12" strokeDashoffset={-t * 28} />
    </g>
  );
}

/** A waveform polyline from samples (-1..1) across a box. */
export function wavePath(samples: number[], x: number, y: number, w: number, amp: number) {
  return samples.map((v, i) => `${i ? "L" : "M"}${(x + (i / (samples.length - 1)) * w).toFixed(1)},${(y - v * amp).toFixed(1)}`).join(" ");
}
