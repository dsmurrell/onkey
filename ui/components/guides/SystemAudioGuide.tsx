"use client";

/**
 * Setup's "System audio recording" row, once macOS has it turned off: where to turn it on.
 *
 * A looping line-art sketch of the macOS steps for letting OnKey record system audio:
 *   1. Open Privacy & Security
 *   2. Screen & System Audio Recording
 *   3. Turn on OnKey under System Audio Recording Only
 *   4. If macOS asks, quit and reopen. Setup carries on
 *
 * - Self-contained: React only. Inline SVG driven by requestAnimationFrame. No images, video or fonts.
 * - 360 × 260 at 360 px wide (illustration, caption and dots). Scales to the container width.
 * - Pauses on mouse hover and on keyboard focus. Step dots jump to a step.
 * - reducedMotion renders the four steps as a static strip with captions. It animates even with Reduce motion on.
 *
 * Usage (App Router or Pages Router):
 *   import SystemAudioSetupAnimation from "@/components/SystemAudioSetupAnimation";
 *   <div style={{ maxWidth: 360 }}><SystemAudioSetupAnimation /></div>
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type CSSProperties,
  type ReactNode,
} from "react";
import StillSteps from "./StillSteps";
import { useT } from "@/lib/i18n";

/* ------------------------------------------------------------------ */
/* Palette, type and timeline                                          */
/* ------------------------------------------------------------------ */

import { GUIDE as C } from "@/lib/colors";
import { Fit } from "./Fit";

const FONT = "system-ui, sans-serif";

/** Illustration viewBox. With the caption and dots the whole thing is about 360 × 260. */
const VB_W = 360;
const VB_H = 190;

/** The steps' captions, by key (guides.systemAudio.step.*). */
const STEPS = ["openPrivacy", "screenRecording", "turnOn", "quitReopen"] as const;

/** Scene lengths in ms (about 2 s each; the last one has two beats). */
const DURATIONS = [2000, 2200, 2200, 2600] as const;
const STARTS = [0, 2000, 4200, 6400] as const;
const TOTAL = 9000;

/** Where a step "lands". A jump made while paused plays up to here and holds. */
const HOLD_AT = [1800, 4100, 6150, 8350] as const;
/** The frame shown for each card in the reduced-motion strip. */
const STILL_AT = [1800, 4100, 6150, 7330] as const;
/** First rendered frame (after the fade-in), so the server render is complete at rest. */
const FIRST_FRAME = 450;

type Range = readonly [number, number];

/** Every beat of the loop, in ms from the start. */
const T = {
  fadeIn: [0, 400],
  // 1 · sidebar
  clickSidebar: 1200,
  sidebarSelect: [1200, 1450],
  // panes fade through (old out, then new in) so their text never overlaps
  paneGeneralOut: [1250, 1420],
  panePrivacyIn: [1420, 1650],
  // 2 · scroll to the row and click it
  scroll: [2200, 3300],
  clickRow: 3650,
  rowPress: [3650, 3900],
  // 3 · toggle
  panePrivacyOut: [4200, 4370],
  paneAudioIn: [4370, 4600],
  clickToggle: 5400,
  toggle: [5400, 5750],
  // 4 · quit & reopen, OnKey comes back
  dialogIn: [6400, 6650],
  clickQuit: 7200,
  quitPress: [7200, 7320],
  dialogOut: [7350, 7600],
  cursorOut: [7450, 7750],
  appIn: [7500, 7850],
  checkFill: [7850, 8000],
  checkDraw: [7950, 8150],
  fadeOut: [8550, 9000],
} as const;

const CLICKS = [T.clickSidebar, T.clickRow, T.clickToggle, T.clickQuit] as const;

/** Pointer path: [time, x, y] of the arrow's tip. Eased between waypoints. */
const CURSOR: ReadonlyArray<readonly [number, number, number]> = [
  [0, 270, 152],
  [350, 270, 152],
  [1100, 76, 131], // Privacy & Security
  [2350, 76, 131],
  [3450, 214, 120], // Screen & System Audio Recording (after the scroll)
  [4450, 214, 120],
  [5300, 322, 119], // OnKey toggle
  [6600, 322, 119],
  [7150, 214, 119], // Quit & Reopen
  [TOTAL, 214, 119],
];

/* ------------------------------------------------------------------ */
/* Maths                                                               */
/* ------------------------------------------------------------------ */

