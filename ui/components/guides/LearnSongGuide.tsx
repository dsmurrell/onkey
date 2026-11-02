"use client";

/**
 * How OnKey learns a song (Setup's last step, and the empty Library).
 *
 * Scene 1: a song plays once in a generic music player; sound waves flow into
 * the OnKey icon while the melody is drawn as note bars in step with the play
 * bar. When the bar reaches the end, a "Learning…" ring spins, then turns into
 * a green check ("Learned").
 * Scene 2: the same song plays again; the note bars scroll past a playhead with
 * the singer's green pitch line riding on them and the lyrics underneath.
 *
 * Self-contained: React only, inline SVG driven by requestAnimationFrame.
 * Fits a 360 x 260 box and scales to its container width. Pauses on hover.
 * It animates even with Reduce motion on (mode="still" shows the four steps as a static strip).
 *
 * Usage (Next.js): import OnKeyLearnSong from "@/components/OnKeyLearnSong";
 *                  <OnKeyLearnSong />
 * Optional prop `mode`: "auto" (default, animated),
 * "animated" or "still".
 */

import { useEffect, useId, useRef, useState } from "react";
import StillSteps from "./StillSteps";
import { useT } from "@/lib/i18n";

/* ---------- palette (the only colours used) ---------- */
import { GUIDE as C } from "@/lib/colors";
import { Fit } from "./Fit";

const FONT = "system-ui, sans-serif";

/* ---------- geometry ---------- */
const VIEW_W = 360;
const VIEW_H = 208;
const LANE_X0 = 28;
const LANE_X1 = 332;
const LANE_W = LANE_X1 - LANE_X0;
const PLAYHEAD_X = 92; // scene 2: fixed "now" line
const ZOOM = 680; // scene 2: px per whole song
const TRACK_X = 56;
const TRACK_W = 124;
const RING = { cx: 217, cy: 60, r: 6.5 };
const RING_C = 2 * Math.PI * RING.r;
const CHECK_LEN = 9.4;

/** Bar centre for a pitch row (0 = lowest). */
const yOf = (p: number) => 174 - p * 7;

/* ---------- timeline (ms) ---------- */
const STEPS = [
  { id: "s0", start: 0 },   // captions: guides.learnSong.steps.<id>
  { id: "s1", start: 3200 },
  { id: "s2", start: 6800 },
  { id: "s3", start: 9800 },
] as const;

const LOOP_MS = 15600;

const T = {
  playStart: 200,
  playEnd: 6600,
  wavesIn: 300,
  wavesOut: 6800,
  micIn: 3400,
  micOut: 6900,
  pillIn: 6900,
  learnedAt: 8650,
  replayAt: 9800,
  song2Start: 10400,
  song2End: 15000,
} as const;

/** Representative moments for the reduced-motion strip, one per step. */
const STILL_AT = [2600, 5600, 9400, 13200] as const;

/* ---------- melody (invented tune and words; the words: guides.learnSong.lyric.<word>) ---------- */
type Note = { s: number; d: number; p: number; word: `w${0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10}` };

const MELODY: readonly Note[] = [
  { s: 0.0, d: 0.07, p: 1, word: "w0" },
  { s: 0.085, d: 0.05, p: 2, word: "w1" },
  { s: 0.15, d: 0.12, p: 4, word: "w2" },
  { s: 0.29, d: 0.05, p: 3, word: "w3" },
  { s: 0.355, d: 0.055, p: 2, word: "w4" },
  { s: 0.425, d: 0.055, p: 3, word: "w5" },
  { s: 0.495, d: 0.1, p: 5, word: "w6" },
  { s: 0.61, d: 0.09, p: 4, word: "w7" },
  { s: 0.725, d: 0.055, p: 2, word: "w8" },
  { s: 0.795, d: 0.06, p: 1, word: "w9" },
  { s: 0.87, d: 0.13, p: 0, word: "w10" },
];

