"use client";

/**
 * Settings → "Fade the guide on songs you know", shown under the switch: four lanes of the same song (Take 1, 3,
 * 5, 8). The notes always stay; your pitch line fades while you're on the note, a little more each take, and a
 * slip of more than a semitone shows as usual (red, full strength). Last, "Full guide" brings the whole line back.
 *
 * - Self-contained: React only, inline SVG, requestAnimationFrame.
 *   No images, video, fonts or other libraries.
 * - Drawn on a 360 × 260 box and scales to the width of its container.
 * - Hovering with a mouse (or keyboard focus on the step dots) pauses it.
 *   The dots jump to a step.
 * - motion="reduce" shows all four takes as a static strip with their captions.
 *   It animates even with Reduce motion on (see usePrefersReducedMotion).
 *
 * Usage:  <GuideFadeLoop />   (wrap it in anything that sets the width)
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { FocusEvent, PointerEvent } from "react";
import { useT } from "@/lib/i18n";

export type FadeGuideDemoProps = {
  /** "auto" animates (also with Reduce motion on); "reduce" shows the static strip. */
  motion?: "auto" | "reduce";
  className?: string;
};

/* ------------------------------------------------------------------ palette */

import { GUIDE as C } from "@/lib/colors";

/* ------------------------------------------------------------------ content */

/** The step captions: guides.fadeDemo.captions.<id>. */
const CAPTIONS = ["c0", "c1", "c2", "c3"] as const;

/** fade = how strongly your line shows while on the note; slip = it goes more than a semitone off once. */
const TAKES = [
  { n: 1, fade: 1, slip: false, score: 82 },   // shown as "Take <n>"
  { n: 3, fade: 0.5, slip: false, score: 86 },
  { n: 5, fade: 0.25, slip: true, score: 84 },
  { n: 8, fade: 0.12, slip: false, score: 90 },
] as const;

/* ----------------------------------------------------------------- timeline */

/** Scene lengths in ms. The last scene also flips the switch and resets the loop. */
const SCENE_MS = [3800, 3800, 3800, 5800] as const;
const SCENE_START = SCENE_MS.map((_, i) => SCENE_MS.slice(0, i).reduce((a, b) => a + b, 0));
const LOOP_MS = SCENE_MS.reduce((a, b) => a + b, 0);

/** Phases inside a scene, [from, to] in ms of scene time. */
const T = {
  headIn: [100, 300],
  sing: [300, 2400],
  headOut: [2400, 2700],
  score: [2400, 2900],
  toggle: [3200, 3500],
  restore: [3500, 4200],
  reset: [5300, 5800],
} as const;

/** Where a step dot lands: the settled moment of each scene. */
const KEY_MS = [3000, 3000, 3000, 4500] as const;

const clamp01 = (x: number) => (x < 0 ? 0 : x > 1 ? 1 : x);
/** Gentle ease-in-out (sine). No overshoot. */
const ease = (x: number) => 0.5 - 0.5 * Math.cos(Math.PI * clamp01(x));
const span = (t: number, [a, b]: readonly [number, number]) => ease((t - a) / (b - a));
const smooth = (x: number) => {
  const k = clamp01(x);
  return k * k * (3 - 2 * k);
};

/* ----------------------------------------------------------------- geometry */
/* Everything is in units of the 360 × 260 design box. */

const VB_W = 360;
const VB_H = 196; // drawing; caption + dots fill the remaining 64

const LANE = { x0: 12, y: 36, w: 76.5, h: 126, gap: 10 } as const;
/** Piano-roll area, in lane-local units. */
const ROLL = { x0: 16, x1: 68.5, y0: 25, y1: 116 } as const;
const ROWS = 6; // semitones, bottom row = 0
const BEATS = 8;
const ROW_H = (ROLL.y1 - ROLL.y0) / ROWS;
const BEAT_W = (ROLL.x1 - ROLL.x0) / BEATS;
const BLACK_ROWS = [1, 3] as const;

