'use client';

/**
 * The take page's "Not the melody?", shown while you're about to drag (components: app/take/page.tsx).
 *
 * A small looping line-art explainer for a singing app's take graph:
 * the expected notes (purple bars) jump up to follow a backing vocal, the
 * singer (green / amber line) stays on the melody, the user drags across that
 * stretch, clicks "Leave it out of the score", the stretch dims to grey
 * stripes and the take's score ticks up from 71 to 76.
 *
 * - One self-contained client component: no libraries, no images, no fonts.
 * - Inline SVG driven by requestAnimationFrame. Fits a 360 x 260 box and
 *   scales to its container width.
 * - Loops, pauses on hover, step dots jump to a step.
 * - It animates even with Reduce motion on.
 *
 * Usage (Next.js App Router or Pages Router):
 *   import LeaveOutOfScoreAnimation from '@/components/leave-out-of-score-animation';
 *   <LeaveOutOfScoreAnimation />
 */

import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import StillSteps from './StillSteps';
import { useT, type Translator } from '@/lib/i18n';
import type { CSSProperties, PointerEvent as ReactPointerEvent } from 'react';

/* ------------------------------------------------------------------ */
/* Palette, type, geometry                                             */
/* ------------------------------------------------------------------ */

import { GUIDE as C } from "@/lib/colors";
import { Fit } from "./Fit";

const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';

const VIEW_W = 360;
const VIEW_H = 260;
const PANEL = { x: 12, y: 48, w: 336, h: 160 } as const;
const GRAPH_X0 = 28;
const GRAPH_X1 = 332;
const GRID_Y = [72, 96, 120, 144, 168, 192];
/** The stretch where the expected notes follow a backing vocal. */
const BAND = { x0: 150, x1: 230, y: 56, h: 144 } as const;
const BAND_MID = (BAND.x0 + BAND.x1) / 2;
const SCORE_BEFORE = 71;
const SCORE_AFTER = 76;

type Note = { x0: number; x1: number; cy: number; backing?: boolean };

/** What the score expects (purple bars). The middle three follow a backing vocal. */
const EXPECTED: readonly Note[] = [
  { x0: 28, x1: 58, cy: 160 },
  { x0: 62, x1: 88, cy: 150 },
  { x0: 92, x1: 118, cy: 156 },
  { x0: 122, x1: 146, cy: 142 },
  { x0: 154, x1: 176, cy: 84, backing: true },
  { x0: 180, x1: 202, cy: 76, backing: true },
  { x0: 206, x1: 228, cy: 90, backing: true },
  { x0: 236, x1: 262, cy: 152 },
  { x0: 266, x1: 296, cy: 164 },
  { x0: 300, x1: 332, cy: 154 },
];

/** What the singer actually sang: the melody, all the way through. */
const SUNG: readonly Note[] = [
  { x0: 28, x1: 58, cy: 160 },
  { x0: 62, x1: 88, cy: 150 },
  { x0: 92, x1: 118, cy: 156 },
  { x0: 122, x1: 146, cy: 142 },
  { x0: 154, x1: 176, cy: 150 },
  { x0: 180, x1: 202, cy: 158 },
  { x0: 206, x1: 228, cy: 146 },
  { x0: 236, x1: 262, cy: 152 },
  { x0: 266, x1: 296, cy: 164 },
  { x0: 300, x1: 332, cy: 154 },
];

/** Mini take cards along the bottom ("for every take of the song"). */
const CHIPS = { x0: 16, y: 216, w: 76, h: 32, gap: 8, count: 4, current: 2 } as const;
const CHIP_TRACK = { x0: 10, x1: 66, y0: 17, y1: 27 } as const;

/* ------------------------------------------------------------------ */
/* Timeline (ms)                                                       */
/* ------------------------------------------------------------------ */

const LOOP_MS = 12000;

const STEPS = [
  { start: 0, key: 3400, id: 's0' }, // captions: guides.leaveOut.steps.<id>
  { start: 3700, key: 7680, id: 's1' },
  { start: 7900, key: 10800, id: 's2' },
] as const;