/* ---------- easing helpers ---------- */
const clamp01 = (x: number) => (x < 0 ? 0 : x > 1 ? 1 : x);
/** Gentle ease-in-out (sine). */
const ease = (x: number) => 0.5 - 0.5 * Math.cos(Math.PI * clamp01(x));
/** Eased 0→1 as t moves from a to b. */
const ramp = (t: number, a: number, b: number) => ease((t - a) / (b - a));
/** Eased in at a, eased out at b. */
const span = (t: number, a: number, b: number, fade = 400) =>
  Math.min(ramp(t, a, a + fade), 1 - ramp(t, b - fade, b));

const stepAt = (t: number) => {
  let i = 0;
  for (let k = 0; k < STEPS.length; k++) if (t >= STEPS[k].start) i = k;
  return i;
};
const stepEnd = (i: number) => (i + 1 < STEPS.length ? STEPS[i + 1].start : LOOP_MS);

/** The singer's pitch at song time s: glides into each note, light vibrato, one note sung a touch sharp. */
function sungPitch(s: number): number {
  for (let i = 0; i < MELODY.length; i++) {
    const n = MELODY[i];
    const prevP = i > 0 ? MELODY[i - 1].p : n.p;
    if (s < n.s) {
      // gap before this note: hold the previous pitch with a slight dip
      const prev = MELODY[i - 1] ?? n;
      const gapStart = prev.s + prev.d;
      const k = clamp01((s - gapStart) / Math.max(0.001, n.s - gapStart));
      return prevP - 0.15 * Math.sin(Math.PI * k);
    }
    if (s <= n.s + n.d) {
      const into = s - n.s;
      const base = prevP + (n.p - prevP) * ease(into / 0.014);
      const sustain = clamp01((into - 0.025) / 0.02) * clamp01((n.d - into) / 0.015);
      const vibrato = 0.16 * Math.sin(into * 210) * sustain;
      const drift = i === 6 ? 0.35 * (1 - ease(into / 0.06)) : 0;
      return base + vibrato + drift;
    }
  }
  return MELODY[MELODY.length - 1].p;
}

