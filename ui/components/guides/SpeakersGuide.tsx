"use client";

/**
 * The share panel, for a take sung with the song on speakers: why headphone takes share better.
 *
 * A small looping explainer (about 8 s) for recording vocals over a backing track.
 *   1. On laptop speakers the song comes out of the speakers, curls into the
 *      microphone, and the take is the voice tangled with the music.
 *   2. In headphones the music stays inside the ear cups and the take is a
 *      clean voice line, ending on a check.
 *
 * Self-contained: one inline SVG driven by requestAnimationFrame. No libraries,
 * images, video or web fonts. The drawing is 360 x 260 and scales to the width
 * of its container. Hover the drawing to pause; the dots jump to a step. With
 * `reducedMotion` the two steps render as a static strip with captions. It
 * animates even with Reduce motion on.
 *
 * Usage (Next.js, app or pages router):
 *   import SpeakersVsHeadphones from "@/components/SpeakersVsHeadphones";
 *   <div style={{ maxWidth: 360 }}>
 *     <SpeakersVsHeadphones />
 *   </div>
 */

import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { useT, type Translator } from "@/lib/i18n";

/* ------------------------------------------------------------------ tokens */

import { GUIDE as C } from "@/lib/colors";

const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';

/* ------------------------------------------------------------------ timing */

type Step = {
  /** caption: guides.speakers.steps.<id> */
  id: "s0" | "s1";
  /** scene length, ms */
  dur: number;
  /** when the recording starts and stops filling, ms into the scene */
  fillStart: number;
  fillEnd: number;
};

const STEPS: readonly [Step, Step] = [
  {
    id: "s0",
    dur: 3800,
    fillStart: 300,
    fillEnd: 3300,
  },
  {
    id: "s1",
    dur: 4400,
    fillStart: 450,
    fillEnd: 3350,
  },
];

const STEP_START = [0, STEPS[0].dur] as const;
const TOTAL = STEPS[0].dur + STEPS[1].dur; // 8.2 s loop
const TAKE_SECONDS = 3; // what the REC timer counts up to

const caption = (t: Translator, s: Step) => t(`guides.speakers.steps.${s.id}`);

/* ------------------------------------------------------------------ maths */

const clamp01 = (x: number) => Math.min(1, Math.max(0, x));
/** gentle sine ease-in-out, no overshoot */
const ease = (x: number) => 0.5 - 0.5 * Math.cos(Math.PI * clamp01(x));
const ramp = (tau: number, start: number, len: number) => ease((tau - start) / len);
const frac = (x: number) => x - Math.floor(x);
/** fade in over the first `a` of a 0..1 journey and out over the last `b` */
const edge = (u: number, a: number, b: number) => Math.min(ease(u / a), ease((1 - u) / b));

type Pt = readonly [number, number];

function cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, u: number) {
  const a = 1 - u;
  const x = a * a * a * p0[0] + 3 * a * a * u * p1[0] + 3 * a * u * u * p2[0] + u * u * u * p3[0];
  const y = a * a * a * p0[1] + 3 * a * a * u * p1[1] + 3 * a * u * u * p2[1] + u * u * u * p3[1];
  const dx = 3 * a * a * (p1[0] - p0[0]) + 6 * a * u * (p2[0] - p1[0]) + 3 * u * u * (p3[0] - p2[0]);
  const dy = 3 * a * a * (p1[1] - p0[1]) + 6 * a * u * (p2[1] - p1[1]) + 3 * u * u * (p3[1] - p2[1]);
  return { x, y, angle: (Math.atan2(dy, dx) * 180) / Math.PI };
}

/** circular arc from angle a0 to a1 (degrees, SVG orientation) */
function arc(cx: number, cy: number, r: number, a0: number, a1: number) {
  const p = (deg: number) => {
    const t = (deg * Math.PI) / 180;
    return `${(cx + r * Math.cos(t)).toFixed(2)} ${(cy + r * Math.sin(t)).toFixed(2)}`;
  };
  return `M${p(a0)}A${r.toFixed(2)} ${r.toFixed(2)} 0 0 1 ${p(a1)}`;
}

/* ------------------------------------------------------------------ signals */

// One sung phrase: syllable bumps along the take (position, height).
const SYLLABLES: ReadonlyArray<Pt> = [
  [0.08, 0.7],
  [0.18, 1],
  [0.29, 0.8],
  [0.41, 0.95],
  [0.51, 0.6],
  [0.63, 1],
  [0.74, 0.85],
  [0.86, 0.7],
];