const clamp = (v: number, lo = 0, hi = 1) => Math.min(hi, Math.max(lo, v));
/** Gentle sine ease-in-out. No overshoot. */
const ease = (x: number) => 0.5 - Math.cos(Math.PI * clamp(x)) / 2;
const seg = (t: number, [a, b]: Range) => ease((t - a) / (b - a));
const lerp = (a: number, b: number, p: number) => a + (b - a) * p;
const r2 = (v: number) => Math.round(v * 100) / 100;

/** Blend two palette colours (used only mid-transition). */
function mix(a: string, b: string, p: number): string {
  if (p <= 0) return a;
  if (p >= 1) return b;
  const x = parseInt(a.slice(1), 16);
  const y = parseInt(b.slice(1), 16);
  const ch = (shift: number) => Math.round(lerp((x >> shift) & 255, (y >> shift) & 255, p));
  return `rgb(${ch(16)}, ${ch(8)}, ${ch(0)})`;
}

function cursorAt(t: number): { x: number; y: number } {
  for (let i = 0; i < CURSOR.length - 1; i++) {
    const [t0, x0, y0] = CURSOR[i];
    const [t1, x1, y1] = CURSOR[i + 1];
    if (t <= t1) {
      const p = ease((t - t0) / (t1 - t0));
      return { x: lerp(x0, x1, p), y: lerp(y0, y1, p) };
    }
  }
  const [, x, y] = CURSOR[CURSOR.length - 1];
  return { x, y };
}

function stepAt(t: number): number {
  let step = 0;
  STARTS.forEach((start, i) => {
    if (t >= start) step = i;
  });
  return step;
}

/** A short press of the pointer around each click (down and back up, no rebound). */
function pressScale(t: number): number {
  for (const c of CLICKS) {
    const p = (t - c + 90) / 220;
    if (p > 0 && p < 1) return 1 - 0.12 * Math.sin(Math.PI * p);
  }
  return 1;
}

/* ------------------------------------------------------------------ */
/* Drawing primitives                                                  */
/* ------------------------------------------------------------------ */

const ROW_H = 19;

interface Row {
  id: string;
  /** Width of the grey placeholder that stands in for text. */
  bar?: number;
  /** Real text instead of a placeholder. */
  label?: string;
  /** Custom icon; defaults to a grey rounded square. */
  icon?: (x: number, cy: number) => ReactNode;
  /** Right-hand control. */
  trailing?: "chevron" | "off" | ((cy: number, right: number) => ReactNode);
  /** 0–1: pressed-row tint. */
  highlight?: number;
}

function Chevron({ x, cy }: { x: number; cy: number }) {
  return (
    <path
      d={`M${x} ${cy - 3} l3 3 l-3 3`}
      fill="none"
      stroke={C.muted}
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  );
}

function Toggle({ x, cy, on }: { x: number; cy: number; on: number }) {
  const w = 20;
  const h = 11;
  return (
    <g>
      <rect x={x} y={cy - h / 2} width={w} height={h} rx={h / 2} fill={mix(C.line, C.accent, on)} />
      <circle cx={r2(x + h / 2 + (w - h) * on)} cy={cy} r={3.75} fill={mix(C.muted, C.text, on)} />
    </g>
  );
}

/** OnKey: an accent rounded square with a small wave. */
function AppIcon({ x, y, s }: { x: number; y: number; s: number }) {
  const x0 = x + s * 0.2;
  const cy = y + s / 2;
  const q = (s * 0.6) / 6;
  const a = s * 0.16;
  return (
    <g>
      <rect x={x} y={y} width={s} height={s} rx={r2(s * 0.28)} fill={C.accent} />
      <path
        d={`M${r2(x0)} ${r2(cy)} q${r2(q)} ${r2(-2 * a)} ${r2(2 * q)} 0 t${r2(2 * q)} 0 t${r2(2 * q)} 0`}
        fill="none"
        stroke={C.text}
        strokeWidth={s >= 16 ? 1.5 : 1.2}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </g>
  );
}

function WindowLights({ x, y, r, gap }: { x: number; y: number; r: number; gap: number }) {
  return (
    <g>
      {[C.red, C.amber, C.good].map((c, i) => (
        <circle key={c} cx={x + i * gap} cy={y} r={r} fill={c} fillOpacity={0.85} />
      ))}
    </g>
  );
}