/* ---------- one frame of the animation, as a pure function of time ---------- */
const Scene = ({ ms, uid }: { ms: number; uid: string }) => {
  const t = useT();
  const learnP = clamp01((ms - T.playStart) / (T.playEnd - T.playStart));
  const waves = span(ms, T.wavesIn, T.wavesOut, 450);
  const mic = span(ms, T.micIn, T.micOut, 400);
  const pill = span(ms, T.pillIn, LOOP_MS, 400);
  const done = ramp(ms, T.learnedAt, T.learnedAt + 300);
  const check = ramp(ms, T.learnedAt + 150, T.learnedAt + 550);
  const scene1 = 1 - ramp(ms, T.replayAt, T.replayAt + 400);
  const scene2 = span(ms, T.replayAt + 200, LOOP_MS, 400);
  const pos = clamp01((ms - T.song2Start) / (T.song2End - T.song2Start));

  const replay = ms >= T.replayAt;
  const bar = replay ? (ms < T.song2Start ? 1 : pos) : learnP;
  const barAlpha = replay
    ? ms < T.song2Start
      ? 1 - ramp(ms, T.replayAt, T.replayAt + 350)
      : 1 - ramp(ms, LOOP_MS - 400, LOOP_MS)
    : 1;
  const playing = Math.max(
    span(ms, T.playStart - 50, T.playEnd + 50, 200),
    span(ms, T.song2Start - 50, T.song2End + 50, 200),
  );

  const arcLen = RING_C * (0.18 + 0.7 * ramp(ms, T.pillIn, T.learnedAt));
  const spin = -90 + Math.max(0, ms - T.pillIn) * 0.32;
  const halo = waves * (0.3 + 0.15 * Math.sin((ms / 1400) * 2 * Math.PI));
  const headAlpha = span(ms, T.playStart, T.playEnd + 250, 250) * scene1;
  const headX = LANE_X0 + learnP * LANE_W;

  // scene 2: pitch trail from the lane's left edge up to the playhead
  const trail: { d: string; o: number }[] = [];
  if (pos > 0) {
    const s0 = Math.max(0, pos - (PLAYHEAD_X - LANE_X0) / ZOOM);
    const N = 36;
    const pts: string[] = [];
    for (let k = 0; k <= N; k++) {
      const s = s0 + (pos - s0) * (k / N);
      pts.push(`${(PLAYHEAD_X + (s - pos) * ZOOM).toFixed(2)} ${yOf(sungPitch(s)).toFixed(2)}`);
    }
    const parts = 6;
    for (let j = 0; j < parts; j++) {
      const a = Math.floor((j * N) / parts);
      const b = Math.floor(((j + 1) * N) / parts);
      trail.push({ d: `M${pts.slice(a, b + 1).join(" L")}`, o: (j + 1) / parts });
    }
  }
  const singerY = yOf(sungPitch(pos));
  const singerAlpha = ramp(ms, T.song2Start, T.song2Start + 200) * (1 - ramp(ms, T.song2End, T.song2End + 300));

  const stroke = { fill: "none", strokeWidth: 1.5, strokeLinecap: "round", strokeLinejoin: "round" } as const;

  return (
    <>
      <defs>
        <clipPath id={`${uid}-reveal`}>
          <rect x={LANE_X0} y={118} width={Math.max(0.01, learnP * LANE_W)} height={82} />
        </clipPath>
        <clipPath id={`${uid}-lane`}>
          <rect x={LANE_X0} y={120} width={LANE_W} height={78} />
        </clipPath>
      </defs>

      {/* music player */}
      <rect x={16} y={12} width={176} height={96} rx={14} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
      <rect x={28} y={24} width={32} height={32} rx={8} fill={C.bg} stroke={C.line} strokeWidth={1.5} />
      <path d="M40 47 V33 L52 30 V44" stroke={C.accentLight} {...stroke} />
      <circle cx={37.5} cy={47} r={2.5} fill={C.accentLight} />
      <circle cx={49.5} cy={44} r={2.5} fill={C.accentLight} />
      <rect x={70} y={29} width={72} height={6} rx={3} fill={C.text} opacity={0.9} />
      <rect x={70} y={42} width={48} height={5} rx={2.5} fill={C.muted} opacity={0.75} />

      <circle cx={36} cy={86} r={11} fill={C.bg} stroke={C.line} strokeWidth={1.5} />
      <g opacity={playing}>
        <rect x={32} y={81} width={2.6} height={10} rx={1.3} fill={C.accentLight} />
        <rect x={37.4} y={81} width={2.6} height={10} rx={1.3} fill={C.accentLight} />
      </g>
      <path
        d="M33.5 81 L41 86 L33.5 91 Z"
        fill={C.accentLight}
        stroke={C.accentLight}
        strokeWidth={1.5}
        strokeLinejoin="round"
        opacity={1 - playing}
      />
      <rect x={TRACK_X} y={84} width={TRACK_W} height={4} rx={2} fill={C.line} />
      <g opacity={barAlpha}>
        <rect x={TRACK_X} y={84} width={Math.max(0.01, bar * TRACK_W)} height={4} rx={2} fill={C.accent} />
        <circle cx={TRACK_X + bar * TRACK_W} cy={86} r={4} fill={C.accentLight} />
      </g>

      {/* sound waves flowing from the player into OnKey */}
      {waves > 0.001 &&
        [0, 1, 2].map((i) => {
          const ph = (ms / 1400 + i / 3) % 1;
          const x = 200 + ph * 88;
          const h = 7 + 6 * Math.sin(Math.PI * ph);
          return (
            <path
              key={i}
              d={`M${x.toFixed(2)} ${(60 - h).toFixed(2)} Q${(x + h * 0.75).toFixed(2)} 60 ${x.toFixed(2)} ${(60 + h).toFixed(2)}`}
              stroke={C.accentLight}
              opacity={waves * Math.sin(Math.PI * ph) * 0.9}
              {...stroke}
            />
          );
        })}

      {/* Learning… → Learned */}
      <g opacity={pill}>
        <rect x={204} y={48} width={86} height={24} rx={12} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
        <g opacity={1 - done}>
          <circle cx={RING.cx} cy={RING.cy} r={RING.r} stroke={C.line} {...stroke} />
          <circle
            cx={RING.cx}
            cy={RING.cy}
            r={RING.r}
            stroke={C.amber}
            strokeDasharray={`${arcLen.toFixed(2)} ${RING_C.toFixed(2)}`}
            transform={`rotate(${spin.toFixed(1)} ${RING.cx} ${RING.cy})`}
            {...stroke}
          />
          {/* (the pill ends at 290: its words fit before it) */}
          <Fit x={229} y={63.5} max={55} fontSize={10} fill={C.muted} fontFamily={FONT}>
            {t("guides.learnSong.learning")}
          </Fit>
        </g>
        <g opacity={done}>
          <circle cx={RING.cx} cy={RING.cy} r={RING.r} stroke={C.good} {...stroke} />
          <path
            d="M214 60.2 L216.3 62.5 L220.2 57.8"
            stroke={C.good}
            strokeDasharray={CHECK_LEN}
            strokeDashoffset={CHECK_LEN * (1 - check)}
            {...stroke}
          />
          <Fit x={229} y={63.5} max={55} fontSize={10} fontWeight={600} fill={C.good} fontFamily={FONT}>
            {t("guides.learnSong.learned")}
          </Fit>
        </g>
      </g>

      {/* OnKey icon (generic: rounded tile with three rising note bars) */}
      <rect x={296} y={34} width={52} height={52} rx={16} stroke={C.accent} opacity={halo} {...stroke} />
      <rect x={300} y={38} width={44} height={44} rx={12} fill={C.panel} stroke={C.accent} strokeWidth={1.5} />
      <rect x={309} y={65} width={8} height={4} rx={2} fill={C.accentLight} />
      <rect x={318} y={58} width={8} height={4} rx={2} fill={C.accentLight} />
      <rect x={327} y={51} width={8} height={4} rx={2} fill={C.accentLight} />
      <text x={322} y={99} fontSize={10} fontWeight={500} textAnchor="middle" fill={C.muted} fontFamily={FONT}>
        {t("guides.learnSong.appName")}
      </text>

      {/* "not your mic" badge */}
      <g opacity={mic}>
        <circle cx={342} cy={40} r={8.5} fill={C.bg} stroke={C.line} strokeWidth={1.5} />
        <rect x={339.6} y={34.6} width={4.8} height={7} rx={2.4} stroke={C.muted} {...stroke} />
        <path d="M337.8 40.6 a4.2 4.2 0 0 0 8.4 0 M342 44.8 V46" stroke={C.muted} {...stroke} />
        <path d="M337 35.5 L347 45.5" stroke={C.bg} strokeWidth={3.5} strokeLinecap="round" />
        <path d="M337 35.5 L347 45.5" stroke={C.muted} {...stroke} />
      </g>

      {/* melody panel */}
      <rect x={16} y={118} width={328} height={82} rx={14} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
      {[1, 3, 5].map((p) => (
        <line
          key={p}
          x1={LANE_X0}
          x2={LANE_X1}
          y1={yOf(p)}
          y2={yOf(p)}
          stroke={C.line}
          strokeWidth={1.5}
          strokeLinecap="round"
          strokeDasharray="0.1 5"
        />
      ))}
      <rect x={LANE_X0} y={126} width={10} height={4} rx={2} fill={C.accent} />
      <text x={42} y={131} fontSize={8.5} fontWeight={600} letterSpacing="0.08em" fill={C.muted} fontFamily={FONT}>
        {t("guides.learnSong.melody")}
      </text>

      {/* scene 1: melody drawn left to right, in step with the play bar */}
      <g opacity={scene1}>
        <g clipPath={`url(#${uid}-reveal)`}>
          {MELODY.map((n, i) => (
            <rect
              key={i}
              x={LANE_X0 + n.s * LANE_W}
              y={yOf(n.p) - 3}
              width={n.d * LANE_W - 1}
              height={6}
              rx={3}
              fill={C.accent}
            />
          ))}
        </g>
        <g opacity={headAlpha}>
          <line x1={headX} x2={headX} y1={130} y2={184} stroke={C.accentLight} strokeWidth={1.5} opacity={0.8} />
          <circle cx={headX} cy={130} r={2.5} fill={C.accentLight} />
        </g>
      </g>

      {/* scene 2: notes scroll by, the singer's pitch rides on them, lyrics underneath */}
      <g opacity={scene2}>
        <path d="M300 128 H310" stroke={C.good} {...stroke} />
        <text x={LANE_X1} y={131} fontSize={8.5} fontWeight={600} letterSpacing="0.08em" textAnchor="end" fill={C.muted} fontFamily={FONT}>
          {t("guides.learnSong.you")}
        </text>
        <line
          x1={PLAYHEAD_X}
          x2={PLAYHEAD_X}
          y1={134}
          y2={182}
          stroke={C.accentLight}
          strokeWidth={1.5}
          strokeLinecap="round"
          strokeDasharray="0.1 4"
        />
        <g clipPath={`url(#${uid}-lane)`}>
          {MELODY.map((n, i) => {
            const x = PLAYHEAD_X + (n.s - pos) * ZOOM;
            const w = n.d * ZOOM - 1.5;
            if (x > LANE_X1 + 8 || x + w < LANE_X0 - 8) return null;
            const fade = clamp01((x + w - LANE_X0) / 24) * clamp01((LANE_X1 - x) / 24);
            const active = pos >= n.s && pos <= n.s + n.d;
            const passed = pos > n.s + n.d;
            return (
              <g key={i} opacity={fade}>
                <rect
                  x={x}
                  y={yOf(n.p) - 3}
                  width={w}
                  height={6}
                  rx={3}
                  fill={active ? C.accentLight : passed ? C.accentDark : C.accent}
                />
                <text x={x + 1} y={192} fontSize={9.5} fill={active ? C.text : C.muted} fontFamily={FONT}>
                  {t(`guides.learnSong.lyric.${n.word}`)}
                </text>
              </g>
            );
          })}
          {trail.map((seg, i) => (
            <path key={i} d={seg.d} stroke={C.good} opacity={seg.o} fill="none" strokeWidth={1.5} strokeLinejoin="round" />
          ))}
        </g>
        <circle cx={PLAYHEAD_X} cy={singerY} r={3} fill={C.good} opacity={singerAlpha} />
      </g>
    </>
  );
};