function voiceEnv(u: number) {
  let s = 0;
  for (const [c, h] of SYLLABLES) {
    const d = (u - c) / 0.032;
    s += h * Math.exp(-d * d);
  }
  return Math.min(1, s);
}

const voice = (u: number) =>
  15 * voiceEnv(u) * Math.sin(2 * Math.PI * 34 * u + 0.6 * Math.sin(2 * Math.PI * 3 * u));

const song = (u: number) =>
  (6.5 + 3.5 * (0.5 + 0.5 * Math.sin(2 * Math.PI * 6 * u))) *
  (0.6 * Math.sin(2 * Math.PI * 41 * u + 1) + 0.4 * Math.sin(2 * Math.PI * 17 * u)) *
  ease(u / 0.03) *
  ease((1 - u) / 0.03);

// Recording lane geometry
const WX = 28;
const WW = 304;
const WY = 212;

function wavePath(fn: (u: number) => number, p: number) {
  if (p <= 0) return "";
  const n = Math.max(2, Math.ceil(WW * p));
  let d = "";
  for (let i = 0; i <= n; i++) {
    const u = (i / n) * p;
    d += `${i ? "L" : "M"}${(WX + u * WW).toFixed(1)} ${(WY - fn(u)).toFixed(1)}`;
  }
  return d;
}

/* ------------------------------------------------------------------ scene state */

type Frame = {
  t: number; // global clock, drives continuous motion
  step: 0 | 1;
  progress: number; // 0..1 through the current step
  music: number; // song on the speakers
  hp: number; // headphones on the head
  hpLift: number; // px, negative while lowering or lifting
  hpMusic: number; // song inside the ear cups
  fill: number; // current take, 0..1
  prevAlpha: number; // previous take fading out
  singing: number;
  voice: number; // strength of the green voice waves
  mouth: number;
  playhead: number;
  amber: number;
  check: number;
  songLegend: number;
  songProgress: number; // player bar on the laptop screen
  captions: readonly [number, number];
};

function frameAt(t: number, origin: number): Frame {
  const loopT = ((t % TOTAL) + TOTAL) % TOTAL;
  const step: 0 | 1 = loopT < STEP_START[1] ? 0 : 1;
  const s = STEPS[step];
  const tau = loopT - STEP_START[step];
  // Arrived here by playing on (not by a jump or first load): animate the hand-over.
  const natural = t - tau > origin + 1;

  const fill = clamp01((tau - s.fillStart) / (s.fillEnd - s.fillStart));
  const singing = Math.min(ramp(tau, s.fillStart - 120, 240), 1 - ramp(tau, s.fillEnd - 120, 240));
  const env = voiceEnv(fill);
  const outgoing = natural ? 1 - ramp(tau, 0, 180) : 0;
  const incoming = natural ? ramp(tau, 160, 220) : 1;

  const shared = {
    t,
    step,
    progress: tau / s.dur,
    fill,
    prevAlpha: natural ? 1 - ramp(tau, 0, 350) : 0,
    singing,
    voice: singing * (0.35 + 0.65 * env),
    mouth: singing * env,
    playhead: fill > 0 && fill < 1 ? singing : 0,
    songProgress: 0.08 + 0.84 * (loopT / TOTAL),
  };

  if (step === 0) {
    const lift = natural ? ramp(tau, 0, 450) : 1; // headphones come off
    return {
      ...shared,
      music: ramp(tau, 0, 450),
      hp: 1 - lift,
      hpLift: -12 * lift,
      hpMusic: 0,
      amber: ramp(tau, 3000, 350),
      check: natural ? 1 - ramp(tau, 0, 300) : 0,
      songLegend: 1,
      captions: [incoming, outgoing],
    };
  }

  const drop = ramp(tau, 0, 650); // headphones go on
  return {
    ...shared,
    music: natural ? 1 - ramp(tau, 0, 450) : 0,
    hp: drop,
    hpLift: -12 * (1 - drop),
    hpMusic: drop,
    amber: natural ? 1 - ramp(tau, 0, 300) : 0,
    check: ramp(tau, 3450, 400),
    songLegend: natural ? 1 - ramp(tau, 0, 350) : 0,
    captions: [outgoing, incoming],
  };
}