/** What the whole guide shows, for screen readers. */
const ariaLabel = (t: Translator) => t('guides.leaveOut.ariaLabel', { before: SCORE_BEFORE, after: SCORE_AFTER });

/* ------------------------------------------------------------------ */
/* Maths helpers                                                       */
/* ------------------------------------------------------------------ */

const clamp01 = (v: number) => (v < 0 ? 0 : v > 1 ? 1 : v);
const lerp = (a: number, b: number, k: number) => a + (b - a) * k;
const easeInOut = (v: number) => -(Math.cos(Math.PI * clamp01(v)) - 1) / 2;
const smooth = (v: number) => {
  const k = clamp01(v);
  return k * k * (3 - 2 * k);
};
/** Eased 0 → 1 progress of t between a and b. */
const ramp = (t: number, a: number, b: number) => easeInOut((t - a) / (b - a));
const wrap = (t: number) => ((t % LOOP_MS) + LOOP_MS) % LOOP_MS;

/** Blend two #rrggbb colours; returns #rrggbb so blends can be chained. */
function mix(a: string, b: string, k: number): string {
  const pa = parseInt(a.slice(1), 16);
  const pb = parseInt(b.slice(1), 16);
  const ch = (shift: number) => Math.round(lerp((pa >> shift) & 255, (pb >> shift) & 255, clamp01(k)));
  return `#${((ch(16) << 16) | (ch(8) << 8) | ch(0)).toString(16).padStart(6, '0')}`;
}

const wobble = (x: number, i: number) => Math.sin(x * 0.55 + i * 1.7) * 1.1;

/** Singer pitch (as a y position) at x: held notes with a little vibrato, smooth glides between. */
function singerY(x: number): number {
  const first = SUNG[0];
  const last = SUNG[SUNG.length - 1];
  if (x <= first.x0) return first.cy + wobble(first.x0, 0);
  for (let i = 0; i < SUNG.length; i++) {
    const n = SUNG[i];
    if (x <= n.x1) {
      if (x >= n.x0) return n.cy + wobble(x, i);
      const prev = SUNG[i - 1];
      const k = smooth((x - prev.x1) / (n.x0 - prev.x1));
      return lerp(prev.cy + wobble(prev.x1, i - 1), n.cy + wobble(n.x0, i), k);
    }
  }
  return last.cy + wobble(last.x1, SUNG.length - 1);
}

function samplePath(x0: number, x1: number, step: number, at: (x: number) => [number, number]): string {
  const pts: string[] = [];
  for (let x = x0; x < x1; x += step) {
    const [px, py] = at(x);
    pts.push(`${px.toFixed(1)} ${py.toFixed(1)}`);
  }
  const [ex, ey] = at(x1);
  pts.push(`${ex.toFixed(1)} ${ey.toFixed(1)}`);
  return `M${pts.join('L')}`;
}

const traceAt = (x: number): [number, number] => [x, singerY(x)];

const SINGER_ON_A = samplePath(GRAPH_X0, BAND.x0, 2, traceAt);
const SINGER_OFF = samplePath(BAND.x0, BAND.x1 + 2, 2, traceAt);
const SINGER_ON_B = samplePath(BAND.x1 + 2, GRAPH_X1, 2, traceAt);

const toChipX = (x: number) =>
  CHIP_TRACK.x0 + ((x - GRAPH_X0) / (GRAPH_X1 - GRAPH_X0)) * (CHIP_TRACK.x1 - CHIP_TRACK.x0);

const CHIP_PATHS = Array.from({ length: CHIPS.count }, (_, take) =>
  samplePath(GRAPH_X0, GRAPH_X1, 8, (x) => {
    const y = singerY(x) + Math.sin(x * 0.09 + take * 2.1) * 3;
    const ky = clamp01((y - 136) / 32);
    return [toChipX(x), lerp(CHIP_TRACK.y0, CHIP_TRACK.y1, ky)];
  }),
);
const CHIP_BAND = { x0: toChipX(BAND.x0), x1: toChipX(BAND.x1) };