/* ---------- hooks ---------- */
// The guides always animate, also with Reduce motion on (Daniel asked, 29 Oct). Smaller motion elsewhere in
// OnKey still follows the setting.
function usePrefersReducedMotion(): boolean {
  return false;
}

/* ---------- styles (scoped by class prefix) ---------- */
const CSS = `
.okl-root{container-type:inline-size;width:100%;font-family:${FONT};color:${C.text}}
.okl-card{background:${C.bg};border-radius:16px;overflow:hidden}
.okl-stage{display:block;width:100%;height:auto}
.okl-caption{display:grid;margin:0;padding:0.8cqw 3.4cqw 0;text-align:center;font-size:clamp(11px,3.5cqw,20px);line-height:1.35;text-wrap:balance;color:${C.text}}
.okl-caption-text{grid-area:1/1;opacity:0;transition:opacity 360ms ease-in-out}
.okl-caption-text[data-active="true"]{opacity:1}
.okl-dots{display:flex;justify-content:center;gap:0.4cqw;padding:0.6cqw 0 1.2cqw}
.okl-dot{appearance:none;-webkit-appearance:none;border:0;margin:0;padding:0;background:transparent;cursor:pointer;width:clamp(22px,6.7cqw,34px);height:clamp(22px,6.7cqw,34px);display:grid;place-items:center;border-radius:12px}
.okl-dot:focus-visible{outline:1.5px solid ${C.accentLight};outline-offset:-3px}
.okl-dot-mark{position:relative;display:block;overflow:hidden;width:clamp(7px,2.2cqw,11px);height:clamp(7px,2.2cqw,11px);border-radius:999px;background:${C.line};transition:width 300ms ease-in-out,background-color 300ms ease-in-out}
.okl-dot:hover .okl-dot-mark{background:${C.muted}}
.okl-dot[aria-current="step"] .okl-dot-mark{width:clamp(16px,5cqw,24px);background:${C.accentDark}}
.okl-dot-fill{position:absolute;left:0;top:0;bottom:0;background:${C.accentLight};border-radius:999px}
.okl-still{display:none;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;padding:12px}
.okl-frame{min-width:0}
.okl-frame svg{display:block;width:100%;height:auto;border:1.5px solid ${C.line};border-radius:12px;box-sizing:border-box}
.okl-frame-cap{display:flex;gap:6px;margin:6px 2px 0;font-size:12px;line-height:1.35;color:${C.text}}
.okl-frame-num{color:${C.accentLight};font-weight:600;font-variant-numeric:tabular-nums}
.okl-mode-still .okl-motion{display:none}
.okl-mode-still .okl-still{display:grid}
`;