/** The melody: [start beat, length in beats, row]. */
const NOTES: ReadonlyArray<readonly [number, number, number]> = [
  [0, 1.5, 0],
  [1.5, 1.5, 2],
  [3, 2, 5],
  [5, 1.5, 4],
  [6.5, 1.5, 2],
];

const rowCenter = (row: number) => ROLL.y1 - (row + 0.5) * ROW_H;
const rowTop = (row: number) => ROLL.y1 - (row + 1) * ROW_H;
const beatX = (beat: number) => ROLL.x0 + beat * BEAT_W;

/** The singer's pitch (in rows) at a given beat: on the note, short glides, light vibrato on long notes. */
function pitchAt(beat: number, slip = false): number {
  const p = pitchOnNote(beat);
  // a slip: under the 4th note, sagging well over a semitone, then back
  if (slip && beat > 5.15 && beat < 6.35) return p - 1.7 * Math.sin((Math.PI * (beat - 5.15)) / 1.2);
  return p;
}

/** The nearest note's row at a beat (the bar you're judged against). */
function noteRow(beat: number): number {
  const k = NOTES.findIndex(([s, l]) => beat < s + l);
  return NOTES[k < 0 ? NOTES.length - 1 : k][2];
}

function pitchOnNote(beat: number): number {
  let k = NOTES.findIndex(([s, l]) => beat < s + l);
  if (k < 0) k = NOTES.length - 1;
  const [start, len, row] = NOTES[k];
  const GLIDE = 0.2;
  let pitch = row;

  if (k > 0 && beat < start + GLIDE) {
    const prev = NOTES[k - 1][2];
    pitch = prev + (row - prev) * smooth((beat - (start - GLIDE)) / (2 * GLIDE));
  }
  if (k < NOTES.length - 1) {
    const [nextStart, , nextRow] = NOTES[k + 1];
    if (beat > nextStart - GLIDE) {
      pitch = row + (nextRow - row) * smooth((beat - (nextStart - GLIDE)) / (2 * GLIDE));
    }
  }
  if (len >= 2) {
    const into = beat - start - 0.6;
    const left = start + len - 0.25 - beat;
    if (into > 0 && left > 0) {
      const env = Math.min(1, into / 0.4, left / 0.2);
      pitch += 0.06 * env * Math.sin((into * 2 * Math.PI) / 0.36);
    }
  }
  return pitch;
}

const SING_FROM = 0.1;
const SING_TO = 7.9;
const SING_STEP = 0.04;

type Pt = { x: number; y: number; off: boolean };
function singer(slip: boolean): Pt[] {
  const pts: Pt[] = [];
  const count = Math.round((SING_TO - SING_FROM) / SING_STEP);
  for (let i = 0; i <= count; i++) {
    const beat = SING_FROM + i * SING_STEP;
    const p = pitchAt(beat, slip);
    // only the slip counts as off: glides between notes are quick and show as usual in the app's grading too
    const inSlip = slip && beat > 5.15 && beat < 6.35;
    pts.push({ x: beatX(beat), y: rowCenter(p), off: inSlip && Math.abs(p - noteRow(beat)) > 1 });
  }
  return pts;
}
const SINGERS = { clean: singer(false), slip: singer(true) };

/** The drawn line as runs: on the note (faded) and off it (shown as usual). */
function runs(pts: Pt[]): { off: boolean; d: string }[] {
  const out: { off: boolean; d: string }[] = [];
  let cur: { off: boolean; cmds: string[] } | null = null;
  pts.forEach((p, i) => {
    const xy = `${p.x.toFixed(2)} ${p.y.toFixed(2)}`;
    if (!cur || cur.off !== p.off) {
      if (cur) out.push({ off: cur.off, d: cur.cmds.join("") });
      const prev = pts[i - 1];
      cur = { off: p.off, cmds: prev ? [`M${prev.x.toFixed(2)} ${prev.y.toFixed(2)}`, `L${xy}`] : [`M${xy}`] };
    } else cur.cmds.push(`L${xy}`);
  });
  if (cur) out.push({ off: (cur as { off: boolean }).off, d: (cur as { cmds: string[] }).cmds.join("") });
  return out;
}