/** Finished-take frames for the reduced-motion strip. */
function staticFrame(step: 0 | 1): Frame {
  const t = STEP_START[step] + STEPS[step].dur - 10;
  const f = frameAt(t, STEP_START[step]);
  return { ...f, singing: 1, voice: 0.9, mouth: 0.75, playhead: 0, captions: [1, 1] };
}

/* ------------------------------------------------------------------ drawing */

const MUSIC_ROUTE: readonly [Pt, Pt, Pt, Pt] = [
  [200, 123], // left speaker
  [194, 68],
  [166, 44],
  [148.5, 75.5], // into the top of the mic
];
const MUSIC_D = `M${MUSIC_ROUTE[0].join(" ")}C${MUSIC_ROUTE.slice(1)
  .map((p) => p.join(" "))
  .join(" ")}`;

const GRILLE: ReadonlyArray<Pt> = [
  [205, 126.5], [208, 126.5], [203, 131], [206, 131], [201, 135.5], [204, 135.5],
  [304, 126.5], [307, 126.5], [306, 131], [309, 131], [308, 135.5], [311, 135.5],
];

type StageProps = { f: Frame; uid: string; compact?: boolean; paused?: boolean };

function Stage({ f, uid, compact = false, paused = false }: StageProps) {
  const t = useT();
  const sp = 220 + 72 * f.songProgress;
  const px = WX + f.fill * WW;
  const recDot = 0.35 + 0.65 * f.singing * (0.7 + 0.3 * Math.cos((2 * Math.PI * f.t) / 1100));
  const timer = `0:${(f.fill * TAKE_SECONDS).toFixed(1).padStart(4, "0")}`;

  const take = (which: 0 | 1, p: number, alpha: number) =>
    alpha > 0.001 && p > 0 ? (
      <g key={`take-${which}-${alpha === 1 ? "now" : "prev"}`} opacity={alpha}>
        <path d={wavePath(voice, p)} stroke={C.good} />
        {which === 0 && <path d={wavePath(song, p)} stroke={C.muted} />}
      </g>
    ) : null;

  return (
    <svg
      className="hpx-svg"
      viewBox="0 0 360 260"
      width="100%"
      aria-hidden="true"
      focusable="false"
      style={{ display: "block", height: "auto" }}
    >
      <defs>
        <clipPath id={`${uid}-l`}>
          <rect x={65.5} y={66.5} width={8} height={17} rx={4} />
        </clipPath>
        <clipPath id={`${uid}-r`}>
          <rect x={102.5} y={66.5} width={8} height={17} rx={4} />
        </clipPath>
      </defs>

      <g
        fill="none"
        stroke={C.muted}
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeLinejoin="round"
        fontFamily={FONT}
      >
        {/* panel + desk */}
        <rect x={0.75} y={0.75} width={358.5} height={258.5} rx={14} fill={C.panel} stroke={C.line} />
        <path d="M24 140H336" stroke={C.line} />

        {/* the route the song takes from speaker to mic */}
        <path d={MUSIC_D} stroke={C.accentDark} strokeDasharray="2 5" opacity={0.6 * f.music} />

        {/* laptop: screen with a generic player */}
        <rect x={208} y={48} width={96} height={70} rx={9} fill={C.panel} />
        <rect x={220} y={60} width={24} height={24} rx={5} stroke={C.line} />
        <circle cx={229} cy={77.5} r={2.4} fill={C.accentLight} stroke="none" />
        <path d="M231.4 77.5V66.5q4.5 1.5 4.5 5.5" stroke={C.accentLight} />
        <path d="M252 66.5H292" />
        <path d="M252 73H278" stroke={C.line} />
        <g opacity={1 - f.hp}>
          <path d="M252 82.5h2.5l4-3v10l-4-3H252z" />
          <path d="M261.5 81.5q2.6 3 0 6" stroke={C.accentLight} opacity={f.music} />
        </g>
        <g opacity={f.hp} stroke={C.accentLight}>
          <path d="M253 87v-1a5.5 5.5 0 0 1 11 0v1" />
          <rect x={251.5} y={86} width={3} height={4.5} rx={1.5} />
          <rect x={262.5} y={86} width={3} height={4.5} rx={1.5} />
        </g>
        <path d="M220 102H292" stroke={C.line} />
        <path d={`M220 102H${sp.toFixed(1)}`} stroke={C.accent} />
        <circle cx={sp} cy={102} r={2.5} fill={C.accentLight} stroke="none" />

        {/* laptop: base with speaker grilles */}
        <path d="M204 122H308L318 140H194Z" fill={C.panel} />
        <path d="M216 126.5H296M214 131H298M212 135.5H300" stroke={C.line} />
        {GRILLE.map(([x, y], i) => (
          <circle key={`g${i}`} cx={x} cy={y} r={1} fill={C.muted} stroke="none" />
        ))}
        <g opacity={f.music}>
          {GRILLE.map(([x, y], i) => (
            <circle key={`ga${i}`} cx={x} cy={y} r={1} fill={C.accentLight} stroke="none" />
          ))}
        </g>

        {/* sound leaving both speakers */}
        {f.music > 0.001 &&
          [0, 1, 2].map((k) => {
            const q = frac(f.t / 1300 + k / 3);
            const r = 4 + 12 * q;
            return (
              <g key={`rip${k}`} stroke={C.accent} opacity={f.music * (1 - q) * Math.min(1, q * 5)}>
                <path d={arc(203, 131, r, 160, 240)} />
                <path d={arc(309, 131, r, -60, 20)} />
              </g>
            );
          })}

        {/* song travelling along the route and curling into the mic */}
        {f.music > 0.001 &&
          [0, 1, 2, 3].map((i) => {
            const u = frac(f.t / 1800 + i / 4);
            const p = cubic(MUSIC_ROUTE[0], MUSIC_ROUTE[1], MUSIC_ROUTE[2], MUSIC_ROUTE[3], u);
            const s = 1.15 - 0.5 * u;
            return (
              <path
                key={`m${i}`}
                d="M-1.5-5.5Q3 0-1.5 5.5"
                stroke={C.accent}
                opacity={f.music * edge(u, 0.12, 0.2)}
                transform={`translate(${p.x.toFixed(2)} ${p.y.toFixed(2)}) rotate(${p.angle.toFixed(1)}) scale(${s.toFixed(3)})`}
              />
            );
          })}

        {/* microphone on a desk stand */}
        <path d="M130 140v-2.5a2 2 0 0 1 2-2h20a2 2 0 0 1 2 2V140" />
        <path d="M142 97V135.5" />
        <path d="M132 84v3a10 10 0 0 0 20 0v-3" />
        <rect x={135} y={70} width={14} height={24} rx={7} fill={C.panel} stroke={C.text} />
        <path d="M138.5 77.5H145.5M138 82H146M138.5 86.5H145.5" />

        {/* singer, three-quarter view facing the mic */}
        <path d="M50 140C50 113 66 101 88 101C110 101 126 113 126 140" stroke={C.text} fill={C.panel} />
        <circle cx={88} cy={74} r={17} fill={C.panel} stroke={C.text} />
        <path d="M71.3 70q-3.6 4 0 8" stroke={C.text} />
        <circle cx={96} cy={71} r={1.3} fill={C.text} stroke="none" />
        <ellipse cx={97.5} cy={82} rx={2.2} ry={1 + 2.6 * f.mouth} stroke={C.text} />

        {/* headphones */}
        {f.hp > 0.001 && (
          <g opacity={f.hp} transform={`translate(0 ${f.hpLift.toFixed(2)})`}>
            <path d="M69.5 66C69.5 46 106.5 46 106.5 66" stroke={C.accentLight} />
            <rect x={64} y={65} width={11} height={20} rx={5.5} fill={C.panel} stroke={C.accentLight} />
            <rect x={101} y={65} width={11} height={20} rx={5.5} fill={C.panel} stroke={C.accentLight} />
            {f.hpMusic > 0.001 &&
              [0, 1].map((k) => {
                const q = frac(f.t / 950 + k / 2);
                const r = 1 + 9 * q;
                return (
                  <g key={`hp${k}`} stroke={C.accent} opacity={f.hpMusic * (1 - q) * Math.min(1, q * 4)}>
                    <circle cx={69.5} cy={75} r={r} clipPath={`url(#${uid}-l)`} />
                    <circle cx={106.5} cy={75} r={r} clipPath={`url(#${uid}-r)`} />
                  </g>
                );
              })}
          </g>
        )}

        {/* voice travelling from mouth to mic */}
        {f.voice > 0.001 &&
          [0, 1, 2].map((i) => {
            const u = frac(f.t / 950 + i / 3);
            return (
              <path
                key={`v${i}`}
                d="M-1.2-4Q2 0-1.2 4"
                stroke={C.good}
                opacity={f.voice * Math.sin(Math.PI * u)}
                transform={`translate(${(105 + 27 * u).toFixed(2)} 82) scale(${(0.8 + 0.45 * u).toFixed(3)})`}
              />
            );
          })}

        {/* amber: the mic is hearing the song too */}
        {f.amber > 0.001 && (
          <g opacity={f.amber} transform={`translate(161 104) scale(${(0.85 + 0.15 * f.amber).toFixed(3)})`}>
            <circle r={7} fill={C.amber} stroke="none" />
            <path d="M0-3.4V0.4" stroke={C.bg} />
            <circle cy={2.9} r={0.95} fill={C.bg} stroke="none" />
          </g>
        )}

        {/* green check by the headphones */}
        {f.check > 0.001 && (
          <g opacity={f.check} transform={`translate(123 57) scale(${(0.85 + 0.15 * f.check).toFixed(3)})`}>
            <circle r={7} fill={C.good} stroke="none" />
            <path d="M-3 0.2L-0.9 2.3L3.1-2" stroke={C.bg} />
          </g>
        )}

        {/* the recording */}
        <rect x={16} y={158} width={328} height={88} rx={12} fill={C.bg} stroke={C.line} />
        <circle cx={31} cy={175} r={3} fill={C.red} stroke="none" opacity={recDot} />
        {!compact && (
          <>
            <text x={39} y={178.5} fontSize={10} fontWeight={600} letterSpacing={0.8} fill={C.muted} stroke="none">
              {t("guides.speakers.rec")}
            </text>
            <path d="M188 175h10" stroke={C.good} />
            <text x={202} y={178.5} fontSize={10} fill={C.muted} stroke="none">
              {t("guides.speakers.voice")}
            </text>
            <g opacity={f.songLegend}>
              <path d="M238 175h10" stroke={C.muted} />
              <text x={252} y={178.5} fontSize={10} fill={C.muted} stroke="none">
                {t("guides.speakers.song")}
              </text>
            </g>
            <text
              x={330}
              y={178.5}
              textAnchor="end"
              fontSize={10}
              fill={C.muted}
              stroke="none"
              style={{ fontVariantNumeric: "tabular-nums" }}
            >
              {timer}
            </text>
          </>
        )}
        <path d={`M${WX} ${WY}H${WX + WW}`} stroke={C.line} />
        {take(f.step === 0 ? 1 : 0, 1, f.prevAlpha)}
        {take(f.step, f.fill, 1)}
        {f.playhead > 0.001 && (
          <path d={`M${px.toFixed(1)} 193V231`} stroke={C.text} opacity={0.55 * f.playhead} />
        )}

        {/* paused */}
        <g opacity={paused ? 1 : 0} style={{ transition: "opacity 200ms ease-in-out" }}>
          <rect x={334} y={14} width={3} height={10} rx={1.2} fill={C.muted} stroke="none" />
          <rect x={340} y={14} width={3} height={10} rx={1.2} fill={C.muted} stroke="none" />
        </g>
      </g>
    </svg>
  );
}