/* ------------------------------------------------------------------ */
/* Frame state: everything the drawing needs at time t                 */
/* ------------------------------------------------------------------ */

type Frame = {
  draw: number;
  playhead: number;
  hint: number;
  line: number;
  bandW: number;
  selected: number;
  greyed: number;
  dim: number;
  tag: number;
  cursor: { x: number; y: number; opacity: number; pressed: number };
  button: { opacity: number; lift: number; pressed: number };
  score: { value: number; opacity: number; good: number; plus: number };
  chips: number[];
  chipStripes: number[];
};

function frameAt(time: number): Frame {
  const t = wrap(time);
  const out = 1 - ramp(t, 11300, 11900); // end-of-loop fade

  // Step 1: playback draws the take, then the high notes are flagged.
  const draw = lerp(22, GRAPH_X1 + 8, ramp(t, 250, 2400));
  const playhead = ramp(t, 150, 450) * (1 - ramp(t, 2400, 2800));
  const hint = ramp(t, 2500, 3100) * (1 - ramp(t, 3700, 4200));

  // Step 2: drag a selection, the button appears, click it.
  const drag = ramp(t, 4850, 6050);
  const pressDrag = ramp(t, 4700, 4850) * (1 - ramp(t, 6050, 6200));
  const pressClick = ramp(t, 7700, 7800) * (1 - ramp(t, 7850, 7950));
  const arrive = ramp(t, 3800, 4700);
  let cx = lerp(300, BAND.x0, arrive);
  let cy = lerp(246, 128, arrive);
  if (t >= 4850) {
    cx = lerp(BAND.x0, BAND.x1, drag);
    cy = 128;
  }
  // Click target sits under the label so the cursor doesn't hide the words.
  const clickX = BAND_MID + 34;
  const clickY = 240;
  if (t >= 7000) {
    const k = ramp(t, 7000, 7700);
    cx = lerp(BAND.x1, clickX, k);
    cy = lerp(128, clickY, k);
  }
  if (t >= 8050) {
    const k = ramp(t, 8050, 8550);
    cx = clickX + 20 * k;
    cy = clickY + 8 * k;
  }
  const cursorOpacity = ramp(t, 3750, 4150) * (1 - ramp(t, 8050, 8550));
  const buttonIn = ramp(t, 6200, 6700);

  // Step 3: left out of the score, for every take.
  const excluded = ramp(t, 7950, 8450);
  const bandW = (BAND.x1 - BAND.x0) * drag;
  const reset = clamp01((t - 11300) / 700);
  const tick = ramp(t, 8250, 9050);
  const value =
    reset >= 0.5 ? SCORE_BEFORE : Math.round(lerp(SCORE_BEFORE, SCORE_AFTER, tick));

  return {
    draw,
    playhead,
    hint,
    line: out,
    bandW,
    selected: clamp01(bandW / 4) * (1 - excluded),
    greyed: excluded * out,
    dim: excluded * out,
    tag: ramp(t, 8300, 8700) * out,
    cursor: { x: cx, y: cy, opacity: cursorOpacity, pressed: Math.max(pressDrag, pressClick) },
    button: {
      opacity: buttonIn * (1 - ramp(t, 7950, 8300)),
      lift: (1 - buttonIn) * 6,
      pressed: pressClick,
    },
    score: {
      value,
      opacity: 1 - Math.pow(Math.sin(Math.PI * reset), 2),
      good: ramp(t, 8250, 8600) * (1 - ramp(t, 11300, 11650)),
      plus: ramp(t, 8850, 9250) * out,
    },
    chips: Array.from({ length: CHIPS.count }, (_, i) => ramp(t, 8900 + i * 130, 9300 + i * 130) * out),
    chipStripes: Array.from({ length: CHIPS.count }, (_, i) => ramp(t, 9200 + i * 160, 9550 + i * 160) * out),
  };
}

