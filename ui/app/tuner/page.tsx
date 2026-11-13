"use client";
import Link from "next/link";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import { getVoiceRange, inApp } from "@/lib/session";
import { useT } from "@/lib/i18n";
import { FRAME_MS, Hold, Sequence, follow, isBlackKey, noteName, patternFrom, patternNotes, pickNote, reading, steady, hzOf, type Pattern, type View } from "@/lib/tuner";

// The tuner: sing any note, with no song playing, and see which note it is (src-tauri/src/tuner.rs listens to the mic
// alone while this page is open, and gives a pitch every 10 ms; OnKey follows no music meanwhile). The picture is
// your pitch over the last seconds as a line across note lanes, as a take's is; above it the note's name and how far
// off it you are. To practise: a note to hear and hold ("Give me a note", or click a lane), or a short pattern of
// notes to sing one after the other.

const SECONDS = 6;                       // of singing the picture shows
const KEPT = (SECONDS + 2) * (1000 / FRAME_MS);
const SPAN = 15;                         // semitones the picture shows
const HOLD = { min: 1, max: 5, first: 3, key: "onkey.tunerHold" };   // seconds a note is held for: yours to set, kept on this computer
const GUTTER = 46;                       // px for the note names on the left
const NOW_AT = 0.86;                     // where "now" is across the picture
const LINGER_MS = 1500;                  // the last note's name stays, dimmed, this long after you stop
const COLOR = { good: "#34d27b", close: "#f5b83d", soft: "#b5a8ff" };
const PATTERN_KEYS: Pattern[] = ["five", "arpeggio", "octave"];

type Read = { on: boolean; level: number; notes: (number | null)[] };
type State = "starting" | "on" | "mic" | "model" | "browser";
type Practice = { kind: "note"; target: number; hold: Hold; cheered: boolean } | { kind: "seq"; pattern: Pattern; seq: Sequence; cheered: boolean };
/** What the page shows of the practice under way (a copy made as the pitches come in). */
type Shown = { kind: "note"; target: number; progress: number; done: boolean; average: number }
           | { kind: "seq"; pattern: Pattern; notes: number[]; at: number; progress: number; results: (number | null)[]; done: boolean };

