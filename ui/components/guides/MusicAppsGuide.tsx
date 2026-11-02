"use client";

/**
 * Setup's "Let OnKey follow Spotify and Apple Music" step, while macOS is asking: its question ("OnKey" wants
 * access to control "Spotify" / "Music"), Allow, the app ticked in OnKey, and that the question can open behind
 * OnKey's window. Only the apps installed are shown (`apps`).
 *
 * - Self-contained: no libraries, images or external fonts. Inline SVG driven
 *   by one requestAnimationFrame clock, so every frame is a pure function of
 *   (step, time) and the step dots can jump anywhere.
 * - 360 × 260 viewBox, scales to the container width.
 * - Pauses while a mouse is over it; offscreen it stops ticking.
 * - reducedMotion: renders the steps as a static, scrollable strip. It animates even with Reduce motion on.
 */

import { useEffect, useId, useMemo, useRef, useState, type CSSProperties } from "react";
import StillSteps from "./StillSteps";
import { useT, type Translator } from "@/lib/i18n";

/* ─── Palette & type ─────────────────────────────────────────────────────── */

import { GUIDE as C } from "@/lib/colors";
import { Fit } from "./Fit";

const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';
const SW = 1.5; // stroke width everywhere

/* ─── Script ─────────────────────────────────────────────────────────────── */

export type GuideApp = "Spotify" | "Apple Music";
/** Each app's key in the catalog: guides.musicApps.app.* (its name) and guides.musicApps.asked.* (the name macOS uses
 *  in its question). */
const APP_KEY: Record<GuideApp, "spotify" | "appleMusic"> = { Spotify: "spotify", "Apple Music": "appleMusic" };

/** The captions, by key (guides.musicApps.caption.*). */
const CAPTIONS = ["ask", "allow", "done", "behind"] as const;

type Kind = "ask" | "allow" | "done" | "behind";

interface Step {
  kind: Kind;
  app: number;
  caption: 0 | 1 | 2 | 3;
  /** Length of the scene in ms. */
  dur: number;
  /** A settled moment in the scene: used for the static strip and as the hold point after a dot jump. */
  pose: number;
}

/** Ask, allow and done for each app, then "it can open behind this window". */
function stepsFor(n: number): Step[] {
  const out: Step[] = [];
  for (let app = 0; app < n; app++) {
    out.push({ kind: "ask", app, caption: 0, dur: app ? 2200 : 2400, pose: 1300 },
             { kind: "allow", app, caption: 1, dur: 2200, pose: 1360 },
             { kind: "done", app, caption: 2, dur: 2400, pose: 1500 });
  }
  out.push({ kind: "behind", app: n - 1, caption: 3, dur: 4400, pose: 2600 });
  return out;
}

const appName = (t: Translator, app: GuideApp) => t(`guides.musicApps.app.${APP_KEY[app]}`);

const labelFor = (t: Translator, apps: readonly GuideApp[]) =>
  apps.length === 1
    ? t("guides.musicApps.labelOne", { app: appName(t, apps[0]) })
    : t("guides.musicApps.labelTwo", { first: appName(t, apps[0]), second: appName(t, apps[1]) });

/* ─── Geometry (viewBox units) ───────────────────────────────────────────── */

const VB = { w: 360, h: 260 };
const WIN = { x: 16, y: 30, w: 254, h: 214 };
const ALERT = { w: 240, h: 142 };
const ALERT_FRONT = { x: 60, y: 52 }; // centred on the "screen"
const ALERT_BEHIND = { x: 112, y: 46 }; // tucked under the OnKey window, right part showing
const ROW = { x: 28, w: 230, h: 30, ys: [146, 184] as const };
const CURSOR_REST = { x: 316, y: 222 };
const CURSOR_ALLOW = { x: 237, y: 170 }; // over the Allow button of the front alert

