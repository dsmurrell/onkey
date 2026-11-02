"use client";
// The singing lane: target note bars scrolling towards a fixed "now" line, the singer's actual pitch as a faint
// line, and your pitch as a bright line coloured by how close you are. Each bar is judged whole, by the
// scoring's rule: green once you've held it within a semitone for half its length, red if you sang it but
// didn't, grey if you didn't sing it.
// Drawn on a canvas every animation frame straight from the live store (no React re-renders per frame).
// A learned song shows its whole range, fixed for the song. A first play (no notes yet) shows one octave that
// wraps: every pitch is drawn at its note, whatever its octave, so the lane never pans; what goes off the bottom
// comes in at the top. Where the octave wraps is set once, early in the take (see wrapEdge).
import { useEffect, useRef } from "react";
import { live } from "@/lib/session";
import { frameDrawn, frameIdle } from "@/lib/perf";
import { ACCENT, INK, NOTICE } from "@/lib/colors";
import { BLACK_KEYS, centreAt, COLORS, credit, creditAt, fold, grade, LEVELS, NOTE_NAMES, noteName, smoothed } from "@/lib/music";
import { drawRibbon, grain, paletteFor, ribbonLive } from "@/components/Ribbon";
import { guide, pitchAlpha } from "@/lib/guideFade";
import { getLocale, t, useT } from "@/lib/i18n";

export const KEYS_W = 54;     // keyboard strip on the left
export const NOW_AT = 0.3;    // the now line, as a fraction of the lane width (same on every play)
const FX = 0.25;              // the ribbon's glow layer is drawn at 1/4 size: the upscale is its blur
/** Development: a faint ribbon behind the notes on learned songs too (off; localStorage onkey.ribbonBehindNotes=1). */
const ribbonBehindNotes = () => { try { return localStorage.getItem("onkey.ribbonBehindNotes") === "1"; } catch { return false; } };
const SECONDS_ACROSS = 9;     // how much of the song fits across the lane

// per bar: frames judged so far, how many within a semitone, how many sung. Your pitch arrives a little after
// the moment it belongs to, so a frame is only final (cached) once it's 0.4 s old; newer ones are re-judged
// every time
type Judged = { upto: number; ok: number; sung: number; cr: number };
let judgedFor: unknown = null;
let judged = new Map<string, Judged>();
const SETTLE = 40;
function judgeFrame(f: number) {
  const u = live.youAt(f);
  if (Number.isNaN(u)) return [0, 0, 0];
  const e = errAt(f, u);
  return [Math.abs(e) <= 100 ? 1 : 0, 1, Number.isNaN(e) ? 0 : credit(e)];
}
function judge(s: number, e: number, upTo: number): Judged {
  if (judgedFor !== live.song?.cacheBars) { judgedFor = live.song?.cacheBars; judged = new Map(); }
  const key = `${Math.round(s)}:${Math.round(e)}`;
  let j = judged.get(key);
  if (!j) { j = { upto: Math.round(s) + 8, ok: 0, sung: 0, cr: 0 }; judged.set(key, j); }   // its first 80 ms (the scoop up) don't count
  for (; j.upto < Math.min(e, upTo - SETTLE); j.upto++) { const [o, v, c] = judgeFrame(j.upto); j.ok += o; j.sung += v; j.cr += c; }
  const out = { ...j };
  for (let f = j.upto; f < Math.min(e, upTo); f++) { const [o, v, c] = judgeFrame(f); out.ok += o; out.sung += v; out.cr += c; }
  return out;
}

// per lyric line, once it has been sung and settled: how it went, graded as the report grades a line (credit
// over the frames where the singer sings and you do, times how much of the line you sang, full from half)
/** A line's rank (its word: lane.verdict.<rank>), and when it appeared. */
type Rank = "perfect" | "excellent" | "great" | "good" | "ok";
type LineVerdict = { rank: Rank; at: number };
let verdictsFor: unknown = null;
let verdicts = new Map<number, LineVerdict | null>();
const RANKS: [number, Rank][] = [[0.95, "perfect"], [0.9, "excellent"], [0.8, "great"], [0.6, "good"], [0.4, "ok"]];
function lineScore(f0: number, f1: number) {
  let ref = 0, both = 0, cr = 0;
  const level = live.scoring, win = LEVELS[level].centre;   // at Steady and Pro, your pitch's centre (vibrato evens out)
  for (let f = Math.round(f0); f < f1; f++) {
    if (Number.isNaN(live.singerAt(f))) continue;
    ref++;
    const u = win ? centreAt(g => live.youAt(g), f, win) : live.youAt(f);
    if (Number.isNaN(u)) continue;
    const e = errAt(f, u);
    if (Number.isNaN(e)) continue;
    both++; cr += creditAt(e, level);
  }
  if (ref < 30 || both / ref < 0.25) return null;                // a short or barely sung line gets no word
  return (cr / both) * Math.min(1, both / ref / 0.5);
}
/** The word to show for the line that ended last (for 1.6 s), on learned songs you can see the notes of. */
function lineVerdict(now: number, perf: number): LineVerdict | null {
  const song = live.song;
  if (!song || song.mode !== "sing" || !live.showNotes() || live.songOffset == null) return null;
  if (verdictsFor !== song.version) { verdictsFor = song.version; verdicts = new Map(); }
  const k = live.songOffset * 100, lyr = song.lyrics, bars = live.bars();
  let show: LineVerdict | null = null;
  for (let i = 0; i < lyr.length; i++) {
    if (!/[A-Za-z0-9\u00C0-\uFFFF]/.test(lyr[i][1])) continue;
    const l0 = lyr[i][0] * 100 - k, nx = i + 1 < lyr.length ? lyr[i + 1][0] * 100 - k : l0 + 600;
    // the line's singing ends with its last bar (else where the next line starts)
    let end = -Infinity;
    for (const [s, e] of bars) if (s >= l0 - 20 && s < nx && e > end) end = e;
    if (!Number.isFinite(end)) end = nx;
    if (now < end + SETTLE) break;                               // not settled yet (lines are in order)
    if (!verdicts.has(i)) {
      if (now > end + SETTLE + 150) { verdicts.set(i, null); continue; }   // joined later: no stale pop-ups
      const sc = lineScore(l0, Math.min(nx, end + 10));
      const rank = sc == null ? null : RANKS.find(([min]) => sc > min)?.[1] ?? null;
      verdicts.set(i, rank ? { rank, at: live.frozenPerf != null ? perf - 400 : perf } : null);   // (a replay is a still: show it at full)
    }
    const v = verdicts.get(i);
    if (v && perf - v.at < 1600) show = v;
  }
  return show;
}