function Group({ x, y, w, rows }: { x: number; y: number; w: number; rows: readonly Row[] }) {
  const right = x + w - 8;
  return (
    <g>
      <rect
        x={x}
        y={y}
        width={w}
        height={rows.length * ROW_H}
        rx={9}
        fill={C.bg}
        fillOpacity={0.35}
        stroke={C.line}
        strokeWidth={1.5}
      />
      {rows.map((row, i) => {
        const top = y + i * ROW_H;
        const cy = top + ROW_H / 2;
        const hl = row.highlight ?? 0;
        let trailing: ReactNode = null;
        if (row.trailing === "chevron") trailing = <Chevron x={right - 4} cy={cy} />;
        else if (row.trailing === "off") trailing = <Toggle x={right - 20} cy={cy} on={0} />;
        else if (row.trailing) trailing = row.trailing(cy, right);
        return (
          <g key={row.id}>
            {i > 0 && <line x1={x + 24} x2={x + w - 8} y1={top} y2={top} stroke={C.line} strokeWidth={1.5} />}
            {hl > 0 && (
              <rect
                x={x + 2}
                y={top + 2}
                width={w - 4}
                height={ROW_H - 4}
                rx={6}
                fill={C.accent}
                fillOpacity={r2(0.28 * hl)}
                stroke={C.accent}
                strokeOpacity={r2(hl)}
                strokeWidth={1.5}
              />
            )}
            {row.icon ? (
              row.icon(x + 6, cy)
            ) : (
              <rect x={x + 8} y={cy - 5} width={10} height={10} rx={3} fill={C.line} />
            )}
            {row.label ? (
              <text x={x + 24} y={cy + 3} fontSize={8.5} fill={C.text}>
                {row.label}
              </text>
            ) : (
              <rect x={x + 24} y={cy - 2.25} width={row.bar ?? 56} height={4.5} rx={2.25} fill={C.line} />
            )}
            {trailing}
          </g>
        );
      })}
    </g>
  );
}

/* ------------------------------------------------------------------ */
/* Settings window                                                     */
/* ------------------------------------------------------------------ */

const SIDEBAR_BARS = [44, 58, 36, 50, 0, 46, 40] as const;
const PRIVACY_ROW = 4;

function Sidebar({ select }: { select: number }) {
  const t = useT();
  return (
    <g>
      {/* search field */}
      <rect x={20} y={33} width={102} height={13} rx={6.5} fill={C.bg} fillOpacity={0.35} stroke={C.line} strokeWidth={1.5} />
      <circle cx={28.5} cy={39.2} r={2.4} fill="none" stroke={C.muted} strokeWidth={1.5} />
      <path d="M30.4 41.1 l1.7 1.7" stroke={C.muted} strokeWidth={1.5} strokeLinecap="round" />
      {SIDEBAR_BARS.map((w, i) => {
        const top = 54 + i * 17;
        const cy = top + 7;
        if (i === PRIVACY_ROW) {
          return (
            <g key="privacy">
              <rect x={16} y={top - 1} width={110} height={16} rx={6} fill={C.accentDark} opacity={r2(select)} />
              <rect x={22} y={cy - 5} width={10} height={10} rx={3} fill={mix(C.line, C.accentLight, select)} />
              {/* (within its row, which ends at 126; cut short with "…" past that, as macOS's sidebar does) */}
              <Fit x={38} y={cy + 3} max={84} floor={0.85} ellipsis fontSize={8.5} fill={mix(C.muted, C.text, select)}>
                {t("guides.systemAudio.mock.privacySecurity")}
              </Fit>
            </g>
          );
        }
        return (
          <g key={`row-${i}`}>
            <rect x={22} y={cy - 5} width={10} height={10} rx={3} fill={C.line} />
            <rect x={38} y={cy - 2.25} width={w} height={4.5} rx={2.25} fill={C.line} />
          </g>
        );
      })}
    </g>
  );
}

/** Whatever pane was open before: grey placeholder rows. */
function GeneralPane() {
  return (
    <g>
      <rect x={146} y={17} width={48} height={7} rx={3.5} fill={C.line} />
      <Group
        x={144}
        y={36}
        w={194}
        rows={[
          { id: "a", bar: 64, trailing: "chevron" },
          { id: "b", bar: 48, trailing: "chevron" },
          { id: "c", bar: 72, trailing: "chevron" },
        ]}
      />
      <Group
        x={144}
        y={101}
        w={194}
        rows={[
          { id: "d", bar: 56, trailing: "off" },
          { id: "e", bar: 40, trailing: "chevron" },
        ]}
      />
      <Group x={144} y={147} w={194} rows={[{ id: "f", bar: 60, trailing: "chevron" }]} />
    </g>
  );
}