const ARROW = { start: { x: 336, y: 238 }, c1: { x: 342, y: 226 }, c2: { x: 330, y: 206 }, tip: { x: 316, y: 196 } };
const ARROW_PATH = `M${ARROW.start.x} ${ARROW.start.y} C ${ARROW.c1.x} ${ARROW.c1.y}, ${ARROW.c2.x} ${ARROW.c2.y}, ${ARROW.tip.x} ${ARROW.tip.y}`;
const ARROW_HEAD = (() => {
  const a = Math.atan2(ARROW.tip.y - ARROW.c2.y, ARROW.tip.x - ARROW.c2.x);
  const len = 6.5;
  const spread = 0.62;
  const p = (s: number) =>
    `${(ARROW.tip.x - len * Math.cos(a + s)).toFixed(2)} ${(ARROW.tip.y - len * Math.sin(a + s)).toFixed(2)}`;
  return `M${p(spread)} L${ARROW.tip.x} ${ARROW.tip.y} L${p(-spread)}`;
})();

/* ─── Timing helpers ─────────────────────────────────────────────────────── */

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
/** Gentle sine ease-in-out. */
const ease = (p: number) => 0.5 - 0.5 * Math.cos(Math.PI * p);
/** Eased progress of t through [a, b]. */
const seg = (t: number, a: number, b: number) => ease(clamp01((t - a) / (b - a)));
const lerp = (a: number, b: number, p: number) => a + (b - a) * p;

/* ─── Scene state: a pure function of (step, t) ─────────────────────────── */

interface RowState {
  opacity: number;
  dy: number;
  check: number;
  glow: number;
}

interface SceneState {
  alert: null | {
    app: GuideApp;
    x: number;
    y: number;
    opacity: number;
    scale: number;
    behind: boolean;
    press: number;
    accent: number;
  };
  cursor: { x: number; y: number; opacity: number; press: number };
  ripple: null | { r: number; opacity: number };
  rows: RowState[];
  arrow: { draw: number; head: number; opacity: number };
}

const ROW_ON: RowState = { opacity: 1, dy: 0, check: 1, glow: 0 };
const ROW_OFF: RowState = { opacity: 0, dy: 6, check: 0, glow: 0 };

function scene(steps: readonly Step[], apps: readonly GuideApp[], stepIndex: number, t: number): SceneState {
  const s = steps[stepIndex];
  const app = apps[s.app];
  const rowsBefore = (i: number): RowState => (i < s.app ? ROW_ON : ROW_OFF);

  const st: SceneState = {
    alert: null,
    cursor: { ...CURSOR_REST, opacity: 1, press: 0 },
    ripple: null,
    rows: apps.map((_, i) => rowsBefore(i)),
    arrow: { draw: 0, head: 0, opacity: 0 },
  };

  switch (s.kind) {
    case "ask": {
      const p = seg(t, 150, 650);
      st.alert = {
        app,
        x: ALERT_FRONT.x,
        y: ALERT_FRONT.y + 6 * (1 - p),
        opacity: p,
        scale: 0.97 + 0.03 * p,
        behind: false,
        press: 0,
        accent: 0,
      };
      // The cursor was hidden during the last scene; bring it back on the first prompt.
      if (s.app === 0) st.cursor.opacity = seg(t, 0, 450);
      break;
    }

    case "allow": {
      const move = seg(t, 100, 1000);
      const press = seg(t, 1100, 1160) * (1 - seg(t, 1380, 1480));
      st.alert = { app, ...ALERT_FRONT, opacity: 1, scale: 1, behind: false, press, accent: 0 };
      st.cursor = {
        x: lerp(CURSOR_REST.x, CURSOR_ALLOW.x, move),
        y: lerp(CURSOR_REST.y, CURSOR_ALLOW.y, move),
        opacity: 1,
        press,
      };
      if (t >= 1120 && t <= 1720) {
        const r = seg(t, 1120, 1720);
        st.ripple = { r: 4 + 20 * r, opacity: 0.85 * (1 - r) };
      }
      break;
    }

    case "done": {
      const close = seg(t, 0, 380);
      if (close < 1) {
        st.alert = {
          app,
          ...ALERT_FRONT,
          opacity: 1 - close,
          scale: 1 - 0.02 * close,
          behind: false,
          press: 0,
          accent: 0,
        };
      }
      const back = seg(t, 400, 1300);
      st.cursor = {
        x: lerp(CURSOR_ALLOW.x, CURSOR_REST.x, back),
        y: lerp(CURSOR_ALLOW.y, CURSOR_REST.y, back),
        opacity: 1,
        press: 0,
      };
      const r = seg(t, 300, 800);
      st.rows[s.app] = {
        opacity: r,
        dy: 6 * (1 - r),
        check: seg(t, 650, 1100),
        glow: seg(t, 600, 950) * (1 - seg(t, 1700, 2300)),
      };
      break;
    }

    case "behind": {
      const fade = 1 - seg(t, 3800, 4400); // hand the loop back to an empty window
      const slide = seg(t, 300, 1300);
      st.rows = apps.map(() => ({ ...ROW_ON, opacity: fade }));
      st.alert = {
        app,
        x: ALERT_BEHIND.x - 56 * (1 - slide),
        y: ALERT_BEHIND.y,
        opacity: seg(t, 300, 750) * fade,
        scale: 1,
        behind: true,
        press: 0,
        accent: seg(t, 1300, 1800),
      };
      st.cursor.opacity = 1 - seg(t, 0, 400);
      st.arrow = { draw: seg(t, 1300, 2100), head: seg(t, 1950, 2250), opacity: fade };
      break;
    }
  }

  return st;
}