/** A line just sung Excellent or Perfect (for the dancer to celebrate): when its word appeared, the same for the
 *  whole 1.6 s it shows, else null. */
export function lineCheer(now: number, perf: number): number | null {
  const v = lineVerdict(now, perf);
  return v && (v.rank === "excellent" || v.rank === "perfect") ? v.at : null;
}

/** The verdict on the bar that ended most recently, if within the last `within` frames (for the dancer). */
export function recentVerdict(now: number, within = 60): "hit" | "miss" | null {
  if (!live.showNotes()) return null;
  let best: [number, number] | null = null;
  for (const [s, e] of live.bars()) if (e <= now && e > now - within && (!best || e > best[1])) best = [s, e];
  if (!best) return null;
  const [s, e] = best, len = Math.max(1, e - (s + 8)), j = judge(s, e, now);
  return j.ok / len >= 0.5 ? "hit" : j.sung / len >= 0.25 ? "miss" : null;
}

function errAt(f: number, u: number) {
  // your error vs the singer's line or the note bar there, whichever is closer, forgiving +-80 ms of timing
  // like the scoring
  let best = NaN;
  const bars = live.showNotes() ? live.bars() : [];
  for (let d = -8; d <= 8; d += 2) {
    const refs = [live.singerAt(f + d)];
    for (const [s, e, m] of bars) if (s <= f + d && f + d < e) { refs.push(m); break; }
    for (const s of refs) {
      if (Number.isNaN(s)) continue;
      const e = fold(100 * (u - s));
      if (Number.isNaN(best) || Math.abs(e) < Math.abs(best)) best = e;
    }
  }
  return best;
}

/** First plays: the wrapped lane's lowest row until you've sung (the C below middle C: its octave runs C3-B3). */
const WRAP_DEFAULT = 48;
/** Sung frames (10 ms) that settle where the octave wraps: your first phrase or so. */
const WRAP_AFTER = 200;

/** Where a wrapped octave should start (its lowest row, a MIDI note) given how much you sang each note (pitch
 *  class, 0 = C): the edge goes between the two neighbouring notes you sang least, so your line rarely crosses it. */
export function wrapEdge(sung: number[]): number {
  let best = 0, least = Infinity;
  for (let pc = 0; pc < 12; pc++) {
    const w = sung[(pc + 11) % 12] + sung[pc];
    if (w < least - 1e-9) { least = w; best = pc; }
  }
  return 48 + best;              // the octave from that note up, near the default (rows are by note, not octave)
}

function pickRange(): [number, number] {
  // Fixed for the whole song, so the lane never shifts: the song's own range on a repeat play,
  // otherwise B2-D5, which covers nearly every lead vocal.
  const bars = live.song?.cacheBars ?? [];
  if (bars.length > 10) {
    const notes = bars.map(b => b[2]).sort((a, b) => a - b);
    let lo = notes[Math.floor(notes.length * 0.005)] - 1, hi = notes[Math.ceil(notes.length * 0.995) - 1] + 1;
    while (hi - lo < 15) { lo--; hi++; }
    return [lo, hi];
  }
  return [47, 74];
}