/* -------------------------------------------------------------- frame state */

type LaneFrame = {
  state: "past" | "active" | "future" | "static";
  sing: number; // 0..1 how much of the line is drawn
  lineAlpha: number;
  head: number; // playhead + voice dot opacity
  count: number; // 0..1 score count-up
  scoreAlpha: number;
  restore: number; // 0..1 full guide shown again
};

type Frame = { scene: number; progress: number; toggle: number; lanes: LaneFrame[] };

function frameAt(t: number): Frame {
  let scene = 0;
  while (scene < SCENE_MS.length - 1 && t >= SCENE_START[scene + 1]) scene++;
  const lt = t - SCENE_START[scene];
  const last = scene === SCENE_MS.length - 1;
  const keep = last ? 1 - span(lt, T.reset) : 1;

  const lanes = TAKES.map((_, i): LaneFrame => {
    if (i < scene) {
      return { state: "past", sing: 1, lineAlpha: keep, head: 0, count: 1, scoreAlpha: keep, restore: 0 };
    }
    if (i > scene) {
      return { state: "future", sing: 0, lineAlpha: 0, head: 0, count: 0, scoreAlpha: 0, restore: 0 };
    }
    const count = span(lt, T.score);
    return {
      state: "active",
      sing: span(lt, T.sing),
      lineAlpha: keep,
      head: span(lt, T.headIn) * (1 - span(lt, T.headOut)),
      count,
      scoreAlpha: count * keep,
      restore: last ? span(lt, T.restore) * keep : 0,
    };
  });

  return {
    scene,
    progress: clamp01(lt / SCENE_MS[scene]),
    toggle: last ? span(lt, T.toggle) * keep : 0,
    lanes,
  };
}

const STATIC_FRAME: Frame = {
  scene: -1,
  progress: 0,
  toggle: 0,
  lanes: TAKES.map(() => ({
    state: "static" as const,
    sing: 1,
    lineAlpha: 1,
    head: 0,
    count: 1,
    scoreAlpha: 1,
    restore: 0,
  })),
};

/* ---------------------------------------------------------- reduced motion */

// The guides always animate, also with Reduce motion on (Daniel asked, 29 Oct). Smaller motion elsewhere in
// OnKey still follows the setting.
function usePrefersReducedMotion(): boolean {
  return false;
}

/* -------------------------------------------------------------------- styles */

/** n design units → container-relative size, with a px fallback first. */
const sz = (prop: string, n: number) => `${prop}:${n}px;${prop}:${+(n / 3.6).toFixed(4)}cqw`;