/* ─── Drawing ────────────────────────────────────────────────────────────── */

function Alert({ a, shadowId }: { a: NonNullable<SceneState["alert"]>; shadowId: string }) {
  const t = useT();
  const asked = t(`guides.musicApps.asked.${APP_KEY[a.app]}`);
  const cx = ALERT.w / 2;
  const cy = ALERT.h / 2;
  return (
    <g
      opacity={a.opacity}
      transform={`translate(${a.x} ${a.y}) translate(${cx} ${cy}) scale(${a.scale}) translate(${-cx} ${-cy})`}
    >
      <rect
        width={ALERT.w}
        height={ALERT.h}
        rx={14}
        fill={C.panel}
        stroke={C.line}
        strokeWidth={SW}
        filter={`url(#${shadowId})`}
      />
      {a.accent > 0 && (
        <rect width={ALERT.w} height={ALERT.h} rx={14} fill="none" stroke={C.accent} strokeWidth={SW} opacity={a.accent} />
      )}

      {/* Generic app icon: a rounded tile with a pitch line */}
      <rect x={104} y={12} width={32} height={32} rx={12} fill={C.bg} stroke={C.line} strokeWidth={SW} />
      <path
        d="M110 31 C 113 22, 117 36, 121 29 S 127 27, 130 28"
        fill="none"
        stroke={C.accent}
        strokeWidth={SW}
        strokeLinecap="round"
      />
      <circle cx={130} cy={28} r={1.8} fill={C.accentLight} />

      {/* (each line fits the alert, 12 in from its sides; each button's word fits the button) */}
      <Fit x={cx} y={60} max={ALERT.w - 24} textAnchor="middle" fontSize={10.5} fontWeight={600} fill={C.text}>
        {t("guides.musicApps.alert.wantsAccess")}
      </Fit>
      <Fit x={cx} y={73} max={ALERT.w - 24} textAnchor="middle" fontSize={10.5} fontWeight={600} fill={C.text}>
        {t("guides.musicApps.alert.control", { app: asked })}
      </Fit>
      <Fit x={cx} y={91} max={ALERT.w - 24} textAnchor="middle" fontSize={9} fill={C.muted}>
        {t("guides.musicApps.alert.allowing", { app: asked })}
      </Fit>

      <rect x={14} y={104} width={102} height={24} rx={12} fill={C.line} />
      <Fit x={65} y={119.3} max={92} textAnchor="middle" fontSize={9.5} fill={C.text}>
        {t("guides.musicApps.alert.dontAllow")}
      </Fit>

      <rect x={124} y={104} width={102} height={24} rx={12} fill={C.accent} />
      {a.press > 0 && <rect x={124} y={104} width={102} height={24} rx={12} fill={C.accentDark} opacity={a.press} />}
      <Fit x={175} y={119.3} max={92} textAnchor="middle" fontSize={9.5} fontWeight={600} fill={C.text}>
        {t("guides.musicApps.alert.allow")}
      </Fit>
    </g>
  );
}