/* ------------------------------------------------------------------ component */

const CSS = `
.hpx-svg * { vector-effect: non-scaling-stroke; }
.hpx-dot {
  appearance: none; background: none; border: 0; margin: 0; padding: 0;
  width: 28px; height: 24px; border-radius: 12px; cursor: pointer;
  display: inline-grid; place-items: center;
}
.hpx-dot:focus-visible { outline: 1.5px solid ${C.accentLight}; outline-offset: 0; }
.hpx-track {
  display: block; width: 8px; height: 8px; border-radius: 4px; overflow: hidden;
  background: ${C.line};
  transition: width 350ms ease-in-out, background-color 200ms ease-in-out;
}
.hpx-dot:not([aria-current]):hover .hpx-track { background: ${C.muted}; }
.hpx-dot[aria-current="step"] .hpx-track { width: 22px; }
.hpx-fill { display: block; height: 100%; background: ${C.accent}; }
`;

const wrapStyle: CSSProperties = {
  boxSizing: "border-box",
  width: "100%",
  padding: 10,
  borderRadius: 16,
  background: C.bg,
  color: C.text,
  fontFamily: FONT,
};

const captionStyle: CSSProperties = {
  gridArea: "1 / 1",
  margin: 0,
  fontSize: 13,
  lineHeight: 1.4,
  textAlign: "center",
  textWrap: "balance",
};