const CSS = `
.gfl{position:relative;box-sizing:border-box;width:100%;aspect-ratio:360/260;container-type:inline-size;display:flex;flex-direction:column;overflow:hidden;border-radius:14px;background:${C.bg};color:${C.text};font-family:system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;-webkit-tap-highlight-color:transparent}
.gfl *{box-sizing:border-box}
.gfl svg{display:block;flex:none;width:100%;height:auto}
.gfl-lane{transition:opacity .45s ease-in-out}
.gfl-frame{transition:stroke .45s ease-in-out}
.gfl-label{transition:fill .45s ease-in-out}
.gfl-cap{flex:none;margin:0;display:grid;place-items:center;text-align:center;font-weight:500;line-height:1.25;letter-spacing:.005em;${sz("height", 30)};${sz("padding-inline", 16)};${sz("font-size", 13)}}
.gfl-cap>span{grid-area:1/1;opacity:0;transition:opacity .22s ease-in-out}
.gfl-cap>span[data-on="true"]{opacity:1;transition:opacity .34s ease-in-out .2s}
.gfl-dots{flex:1;display:flex;justify-content:center;align-items:flex-start;${sz("gap", 2)}}
.gfl-dot{appearance:none;-webkit-appearance:none;margin:0;padding:0;border:0;background:none;cursor:pointer;display:grid;place-items:center;border-radius:999px;${sz("width", 24)};${sz("height", 24)}}
.gfl-dot:focus-visible{outline:1.5px solid ${C.accentLight};outline-offset:-1.5px}
.gfl-dot>span{position:relative;display:block;overflow:hidden;border-radius:999px;background:${C.line};transition:width .4s ease-in-out,background-color .3s ease-in-out;${sz("width", 7)};${sz("height", 7)}}
.gfl-dot:hover>span{background:${C.muted}}
.gfl-dot[aria-current="step"]>span{background:${C.accentDark};${sz("width", 20)}}
.gfl-dot>span>span{position:absolute;inset:0 auto 0 0;border-radius:inherit;background:${C.accentLight}}
.gfl-strip{flex:1;margin:0;list-style:none;display:grid;grid-template-columns:repeat(4,minmax(0,1fr));align-content:start;text-align:center;font-weight:500;line-height:1.3;${sz("column-gap", 10)};${sz("padding-inline", 12)};${sz("padding-top", 2)};${sz("font-size", 10.5)}}
`;

/* ---------------------------------------------------------------- drawing */

function Header({ toggle }: { toggle: number }) {
  const t = useT();
  return (
    <g>
      {/* The song: a note glyph and two placeholder bars for title and artist. */}
      <g fill="none" stroke={C.muted} strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
        <ellipse cx={15.6} cy={24.4} rx={3} ry={2.3} transform="rotate(-20 15.6 24.4)" />
        <path d="M18.4 23.6V12.6c2.3.5 4.3 2.1 4.7 4.6" />
      </g>
      <rect x={31} y={14} width={56} height={4.5} rx={2.25} fill={C.line} />
      <rect x={31} y={22} width={34} height={4} rx={2} fill={C.line} opacity={0.65} />

      {/* Full guide switch */}
      <text x={319} y={23.6} textAnchor="end" fontSize={10} fontWeight={500} fill={C.muted}>
        {t("guides.fadeDemo.fullGuide")}
      </text>
      <text x={319} y={23.6} textAnchor="end" fontSize={10} fontWeight={500} fill={C.accentLight} opacity={toggle}>
        {t("guides.fadeDemo.fullGuide")}
      </text>
      <rect x={326} y={14} width={22} height={12} rx={6} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
      <rect x={326} y={14} width={22} height={12} rx={6} fill={C.accent} stroke={C.accent} strokeWidth={1.5} opacity={toggle} />
      <circle cx={332 + 10 * toggle} cy={20} r={3.6} fill={C.muted} />
      <circle cx={332 + 10 * toggle} cy={20} r={3.6} fill={C.text} opacity={toggle} />
    </g>
  );
}