const PRIVACY_BARS = [62, 48, 70, 54, 40, 66, 58, 44, 72, 0, 50, 60, 46] as const;
const TARGET_ROW = 9;
const SCROLL_BY = 100;

function PrivacyPane({ scroll, press }: { scroll: number; press: number }) {
  const t = useT();
  const rows: Row[] = PRIVACY_BARS.map((w, i) =>
    i === TARGET_ROW
      ? { id: "target", label: t("guides.systemAudio.mock.screenRecording"), trailing: "chevron", highlight: press }
      : { id: `p${i}`, bar: w, trailing: "chevron" },
  );
  return (
    <g>
      <text x={146} y={23.5} fontSize={9.5} fontWeight={600} fill={C.text}>
        {t("guides.systemAudio.mock.privacySecurity")}
      </text>
      {/* toolbar edge appears once the list scrolls under it */}
      <line x1={132} x2={350} y1={33} y2={33} stroke={C.line} strokeWidth={1.5} opacity={r2(scroll)} />
      {/* nested svg = scroll viewport (clips by default, no ids needed) */}
      <svg x={132} y={34} width={218} height={146} overflow="hidden">
        <g transform={`translate(0 ${r2(-SCROLL_BY * scroll)})`}>
          <Group x={12} y={4} w={194} rows={rows} />
        </g>
      </svg>
    </g>
  );
}

function AudioPane({ on }: { on: number }) {
  const t = useT();
  return (
    <g>
      <path d="M150 16.5 l-3.5 3.5 l3.5 3.5" fill="none" stroke={C.muted} strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" />
      <text x={156} y={23.5} fontSize={9} fontWeight={600} fill={C.text}>
        {t("guides.systemAudio.mock.screenRecording")}
      </text>
      <rect x={146} y={37} width={74} height={4.5} rx={2.25} fill={C.line} />
      <Group
        x={144}
        y={46}
        w={194}
        rows={[
          { id: "a", bar: 52, trailing: "off" },
          { id: "b", bar: 40, trailing: "off" },
        ]}
      />
      <text x={146} y={100} fontSize={8.5} fontWeight={600} fill={C.text}>
        {t("guides.systemAudio.mock.audioOnly")}
      </text>
      <Group
        x={144}
        y={107}
        w={194}
        rows={[
          {
            id: "onkey",
            label: t("guides.systemAudio.mock.appName"),
            icon: (ix, cy) => <AppIcon x={ix} y={cy - 6.5} s={13} />,
            trailing: (cy, right) => <Toggle x={right - 20} cy={cy} on={on} />,
          },
          { id: "c", bar: 46, trailing: "off" },
        ]}
      />
    </g>
  );
}

interface WindowState {
  select: number;
  generalOut: number;
  privacyIn: number;
  privacyOut: number;
  audioIn: number;
  scroll: number;
  press: number;
  on: number;
}

function SettingsWindow({ select, generalOut, privacyIn, privacyOut, audioIn, scroll, press, on }: WindowState) {
  return (
    <g>
      <rect x={10} y={8} width={340} height={174} rx={14} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
      <line x1={132} x2={132} y1={8} y2={182} stroke={C.line} strokeWidth={1.5} />
      <WindowLights x={24} y={20} r={3.5} gap={12} />
      <Sidebar select={select} />
      {generalOut < 1 && (
        <g opacity={r2(1 - generalOut)}>
          <GeneralPane />
        </g>
      )}
      {privacyIn > 0 && privacyOut < 1 && (
        <g opacity={r2(privacyIn * (1 - privacyOut))} transform={`translate(${r2((1 - privacyIn) * 8)} 0)`}>
          <PrivacyPane scroll={scroll} press={press} />
        </g>
      )}
      {audioIn > 0 && (
        <g opacity={r2(audioIn)} transform={`translate(${r2((1 - audioIn) * 8)} 0)`}>
          <AudioPane on={on} />
        </g>
      )}
    </g>
  );
}

/* ------------------------------------------------------------------ */
/* Dialog, OnKey window, pointer                                       */
/* ------------------------------------------------------------------ */