// The guides always animate, also with Reduce motion on (Daniel asked, 29 Oct). Smaller motion elsewhere in
// OnKey still follows the setting.
function usePrefersReducedMotion(): boolean {
  return false;
}

export type SpeakersVsHeadphonesProps = {
  /** Force the static strip (true) or the animation (false). Defaults to the animation. */
  reducedMotion?: boolean;
  className?: string;
  style?: CSSProperties;
};

export default function SpeakersGuide({ reducedMotion, className, style }: SpeakersVsHeadphonesProps) {
  const t = useT();
  const uid = "hpx" + useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const prefersReduced = usePrefersReducedMotion();
  const isStatic = reducedMotion ?? prefersReduced;

  const clock = useRef({ t: 0, origin: 0 });
  const pausedRef = useRef(false);
  const [paused, setPaused] = useState(false);
  const [snap, setSnap] = useState({ t: 0, origin: 0 });

  useEffect(() => {
    if (isStatic) return;
    let raf = 0;
    let last = -1;
    const tick = (now: number) => {
      if (last >= 0 && !pausedRef.current) {
        const c = clock.current;
        c.t += Math.min(now - last, 50); // no jump after a background tab
        if (c.t > TOTAL * 1000) {
          c.t -= TOTAL * 900;
          c.origin -= TOTAL * 900;
        }
        setSnap({ t: c.t, origin: c.origin });
      }
      last = now;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [isStatic]);

  const jump = useCallback((i: 0 | 1) => {
    clock.current = { t: STEP_START[i], origin: STEP_START[i] };
    setSnap({ ...clock.current });
  }, []);

  const onEnter = useCallback((e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.pointerType === "touch") return; // hover only; a tap should not stick
    pausedRef.current = true;
    setPaused(true);
  }, []);
  const onLeave = useCallback(() => {
    pausedRef.current = false;
    setPaused(false);
  }, []);

  if (isStatic) {
    return (
      <div className={className} style={{ ...wrapStyle, ...style }}>
        <style>{CSS}</style>
        <div
          role="img"
          aria-label={t("guides.speakers.label")}
          style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)", gap: 8 }}
        >
          {STEPS.map((s, i) => (
            <div key={s.id} style={{ display: "grid", gap: 8, alignContent: "start" }}>
              <Stage f={staticFrame(i as 0 | 1)} uid={`${uid}-s${i}`} compact />
              <p style={{ margin: 0, fontSize: 12, lineHeight: 1.4, textWrap: "balance" }}>
                <span style={{ color: C.muted, fontVariantNumeric: "tabular-nums" }}>{i + 1} </span>
                {caption(t, s)}
              </p>
            </div>
          ))}
        </div>
      </div>
    );
  }

  const f = frameAt(snap.t, snap.origin);

  return (
    <div className={className} style={{ ...wrapStyle, ...style }}>
      <style>{CSS}</style>
      <div role="img" aria-label={t("guides.speakers.label")}>
        <div onPointerEnter={onEnter} onPointerLeave={onLeave}>
          <Stage f={f} uid={uid} paused={paused} />
        </div>
        <div aria-hidden="true" style={{ display: "grid", marginTop: 10, padding: "0 6px" }}>
          {STEPS.map((s, i) => (
            <p key={s.id} style={{ ...captionStyle, opacity: f.captions[i] }}>
              {caption(t, s)}
            </p>
          ))}
        </div>
      </div>
      <div role="group" aria-label={t("guides.speakers.stepsGroup")} style={{ display: "flex", justifyContent: "center", gap: 2, marginTop: 4 }}>
        {STEPS.map((s, i) => {
          const active = f.step === i;
          return (
            <button
              key={s.id}
              type="button"
              className="hpx-dot"
              aria-label={t("guides.speakers.stepLabel", { n: i + 1, count: STEPS.length, caption: caption(t, s) })}
              aria-current={active ? "step" : undefined}
              onClick={() => jump(i as 0 | 1)}
            >
              <span className="hpx-track">
                <span className="hpx-fill" style={{ width: active ? `${(f.progress * 100).toFixed(1)}%` : "0%" }} />
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