export default function Lane() {
  useT();      // (re-rendered on a language change; the canvas draws with the module's `t`, and the signature has the language)
  const ref = useRef<HTMLCanvasElement>(null);
  const grainRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const cv = ref.current!;
    const g = cv.getContext("2d")!;
    // the ribbon's glow, drawn small and composited over the lane with "screen"
    const fxCv = document.createElement("canvas"), fx = fxCv.getContext("2d")!;
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    const behind = ribbonBehindNotes();
    let lastDrawn = 0, grainOn = false;
    const FONT = getComputedStyle(document.body).fontFamily;   // canvas can't resolve CSS variables
    let raf = 0, range: [number, number] | null = null, rangeFor = -1;
    // first play: each frame's shown pitch, with a moment's octave slip put back in its phrase's octave, decided
    // once, in order, as frames arrive (the past never moves)
    const fp = { song: null as unknown, upto: -1, v: new Map<number, number>(), phrase: [] as number[], away: 0, prevF: -1e9 };
    const firstPlayFold = (now: number) => {
      if (fp.song !== live.song?.version) {
        fp.song = live.song?.version; fp.upto = -1; fp.v = new Map(); fp.phrase = []; fp.away = 0; fp.prevF = -1e9;
      }
      let newest = -1;                                      // the newest frame that has arrived
      for (let f = now; f > now - 60; f--) if (!Number.isNaN(live.youAt(f))) { newest = f; break; }
      if (fp.upto < 0) fp.upto = Math.max(0, newest - 1000);
      for (let f = fp.upto + 1; f <= newest; f++) {
        const u = live.youAt(f);
        if (Number.isNaN(u)) continue;
        if (f - fp.prevF > 30) { fp.phrase = []; fp.away = 0; }      // a new phrase
        let v = u;
        if (fp.phrase.length >= 5) {
          const sorted = fp.phrase.slice(-25).sort((a, b) => a - b), ref = sorted[sorted.length >> 1];
          const k = Math.round((ref - u) / 12);
          if (k !== 0 && Math.abs(u + 12 * k - ref) < 4) {
            if (++fp.away < 15) v = u + 12 * k; else { fp.phrase = []; fp.away = 0; }
          } else fp.away = 0;
        }
        fp.phrase.push(v); fp.prevF = f; fp.v.set(f, v - u);
      }
      if (newest > fp.upto) fp.upto = newest;
      if (fp.v.size > 6000) for (const k of fp.v.keys()) { if (k < newest - 3000) fp.v.delete(k); else break; }
    };
    // the "you" ball: where it is, where it's gliding from/to and since when, how loud and what colour
    const ball = { y: 0, r: 3.5, a: 0, from: 0, to: 0, t0: 0, level: 8, col: INK[100] as string, seen: -1e9, on: false, okAt: -1e9, fade: 1 };
    // first play: where the wrapped octave starts, set once per take from your first phrase
    const wrap = { song: null as unknown, lo: WRAP_DEFAULT, set: false, scanned: -1, sung: new Array(12).fill(0), n: 0 };
    const resize = () => {
      const r = cv.getBoundingClientRect(), dpr = devicePixelRatio || 1;
      cv.width = Math.round(r.width * dpr); cv.height = Math.round(r.height * dpr);
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      fxCv.width = Math.max(1, Math.ceil(r.width * FX)); fxCv.height = Math.max(1, Math.ceil(r.height * FX));
    };
    // (resizing clears the canvas: drawn again at once)
    const ro = new ResizeObserver(() => { resize(); force = true; wake(); });
    ro.observe(cv);
    resize();

    // Drawn only when something on it changes. What's drawn depends on the song, its clock, your pitch, the ribbon's
    // frames and the lane's size (the signature): while the song plays that's every frame (at most 60 a second;
    // 30 under reduced motion or Low Power Mode); paused, finished, with nothing playing or with the window hidden
    // it's still, so after the last change's short animations (a verdict's word, the ball's glide, the notes'
    // fade-in) nothing is drawn and macOS has nothing to composite. A still lane with a song looks for changes 4
    // times a second; without one, only the session's events (and a resize) wake it.
    let lastSig = "", changedAt = -1e9, poll = 0, force = true;
    const signature = (perf: number) => {
      const s = live.song, v = live.viz;
      return !s ? `-|${cv.width}x${cv.height}`
        : `${s.version}|${live.now(perf).toFixed(1)}|${live.usr.length}|${v.length ? v[v.length - 1].f : 0}|${live.showNotes()}|${live.blind}|${guide.level}|`
          + `${s.cache ? 1 : 0}|${s.cacheBars.length}|${s.track.art}|${live.songOffset}|${cv.width}x${cv.height}|${motion.matches}|${live.lowPower}|${live.draw30}|${getLocale()}`;
    };
    const tick = (perfNow: number) => {
      raf = 0;
      if (document.visibilityState !== "visible") { showGrain(false); frameIdle(); return; }    // shown again: woken
      const sig = signature(live.frozenPerf ?? perfNow);
      if (sig !== lastSig) { lastSig = sig; changedAt = perfNow; force = true; }
      const settling = !!live.song && perfNow - changedAt < 1700;
      const fps = motion.matches || live.lowPower || live.draw30 ? 30 : 60;
      if ((force || settling) && (force || perfNow - lastDrawn >= 1000 / fps - 3)) {
        force = false; draw(perfNow);
        if (live.playing()) frameDrawn(perfNow, fps); else frameIdle();      // the records: how evenly it draws (lib/perf.ts)
      }
      if (live.playing() || settling || force) raf = requestAnimationFrame(tick);
      else if (live.song && document.visibilityState === "visible") poll = window.setTimeout(wake, 250);
    };
    const wake = () => {
      if (raf) return;
      clearTimeout(poll);
      raf = requestAnimationFrame(tick);
    };
    const showGrain = (on: boolean) => {
      // the grain dithers the ribbon's gradients while it moves; a still or hidden lane has no layer for it at all
      const el = grainRef.current;
      if (!el || on === grainOn) return;
      grainOn = on;
      if (on) { el.style.display = "block"; requestAnimationFrame(() => { if (grainOn) el.style.opacity = "0.55"; }); }
      else { el.style.opacity = "0"; window.setTimeout(() => { if (!grainOn) el.style.display = "none"; }, 700); }
    };

    const draw = (perfNow: number) => {
      lastDrawn = perfNow;
      const perf = live.frozenPerf ?? perfNow;
      const W = cv.clientWidth, H = cv.clientHeight;
      g.clearRect(0, 0, W, H);
      const song = live.song;
      if (!song) { showGrain(false); return; }
      if (rangeFor !== song.version) { range = pickRange(); rangeFor = song.version; }
      // a first play: one octave, wrapped (never pans). Its edge is set once, after your first phrase: the frames
      // are taken in order as they arrive, so a replay decides the same
      const wrapped = !song.cache;
      const nowF = live.now(perf);
      if (wrapped) {
        if (wrap.song !== song.version) { Object.assign(wrap, { song: song.version, lo: WRAP_DEFAULT, set: false, scanned: -1, sung: new Array(12).fill(0), n: 0 }); }
        for (let f = Math.max(wrap.scanned + 1, 0); f <= nowF - 3; f++) {
          const u = live.youAt(f);
          if (!wrap.set && !Number.isNaN(u)) { wrap.sung[((Math.round(u) % 12) + 12) % 12]++; if (++wrap.n >= WRAP_AFTER) { wrap.lo = wrapEdge(wrap.sung); wrap.set = true; } }
          wrap.scanned = f;
        }
      }
      const lo = wrapped ? wrap.lo - 0.5 : range![0], hi = wrapped ? wrap.lo + 11.5 : range![1];
      const mLo = wrapped ? wrap.lo : Math.floor(lo), mHi = wrapped ? wrap.lo + 11 : Math.ceil(hi);
      const n = wrapped ? 12 : hi - lo + 1, top = 10, bottom = H - 10, rowH = (bottom - top) / n;
      const y = wrapped
        ? (m: number) => bottom - ((((m - lo) % 12) + 12) % 12) * rowH
        : (m: number) => {
          // a note outside the song's range (a low or high outlier) is drawn an octave in, never off the lane
          // (scoring doesn't mind the octave); the range spans more than an octave, so it always lands inside
          while (m < lo) m += 12;
          while (m > hi) m -= 12;
          return bottom - (m - lo + 0.5) * rowH;
        };
      // a jump of more than half the lane is a wrap, an octave folded in, or a leap: no stroke across it
      const across = (y0: number, y1: number) => Math.abs(y1 - y0) > (bottom - top) / 2;
      const L = KEYS_W, laneW = W - L;
      const PX = laneW / (SECONDS_ACROSS * 100);
      const now = live.now(perf), nowX = L + laneW * NOW_AT;
      const xOf = (f: number) => nowX + (f - now) * PX;
      const f0 = Math.floor(now - (nowX - L) / PX), f1 = Math.ceil(now + (W - nowX) / PX);
      const show = live.showNotes();            // learned, placed, and not blind
      const learning = song.mode === "learn";

      // rows: black-key rows a touch darker, a hairline under each C
      g.save(); g.beginPath(); g.rect(0, top, W, bottom - top); g.clip();
      for (let m = mLo; m <= mHi; m++) {
        const black = BLACK_KEYS.has(((m % 12) + 12) % 12);
        g.fillStyle = black ? "#0d1017" : "#10141c";
        g.fillRect(L, y(m) - rowH / 2, laneW, rowH);
        if (m % 12 === 0) { g.fillStyle = "rgba(255,255,255,.06)"; g.fillRect(L, y(m) + rowH / 2 - 0.5, laneW, 1); }
      }

      // no notes ahead (learning this song, or blind): the music as a ribbon of light coming out of the now line
      // (once its frames arrive; until then a quiet hatched future), with a short explanation
      const ribbon = (!show || behind) && ribbonLive(now);
      const reduced = motion.matches;
      // the glow (a second canvas, composited with "screen"): not under reduced motion or Low Power Mode
      const glow = !reduced && !live.lowPower;
      showGrain(ribbon && live.playing());
      if (ribbon) {
        if (glow) { fx.setTransform(FX, 0, 0, FX, 0, 0); fx.clearRect(0, 0, W, H); }
        drawRibbon(g, glow ? fx : null, { W, H, L, top, bottom, nowX, px: PX, now }, paletteFor(song.track.art), reduced, show ? 0.35 : 1, !show);
        if (glow) { g.save(); g.globalCompositeOperation = "screen"; g.drawImage(fxCv, 0, 0, W, H); g.restore(); }
      }
      if (!show) {
        if (!ribbon) {
          const grad = g.createLinearGradient(nowX, 0, W, 0);
          grad.addColorStop(0, "rgba(124,108,246,.07)"); grad.addColorStop(1, "rgba(124,108,246,0)");
          g.fillStyle = grad; g.fillRect(nowX, top, W - nowX, bottom - top);
          g.save(); g.beginPath(); g.rect(nowX + 1, top, W - nowX, bottom - top); g.clip();
          g.strokeStyle = "rgba(255,255,255,.025)"; g.lineWidth = 1;
          for (let x = nowX - H; x < W; x += 14) { g.beginPath(); g.moveTo(x, bottom); g.lineTo(x + H, top); g.stroke(); }
          g.restore();
        }
        g.fillStyle = "rgba(185,192,208,.55)"; g.font = `500 13px ${FONT}`; g.textAlign = "center";
        const why = learning ? "" : live.blind ? t("lane.blind") : song.cache ? t("lane.findingPlace") : "";
        if (why) g.fillText(why, nowX + (W - nowX) / 2, top + 26);
        g.textAlign = "left";
      }

      // target bars, filled green where you hit them
      // your pitch "now": the newest that has arrived (live, it reaches the window ~0.25 s after its moment),
      // if under 0.4 s old
      let youF = now - 3, youNowRaw = NaN;
      for (let f = now - 3; f >= now - 40; f--) {
        const v = smoothed(x => live.youAt(x), f, 3);
        if (!Number.isNaN(v)) { youF = f; youNowRaw = v; break; }
      }
      const errNow = Number.isNaN(youNowRaw) ? NaN : errAt(youF, youNowRaw);
      const hitting = !Number.isNaN(errNow) && Math.abs(errNow) <= 50;
      let target: number | null = null;
      const h = Math.max(6, rowH * 0.62);
      const allBars = show ? live.bars() : [];
      // the notes fade in once the place is settled; dimmed and softened while the music confirms it (live.lockDim)
      const dim = show ? live.lockDim(perf) : 0;
      g.globalAlpha = live.placeFade(perf) * (1 - 0.55 * dim);
      if (dim > 0) g.filter = `blur(${(1.6 * dim).toFixed(2)}px)`;
      const syllables = !!live.song?.barText?.length;
      for (let bi = 0; bi < allBars.length; bi++) {
        const [s, e, m] = allBars[bi];
        if (e < f0 || s > f1 || m < lo - 1 || m > hi + 1) continue;
        // a hairline gap between bars, so repeated notes read as separate syllables
        const x0 = Math.max(L, xOf(s) + (syllables ? 1.5 : 0)), x1 = Math.min(W, xOf(e) - (syllables ? 1.5 : 0));
        if (x1 <= x0) continue;
        const active = s <= now && now < e;
        if (active) target = m;
        const past = e <= now;
        const yy = y(m) - h / 2, r = Math.min(h / 2, 7);
        // judged whole: hit once held for half its length; at its end, missed if sung, untouched if not
        const len = Math.max(1, e - (s + 8));
        const j = s < now ? judge(s, e, now - 3) : null;
        const hit = !!j && j.ok / len >= 0.5;
        const missed = !!j && past && !hit && j.sung / len >= 0.25;
        // glow: while you hold it, brighter the more you're on it; for a moment after a hit, fading
        const nail = j ? j.cr / len : 0;
        // (a few soft rings, not a canvas shadow: a shadow blur is redrawn pixel by pixel every frame, the costliest
        // thing on the lane while a song plays)
        if (active) halo(g, (gg, d) => pill(gg, x0 - d, yy - d, x1 - x0 + 2 * d, h + 2 * d, r + d), hit || hitting ? COLORS.good : ACCENT[400], 10 + 14 * Math.min(1, nail * 1.6));
        else if (past && hit && now - e < 50) halo(g, (gg, d) => pill(gg, x0 - d, yy - d, x1 - x0 + 2 * d, h + 2 * d, r + d), COLORS.good, 22 * nail * (1 - (now - e) / 50));
        if (hit) g.fillStyle = past ? "rgba(52,210,123,.6)" : COLORS.good;
        else if (missed) g.fillStyle = "rgba(240,82,82,.55)";
        else if (past) g.fillStyle = "#232a3a";
        else {
          const gr = g.createLinearGradient(0, yy, 0, yy + h);
          gr.addColorStop(0, active ? "#c4b9ff" : "#8f80ff"); gr.addColorStop(1, active ? ACCENT[400] : ACCENT[600]);
          g.fillStyle = gr;
        }
        pill(g, x0, yy, x1 - x0, h, r); g.fill();
        // nailed (on it for 80% of the note): a small star at its end
        if (past && nail >= 0.8 && x1 - x0 > 4) star(g, x1 - 1, yy - 1, Math.max(3.5, Math.min(5.5, h * 0.45)), Math.min(1, (now - e) / 12));
        // the syllable, under its bar
        const text = live.barText(bi);
        if (text && text !== "~" && x1 - x0 > 6) {                // "~": the same syllable carried on to a new note
          g.font = `500 ${Math.max(10, Math.min(13, rowH * 0.9))}px ${FONT}`;
          g.fillStyle = past ? "rgba(139,148,171,.55)" : active ? "#ffffff" : "rgba(230,233,240,.85)";
          g.textBaseline = "top";
          g.fillText(text, x0 + 1, yy + h + 3, Math.max(x1 - x0 + 30, 40));
        }
      }
      g.globalAlpha = 1;
      g.filter = "none";
      if (dim > 0.01) {
        // what's going on, so a dimmed lane doesn't look broken
        g.globalAlpha = dim; g.fillStyle = "rgba(185,192,208,.75)"; g.font = `500 13px ${FONT}`; g.textAlign = "center";
        g.fillText(t("lane.liningUp"), nowX + (W - nowX) / 2, top + 26);
        g.textAlign = "left"; g.globalAlpha = 1;
      }

      const barsNow = show ? live.bars() : [];
      const barAt = (f: number) => { for (const [s, e, m] of barsNow) if (s <= f && f < e) return m; return NaN; };
      // the singer's line in the bars' octave: the learned line can sit an octave off where the note bars were
      // corrected (it showed your voice an octave under the bars whenever you were a little off the note)
      const nearBar = (f: number) => {
        let best = NaN, dist = 150;
        for (const [s, e, m] of barsNow) { const d = f < s ? s - f : f >= e ? f - e : 0; if (d < dist) { dist = d; best = m; } }
        return best;
      };
      const singerIn8va = (f: number) => {
        const v = smoothed(x => live.singerAt(x), f, 3), b = nearBar(f);
        return Number.isNaN(v) || Number.isNaN(b) ? v : b + fold(100 * (v - b)) / 100;
      };
      // the singer's actual pitch, faint. Drawn phrase by phrase (a phrase: tracked pitch with no gap over
      // 120 ms), each phrase found whole, beyond the view's edges, and moved by one whole number of octaves so it
      // sits with the note bars it overlaps (the learned line can sit an octave off where the bars were
      // corrected). Before, the octave was carried from whichever point was at the view's left edge, so a phrase
      // ahead could show an octave off and snap into place as its start scrolled by. A leap of over half an octave
      // starts a new phrase with its own octave. Sampled at fixed song frames so it doesn't shimmer as it scrolls.
      g.strokeStyle = "rgba(181,168,255,.34)"; g.lineWidth = 1.5; g.lineJoin = "round";
      const sing = (f: number) => smoothed(x => live.singerAt(x), f, 3);
      const fe = show ? f1 : f0 - 1;
      let fa = Math.ceil(f0 / 2) * 2;
      for (let gapB = 0, k = 0; k < 1500; k++) {          // back to the start of the phrase at the left edge
        if (Number.isNaN(sing(fa - 2))) { if (++gapB > 6) break; } else gapB = 0;
        fa -= 2;
      }
      const drawPhrase = (pts: { f: number; v: number }[]) => {
        if (pts.length < 4) return;                          // a few stray frames: not a phrase
        const ds: number[] = [];
        for (const p of pts) { const b = barAt(p.f); if (!Number.isNaN(b)) ds.push(b - p.v); }
        if (!ds.length) for (const p of pts) { const b = nearBar(p.f); if (!Number.isNaN(b)) ds.push(b - p.v); }
        ds.sort((a, b) => a - b);
        const sh = ds.length ? 12 * Math.round(ds[ds.length >> 1] / 12) : 0;
        g.beginPath();
        let pen = false, last = NaN, lastY = NaN;
        for (const p of pts) {
          // under a bar, the line takes that bar's octave (bars can leap an octave mid-phrase); between bars, the
          // phrase's. The pen lifts where the octave changes, so no stroke is drawn across the leap
          const b = barAt(p.f), k = Number.isNaN(b) ? sh : 12 * Math.round((b - p.v) / 12), v = p.v + k;
          if (p.f < f0 - 2 || p.f > fe + 2 || (!wrapped && (v < lo - 1 || v > hi + 1))) { pen = false; continue; }
          if (pen && (k !== last || across(lastY, y(v)))) pen = false;
          if (pen) g.lineTo(xOf(p.f), y(v)); else g.moveTo(xOf(p.f), y(v));
          pen = true; last = k; lastY = y(v);
        }
        g.stroke();
      };
      let phrase: { f: number; v: number }[] = [], pv = NaN, gap = 0;
      for (let f = fa, tail = 0; tail < 1500; f += 2) {
        let v = sing(f);
        if (f > fe) tail++;
        if (Number.isNaN(v)) {
          if (++gap > 6 && phrase.length) { drawPhrase(phrase); phrase = []; if (f > fe) break; }
          if (gap > 6 && f > fe) break;
          continue;
        }
        // a leap of more than half an octave starts a new phrase, which gets its own octave from its own bars
        // (folding it into the octave before let a few stray frames an octave off carry the rest of the phrase
        // an octave away from its bars)
        if (phrase.length && Math.abs(v - pv) > 6) { drawPhrase(phrase); phrase = []; }
        gap = 0; phrase.push({ f, v }); pv = v;
      }
      drawPhrase(phrase);

      // your pitch, coloured by closeness on a learned song (in the singer's octave there, as singing games do:
      // an octave down still counts); on a first play your real pitch, moved by whole octaves only if it would
      // leave the lane, and then kept for the whole phrase so it doesn't flick between octaves. Sampled at fixed
      // song frames (so it doesn't shimmer as it scrolls); gaps up to 100 ms inside a phrase are bridged.
      const start = Math.ceil(Math.max(f0, 0) / 2) * 2, stop = now - 3;
      const pts: { f: number; u: number }[] = [];
      for (let f = start; f <= stop; f += 2) pts.push({ f, u: smoothed(x => live.youAt(x), f, show ? 2 : 3) });
      for (let i = 0; i < pts.length; i++) {                      // bridge short gaps
        if (!Number.isNaN(pts[i].u)) continue;
        let j = i;
        while (j < pts.length && Number.isNaN(pts[j].u)) j++;
        if (i > 0 && j < pts.length && (pts[j].f - pts[i - 1].f) <= 12 && Math.abs(pts[j].u - pts[i - 1].u) < 2) {
          for (let k = i; k < j; k++) pts[k].u = pts[i - 1].u + (pts[j].u - pts[i - 1].u) * (k - i + 1) / (j - i + 1);
        }
        i = j;
      }
      let anchor = (lo + hi) / 2, prevF = -1e9;
      if (!show) firstPlayFold(now);
      // first play: the pitch tracker sometimes reports a note an octave off for a moment; within a phrase, a
      // sudden leap of about an octave is put back in the phrase's octave, unless you stay there (0.3 s)
      // "Fade the guide" (lib/guideFade.ts): on a song you know, your line fades where you're within a semitone
      const fade = show ? guide.level : 0;
      const drawn: { f: number; x: number; y: number; col: string; a: number }[] = [];
      for (const { f, u } of pts) {
        if (Number.isNaN(u)) continue;
        let pm: number, col: string, a = 1;
        if (show) {
          const e = errAt(f, u), bm = barAt(f);
          const sv = Number.isNaN(bm) ? singerIn8va(f) : bm;
          if (!Number.isNaN(sv)) anchor = sv;
          pm = !Number.isNaN(bm) && Math.abs(e) <= 50 ? bm : anchor + fold(100 * (u - anchor)) / 100;
          col = COLORS[grade(e)];
          a = pitchAlpha(fade, e);
        } else {
          // decided once per frame as it arrives (see firstPlayFold): redeciding from what's still on screen
          // made stretches jump an octave as the start of their phrase scrolled away
          let sh = fp.v.get(f);                              // this frame's octave shift (0, ±12)
          for (let d = 1; sh == null && d <= 8; d++) sh = fp.v.get(f - d) ?? fp.v.get(f + d);
          pm = u + (sh ?? 0);
          // a whistle (or anything else above or below the lane) is shown whole octaves in, for display only:
          // scoring compares notes whatever the octave. In the octave nearest the point before when that's in the
          // lane, so a line near the lane's edge doesn't flick between octaves
          if (pm > hi + 0.5 || pm < lo - 0.5) {
            const last = drawn[drawn.length - 1], prevPm = last ? (bottom - last.y) / rowH + lo - 0.5 : NaN;
            const near = pm + 12 * Math.round((prevPm - pm) / 12);
            if (last && f - last.f <= 4 && near >= lo - 0.5 && near <= hi + 0.5) pm = near;
            else if (pm > hi + 0.5) pm -= 12 * Math.ceil((pm - hi - 0.5) / 12);
            else pm += 12 * Math.ceil((lo - 0.5 - pm) / 12);
          }
          col = INK[100];
        }
        prevF = f;
        drawn.push({ f, x: xOf(f), y: y(pm), col, a });
      }
      // one path per run of the same colour, so joints don't double up; older parts fade. Over the ribbon, a
      // dark under-stroke first, so the line stays crisp on the light
      g.lineCap = "round"; g.lineJoin = "round";
      for (const under of ribbon && !show ? [true, false] : [false]) {
      g.lineWidth = under ? 6.5 : 3.5;
      let i = 0;
      while (i < drawn.length) {
        let j = i + 1;
        while (j < drawn.length && drawn[j].col === drawn[i].col && drawn[j].a === drawn[i].a && drawn[j].f - drawn[j - 1].f <= 4 && Math.abs(drawn[j].y - drawn[j - 1].y) < 3 * rowH
               && !across(drawn[j - 1].y, drawn[j].y)) j++;
        if (drawn[i].a > 0 && (j - i >= 2 || (i + 1 < drawn.length && drawn[i + 1].f - drawn[i].f <= 4))) {
          const last = Math.min(j, drawn.length - 1);
          g.strokeStyle = under ? "rgba(7,8,12,.5)" : drawn[i].col;
          g.globalAlpha = drawn[i].a * Math.max(0.35, 1 - (now - drawn[i].f) / 500);
          g.beginPath(); g.moveTo(drawn[i].x, drawn[i].y);
          for (let k = i + 1; k <= last; k++) {
            if (drawn[k].f - drawn[k - 1].f > 4 || Math.abs(drawn[k].y - drawn[k - 1].y) >= 3 * rowH || across(drawn[k - 1].y, drawn[k].y)) break;
            g.lineTo(drawn[k].x, drawn[k].y);
          }
          g.stroke();
        }
        i = j;
      }
      }
      g.globalAlpha = 1;

      // count-in: when the next note is more than 3 s away, say when the singing starts
      const nextBar = barsNow.filter(b => b[0] > now).reduce((a, b) => (a && a[0] < b[0] ? a : b), null as null | number[]);
      const inBar = barsNow.some(b => b[0] <= now && now < b[1]);
      const recent = barsNow.some(b => b[1] <= now && now - b[1] < 150);
      if (show && !inBar && !recent && nextBar && nextBar[0] - now > 300) {
        const secs = Math.ceil((nextBar![0] - now) / 100);
        g.fillStyle = "rgba(230,233,240,.9)"; g.font = `600 15px ${FONT}`; g.textAlign = "center"; g.textBaseline = "middle";
        g.fillText(t("lane.singingStarts", { secs }), nowX + (W - nowX) * 0.35, top + (bottom - top) * 0.5);
        g.textAlign = "left";
      }

      // how the line just sung went: a word that pops up over the notes behind the now line and fades
      const verdict = show ? lineVerdict(now, perf) : null;
      if (verdict) {
        const age = (perf - verdict.at) / 1000, k = Math.min(1, age / 0.16);
        const pop = 0.8 + 0.2 * (1 - (1 - k) ** 3) + 0.06 * Math.sin(Math.min(1, age / 0.3) * Math.PI);
        g.save();
        g.globalAlpha = Math.min(k, age > 1.1 ? Math.max(0, 1 - (age - 1.1) / 0.5) : 1);
        g.translate(L + (nowX - L) * 0.55, top + (bottom - top) * 0.2); g.scale(pop, pop);
        const hue = verdict.rank === "perfect" || verdict.rank === "excellent" ? NOTICE : verdict.rank === "great" || verdict.rank === "good" ? COLORS.good : "#c9cfdd";
        const word = t(`lane.verdict.${verdict.rank}`);
        g.font = `800 ${Math.round(Math.max(18, Math.min(30, H * 0.07)))}px ${FONT}`; g.textAlign = "center"; g.textBaseline = "middle";
        g.lineJoin = "round"; g.lineWidth = 5; g.strokeStyle = "rgba(0,0,0,.4)"; g.strokeText(word, 0, 1.5);
        g.fillStyle = hue; g.fillText(word, 0, 0);
        g.restore();
      }

      // now line
      const nl = g.createLinearGradient(0, top, 0, bottom);
      nl.addColorStop(0, "rgba(255,255,255,0)"); nl.addColorStop(0.15, "rgba(255,255,255,.55)");
      nl.addColorStop(0.85, "rgba(255,255,255,.55)"); nl.addColorStop(1, "rgba(255,255,255,0)");
      g.fillStyle = nl; g.fillRect(nowX - 1, top, 2, bottom - top);

      // you, right now: one small see-through ball at your pitch (on the bar while you're on the note, green
      // then), a little bigger the louder you sing, gliding to each new place on a sine ease over 0.15 s, and
      // fading out when you're not heard
      let youRow: number | null = null;
      // the ball appears once your voice is clearly there (9 of the last 10 frames) and then stays until you've
      // stopped for a moment: singing comes through in short pieces, noise in silence in rare blips (on your plays:
      // shown 81% of the time while singing, 1.7% while silent)
      let voiced = 0;
      for (let f = youF; f > youF - 10; f--) if (!Number.isNaN(live.youAt(f))) voiced++;
      if (!Number.isNaN(youNowRaw) && (ball.on ? voiced >= 1 : voiced >= 9)) ball.okAt = perf;
      ball.on = perf - ball.okAt < 250;
      if (!Number.isNaN(youNowRaw) && ball.on) {
        // steadier than one frame: the median of the newest few arrived frames
        const vals: number[] = [];
        for (let f = youF; f > youF - 12 && vals.length < 6; f--) { const u = live.youAt(f); if (!Number.isNaN(u)) vals.push(u); }
        vals.sort((p, q) => p - q);
        const v = vals.length ? vals[vals.length >> 1] : youNowRaw;
        const bmNow = barAt(youF);
        const sv = Number.isNaN(bmNow) ? singerIn8va(youF) : bmNow, anc = Number.isNaN(sv) ? anchor : sv;
        const pm = !show ? v : !Number.isNaN(bmNow) && hitting ? bmNow : anc + fold(100 * (v - anc)) / 100;
        youRow = Math.round(pm);
        const ty = y(pm);
        if (ball.a < 0.03 || live.frozenPerf != null) { ball.from = ball.to = ty; ball.t0 = perf; }
        else if (across(ball.to, ty)) { ball.from = ball.to = ty; ball.t0 = perf; }       // over the wrap: no glide across the lane
        else if (Math.abs(ty - ball.to) > 0.5) { ball.from = ball.y; ball.to = ty; ball.t0 = perf; }
        if (live.level != null) ball.level = live.level;
        ball.col = show ? COLORS[grade(errNow)] : INK[100];
        ball.fade = show ? pitchAlpha(fade, errNow) : 1;
        ball.seen = perf;
      }
      {
        const heard = perf - ball.seen < 250;
        const k = Math.max(0, Math.min(1, (perf - ball.t0) / 150));
        ball.y = ball.from + (ball.to - ball.from) * (0.5 - 0.5 * Math.cos(Math.PI * k));
        ball.r += ((3.5 + 4.5 * Math.max(0, Math.min(1, ball.level / 18))) - ball.r) * 0.15;
        ball.a += ((heard ? 0.6 * ball.fade : 0) - ball.a) * (heard ? 0.35 : 0.15);
        if (ball.a > 0.02) {
          g.globalAlpha = ball.a;
          halo(g, (gg, d) => { gg.beginPath(); gg.arc(nowX, ball.y, ball.r + d, 0, Math.PI * 2); }, ball.col, 10);
          g.globalAlpha = ball.a; g.fillStyle = ball.col;
          g.beginPath(); g.arc(nowX, ball.y, ball.r, 0, Math.PI * 2); g.fill();
          g.globalAlpha = 1;
        }
      }

      // keyboard strip: the target note lit (green while you're on it), your note marked
      for (let m = mLo; m <= mHi; m++) {
        const pc = ((m % 12) + 12) % 12, black = BLACK_KEYS.has(pc);
        const isTarget = m === target;
        // wrapped: rows are notes in any octave; yours is lit, with its real octave
        const isYou = wrapped && youRow != null && ((youRow % 12) + 12) % 12 === pc;
        g.fillStyle = isTarget ? (hitting ? COLORS.good : ACCENT[400]) : isYou ? "rgba(230,233,240,.85)" : black ? "#1a1f2b" : INK[600];
        g.fillRect(6, y(m) - rowH / 2 + 0.5, black ? KEYS_W - 26 : KEYS_W - 12, rowH - 1);
        if (wrapped) {
          g.textBaseline = "middle";
          g.font = `${isYou ? 650 : 500} ${Math.min(12, rowH * 0.5)}px ${FONT}`;
          g.fillStyle = isYou ? INK[900] : black ? "rgba(139,148,171,.5)" : INK[300];
          g.fillText(isYou ? noteName(youRow!) : NOTE_NAMES[pc], 11, y(m) + 0.5);
          continue;
        }
        if (m === youRow && !isTarget) { g.fillStyle = "rgba(255,255,255,.35)"; g.fillRect(KEYS_W - 10, y(m) - rowH / 2 + 1, 3, rowH - 2); }
        if (m % 12 === 0 || isTarget) {
          g.fillStyle = isTarget ? INK[900] : INK[300];
          g.font = `${isTarget ? 650 : 500} ${Math.min(11, rowH * 0.8)}px ${FONT}`;
          g.textBaseline = "middle";
          g.fillText(noteName(m), 11, y(m) + 0.5);
        }
      }
      g.restore();
    };
    const offSession = live.subscribe(wake), offPlaying = live.onPlaying(wake);
    document.addEventListener("visibilitychange", wake);
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf); clearTimeout(poll); ro.disconnect(); offSession(); offPlaying();
      document.removeEventListener("visibilitychange", wake);
    };
  }, []);
  return (
    <>
      <canvas ref={ref} className="block h-full w-full" />
      {/* grain over the ribbon's gradients (dithering), shown with it while it moves */}
      <div ref={grainRef} aria-hidden className="pointer-events-none absolute inset-0 hidden opacity-0 transition-opacity duration-700"
           style={{ backgroundImage: typeof document === "undefined" ? undefined : grain() }} />
    </>
  );
}