function QuitDialog({ opacity, scale, pressed }: { opacity: number; scale: number; pressed: number }) {
  const t = useT();
  return (
    <g opacity={r2(opacity)} transform={`translate(180 90) scale(${r2(scale)}) translate(-180 -90)`}>
      <rect x={100} y={42} width={160} height={96} rx={14} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
      <AppIcon x={172} y={52} s={16} />
      <text x={180} y={84} fontSize={10} fontWeight={600} fill={C.text} textAnchor="middle">
        {t("guides.systemAudio.mock.quitReopenQuestion")}
      </text>
      <rect x={124} y={90} width={112} height={4} rx={2} fill={C.line} />
      <rect x={140} y={97} width={80} height={4} rx={2} fill={C.line} />
      <rect x={110} y={108} width={64} height={18} rx={7} fill={C.line} />
      <text x={142} y={120} fontSize={8.5} fill={C.muted} textAnchor="middle">
        {t("guides.systemAudio.mock.later")}
      </text>
      <rect x={180} y={108} width={70} height={18} rx={7} fill={mix(C.accent, C.accentDark, pressed)} />
      <text x={215} y={120} fontSize={8.5} fontWeight={500} fill={C.text} textAnchor="middle">
        {t("guides.systemAudio.mock.quitReopen")}
      </text>
    </g>
  );
}

function AppWindow({ appear, fill, draw }: { appear: number; fill: number; draw: number }) {
  const t = useT();
  return (
    <g opacity={r2(appear)} transform={`translate(0 ${r2((1 - appear) * 6)})`}>
      <rect x={98} y={40} width={164} height={106} rx={14} fill={C.panel} stroke={C.line} strokeWidth={1.5} />
      <WindowLights x={110} y={52} r={3} gap={10} />
      <AppIcon x={110} y={62} s={20} />
      <text x={138} y={75} fontSize={11} fontWeight={600} fill={C.text}>
        {t("guides.systemAudio.mock.appName")}
      </text>
      <text x={138} y={86} fontSize={8} fill={C.muted}>
        {t("guides.systemAudio.mock.setup")}
      </text>
      <line x1={110} x2={250} y1={93} y2={93} stroke={C.line} strokeWidth={1.5} />
      <circle
        cx={117}
        cy={107}
        r={6.5}
        fill={C.good}
        fillOpacity={r2(fill)}
        stroke={mix(C.line, C.good, fill)}
        strokeWidth={1.5}
      />
      <path
        d="M113.9 107.2 l2.2 2.2 l4.1 -4.3"
        fill="none"
        stroke={C.bg}
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeLinejoin="round"
        pathLength={1}
        strokeDasharray="1"
        strokeDashoffset={r2(1 - draw)}
        opacity={draw > 0 ? 1 : 0}
      />
      <text x={130} y={110} fontSize={8.5} fill={C.text}>
        {t("guides.systemAudio.mock.systemAudioRow")}
      </text>
      <circle cx={117} cy={126} r={6.5} fill="none" stroke={C.line} strokeWidth={1.5} />
      <rect x={130} y={123.75} width={78} height={4.5} rx={2.25} fill={C.line} />
    </g>
  );
}

function Pointer({ t }: { t: number }) {
  const { x, y } = cursorAt(t);
  const s = pressScale(t);
  let ring: ReactNode = null;
  for (const c of CLICKS) {
    const p = (t - c) / 450;
    if (p >= 0 && p <= 1) {
      const e = ease(p);
      ring = (
        <circle
          cx={r2(x)}
          cy={r2(y)}
          r={r2(2 + 8 * e)}
          fill="none"
          stroke={C.accentLight}
          strokeWidth={1.5}
          opacity={r2(0.75 * (1 - e))}
        />
      );
      break;
    }
  }
  return (
    <g>
      {ring}
      <g transform={`translate(${r2(x)} ${r2(y)}) scale(${r2(s)})`}>
        <path
          d="M0 0 V12.6 L3.1 9.7 L5.3 14.6 L7.5 13.7 L5.4 8.9 H9.6 Z"
          fill={C.text}
          stroke={C.bg}
          strokeWidth={1.5}
          strokeLinejoin="round"
        />
      </g>
    </g>
  );
}