function AppGlyph({ app }: { app: GuideApp }) {
  const common = { fill: "none", stroke: C.muted, strokeWidth: SW, strokeLinecap: "round" as const };
  return app === "Spotify" ? (
    // Equaliser bars
    <g {...common}>
      <line x1={1} y1={10} x2={1} y2={5} />
      <line x1={4.5} y1={10} x2={4.5} y2={1} />
      <line x1={8} y1={10} x2={8} y2={6.5} />
    </g>
  ) : (
    // Eighth note
    <g>
      <path d="M5.4 8.4 V0.8 C 7 1.8, 9 2.6, 9 5.2" {...common} />
      <ellipse cx={3.4} cy={8.6} rx={2.3} ry={1.7} transform="rotate(-20 3.4 8.6)" fill={C.muted} />
    </g>
  );
}

function Row({ i, name, r }: { i: number; name: GuideApp; r: RowState }) {
  const t = useT();
  if (r.opacity <= 0) return null;
  return (
    <g transform={`translate(${ROW.x} ${ROW.ys[i] + r.dy})`} opacity={r.opacity}>
      <rect width={ROW.w} height={ROW.h} rx={12} fill={C.bg} stroke={C.line} strokeWidth={SW} />
      {r.glow > 0 && (
        <rect width={ROW.w} height={ROW.h} rx={12} fill="none" stroke={C.good} strokeWidth={SW} opacity={0.75 * r.glow} />
      )}
      <g transform="translate(12 10)">
        <AppGlyph app={name} />
      </g>
      <text x={32} y={19} fontSize={10.5} fontWeight={500} fill={C.text}>
        {appName(t, name)}
      </text>
      <path
        d="M0 4.5 L3.6 8 L10 1"
        transform="translate(206 11)"
        fill="none"
        stroke={C.good}
        strokeWidth={SW}
        strokeLinecap="round"
        strokeLinejoin="round"
        pathLength={1}
        strokeDasharray="1 1"
        strokeDashoffset={1 - r.check}
        opacity={r.check > 0.001 ? 1 : 0}
      />
    </g>
  );
}