/* ---------- component ---------- */
export type OnKeyLearnSongProps = {
  /** "auto" and "animated" animate; "still" shows the static strip. */
  mode?: "auto" | "animated" | "still";
  className?: string;
};

export default function LearnSongGuide({ mode = "auto", className }: OnKeyLearnSongProps) {
  const t = useT();
  const uid = "okl" + useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const reduced = usePrefersReducedMotion();
  const animate = mode === "animated" || (mode === "auto" && !reduced);

  const [time, setTime] = useState(0);
  const timeRef = useRef(0);
  const pausedRef = useRef(false);

  useEffect(() => {
    if (!animate) return;
    let raf = 0;
    let last: number | null = null;
    const tick = (now: number) => {
      if (last !== null && !pausedRef.current) {
        timeRef.current = (timeRef.current + Math.min(64, now - last)) % LOOP_MS;
        setTime(timeRef.current);
      }
      last = now;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [animate]);

  const jump = (i: number) => {
    timeRef.current = STEPS[i].start + 1;
    setTime(timeRef.current);
  };

  if (!animate) {
    return <StillSteps count={STEPS.length} label={t("guides.learnSong.summary")} caption={i => t(`guides.learnSong.steps.${STEPS[i].id}`)}
                       frame={i => <svg viewBox={`0 0 ${VIEW_W} ${VIEW_H}`} style={{ display: "block", width: "100%", height: "auto" }} aria-hidden="true" focusable="false"><Scene ms={STILL_AT[i]} uid={`${uid}-s${i}`} /></svg>} />;
  }

  const step = stepAt(time);
  const progress = clamp01((time - STEPS[step].start) / (stepEnd(step) - STEPS[step].start));

  return (
    <div className={`okl-root okl-mode-${mode}${className ? ` ${className}` : ""}`}>
      <style>{CSS}</style>
      <div className="okl-card">
        {(
          <div className="okl-motion">
            <div
              onPointerEnter={(e) => {
                if (e.pointerType === "mouse") pausedRef.current = true;
              }}
              onPointerLeave={() => {
                pausedRef.current = false;
              }}
            >
              <svg
                className="okl-stage"
                viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
                role="img"
                aria-label={t("guides.learnSong.summary")}
                style={{ fontFamily: FONT }}
              >
                <Scene ms={time} uid={`${uid}-m`} />
              </svg>
              {/* every caption in the same place, only the current one shown: the box keeps the tallest's height */}
              <p className="okl-caption">
                {STEPS.map((s, i) => (
                  <span key={s.id} className="okl-caption-text" data-active={i === step} aria-hidden={i !== step}>
                    {t(`guides.learnSong.steps.${s.id}`)}
                  </span>
                ))}
              </p>
            </div>
            <div className="okl-dots" role="group" aria-label={t("guides.learnSong.stepsGroup")}>
              {STEPS.map((s, i) => (
                <button
                  key={i}
                  type="button"
                  className="okl-dot"
                  aria-label={t("guides.learnSong.stepLabel", { n: i + 1, count: STEPS.length, caption: t(`guides.learnSong.steps.${s.id}`) })}
                  aria-current={i === step ? "step" : undefined}
                  onClick={() => jump(i)}
                >
                  <span className="okl-dot-mark">
                    {i === step && <span className="okl-dot-fill" style={{ width: `${progress * 100}%` }} />}
                  </span>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