/** One frame of the loop as a pure function of time. */
function Scene({ t }: { t: number }) {
  const show = seg(t, T.fadeIn) * (1 - seg(t, T.fadeOut));
  const dialogIn = seg(t, T.dialogIn);
  const dialogOut = seg(t, T.dialogOut);
  const appear = seg(t, T.appIn);
  const pointer = 1 - seg(t, T.cursorOut);
  return (
    <g opacity={r2(show)}>
      <SettingsWindow
        select={seg(t, T.sidebarSelect)}
        generalOut={seg(t, T.paneGeneralOut)}
        privacyIn={seg(t, T.panePrivacyIn)}
        privacyOut={seg(t, T.panePrivacyOut)}
        audioIn={seg(t, T.paneAudioIn)}
        scroll={seg(t, T.scroll)}
        press={seg(t, T.rowPress)}
        on={seg(t, T.toggle)}
      />
      {dialogIn > 0 && (
        <rect x={10} y={8} width={340} height={174} rx={14} fill={C.bg} opacity={r2(0.55 * dialogIn)} />
      )}
      {dialogIn > 0 && dialogOut < 1 && (
        <QuitDialog
          opacity={dialogIn * (1 - dialogOut)}
          scale={lerp(0.96, 1, dialogIn)}
          pressed={seg(t, T.quitPress)}
        />
      )}
      {appear > 0 && <AppWindow appear={appear} fill={seg(t, T.checkFill)} draw={seg(t, T.checkDraw)} />}
      {pointer > 0 && (
        <g opacity={r2(pointer)}>
          <Pointer t={t} />
        </g>
      )}
    </g>
  );
}

function Frame({ t }: { t: number }) {
  return (
    <svg
      className="osa-svg"
      style={{ display: "block", width: "100%", height: "auto" }}
      viewBox={`0 0 ${VB_W} ${VB_H}`}
      fontFamily={FONT}
      aria-hidden="true"
      focusable="false"
    >
      <Scene t={t} />
    </svg>
  );
}

/* ------------------------------------------------------------------ */
/* Styles                                                              */
/* ------------------------------------------------------------------ */

const CSS = `
.osa{position:relative;width:100%;display:flex;flex-direction:column;background:${C.bg};color:${C.text};font-family:${FONT};border-radius:16px;overflow:hidden}
.osa-svg{display:block;width:100%;height:auto}
.osa-caption{margin:0;padding:2px 16px 0;min-height:2.7em;display:flex;align-items:center;justify-content:center;text-align:center;font-size:13px;line-height:1.35;text-wrap:balance;animation:osa-in .35s ease-in-out both}
@keyframes osa-in{from{opacity:0;transform:translateY(2px)}to{opacity:1;transform:none}}
.osa-dots{display:flex;justify-content:center;gap:2px;padding:0 0 4px}
.osa-dot{appearance:none;-webkit-appearance:none;background:none;border:0;margin:0;padding:0;width:26px;height:24px;display:grid;place-items:center;cursor:pointer;border-radius:12px;-webkit-tap-highlight-color:transparent}
.osa-dot:focus{outline:none}
.osa-dot:focus-visible{outline:1.5px solid ${C.accentLight};outline-offset:-3px}
.osa-mark{display:block;width:6px;height:6px;border-radius:3px;background:${C.line};overflow:hidden;transition:width .3s ease-in-out,background-color .2s ease-in-out}
.osa-dot:hover .osa-mark{background:${C.muted}}
.osa-dot[aria-current="step"] .osa-mark{width:18px;background:${C.line}}
.osa-fill{display:block;height:100%;background:${C.accent};border-radius:3px}
.osa-strip{display:grid;grid-template-columns:repeat(auto-fit,minmax(148px,1fr));gap:12px;margin:0;padding:12px;list-style:none}
.osa-card{display:flex;flex-direction:column;gap:6px;min-width:0}
.osa-card-cap{display:flex;gap:6px;align-items:flex-start;margin:0;padding:0 2px;font-size:12px;line-height:1.35}
.osa-num{flex:none;box-sizing:border-box;width:16px;height:16px;border-radius:8px;border:1.5px solid ${C.accent};color:${C.accentLight};font-size:10px;line-height:13px;text-align:center;font-variant-numeric:tabular-nums}
`;

/* ------------------------------------------------------------------ */
/* Reduced-motion preference                                           */
/* ------------------------------------------------------------------ */

// The guides always animate, also with Reduce motion on (Daniel asked, 29 Oct). Smaller motion elsewhere in
// OnKey still follows the setting.
function usePrefersReducedMotion(): boolean {
  return false;
}

function isFocusVisible(el: EventTarget | null): boolean {
  if (!(el instanceof Element)) return false;
  try {
    return el.matches(":focus-visible");
  } catch {
    return true;
  }
}