export default function TunerPage() {
  const t = useT();
  const [state, setState] = useState<State>("starting");
  const [now, setNow] = useState<number | null>(null);           // the pitch being sung (steadied), or null
  const [lingering, setLingering] = useState<number | null>(null); // the last note sung, for a moment after
  const [shown, setShown] = useState<Shown | null>(null);
  const [strict, setStrict] = useState(false);
  const [holdS, setHoldS] = useState(HOLD.first);
  const holdRef = useRef(holdS);
  holdRef.current = holdS;
  useEffect(() => { try { const v = Number(localStorage.getItem(HOLD.key)); if (Number.isInteger(v) && v >= HOLD.min && v <= HOLD.max) setHoldS(v); } catch {} }, []);
  const chooseHold = (v: number) => { setHoldS(v); try { localStorage.setItem(HOLD.key, String(v)); } catch {} };
  const [listenFirst, setListenFirst] = useState(false);          // a note is playing: the mic isn't heard
  const [played, setPlayed] = useState(false);                    // a note has been played: the speakers hint shows
  const canvas = useRef<HTMLCanvasElement>(null);
  const notes = useRef<(number | null)[]>([]);
  const view = useRef<View>({ lo: 52, hi: 52 + SPAN });           // E3 up, until your range or your voice says otherwise
  const range = useRef<{ lo: number; hi: number }>({ lo: 55, hi: 67 });
  const practice = useRef<Practice | null>(null);
  const deafUntil = useRef(0);                                    // (performance.now) OnKey's own notes aren't your voice
  const flash = useRef<{ note: number; until: number } | null>(null);
  const lastSung = useRef<{ midi: number; at: number } | null>(null);
  const tolerance = strict ? 10 : 25;
  const tolRef = useRef(tolerance);
  tolRef.current = tolerance;
  const audio = useRef<AudioContext | null>(null);
  const sounding = useRef<{ bus: GainNode; oscs: OscillatorNode[] } | null>(null);   // the notes playing now

  // your comfortable range, when your takes have shown it: where the picture opens and where notes to try come from
  useEffect(() => {
    void getVoiceRange().then(v => {
      const r = v?.range;
      if (!r) return;
      range.current = { lo: r.mid_lo, hi: r.mid_hi };
      const mid = (r.mid_lo + r.mid_hi) / 2;
      view.current = { lo: mid - SPAN / 2, hi: mid + SPAN / 2 };
    }).catch(() => {});
  }, []);

  /** Notes out loud, one after the other (a soft tone each), from a click. The mic isn't heard until they're over:
   *  from speakers it would hear them as your voice. */
  const play = (midis: number[], each = 1.5, gainAt = 0.22) => {
    try {
      const ctx = (audio.current ??= new AudioContext());
      void ctx.resume();
      // (whatever was still playing stops: a quick fade, so it doesn't click)
      const old = sounding.current;
      if (old) {
        old.bus.gain.cancelScheduledValues(ctx.currentTime);
        old.bus.gain.setValueAtTime(old.bus.gain.value, ctx.currentTime);
        old.bus.gain.linearRampToValueAtTime(0, ctx.currentTime + 0.03);
        old.oscs.forEach(o => { try { o.stop(ctx.currentTime + 0.04); } catch {} });
      }
      const bus = ctx.createGain(), oscs: OscillatorNode[] = [];
      bus.connect(ctx.destination);
      sounding.current = { bus, oscs };
      const t0 = ctx.currentTime + 0.05;
      midis.forEach((m, i) => {
        const o = ctx.createOscillator(), gain = ctx.createGain(), a = t0 + i * each, ring = Math.max(0.08, each - 0.12);
        o.type = "triangle"; o.frequency.value = hzOf(m);
        gain.gain.setValueAtTime(0, a); gain.gain.linearRampToValueAtTime(gainAt, a + 0.025);
        gain.gain.setValueAtTime(gainAt, a + ring * 0.8); gain.gain.linearRampToValueAtTime(0, a + ring);
        o.connect(gain).connect(bus); o.start(a); o.stop(a + ring + 0.02);
        oscs.push(o);
      });
      const ms = midis.length * each * 1000 + 250;
      deafUntil.current = performance.now() + ms;
      return ms;
    } catch { return 0; }
  };
  const listen = (midis: number[], each?: number) => {
    const ms = play(midis, each);
    setPlayed(true);
    setListenFirst(true);
    window.setTimeout(() => { if (performance.now() >= deafUntil.current - 20) setListenFirst(false); }, ms + 30);
  };
  /** Two quick high notes: you did it. */
  const cheer = (note: number) => { flash.current = { note, until: performance.now() + 900 }; play([note + 12, note + 19], 0.13, 0.12); };

  const show = () => {
    const p = practice.current;
    setShown(p == null ? null
      : p.kind === "note" ? { kind: "note", target: p.target, progress: p.hold.progress, done: p.hold.done, average: p.hold.average }
      : { kind: "seq", pattern: p.pattern, notes: p.seq.notes, at: p.seq.at, progress: p.seq.progress, results: [...p.seq.results], done: p.seq.done });
  };
  /** The picture comes to these notes, if any is out of sight. */
  const bring = (ms: number[]) => {
    const lo = Math.min(...ms), hi = Math.max(...ms), v = view.current;
    if (lo < v.lo + 1.5 || hi > v.hi - 1.5) { const mid = (lo + hi) / 2; view.current = { lo: mid - SPAN / 2, hi: mid + SPAN / 2 }; }
  };
  const aim = (midi: number | null) => {
    practice.current = midi == null ? null : { kind: "note", target: midi, hold: new Hold(midi, tolRef.current, holdRef.current * 1000), cheered: false };
    if (midi != null) { bring([midi]); listen([midi]); }
    show();
  };
  /** A pattern, from `from` when given; else from the note being aimed for or the pattern's own start (so another
   *  pattern keeps your place); else placed in your range. */
  const startPattern = (pattern: Pattern, from?: number) => {
    const p = practice.current;
    const start = from ?? (p?.kind === "note" ? p.target : p?.kind === "seq" ? p.seq.notes[0] : null);
    const ns = start != null ? patternFrom(pattern, start) : patternNotes(pattern, range.current.lo, range.current.hi);
    practice.current = { kind: "seq", pattern, seq: new Sequence(ns, tolRef.current), cheered: false };
    bring(ns);
    listen(ns, 0.5);
    show();
  };
  // (how close counts, or how long to hold, changed: what's being practised starts again with it)
  useEffect(() => {
    const p = practice.current;
    if (p?.kind === "note") practice.current = { ...p, hold: new Hold(p.target, tolerance, holdS * 1000), cheered: false };
    else if (p?.kind === "seq") practice.current = { ...p, seq: new Sequence(p.seq.notes, tolerance), cheered: false };
    show();
  }, [tolerance, holdS]);   // eslint-disable-line react-hooks/exhaustive-deps

  // listen while the page is open
  useEffect(() => {
    if (!inApp()) { setState("browser"); return; }
    let alive = true, busy = false;
    void invoke<string>("tuner_start").then(why => { if (alive) setState(why === "" ? "on" : why === "model" ? "model" : "mic"); }).catch(() => { if (alive) setState("mic"); });
    const tick = window.setInterval(() => {
      if (busy) return;
      busy = true;
      void invoke<Read>("tuner_read").then(r => {
        if (!alive || !r.on) return;
        const ns = notes.current, p = practice.current, deaf = performance.now() < deafUntil.current;
        for (const heard of r.notes) {
          const m = deaf ? null : heard;
          ns.push(m);
          if (p?.kind === "note") p.hold.push(m);
          else if (p?.kind === "seq") { const was = p.seq.target; if (p.seq.push(m) && was != null) flash.current = { note: was, until: performance.now() + 500 }; }
        }
        if (ns.length > KEPT) ns.splice(0, ns.length - KEPT);
        if (p && !p.cheered && (p.kind === "note" ? p.hold.done : p.seq.done)) {
          p.cheered = true;
          cheer(p.kind === "note" ? p.target : p.seq.notes[p.seq.notes.length - 1]);
        }
        const s = steady(ns);
        view.current = follow(view.current, s);
        if (s != null) lastSung.current = { midi: s, at: performance.now() };
        setNow(s);
        const l = lastSung.current;
        setLingering(s == null && l && performance.now() - l.at < LINGER_MS ? l.midi : null);
        show();
      }).catch(() => {}).finally(() => { busy = false; });
    }, 33);
    return () => { alive = false; window.clearInterval(tick); void invoke("tuner_stop").catch(() => {}); void audio.current?.close().catch(() => {}); };
  }, []);   // eslint-disable-line react-hooks/exhaustive-deps

  // the picture, redrawn every frame
  useEffect(() => {
    let raf = 0;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      const c = canvas.current;
      if (!c) return;
      const dpr = window.devicePixelRatio || 1, w = c.clientWidth, h = c.clientHeight;
      if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) { c.width = Math.round(w * dpr); c.height = Math.round(h * dpr); }
      const g = c.getContext("2d");
      if (!g) return;
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.clearRect(0, 0, w, h);
      const { lo, hi } = view.current, lane = h / (hi - lo);
      const y = (m: number) => h - (m - lo + 0.5) * lane;      // a note's lane is centred on its line
      const p = practice.current, tol = tolRef.current;
      const tgt = p == null ? null : p.kind === "note" ? p.target : p.seq.target;
      const others = p?.kind === "seq" ? p.seq.notes : [];
      const fl = flash.current && performance.now() < flash.current.until ? flash.current.note : null;
      // the lanes; down the left a strip of piano keys with the notes' names
      g.font = "11px ui-sans-serif, system-ui, sans-serif";
      g.textBaseline = "middle";
      for (let m = Math.floor(lo) - 1; m <= Math.ceil(hi) + 1; m++) {
        const top = y(m) - lane / 2, black = isBlackKey(m);
        if (black) { g.fillStyle = "rgba(255,255,255,0.028)"; g.fillRect(GUTTER, top, w - GUTTER, lane); }
        if (m !== tgt && others.includes(m)) { g.fillStyle = "rgba(124,108,246,0.09)"; g.fillRect(GUTTER, top, w - GUTTER, lane); }
        if (m === fl) { g.fillStyle = "rgba(52,210,123,0.30)"; g.fillRect(GUTTER, top, w - GUTTER, lane); }
        else if (m === tgt) {
          g.fillStyle = "rgba(124,108,246,0.16)"; g.fillRect(GUTTER, top, w - GUTTER, lane);
          g.fillStyle = "rgba(124,108,246,0.30)"; g.fillRect(GUTTER, y(m) - (tol / 100) * lane, w - GUTTER, 2 * (tol / 100) * lane);
        }
        g.fillStyle = "rgba(255,255,255,0.05)"; g.fillRect(GUTTER, top, w - GUTTER, 1);
        g.fillStyle = black ? "rgba(255,255,255,0.03)" : "rgba(255,255,255,0.10)"; g.fillRect(0, top + 1, GUTTER - 4, lane - 1);
        const c4 = ((m % 12) + 12) % 12 === 0;
        g.fillStyle = m === tgt ? COLOR.soft : c4 ? "rgba(240,242,248,0.95)" : black ? "rgba(139,148,171,0.7)" : "rgba(200,205,220,0.85)";
        g.fillText(noteName(m), 7, y(m));
      }
      // your pitch: the newest at NOW_AT, older to the left; green when on a note (the one aimed for, when there is one)
      const ns = notes.current, xNow = GUTTER + (w - GUTTER) * NOW_AT, perFrame = (xNow - GUTTER) / (SECONDS * (1000 / FRAME_MS));
      g.lineWidth = 3; g.lineCap = "round"; g.lineJoin = "round";
      const on = (m: number) => Math.abs((m - (tgt ?? Math.round(m))) * 100) <= tol;
      for (let i = ns.length - 1; i > 0; i--) {
        const a = ns[i - 1], b = ns[i];
        if (a == null || b == null) continue;
        const x1 = xNow - (ns.length - 1 - i) * perFrame, x0 = x1 - perFrame;
        if (x1 < GUTTER) break;
        g.strokeStyle = on(b) ? COLOR.good : COLOR.close;
        g.beginPath(); g.moveTo(Math.max(GUTTER, x0), y(a)); g.lineTo(x1, y(b)); g.stroke();
      }
      const last = ns[ns.length - 1];
      g.fillStyle = "rgba(255,255,255,0.10)"; g.fillRect(xNow, 0, 1, h);
      if (last != null) { g.fillStyle = on(last) ? COLOR.good : COLOR.close; g.beginPath(); g.arc(xNow, y(last), 6, 0, 2 * Math.PI); g.fill(); }
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  const clickLane = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect(), { lo, hi } = view.current;
    const note = Math.round(lo + (1 - (e.clientY - r.top) / r.height) * (hi - lo) - 0.5);
    // (with a pattern up, the note clicked is where it starts from now)
    const p = practice.current;
    if (p?.kind === "seq") startPattern(p.pattern, note); else aim(note);
  };

  const target = shown == null ? null : shown.kind === "note" ? shown.target : shown.done ? null : shown.notes[shown.at];
  const r = now == null ? null : reading(now);
  const off = r == null ? null : target != null ? Math.round((now! - target) * 100) : r.cents;
  const onIt = off != null && Math.abs(off) <= tolerance;
  const pill = (on: boolean) => `rounded-full px-3 py-1.5 text-xs font-medium transition ${on ? "bg-white/10 text-white" : "text-ink-300 hover:text-white"}`;
  const button = "rounded-xl bg-white/6 px-3.5 py-2 text-sm font-medium text-ink-100 transition hover:bg-white/10";
  const link = "text-accent-300 underline-offset-4 hover:underline";
  const bar = (progress: number, done: boolean) => (
    <span className="relative h-2 w-40 overflow-hidden rounded-full bg-white/8">
      <span className={`absolute inset-y-0 left-0 rounded-full ${done ? "bg-good" : "bg-accent-500"}`} style={{ width: `${Math.round(progress * 100)}%` }} />
    </span>
  );
  const offWords = (cents: number) => Math.abs(cents) <= 5 ? t("tuner.cents.on") : cents > 0 ? t("tuner.cents.sharp", { cents }) : t("tuner.cents.flat", { cents: -cents });

  return (
    <div className="min-h-screen">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-5xl px-6 pb-16">
        <h1 className="mt-4 text-2xl font-semibold tracking-tight text-white">{t("tuner.title")}</h1>
        <p className="mt-1 text-sm text-ink-400">{t("tuner.intro")}</p>

        {state === "browser" || state === "mic" || state === "model" ? (
          <div className="mt-8 rounded-2xl border border-white/8 bg-white/3 px-6 py-8 text-sm text-ink-200">
            <p>{state === "browser" ? t("tuner.onlyInApp") : state === "model" ? t("tuner.broken") : t("tuner.noMic")}</p>
            {state === "mic" && <Link href="/settings" className={`mt-3 inline-block ${link}`}>{t("tuner.openSettings")}</Link>}
          </div>
        ) : (
          <>
            <div className="mt-6 flex flex-wrap items-end gap-x-10 gap-y-4">
              <div className="w-44">
                <div className={`text-7xl font-bold leading-none tracking-tight tabular-nums ${r == null ? "text-ink-500" : onIt ? "text-good" : "text-white"}`}>
                  {r?.name ?? (lingering != null ? noteName(lingering) : "–")}
                </div>
                <div className="mt-2 h-4 text-xs text-ink-400">{listenFirst ? t("tuner.listen") : r == null ? t("tuner.waiting") : t("tuner.hz", { hz: Math.round(r.hz) })}</div>
              </div>
              <div className="min-w-64 flex-1">
                {/* how far off: 50 cents either side, the middle band green */}
                <div className="relative h-3 rounded-full bg-white/6">
                  <div className="absolute inset-y-0 rounded-full bg-good/25" style={{ left: `${50 - tolerance}%`, width: `${2 * tolerance}%` }} />
                  <div className="absolute inset-y-[-3px] left-1/2 w-px bg-white/30" />
                  {off != null && <div className={`absolute top-1/2 size-4 -translate-x-1/2 -translate-y-1/2 rounded-full shadow ${onIt ? "bg-good" : "bg-close"}`} style={{ left: `${Math.max(0, Math.min(100, 50 + off))}%` }} />}
                </div>
                <div className="mt-2 flex h-4 justify-between text-xs text-ink-400">
                  <span>{t("tuner.flat")}</span>
                  <span className={off == null ? "" : onIt ? "text-good" : "text-close"}>{off == null ? "" : offWords(off)}</span>
                  <span>{t("tuner.sharp")}</span>
                </div>
              </div>
            </div>

            <canvas ref={canvas} onClick={clickLane} title={t("tuner.clickLane")}
                    className="mt-6 h-[380px] w-full cursor-pointer rounded-2xl border border-white/8 bg-ink-900/60" />

            <div className="mt-4 flex flex-wrap items-center gap-2">
              <span className="mr-1 text-xs text-ink-400">{t("tuner.practise")}</span>
              <button className={button} onClick={() => aim(pickNote(range.current.lo, range.current.hi, shown?.kind === "note" ? shown.target : null))}>{t("tuner.giveNote")}</button>
              {PATTERN_KEYS.map(k => <button key={k} className={button} onClick={() => startPattern(k)}>{t(`tuner.pattern.${k}`)}</button>)}
              <label className="ml-auto flex items-center gap-2 text-xs text-ink-300" title={t("tuner.hold.title")}>
                <span className="w-24 text-right tabular-nums">{t("tuner.hold.setting", { seconds: holdS })}</span>
                <input type="range" min={HOLD.min} max={HOLD.max} step={1} value={holdS} onChange={e => chooseHold(Number(e.target.value))}
                       aria-label={t("tuner.hold.title")} className="w-24 accent-accent-500" />
              </label>
              <span className="flex items-center gap-1 rounded-full bg-white/4 p-1" title={t("tuner.close.title")}>
                <button className={pill(!strict)} onClick={() => setStrict(false)}>{t("tuner.close.relaxed")}</button>
                <button className={pill(strict)} onClick={() => setStrict(true)}>{t("tuner.close.strict")}</button>
              </span>
            </div>

            <div className="mt-3 min-h-16 rounded-2xl border border-white/8 bg-white/3 px-5 py-4 text-sm text-ink-200">
              {shown == null ? t("tuner.target.none") : shown.kind === "note" ? (
                <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
                  <span className="font-medium text-white">{t("tuner.target.aim", { note: noteName(shown.target) })}</span>
                  {bar(shown.progress, shown.done)}
                  <span className={shown.done ? "text-good" : "text-ink-400"}>
                    {listenFirst ? t("tuner.listen") : !shown.done ? t("tuner.hold.label", { seconds: holdS })
                      : Math.abs(shown.average) <= 5 ? t("tuner.hold.on") : shown.average > 0 ? t("tuner.hold.sharp", { cents: shown.average }) : t("tuner.hold.flat", { cents: -shown.average })}
                  </span>
                  <span className="ml-auto flex items-center gap-4">
                    <button className={link} onClick={() => listen([shown.target])}>{t("tuner.playAgain")}</button>
                    {shown.done && <button className={link} onClick={() => aim(pickNote(range.current.lo, range.current.hi, shown.target))}>{t("tuner.another")}</button>}
                    <button className="text-ink-400 hover:text-white" onClick={() => aim(null)}>{t("tuner.clear")}</button>
                  </span>
                </div>
              ) : (
                <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
                  <span className="font-medium text-white">{t(`tuner.pattern.${shown.pattern}`)}</span>
                  {/* each note of the pattern: done (green, or amber with how far off), skipped, the one to sing now, to come */}
                  <span className="flex flex-wrap items-center gap-1.5">
                    {shown.notes.map((m, i) => {
                      const res = shown.results[i], doneNote = i < shown.results.length, nowNote = !shown.done && i === shown.at;
                      const cls = doneNote ? (res == null ? "bg-white/5 text-ink-500 line-through" : Math.abs(res) <= 10 ? "bg-good/20 text-good" : "bg-close/20 text-close")
                        : nowNote ? "bg-accent-500/30 text-white ring-1 ring-accent-400" : "bg-white/5 text-ink-300";
                      return <span key={i} title={doneNote && res != null ? offWords(res) : undefined} className={`rounded-md px-2 py-1 text-xs font-medium tabular-nums ${cls}`}>{noteName(m)}</span>;
                    })}
                  </span>
                  <span className={shown.done ? "text-good" : "text-ink-400"}>
                    {listenFirst ? t("tuner.listen") : shown.done ? t("tuner.seq.done", { hit: shown.results.filter(x => x != null).length, count: shown.notes.length })
                      : t("tuner.seq.sing", { n: shown.at + 1, count: shown.notes.length })}
                  </span>
                  {!shown.done && !listenFirst && bar(shown.progress, false)}
                  <span className="ml-auto flex items-center gap-4">
                    {!shown.done && <button className={link} onClick={() => { const p = practice.current; if (p?.kind === "seq") { p.seq.skip(); show(); } }}>{t("tuner.seq.skip")}</button>}
                    <button className={link} onClick={() => startPattern(shown.pattern, shown.notes[0])}>{shown.done ? t("tuner.seq.again") : t("tuner.playAgain")}</button>
                    <button className={link} title={t("tuner.seq.lowerTitle")} onClick={() => startPattern(shown.pattern, shown.notes[0] - 1)}>{t("tuner.seq.lower")}</button>
                    <button className={link} title={t("tuner.seq.higherTitle")} onClick={() => startPattern(shown.pattern, shown.notes[0] + 1)}>{t("tuner.seq.higher")}</button>
                    <button className="text-ink-400 hover:text-white" onClick={() => aim(null)}>{t("tuner.clear")}</button>
                  </span>
                </div>
              )}
            </div>

            {played && <p className="mt-3 text-xs text-ink-400">{t("tuner.headphones")}</p>}
            <p className="mt-3 text-xs text-ink-400">{t("tuner.tip")}</p>
            <p className="mt-1 text-xs text-ink-500">{t("tuner.private")}</p>
          </>
        )}
      </main>
    </div>
  );
}