function Lane({ index, f }: { index: number; f: LaneFrame }) {
  const t = useT();
  const take = TAKES[index];
  const x = LANE.x0 + index * (LANE.w + LANE.gap);
  const active = f.state === "active";

  // Your line while on the note: this take's fade, back to full with the switch.
  const fade = take.fade + (1 - take.fade) * f.restore;
  const SINGER = take.slip ? SINGERS.slip : SINGERS.clean;

  // How much of the singer's line is drawn.
  const target = f.sing * BEATS;
  const shown =
    f.sing >= 1
      ? SINGER.length
      : Math.max(0, Math.min(SINGER.length, Math.floor((target - SING_FROM) / SING_STEP) + 1));
  const parts = runs(SINGER.slice(0, shown));
  const voice = shown > 0 ? SINGER[shown - 1] : null;
  const playheadX = ROLL.x0 + f.sing * (ROLL.x1 - ROLL.x0);

  const from = index > 0 ? TAKES[index - 1].score : 76;
  const score = Math.round(from + (take.score - from) * f.count);
  const cx = LANE.w / 2;

  return (
    <g className="gfl-lane" transform={`translate(${x} ${LANE.y})`} style={{ opacity: f.state === "future" ? 0.6 : 1 }}>
      <rect
        className="gfl-frame"
        x={0}
        y={0}
        width={LANE.w}
        height={LANE.h}
        rx={12}
        fill={C.panel}
        strokeWidth={1.5}
        style={{ stroke: active ? C.accent : C.line }}
      />
      <text className="gfl-label" x={10} y={16} fontSize={10} fontWeight={600} style={{ fill: active ? C.text : C.muted }}>
        {t("guides.fadeDemo.take", { n: take.n })}
      </text>

      {/* Keyboard strip and black-key rows */}
      {BLACK_ROWS.map((r) => (
        <rect key={`band${r}`} x={ROLL.x0} y={rowTop(r)} width={ROLL.x1 - ROLL.x0} height={ROW_H} fill={C.line} opacity={0.32} />
      ))}
      {BLACK_ROWS.map((r) => (
        <rect key={`key${r}`} x={8} y={rowTop(r)} width={4.5} height={ROW_H} fill={C.line} />
      ))}
      <rect x={8} y={ROLL.y0} width={4.5} height={ROLL.y1 - ROLL.y0} rx={2.25} fill="none" stroke={C.line} strokeWidth={1.5} />

      {/* Guide notes */}
      {NOTES.map(([s, l, r], k) => (
        <rect
          key={k}
          x={beatX(s) + 0.9}
          y={rowCenter(r) - 5}
          width={l * BEAT_W - 1.8}
          height={10}
          rx={3}
          fill={C.accent}
          fillOpacity={0.55}
        />
      ))}

      {/* Playhead */}
      {f.head > 0.001 && (
        <line
          x1={playheadX}
          x2={playheadX}
          y1={ROLL.y0 - 2}
          y2={ROLL.y1 + 2}
          stroke={C.accentLight}
          strokeWidth={1.5}
          strokeLinecap="round"
          opacity={0.6 * f.head}
        />
      )}

      {/* Singer's pitch line */}
      {shown > 1 && f.lineAlpha > 0.001 && parts.map((r, k) => (
        <path
          key={k}
          d={r.d}
          fill="none"
          stroke={r.off ? C.red : C.good}
          strokeWidth={r.off ? 1.75 : 1.5}
          strokeLinecap="round"
          strokeLinejoin="round"
          opacity={f.lineAlpha * (r.off ? 1 : fade)}
        />
      ))}
      {voice && f.head > 0.001 && (
        <g opacity={f.head * (voice.off ? 1 : Math.max(0.35, fade))}>
          <circle cx={voice.x} cy={voice.y} r={4.6} fill={voice.off ? C.red : C.good} opacity={0.18} />
          <circle cx={voice.x} cy={voice.y} r={2.2} fill={voice.off ? C.red : C.good} />
        </g>
      )}

      {/* Score */}
      <g transform={`translate(${cx} ${LANE.h + 19})`}>
        <rect x={-7} y={-5.5} width={14} height={3} rx={1.5} fill={C.line} opacity={1 - f.scoreAlpha} />
        <g opacity={f.scoreAlpha} transform={`translate(0 ${(1 - f.count) * 3})`}>
          <text
            x={0}
            y={0}
            textAnchor="middle"
            fontSize={12}
            fontWeight={600}
            fill={C.text}
            style={{ fontVariantNumeric: "tabular-nums" }}
          >
            {score}
          </text>
          {index > 0 && take.score > from && (
            <path
              d="M13.5 -3.2 L16.5 -6.4 L19.5 -3.2"
              fill="none"
              stroke={C.good}
              strokeWidth={1.5}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          )}
        </g>
      </g>
    </g>
  );
}