function stepAt(time: number): number {
  const t = wrap(time);
  let step = 0;
  STEPS.forEach((s, i) => {
    if (t >= s.start) step = i;
  });
  return step;
}

/* ------------------------------------------------------------------ */
/* Drawing                                                             */
/* ------------------------------------------------------------------ */

const SVG_STYLE: CSSProperties = {
  display: 'block',
  width: '100%',
  height: 'auto',
  aspectRatio: `${VIEW_W} / ${VIEW_H}`,
  fontFamily: FONT,
};

function Scene({ time }: { time: number }) {
  const t = useT();
  const uid = 'lvo' + useId().replace(/[^a-zA-Z0-9_-]/g, '');
  const clipId = `${uid}-clip`;
  const stripesId = `${uid}-stripes`;
  const f = frameAt(time);
  // the score sits just after its label, wherever a translation's label ends ("Puntuación" ran under it); English's
  // ends in time for 52, where it was drawn
  const scoreLabel = t('guides.leaveOut.score');
  const labelRef = useRef<SVGTextElement>(null);
  const [labelEnd, setLabelEnd] = useState(46);
  useLayoutEffect(() => {
    const el = labelRef.current;
    if (!el) return;
    let live = true;
    const measure = () => { if (live) setLabelEnd(16 + el.getComputedTextLength()); };
    measure();
    document.fonts?.ready.then(measure);
    return () => { live = false; };
  }, [scoreLabel]);
  const scoreX = Math.max(52, labelEnd + 5);

  const playX = Math.min(GRAPH_X1, Math.max(GRAPH_X0, f.draw));
  const playInBand = playX >= BAND.x0 && playX <= BAND.x1;
  const backingFill = mix(mix(C.accent, C.accentLight, f.hint * 0.85), C.muted, f.dim * 0.55);

  return (
    <svg viewBox={`0 0 ${VIEW_W} ${VIEW_H}`} style={SVG_STYLE} aria-hidden="true" focusable="false">
      <defs>
        <clipPath id={clipId}>
          <rect x={20} y={50} width={Math.max(0, f.draw - 20)} height={156} />
        </clipPath>
        <pattern id={stripesId} width={6} height={6} patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <line x1={0} y1={0} x2={0} y2={6} stroke={C.muted} strokeWidth={1.5} strokeOpacity={0.32} />
        </pattern>
      </defs>

      {/* Header: score on the left, take on the right */}
      <text ref={labelRef} x={16} y={34} fontSize={11} fontWeight={500} fill={C.muted}>
        {scoreLabel}
      </text>
      <text
        x={scoreX}
        y={35}
        fontSize={22}
        fontWeight={650}
        fill={mix(C.text, C.good, f.score.good)}
        opacity={f.score.opacity}
        style={{ fontVariantNumeric: 'tabular-nums' }}
      >
        {f.score.value}
      </text>
      <text
        x={scoreX + 31}
        y={34 + (1 - f.score.plus) * 3}
        fontSize={12}
        fontWeight={600}
        fill={C.good}
        opacity={f.score.plus}
        style={{ fontVariantNumeric: 'tabular-nums' }}
      >
        +{SCORE_AFTER - SCORE_BEFORE}
      </text>
      <text x={344} y={34} fontSize={12} textAnchor="end">
        <tspan fill={C.text} fontWeight={600}>
          {t('guides.leaveOut.take', { n: CHIPS.current + 1 })}
        </tspan>
      </text>

      {/* Graph panel and pitch grid */}
      <rect x={PANEL.x} y={PANEL.y} width={PANEL.w} height={PANEL.h} rx={14} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
      {GRID_Y.map((y) => (
        <line key={y} x1={24} x2={336} y1={y} y2={y} stroke={C.line} strokeWidth={1.5} strokeOpacity={0.7} />
      ))}

      {/* Selection band, then the same band left out (grey stripes) */}
      <rect
        x={BAND.x0}
        y={BAND.y}
        width={f.bandW}
        height={BAND.h}
        rx={8}
        fill={C.accent}
        fillOpacity={0.16}
        stroke={C.accent}
        strokeWidth={1.5}
        opacity={f.selected}
      />
      <g opacity={f.greyed}>
        <rect x={BAND.x0} y={BAND.y} width={f.bandW} height={BAND.h} rx={8} fill={C.bg} fillOpacity={0.35} />
        <rect
          x={BAND.x0}
          y={BAND.y}
          width={f.bandW}
          height={BAND.h}
          rx={8}
          fill={`url(#${stripesId})`}
          stroke={C.muted}
          strokeOpacity={0.45}
          strokeWidth={1.5}
        />
      </g>

      {/* Expected notes */}
      {EXPECTED.map((n) => (
        <rect
          key={n.x0}
          x={n.x0}
          y={n.cy - 3.5}
          width={n.x1 - n.x0}
          height={7}
          rx={3.5}
          fill={n.backing ? backingFill : C.accent}
          opacity={n.backing ? lerp(0.9, 0.3, f.dim) : 0.9}
        />
      ))}

      {/* "These don't look right" hint around the high notes */}
      <rect
        x={147}
        y={65}
        width={86}
        height={36}
        rx={10}
        fill="none"
        stroke={C.accentLight}
        strokeWidth={1.5}
        strokeDasharray="3 4"
        opacity={f.hint * 0.9}
      />

      {/* Singer line, revealed by playback */}
      <g clipPath={`url(#${clipId})`} opacity={f.line} fill="none" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
        <path d={SINGER_ON_A} stroke={C.good} />
        <path d={SINGER_OFF} stroke={C.amber} opacity={lerp(1, 0.3, f.dim)} />
        <path d={SINGER_ON_B} stroke={C.good} />
      </g>
      <g opacity={f.playhead}>
        <line x1={playX} x2={playX} y1={BAND.y} y2={BAND.y + BAND.h} stroke={C.muted} strokeWidth={1.5} strokeOpacity={0.5} />
        <circle cx={playX} cy={singerY(playX)} r={3} fill={playInBand ? C.amber : C.good} />
      </g>

      <text x={BAND_MID} y={194} fontSize={9.5} fontWeight={500} textAnchor="middle" letterSpacing={0.4} fill={C.muted} opacity={f.tag}>
        {t('guides.leaveOut.leftOut')}
      </text>

      {/* Popover button under the selection */}
      <g opacity={f.button.opacity} transform={`translate(0 ${f.button.lift.toFixed(2)})`}>
        <path d={`M${BAND_MID - 6} 218.5 L${BAND_MID} 212.5 L${BAND_MID + 6} 218.5 Z`} fill={C.accentDark} />
        <g
          transform={`translate(${BAND_MID} 232) scale(${(1 - 0.03 * f.button.pressed).toFixed(3)}) translate(${-BAND_MID} -232)`}
        >
          <rect
            x={BAND_MID - 84}
            y={218}
            width={168}
            height={28}
            rx={14}
            fill={mix(C.accentDark, C.accent, f.button.pressed * 0.6)}
            stroke={C.accentLight}
            strokeOpacity={0.9 * f.button.pressed}
            strokeWidth={1.5}
          />
          <Fit x={BAND_MID} y={236} max={152} fontSize={11} fontWeight={600} textAnchor="middle" fill={C.text}>
            {t('guides.leaveOut.leaveOutButton')}
          </Fit>
        </g>
      </g>

      {/* Every take of the song gets the same stretch left out */}
      {f.chips.map((o, i) => {
        const x = CHIPS.x0 + i * (CHIPS.w + CHIPS.gap);
        const current = i === CHIPS.current;
        return (
          <g key={i} opacity={o} transform={`translate(${x} ${(CHIPS.y + (1 - o) * 4).toFixed(2)})`}>
            <rect
              width={CHIPS.w}
              height={CHIPS.h}
              rx={12}
              fill={C.panel}
              stroke={current ? C.accent : C.line}
              strokeOpacity={current ? 0.8 : 1}
              strokeWidth={1.5}
            />
            <text x={10} y={12} fontSize={9} fontWeight={current ? 600 : 500} fill={current ? C.text : C.muted}>
              {t('guides.leaveOut.take', { n: i + 1 })}
            </text>
            <rect
              x={CHIP_BAND.x0}
              y={CHIP_TRACK.y0 - 2}
              width={CHIP_BAND.x1 - CHIP_BAND.x0}
              height={CHIP_TRACK.y1 - CHIP_TRACK.y0 + 4}
              rx={3}
              fill={`url(#${stripesId})`}
              stroke={C.muted}
              strokeOpacity={0.45}
              strokeWidth={1}
              opacity={f.chipStripes[i]}
            />
            <path
              d={CHIP_PATHS[i]}
              fill="none"
              stroke={current ? C.good : C.muted}
              strokeOpacity={current ? 0.9 : 0.7}
              strokeWidth={1.25}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </g>
        );
      })}

      {/* Cursor */}
      <g opacity={f.cursor.opacity} transform={`translate(${f.cursor.x.toFixed(2)} ${f.cursor.y.toFixed(2)})`}>
        <circle
          r={lerp(4, 11, f.cursor.pressed)}
          fill={C.accentLight}
          fillOpacity={0.14 * f.cursor.pressed}
          stroke={C.accentLight}
          strokeOpacity={0.7 * f.cursor.pressed}
          strokeWidth={1.5}
        />
        <path
          d="M0 0 L0 15.5 L4.2 11.8 L7 18 L9.8 16.8 L7.1 10.8 L12.2 10.8 Z"
          fill={C.text}
          stroke={C.bg}
          strokeWidth={1.5}
          strokeLinejoin="round"
          transform={`scale(${(1 - 0.08 * f.cursor.pressed).toFixed(3)})`}
        />
      </g>
    </svg>
  );
}