function OnKeyWindow({ rows, apps, shadowId }: { rows: SceneState["rows"]; apps: readonly GuideApp[]; shadowId: string }) {
  const t = useT();
  const midX = WIN.x + WIN.w / 2;
  return (
    <g>
      <rect
        x={WIN.x}
        y={WIN.y}
        width={WIN.w}
        height={WIN.h}
        rx={14}
        fill={C.panel}
        stroke={C.line}
        strokeWidth={SW}
        filter={`url(#${shadowId})`}
      />
      {/* Title bar */}
      {[30, 41, 52].map((x) => (
        <circle key={x} cx={x} cy={42} r={3.25} fill="none" stroke={C.line} strokeWidth={SW} />
      ))}
      <text x={midX} y={45.5} textAnchor="middle" fontSize={10} fontWeight={600} fill={C.muted}>
        {t("guides.musicApps.window.title")}
      </text>
      <line x1={WIN.x} y1={54} x2={WIN.x + WIN.w} y2={54} stroke={C.line} strokeWidth={SW} />

      {/* Pitch sketch: target note and a sung line settling onto it */}
      <line x1={30} y1={90} x2={232} y2={90} stroke={C.line} strokeWidth={SW} strokeDasharray="3 4" />
      <path
        d="M30 80 C 48 66, 66 104, 88 96 S 124 80, 144 90 S 180 97, 200 91 S 222 90, 232 90"
        fill="none"
        stroke={C.accent}
        strokeWidth={SW}
        strokeLinecap="round"
      />
      <circle cx={232} cy={90} r={3} fill={C.accentLight} />
      <text x={241} y={93.4} fontSize={9} fontWeight={500} fill={C.muted}>
        {t("guides.musicApps.window.note")}
      </text>

      {/* Connected music apps */}
      <text x={30} y={136} fontSize={8} fontWeight={600} fill={C.muted} style={{ letterSpacing: "0.08em" }}>
        {t("guides.musicApps.window.musicApps")}
      </text>
      {ROW.ys.slice(0, apps.length).map((y) => (
        <rect
          key={y}
          x={ROW.x}
          y={y}
          width={ROW.w}
          height={ROW.h}
          rx={12}
          fill="none"
          stroke={C.line}
          strokeWidth={SW}
          strokeDasharray="3 4"
        />
      ))}
      {apps.map((name, i) => <Row key={name} i={i} name={name} r={rows[i]} />)}
    </g>
  );
}

function Frame({ steps, apps, step, time }: { steps: readonly Step[]; apps: readonly GuideApp[]; step: number; time: number }) {
  const t = useT();
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const clipId = `okp-clip-${uid}`;
  const shadowId = `okp-shadow-${uid}`;
  const st = scene(steps, apps, step, time);
  const alert = st.alert && st.alert.opacity > 0 ? <Alert a={st.alert} shadowId={shadowId} /> : null;
  const win = <OnKeyWindow rows={st.rows} apps={apps} shadowId={shadowId} />;

  return (
    <svg
      viewBox={`0 0 ${VB.w} ${VB.h}`}
      width="100%"
      fontFamily={FONT}
      aria-hidden="true"
      focusable="false"
      style={{ display: "block", height: "auto" }}
    >
      <defs>
        <clipPath id={clipId}>
          <rect width={VB.w} height={VB.h} rx={16} />
        </clipPath>
        <filter id={shadowId} x="-20%" y="-20%" width="140%" height="160%">
          <feDropShadow dx={0} dy={8} stdDeviation={9} floodColor={C.bg} floodOpacity={0.95} />
        </filter>
      </defs>

      {/* Desktop */}
      <rect width={VB.w} height={VB.h} rx={16} fill={C.bg} />
      <g clipPath={`url(#${clipId})`}>
        <rect width={VB.w} height={18} fill={C.panel} />
        <line x1={0} y1={18} x2={VB.w} y2={18} stroke={C.line} strokeWidth={SW} />
        <text x={16} y={12.4} fontSize={8.5} fontWeight={700} fill={C.text}>
          {t("guides.musicApps.menuBar.appName")}
        </text>
        {[
          [50, 16],
          [72, 12],
          [90, 18],
          [114, 14],
          [292, 10],
          [308, 36],
        ].map(([x, w]) => (
          <rect key={x} x={x} y={7} width={w} height={4} rx={2} fill={C.line} />
        ))}
      </g>
      <rect
        x={SW / 2}
        y={SW / 2}
        width={VB.w - SW}
        height={VB.h - SW}
        rx={16 - SW / 2}
        fill="none"
        stroke={C.line}
        strokeWidth={SW}
      />

      {/* Layering: the alert sits above the window, except in the last scene */}
      {st.alert?.behind ? (
        <>
          {alert}
          {win}
        </>
      ) : (
        <>
          {win}
          {alert}
        </>
      )}

      {st.arrow.opacity > 0 && st.arrow.draw > 0 && (
        <g
          opacity={st.arrow.opacity}
          fill="none"
          stroke={C.accentLight}
          strokeWidth={SW}
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d={ARROW_PATH} pathLength={1} strokeDasharray="1 1" strokeDashoffset={1 - st.arrow.draw} />
          <path d={ARROW_HEAD} opacity={st.arrow.head} />
        </g>
      )}

      {st.ripple && (
        <circle
          cx={CURSOR_ALLOW.x}
          cy={CURSOR_ALLOW.y}
          r={st.ripple.r}
          fill="none"
          stroke={C.accentLight}
          strokeWidth={SW}
          opacity={st.ripple.opacity}
        />
      )}

      {st.cursor.opacity > 0 && (
        <g
          transform={`translate(${st.cursor.x} ${st.cursor.y}) scale(${1 - 0.12 * st.cursor.press})`}
          opacity={st.cursor.opacity}
        >
          <path
            d="M0 0 L0 15.5 L4.1 11.8 L6.9 17.9 L9.5 16.7 L6.8 10.8 L12 10.8 Z"
            fill={C.text}
            stroke={C.bg}
            strokeWidth={SW}
            strokeLinejoin="round"
          />
        </g>
      )}
    </svg>
  );
}