function star(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, grow: number) {
  const R = r * (0.6 + 0.4 * grow);
  g.save(); g.beginPath();
  for (let k = 0; k < 8; k++) {
    const a = -Math.PI / 2 + (k * Math.PI) / 4, d = k % 2 ? R * 0.38 : R;
    g.lineTo(cx + d * Math.cos(a), cy + d * Math.sin(a));
  }
  g.closePath(); g.fillStyle = NOTICE; g.globalAlpha = 0.5 + 0.5 * grow; g.fill();
  g.restore();
}

/** A soft glow `blur` px wide around a shape (`path(g, d)` traces it grown by d px), as a canvas shadow of that
 *  blur looks, from three faint rings: a few flat fills instead of a blur over every pixel each frame. Drawn at the
 *  canvas's current alpha. */
function halo(g: CanvasRenderingContext2D, path: (g: CanvasRenderingContext2D, d: number) => void, color: string, blur: number) {
  if (blur < 0.5) return;
  const a = g.globalAlpha;
  g.fillStyle = color;
  for (const [d, k] of [[0.55, 0.1], [0.32, 0.14], [0.14, 0.2]] as const) {
    g.globalAlpha = a * k; path(g, blur * d); g.fill();
  }
  g.globalAlpha = a;
}

function pill(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  r = Math.min(r, w / 2);
  g.beginPath(); g.moveTo(x + r, y); g.arcTo(x + w, y, x + w, y + h, r); g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r); g.arcTo(x, y, x + w, y, r); g.closePath();
}