/** A single still frame of the animation at `time` ms (0 – 12000). Handy for posters or tests. */
export function LeaveOutOfScoreFrame({ time }: { time: number }) {
  return <Scene time={time} />;
}

/* ------------------------------------------------------------------ */
/* Styles                                                              */
/* ------------------------------------------------------------------ */

const CSS = `
.lvo{width:100%;margin:0;background:${C.bg};color:${C.text};border-radius:16px;font-family:${FONT};container-type:inline-size;-webkit-font-smoothing:antialiased}
.lvo-caption{display:grid;margin:2px 16px 0;text-align:center;font-size:13.5px;line-height:1.4;font-weight:500;color:${C.text};text-wrap:balance}
.lvo-caption>span{grid-area:1/1;opacity:0;transition:opacity 420ms ease-in-out}
.lvo-caption>span[data-active="true"]{opacity:1}
.lvo-dots{display:flex;justify-content:center;gap:2px;padding:6px 0 10px}
.lvo-dot{appearance:none;-webkit-appearance:none;border:0;background:none;padding:0;margin:0;width:24px;height:24px;display:grid;place-items:center;cursor:pointer;border-radius:12px}
.lvo-dot>span{display:block;width:6px;height:6px;border-radius:3px;background:${C.line};transition:width 320ms ease-in-out,background-color 320ms ease-in-out}
.lvo-dot:hover>span{background:${C.muted}}
.lvo-dot[aria-current="step"]>span{width:18px;background:${C.accent}}
.lvo-dot:focus-visible{outline:1.5px solid ${C.accentLight};outline-offset:0}
.lvo-strip{display:grid;grid-template-columns:1fr;gap:14px;padding:12px}
@container (min-width:600px){.lvo-strip{grid-template-columns:repeat(3,minmax(0,1fr))}}
.lvo-frame{display:flex;flex-direction:column;gap:8px;min-width:0}
.lvo-frame>svg{border:1.5px solid ${C.line};border-radius:12px}
.lvo-frame>p{margin:0;display:flex;gap:8px;font-size:13px;line-height:1.4;color:${C.text}}
.lvo-num{color:${C.muted};font-variant-numeric:tabular-nums}
`;