/* --------------------------------------------------------------- component */

export default function FadeGuideDemo({ motion = "auto", className }: FadeGuideDemoProps) {
  const t = useT();
  const prefersReduced = usePrefersReducedMotion();
  const isStatic = motion === "reduce" || prefersReduced;

  const [time, setTime] = useState(0);
  const timeRef = useRef(0);
  const hoverRef = useRef(false);
  const focusRef = useRef(false);
  const inViewRef = useRef(true);
  const rootRef = useRef<HTMLDivElement>(null);

  // Clock: advances only while visible, not hovered and not keyboard-focused.
  useEffect(() => {
    if (isStatic) return;
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(now - last, 50);
      last = now;
      if (!hoverRef.current && !focusRef.current && inViewRef.current) {
        timeRef.current = (timeRef.current + dt) % LOOP_MS;
        setTime(timeRef.current);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [isStatic]);

  // Skip work while scrolled out of view.
  useEffect(() => {
    const el = rootRef.current;
    if (!el || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(([entry]) => {
      inViewRef.current = entry.isIntersecting;
    });
    io.observe(el);
    return () => io.disconnect();
  }, []);

  const jump = useCallback((step: number) => {
    timeRef.current = SCENE_START[step] + KEY_MS[step];
    setTime(timeRef.current);
  }, []);

  const onPointerEnter = (e: PointerEvent<HTMLDivElement>) => {
    if (e.pointerType === "mouse") hoverRef.current = true;
  };
  const onPointerLeave = () => {
    hoverRef.current = false;
  };
  const onFocus = (e: FocusEvent<HTMLDivElement>) => {
    let keyboard = true;
    try {
      keyboard = e.target.matches(":focus-visible");
    } catch {
      /* :focus-visible unsupported: treat focus as keyboard focus */
    }
    focusRef.current = keyboard;
  };
  const onBlur = (e: FocusEvent<HTMLDivElement>) => {
    const next = e.relatedTarget as Node | null;
    if (!next || !rootRef.current?.contains(next)) focusRef.current = false;
  };

  const frame = isStatic ? STATIC_FRAME : frameAt(time);

  return (
    <div
      ref={rootRef}
      className={className ? `gfl ${className}` : "gfl"}
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
      onFocus={onFocus}
      onBlur={onBlur}
    >
      <style>{CSS}</style>

      <svg viewBox={`0 0 ${VB_W} ${VB_H}`} role="img" aria-label={t("guides.fadeDemo.ariaLabel")}>
        <Header toggle={frame.toggle} />
        {TAKES.map((take, i) => (
          <Lane key={take.n} index={i} f={frame.lanes[i]} />
        ))}
      </svg>

      {isStatic ? (
        <ol className="gfl-strip">
          {CAPTIONS.map((caption) => (
            <li key={caption}>{t(`guides.fadeDemo.captions.${caption}`)}</li>
          ))}
        </ol>
      ) : (
        <>
          <p className="gfl-cap">
            {CAPTIONS.map((caption, i) => (
              <span key={caption} data-on={i === frame.scene} aria-hidden={i !== frame.scene}>
                {t(`guides.fadeDemo.captions.${caption}`)}
              </span>
            ))}
          </p>
          <div className="gfl-dots" role="group" aria-label={t("guides.fadeDemo.stepsGroup")}>
            {CAPTIONS.map((caption, i) => {
              const current = i === frame.scene;
              return (
                <button
                  key={caption}
                  type="button"
                  className="gfl-dot"
                  aria-label={t("guides.fadeDemo.stepLabel", { n: i + 1, count: CAPTIONS.length, caption: t(`guides.fadeDemo.captions.${caption}`) })}
                  aria-current={current ? "step" : undefined}
                  onClick={() => jump(i)}
                >
                  <span>{current && <span style={{ width: `${frame.progress * 100}%` }} />}</span>
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