/* ─── Component ──────────────────────────────────────────────────────────── */

const CSS = `
.okp{font-family:${FONT};color:${C.text};width:100%}
.okp-cap{display:grid;margin-top:10px;text-align:center;font-size:14px;line-height:1.35}
.okp-cap span{grid-area:1/1;opacity:0;transition:opacity .35s ease-in-out;text-wrap:balance}
.okp-cap span[data-on="true"]{opacity:1}
.okp-dots{display:flex;justify-content:center;margin-top:4px}
.okp-dot{appearance:none;border:0;background:transparent;padding:0;margin:0;width:24px;height:24px;display:grid;place-items:center;cursor:pointer;border-radius:12px}
.okp-dot i{display:block;width:6px;height:6px;border-radius:3px;background:${C.line};transition:width .3s ease-in-out,background-color .3s ease-in-out}
.okp-dot:hover i{background:${C.muted}}
.okp-dot[aria-current="step"] i{width:16px;background:${C.accent}}
.okp-dot:focus-visible{outline:${SW}px solid ${C.accentLight};outline-offset:-3px}
.okp-strip{overflow-x:auto;scroll-snap-type:x proximity;scrollbar-color:${C.line} transparent;padding-bottom:8px;border-radius:12px}
.okp-strip:focus-visible{outline:${SW}px solid ${C.accentLight};outline-offset:2px}
.okp-strip-row{display:flex;gap:12px}
.okp-frame{flex:0 0 82%;margin:0;scroll-snap-align:start}
.okp-frame figcaption{margin-top:8px;font-size:13px;line-height:1.35;display:flex;gap:6px}
.okp-frame figcaption b{color:${C.muted};font-weight:500;font-variant-numeric:tabular-nums}
`;

// The guides always animate, also with Reduce motion on (Daniel asked, 29 Oct). Smaller motion elsewhere in
// OnKey still follows the setting.
function usePrefersReducedMotion(): boolean {
  return false;
}

interface Clock {
  step: number;
  t: number;
}

function advance(steps: readonly Step[], c: Clock, dt: number): Clock {
  let { step, t } = c;
  if (step >= steps.length) step = 0;
  t += dt;
  while (t >= steps[step].dur) {
    t -= steps[step].dur;
    step = (step + 1) % steps.length;
  }
  return { step, t };
}

export interface MusicAppsGuideProps {
  /** The apps to show (the installed ones), in order. */
  apps?: readonly GuideApp[];
  className?: string;
  style?: CSSProperties;
  /**
   * Force the static strip (true) or the animation (false).
   * Leave unset to animate.
   */
  reducedMotion?: boolean;
}