/* ------------------------------------------------------------------ */
/* Animated version                                                    */
/* ------------------------------------------------------------------ */

// The guides always animate, also with Reduce motion on (Daniel asked, 29 Oct). Smaller motion elsewhere in
// OnKey still follows the setting.
function usePrefersReducedMotion(): boolean {
  return false;
}

function Animated() {
  const t = useT();
  const reduced = usePrefersReducedMotion();
  const [time, setTime] = useState(0);
  const timeRef = useRef(0);
  const hoverRef = useRef(false);
  const visibleRef = useRef(true);
  /** After a dot click while hovered, play on to this time, then hold. */
  const playUntilRef = useRef<number | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = rootRef.current;
    if (!el || typeof IntersectionObserver === 'undefined') return;
    const io = new IntersectionObserver(([entry]) => {
      visibleRef.current = entry.isIntersecting;
    });
    io.observe(el);
    return () => io.disconnect();
  }, []);

  useEffect(() => {
    if (reduced) return;
    let raf = 0;
    let last = performance.now();
    const loop = (now: number) => {
      const dt = Math.min(64, now - last);
      last = now;
      const until = playUntilRef.current;
      if (until !== null && timeRef.current >= until) playUntilRef.current = null;
      const forced = playUntilRef.current !== null;
      if (visibleRef.current && (!hoverRef.current || forced)) {
        let next = timeRef.current + dt;
        if (forced && hoverRef.current) next = Math.min(next, playUntilRef.current as number);
        timeRef.current = wrap(next);
        setTime(timeRef.current);
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [reduced]);

  const jumpTo = (i: number) => {
    timeRef.current = STEPS[i].start;
    playUntilRef.current = STEPS[i].key;
    setTime(timeRef.current);
  };

  const onEnter = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.pointerType === 'mouse') hoverRef.current = true;
  };
  const onLeave = () => {
    hoverRef.current = false;
    playUntilRef.current = null;
  };

  const active = stepAt(time);

  return (
    <div ref={rootRef} className="lvo-anim" onPointerEnter={onEnter} onPointerLeave={onLeave}>
      <div role="img" aria-label={ariaLabel(t)}>
        <Scene time={time} />
      </div>
      <p className="lvo-caption">
        {STEPS.map((s, i) => (
          <span key={s.id} data-active={i === active} aria-hidden={i !== active}>
            {t(`guides.leaveOut.steps.${s.id}`)}
          </span>
        ))}
      </p>
      <div className="lvo-dots" role="group" aria-label={t('guides.leaveOut.stepsGroup')}>
        {STEPS.map((s, i) => (
          <button
            key={s.id}
            type="button"
            className="lvo-dot"
            aria-label={t('guides.leaveOut.stepLabel', { n: i + 1, caption: t(`guides.leaveOut.steps.${s.id}`) })}
            aria-current={i === active ? 'step' : undefined}
            onClick={() => jumpTo(i)}
          >
            <span />
          </button>
        ))}
      </div>
    </div>
  );
}


/* ------------------------------------------------------------------ */
/* Public component                                                    */
/* ------------------------------------------------------------------ */

export type LeaveOutOfScoreAnimationProps = {
  className?: string;
  style?: CSSProperties;
};

export default function LeaveOutGuide({ className, style }: LeaveOutOfScoreAnimationProps) {
  const t = useT();
  const reduced = usePrefersReducedMotion();
  if (reduced) {
    return <StillSteps count={STEPS.length} label={ariaLabel(t)} caption={i => t(`guides.leaveOut.steps.${STEPS[i].id}`)} frame={i => <Scene time={STEPS[i].key} />} />;
  }
  return (
    <div className={className ? `lvo ${className}` : 'lvo'} style={style}>
      <style>{CSS}</style>
      <Animated />
    </div>
  );
}