/* ------------------------------------------------------------------ */
/* Component                                                           */
/* ------------------------------------------------------------------ */

export interface SystemAudioSetupAnimationProps {
  /** Force the static strip (true) or the animation (false). Defaults to the animation. */
  reducedMotion?: boolean;
  /** Replaces the built-in accessible summary. */
  ariaLabel?: string;
  className?: string;
  style?: CSSProperties;
}

export default function SystemAudioGuide({
  reducedMotion,
  ariaLabel: givenLabel,
  className,
  style,
}: SystemAudioSetupAnimationProps) {
  const t = useT();
  const ariaLabel = givenLabel ?? t("guides.systemAudio.summary");
  const prefersReduced = usePrefersReducedMotion();
  const still = reducedMotion ?? prefersReduced;

  const [time, setTime] = useState(FIRST_FRAME);
  const timeRef = useRef(FIRST_FRAME);
  const holdAtRef = useRef(FIRST_FRAME);
  const pauseRef = useRef({ hover: false, focus: false });
  const visibleRef = useRef(true);
  const rootRef = useRef<HTMLDivElement>(null);

  // Clock. Advances only while visible; while paused it may still play up to holdAt (after a jump).
  useEffect(() => {
    if (still) return;
    let raf = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(64, now - last);
      last = now;
      if (visibleRef.current) {
        const paused = pauseRef.current.hover || pauseRef.current.focus;
        let next = timeRef.current;
        if (!paused) next = (next + dt) % TOTAL;
        else if (next < holdAtRef.current) next = Math.min(holdAtRef.current, next + dt);
        if (next !== timeRef.current) {
          timeRef.current = next;
          setTime(next);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [still]);

  // Skip work while scrolled out of view.
  useEffect(() => {
    const el = rootRef.current;
    if (still || !el || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver((entries) => {
      const entry = entries[entries.length - 1];
      visibleRef.current = entry ? entry.isIntersecting : true;
    });
    io.observe(el);
    return () => io.disconnect();
  }, [still]);

  const setPaused = useCallback((source: "hover" | "focus", on: boolean) => {
    const p = pauseRef.current;
    const was = p.hover || p.focus;
    p[source] = on;
    if (!was && (p.hover || p.focus)) holdAtRef.current = timeRef.current;
  }, []);

  const jump = useCallback((i: number) => {
    timeRef.current = STARTS[i];
    holdAtRef.current = HOLD_AT[i];
    setTime(STARTS[i]);
  }, []);

  const rootClass = className ? `osa ${className}` : "osa";

  if (still) {
    return <StillSteps count={STEPS.length} label={ariaLabel} frame={i => <Frame t={STILL_AT[i]} />} caption={i => t(`guides.systemAudio.step.${STEPS[i]}`)} />;
  }

  const step = stepAt(time);

  return (
    <div
      ref={rootRef}
      className={rootClass}
      style={style}
      onPointerEnter={(e) => {
        if (e.pointerType !== "touch") setPaused("hover", true);
      }}
      onPointerLeave={(e) => {
        if (e.pointerType !== "touch") setPaused("hover", false);
      }}
      onFocus={(e) => {
        if (isFocusVisible(e.target)) setPaused("focus", true);
      }}
      onBlur={(e) => {
        const next = e.relatedTarget;
        if (!(next instanceof Node) || !e.currentTarget.contains(next)) setPaused("focus", false);
      }}
    >
      <style>{CSS}</style>
      <div role="img" aria-label={ariaLabel}>
        <Frame t={time} />
        <p key={step} className="osa-caption">
          {t(`guides.systemAudio.step.${STEPS[step]}`)}
        </p>
      </div>
      <div className="osa-dots" role="group" aria-label={t("guides.systemAudio.dots")}>
        {STEPS.map((id, i) => {
          const caption = t(`guides.systemAudio.step.${id}`);
          const active = i === step;
          const progress = active ? clamp((time - STARTS[i]) / DURATIONS[i]) : 0;
          return (
            <button
              key={id}
              type="button"
              className="osa-dot"
              aria-label={t("guides.systemAudio.stepOf", { step: i + 1, count: STEPS.length, caption })}
              aria-current={active ? "step" : undefined}
              onClick={() => jump(i)}
            >
              <span className="osa-mark">
                <span className="osa-fill" style={{ width: `${r2(progress * 100)}%` }} />
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