export default function MusicAppsGuide({ apps: given, className, style, reducedMotion }: MusicAppsGuideProps) {
  const key = (given?.length ? given : ["Spotify", "Apple Music"]).join("|");
  const apps = useMemo(() => key.split("|") as GuideApp[], [key]);
  const STEPS = useMemo(() => stepsFor(apps.length), [apps]);
  const t = useT();
  const LABEL = labelFor(t, apps);
  const systemReduced = usePrefersReducedMotion();
  const reduced = reducedMotion ?? systemReduced;

  const [clock, setClock] = useState<Clock>({ step: 0, t: 0 });
  const clockRef = useRef<Clock>(clock);
  const hovering = useRef(false);
  const onScreen = useRef(true);
  /** After a dot jump while paused, play up to that step's pose, then hold. */
  const seek = useRef<Clock | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // Stop ticking while scrolled out of view.
  useEffect(() => {
    const el = rootRef.current;
    if (!el || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(([entry]) => {
      onScreen.current = entry.isIntersecting;
    });
    io.observe(el);
    return () => io.disconnect();
  }, [reduced]);

  useEffect(() => {
    if (reduced) return;
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(now - last, 64); // no big jumps after a background tab
      last = now;
      if (onScreen.current) {
        const c = clockRef.current;
        let next = c;
        if (!hovering.current) {
          next = advance(STEPS, c, dt);
        } else if (seek.current && seek.current.step === c.step && c.t < seek.current.t) {
          next = { step: c.step, t: Math.min(c.t + dt, seek.current.t) };
        }
        if (next !== c) {
          clockRef.current = next;
          setClock(next);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [reduced, STEPS]);

  const jump = (i: number) => {
    const c = { step: i, t: 0 };
    clockRef.current = c;
    seek.current = { step: i, t: STEPS[i].pose };
    setClock(c);
  };

  const rootClass = className ? `okp ${className}` : "okp";

  if (reduced) {
    return <StillSteps count={STEPS.length} label={LABEL} caption={i => t(`guides.musicApps.caption.${CAPTIONS[STEPS[i].caption]}`)}
                       frame={i => <Frame steps={STEPS} apps={apps} step={i} time={STEPS[i].pose} />} />;
  }

  const active = STEPS[clock.step].caption;

  return (
    <div
      ref={rootRef}
      className={rootClass}
      style={style}
      onPointerEnter={(e) => {
        if (e.pointerType !== "touch") hovering.current = true;
      }}
      onPointerLeave={() => {
        hovering.current = false;
        seek.current = null;
      }}
    >
      <style>{CSS}</style>
      <div role="img" aria-label={LABEL}>
        <Frame steps={STEPS} apps={apps} step={Math.min(clock.step, STEPS.length - 1)} time={clock.t} />
        <div className="okp-cap">
          {CAPTIONS.map((id, i) => (
            <span key={i} data-on={i === active}>
              {t(`guides.musicApps.caption.${id}`)}
            </span>
          ))}
        </div>
      </div>
      <div className="okp-dots" role="group" aria-label={t("guides.musicApps.dots")}>
        {STEPS.map((s, i) => (
          <button
            key={i}
            type="button"
            className="okp-dot"
            aria-label={s.kind === "behind"
              ? t("guides.musicApps.stepOf", { step: i + 1, count: STEPS.length, caption: t(`guides.musicApps.caption.${CAPTIONS[s.caption]}`) })
              : t("guides.musicApps.stepOfApp", { step: i + 1, count: STEPS.length, caption: t(`guides.musicApps.caption.${CAPTIONS[s.caption]}`), app: appName(t, apps[s.app]) })}
            aria-current={i === clock.step ? "step" : undefined}
            onClick={() => jump(i)}
          >
            <i />
          </button>
        ))}
      </div>
    </div>
  );
}
