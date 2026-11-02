"use client";
// The pieces around the lane: lyrics, score and hint, transport controls, the takes tray and status chips.
import Link from "next/link";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { TIMING_CHECK_VERIFIED } from "@/components/TimingCheck";
import { dismissTimingHint, finishTake, setListeningPaused, learnOf, learnPct, live, playTrack, setBlind, setEchoCancel, skipToSinging, control, type Take, usePlaying, useSession } from "@/lib/session";
import { typing } from "@/lib/keys";
import { clock, fold } from "@/lib/music";
import { FADE_LEVELS, guide, onGuideLevel } from "@/lib/guideFade";
import { setDancer, useDancer } from "@/components/Dancer";
import Cover from "@/components/Cover";
import NoLyrics from "@/components/NoLyrics";
import { useT } from "@/lib/i18n";

/** Re-render a component every `ms` (for things derived from the live clock), only while the clock runs (see
 *  live.playing): paused, finished, nothing playing or the window hidden, once more and then no timer at all. */
export function useTick(ms: number) {
  const [, set] = useState(0);
  const playing = usePlaying();
  useEffect(() => {
    set(x => x + 1);
    if (!playing) return;
    const id = setInterval(() => set(x => x + 1), ms);
    return () => clearInterval(id);
  }, [ms, playing]);
}
const perfNow = () => live.frozenPerf ?? performance.now();

// ------------------------------------------------------------------ lyrics
/** Mini bar font size: full size for ordinary lines, smaller for long ones (they wrap rather than being cut). */
function compactSize(text: string) {
  const n = text.length;
  return n <= 34 ? "16px" : n <= 44 ? "14.5px" : "13.5px";
}

/** How far each word of the current line has been sung (0..1), from the syllable bars inside the line: each
 *  bar's syllable is matched to the line's letters in order, and fills as the bar plays. */
function syllableFills(words: string[], song: NonNullable<typeof live.song>, start: number, end: number, t: number) {
  // (every letter counts, accented ones too: a Spanish "zón" is three letters, as the bars' letters are)
  const letters = (w: string) => w.toLowerCase().replace(/[^\p{L}\p{N}']/gu, "");
  const wl = words.map(w => letters(w).length);
  const fills = words.map(() => 0);
  const got = words.map(() => 0);
  const hasBar = words.map(() => false);
  const bars = song.cacheBars.map((b, i) => [b[0], b[1], i] as const)
    .filter(([s]) => s >= (start - 0.5) * 100 && s < end * 100).sort((x, y) => x[0] - y[0]);
  let w = 0, used = 0;
  // a word starts filling 0.1 s before its first syllable is sung (a little early, never late)
  const now = t * 100 + 10;
  for (const [s, e, i] of bars) {
    if (!letters(song.barText[i] ?? "")) continue;            // a run's later notes ("~") belong to the syllable before
    while (w < words.length && (wl[w] === 0 || used >= wl[w])) { w++; used = 0; }
    if (w >= words.length) break;
    const n = Math.max(1, letters(song.barText[i] ?? "").length);
    const part = Math.min(n, wl[w] - used) / wl[w];
    got[w] += part * Math.max(0, Math.min(1, (now - s) / Math.max(1, e - s)));
    hasBar[w] = true;
    used += n;
  }
  // a word with no bar of its own (no clear pitch, or the bars' words couldn't be given back) fills as the spread
  // says (see spreadFills), or once the next word has started if that's sooner
  const spread = spreadFills(words, start, end, t, song.pace);
  for (let k = 0; k < words.length; k++) fills[k] = Math.min(1, hasBar[k] ? got[k] : spread[k]);
  for (let k = words.length - 2; k >= 0; k--) if (fills[k] === 0 && fills[k + 1] > 0) fills[k] = 1;
  return fills;
}

/** Syllables in a word, roughly (groups of vowels, a silent final e): to space a line's words in time. */
function syllables(w: string) {
  const s = w.toLowerCase().replace(/[^a-z']/g, "");
  let g = s.split(/[^aeiouy]+/).filter(Boolean).length;
  if (s.endsWith("e") && s.length > 3 && g > 1) g--;
  return Math.max(1, g);
}

/** The singer's pace when nothing's known (s per syllable, word start to word start), and the slowest used:
 *  measured on first plays (Shallow, Castle on the Hill: 535 words, wav2vec2's word starts), a pace no slower
 *  than 0.25 s lights 96% of words no more than 0.1 s after they're sung (median 0.55 s before). */
const PACE_DEFAULT = 0.2, PACE_MAX = 0.25;

/** How far each word of a line is sung (0..1) where there are no syllable bars to say (a first play, a song
 *  learned without them): word k starts at the line's start plus the syllables before it at the singer's pace
 *  (`pace`, from the play's line-ups of the words heard, else PACE_DEFAULT; never slower than PACE_MAX),
 *  squeezed so the last starts 0.6 s before the next line, and lights 0.1 s early, over its predicted length. */
function spreadFills(words: string[], start: number, end: number, t: number, pace?: number | null) {
  const p = Math.min(pace ?? PACE_DEFAULT, PACE_MAX);
  const starts: number[] = [];
  let acc = 0;
  for (const w of words) { starts.push(start + acc); acc += syllables(w) * p; }
  const last = end - 0.6;
  if (starts.length > 1 && starts[starts.length - 1] > last) {
    const k = Math.max(0, last - start) / (starts[starts.length - 1] - start);
    for (let i = 0; i < starts.length; i++) starts[i] = start + (starts[i] - start) * k;
  }
  return words.map((w, k) => {
    const a = starts[k] - 0.1, b = Math.max(a + 0.15, (starts[k + 1] ?? starts[k] + syllables(w) * p) - 0.1);
    return Math.max(0, Math.min(1, (t - a) / (b - a)));
  });
}

/** When line i's singing ends (song seconds): its last syllable bar on a learned song; otherwise an estimate
 *  from its length in words (about 0.45 s a word plus a second), never past the next line. */
function lineEnd(song: NonNullable<typeof live.song>, i: number) {
  const lyr = song.lyrics, t0 = lyr[i][0], nx = lyr[i + 1]?.[0] ?? t0 + 6;
  if (song.cacheBars?.length) {
    let end = -Infinity;
    for (const [s, e] of song.cacheBars) if (s >= (t0 - 0.2) * 100 && s < nx * 100 && e > end) end = e;
    if (Number.isFinite(end)) return end / 100;
  }
  // words, or the characters of a line written without spaces (Chinese, Japanese, Korean: about a syllable each)
  const text = lyr[i][1].trim();
  const cjk = (text.match(/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/gu) || []).length;
  const n = text ? Math.max(text.split(/\s+/).length, cjk) : 0;
  return Math.min(nx, t0 + (n ? 1 + 0.45 * n : 0));
}

export function Lyrics({ compact = false }: { compact?: boolean }) {
  const t = useT();
  useTick(80);
  const song = live.song;
  if (!song) return null;
  const time = live.songTime(perfNow());
  const lyr = song.lyrics;
  // the line (and the next-line preview) changes LINE_LEAD s before it's sung; its words still fill on the singing
  const i = live.lyricIndex(time);
  const cur = i >= 0 ? lyr[i] : null, next = lyr[i + 1];
  if (!lyr.length) {
    // LRCLIB answered and has none: say where lyrics come from, with Look again (not in the mini bar, and not while
    // LRCLIB is down: the Sing page's notice says so)
    if (!compact && live.lyricsOn && song.lyricsNone && !live.status.lyrics_down) return <NoLyrics id={song.track.id} className="text-center" />;
    return <p className={`text-center text-sm text-ink-400 ${compact ? "truncate" : ""}`}>{live.lyricsOn ? t("live.lyrics.none") : t("live.lyrics.off")}</p>;
  }
  // the place in the song isn't settled yet: a calm wait rather than words that then jump (see live.placeReady)
  const perf = perfNow();
  if (!live.placeReady(perf)) return (
    <div className={compact ? "min-w-0 text-left" : "mx-auto max-w-4xl px-6 text-center"}>
      <div className={compact ? "" : "flex min-h-[2lh] items-end justify-center text-[clamp(1.4rem,2.6vw,2.2rem)] leading-tight"}>
        <p className={`font-medium tracking-tight text-ink-500 ${compact ? "text-sm" : "text-[0.7em]"}`}>{t("live.lyrics.findingPlace")}</p>
      </div>
      {!compact && <p className="mt-1 min-h-[1.5rem] text-base text-ink-400">{" "}</p>}
    </div>
  );
  // (a little dimmer while the music confirms the place: live.lockDim)
  const fade = live.placeFade(perf) * (1 - 0.3 * live.lockDim(perf));
  const end = next ? next[0] : (cur ? cur[0] + 4 : 0);
  const words = (cur?.[1] || (cur ? "♪" : "")).split(" ");
  // with syllable bars (a learned song), each word fills exactly as its syllables' bars play; without them (a first
  // play, a song learned without them) the words' times are only a guess from the line's, which visibly didn't match
  // the singing, so the current line is shown whole, bright, with no word fill
  const fills = !cur ? null : song.mode === "sing" && song.barText?.length ? syllableFills(words, song, cur[0], end, time) : null;
  // an instrumental break (3.5 s or more from the end of this line's singing to the next line, or before the
  // first line): the next line isn't shown early; a countdown says when it comes, and it appears 3 s before
  const wait = next ? next[0] - time : 0;
  const brk = !!next && wait > 0 && (cur ? next[0] - lineEnd(song, i) >= 3.5 && time >= lineEnd(song, i) + 0.4 : wait >= 0);
  if (brk) {
    const upcoming = next[1] || "♪";
    return (
      <div className={compact ? "min-w-0 text-left" : "mx-auto max-w-4xl px-6 text-center"} style={{ opacity: fade }} data-lyric-index={i}>
        <div className={compact ? "" : "flex min-h-[2lh] items-end justify-center text-[clamp(1.4rem,2.6vw,2.2rem)] leading-tight"}>
          {wait <= 3 ? (
            <p className={`flex items-center gap-3 font-semibold tracking-tight text-ink-500 ${compact ? "" : "justify-center"}`}
               style={compact ? { fontSize: compactSize(upcoming) } : undefined}>
              <span key={Math.ceil(wait)} className="grid h-[1.4em] w-[1.4em] shrink-0 place-items-center rounded-full bg-accent-500/20 text-[0.6em] font-bold text-accent-300 animate-rise">
                {Math.ceil(wait)}</span>
              <span className="min-w-0">{upcoming}</span>
            </p>
          ) : (
            <p className={`font-medium tracking-tight text-ink-500 ${compact ? "text-sm" : "text-[0.7em]"}`}>
              {t.rich("live.lyrics.countdown", { which: cur ? "next" : "first", secs: Math.ceil(wait), num: c => <span className="tabular">{c}</span> })}
            </p>
          )}
        </div>
        {!compact && <p className="mt-1 min-h-[1.5rem] text-base text-ink-400">{" "}</p>}
      </div>
    );
  }
  return (
    <div className={compact ? "min-w-0 text-left" : "mx-auto max-w-4xl px-6 text-center"} style={{ opacity: fade }} data-lyric-index={i}>
      {/* room for two lines always, a one-line lyric at its foot: the lane above never changes size mid-song */}
      <div className={compact ? "" : "flex min-h-[2lh] items-end justify-center text-[clamp(1.4rem,2.6vw,2.2rem)] leading-tight"}>
      <p className={compact ? "font-semibold leading-snug tracking-tight" : "font-semibold tracking-tight"}
         style={compact ? { fontSize: compactSize(cur?.[1] ?? "") } : undefined}>
        {/* no word fill: the line whole and bright, free to wrap anywhere (a Japanese or Chinese line has no spaces) */}
        {!fills ? <span className={cur ? "text-ink-100 [overflow-wrap:anywhere]" : "text-ink-500"}>{words.join(" ")}</span> : words.map((w, k) => {
          const fill = fills ? fills[k] : cur ? 1 : 0;
          return (
            <span key={k} className="relative inline-block whitespace-pre">
              <span className="text-ink-500">{w}</span>
              <span className="absolute inset-0 overflow-hidden text-ink-100" style={{ width: `${fill * 100}%` }}>{w}</span>
              {k < words.length - 1 ? " " : ""}
            </span>
          );
        })}
      </p>
      </div>
      {/* in the mini bar a long current line takes two rows, and the next line waits its turn */}
      {(!compact || (cur?.[1] ?? "").length <= 52) && (
        <p className={compact ? "text-xs leading-snug text-ink-400" : "mt-1 min-h-[1.5rem] truncate text-base text-ink-400"}
           style={compact ? { fontSize: Math.min(12, parseFloat(compactSize(next?.[1] ?? ""))) } : undefined}>
          {next?.[1] ?? (cur && !next ? "" : "♪")}
        </p>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ score + hint
export function ScoreAndHint() {
  const t = useT();
  const { score, song, blind } = useSession();
  const said = useHint();
  // the guide faded (lib/guideFade.ts): "✓ On pitch" isn't said, only when you're off
  const level = useSyncExternalStore(onGuideLevel, () => guide.level, () => 0 as const);
  const hint = said === "ok" && !FADE_LEVELS[level].okPill ? "" : said;
  if (song?.mode === "learn" || blind) {
    return (
      <div className="text-right">
        <div className="text-[10px] font-medium uppercase tracking-[.14em] text-ink-400">{t("live.score.label")}</div>
        <div className="text-sm font-medium text-ink-300">{t("live.score.afterSong")}</div>
      </div>
    );
  }
  return (
    <div className="flex items-center gap-4">
      <div className={`flex h-8 items-center rounded-full px-3 text-sm font-semibold transition-all duration-300 ${
        hint === "ok" ? "bg-good/12 text-good" : hint ? "bg-off/12 text-off" : "opacity-0"}`}>
        {hint === "ok" ? t("live.score.onPitch") : hint === "up" ? t("live.score.higher") : hint === "down" ? t("live.score.lower") : "·"}
      </div>
      <div className="text-right">
        <div className="text-[10px] font-medium uppercase tracking-[.14em] text-ink-400">{t("live.score.label")}</div>
        <div className="tabular text-3xl font-bold leading-none tracking-tight">{score ?? "–"}</div>
      </div>
    </div>
  );
}

/** ▲/▼/✓ from the last 1.5 s of you against the singer; a new hint must hold 0.5 s before it shows. */
function useHint() {
  const [shown, setShown] = useState("");
  const playing = usePlaying();
  useEffect(() => {
    if (!playing) return;
    let cand = "", since = 0, cur = "";
    const id = setInterval(() => {
      const perf = perfNow(), now = live.now(perf);
      const errs: number[] = [];
      for (let f = now - 150; f < now - 3; f += 3) {
        const u = live.youAt(f), s = live.singerAt(f);
        if (!Number.isNaN(u) && !Number.isNaN(s)) errs.push(fold(100 * (u - s)));
      }
      const singing = !Number.isNaN(live.youAt(now - 4));
      let want = "";
      if (singing && errs.length >= 14) {
        errs.sort((a, b) => a - b);
        const e = errs[errs.length >> 1];
        want = e > 50 ? "down" : e < -50 ? "up" : "ok";
      }
      if (want !== cand) { cand = want; since = perf; }
      if (want !== cur && perf - since >= 500) { cur = want; setShown(want); }
    }, 100);
    return () => clearInterval(id);
  }, [playing]);
  return shown;
}

// ------------------------------------------------------------------ transport
export function Transport() {
  const t = useT();
  const { status } = useSession();
  const playing = status.player?.state === "playing";
  useTick(250);
  const lull = live.lull(live.frozenPerf ?? performance.now());
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (typing(e) || e.metaKey || e.ctrlKey) return;
      if (e.code === "Space") { e.preventDefault(); control("playpause", "Space key"); }
      else if (e.key === "r" || e.key === "R") control("restart", "R key");
      else if (e.key === "ArrowRight") { if (!skipToSinging()) control("next"); }
      else if (e.key === "ArrowLeft") control("previous");
      else if (e.key === "b" || e.key === "B") { if (live.song?.mode === "sing") setBlind(!live.blind); }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);
  const btn = "grid h-9 w-9 place-items-center rounded-full text-ink-200 transition hover:bg-white/8 hover:text-white active:scale-95";
  return (
    <div className="glass flex items-center gap-1 rounded-full px-2 py-1 shadow-lg shadow-black/30">
      <button className={btn} title={t("live.transport.restart")} onClick={() => control("restart", "restart button")}><Icon d={ICONS.restart} /></button>
      <button className={btn} title={t("live.transport.previous")} onClick={() => control("previous")}><Icon d={ICONS.prev} /></button>
      <button className="grid h-11 w-11 place-items-center rounded-full bg-white text-ink-950 shadow transition hover:scale-105 active:scale-95"
              title={playing ? t("live.transport.pause") : t("live.transport.play")} onClick={() => control("playpause", "play button")}>
        <Icon d={playing ? ICONS.pause : ICONS.play} filled />
      </button>
      <button className={`${btn} ${lull ? "text-accent-300" : ""}`} title={lull ? t("live.transport.skipToSinging") : t("live.transport.next")}
              onClick={() => { if (!skipToSinging()) control("next"); }}><Icon d={ICONS.next} /></button>
    </div>
  );
}

const ICONS = {
  play: "M8 5.5v13l11-6.5z", pause: "M7 5h4v14H7zM13 5h4v14h-4z", next: "M6 6l9 6-9 6zM17 6h2v12h-2z",
  prev: "M18 6l-9 6 9 6zM5 6h2v12H5z", restart: "M4 12a8 8 0 1 0 2.4-5.7M4 4v4.5h4.5",
  check: "M5 12.5l4.5 4.5L19 7.5", chart: "M4 19h16M7 16V10M12 16V6M17 16v-4", mic: "M12 4a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V7a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 17v3",
  head: "M4 15v-3a8 8 0 0 1 16 0v3M4 15a2 2 0 0 1 2-2h1v6H6a2 2 0 0 1-2-2zM20 15a2 2 0 0 0-2-2h-1v6h1a2 2 0 0 0 2-2z",
};
export function Icon({ d, filled, size = 18 }: { d: string; filled?: boolean; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill={filled ? "currentColor" : "none"} stroke={filled ? "none" : "currentColor"}
         strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={d} />
    </svg>
  );
}
export { ICONS };

// ------------------------------------------------------------------ takes
export function TakesTray() {
  const { takes } = useSession();
  if (!takes.length) return null;
  return (
    <div className="pointer-events-auto flex max-w-full gap-2 overflow-x-auto pb-1">
      {takes.slice().reverse().slice(0, 6).map((tk, i) => <TakeChip key={takes.length - i} tk={tk} />)}
    </div>
  );
}
function TakeChip({ tk }: { tk: Take }) {
  const t = useT();
  const learning = tk.status === "learning";
  const { learning: jobs } = useSession();
  // how far its learning has got (src-tauri/src/progress.rs)
  const job = learning || tk.status === "scoring" ? learnOf(jobs, [], { id: tk.track_id ?? "", name: tk.name, artist: tk.artist }) : null;
  const pct = job ? learnPct(job) : null;
  const readyOnly = tk.status === "learned" || tk.status === "partly";
  const right = tk.status === "done" ? (
    <>
      <div className="tabular text-xl font-bold leading-none">{tk.score}</div>
      <div className="text-[10px] text-ink-400 group-hover:text-accent-300">
        {t("live.take.done", { blind: tk.mode === "learn" || tk.blind ? "yes" : "no", hasNotes: tk.notes_total ? "yes" : "no", hit: tk.notes_hit ?? 0, total: tk.notes_total ?? 0 })}
      </div>
    </>
  ) : tk.status === "failed" ? <div className="text-xs text-off">{t("live.take.couldntScore")}</div>
    : readyOnly ? (
      tk.status === "learned" && tk.track_id ? (
        <button onClick={e => { e.preventDefault(); void playTrack(tk.track_id!, true, tk.name); }}
                className="rounded-full bg-accent-500 px-2.5 py-1 text-xs font-semibold text-white transition hover:bg-accent-400">{t("live.take.singNow")}</button>
      ) : <div className="text-right text-[11px] text-ink-300">{t("live.take.learnedPct", { pct: Math.round(100 * (tk.learned ?? 0)) })}<br /><span className="text-ink-400">{t("live.take.playThrough")}</span></div>
    ) : (
      <>
        <div className="tabular text-sm font-semibold text-ink-300">{learning ? "" : tk.live != null ? Math.round(tk.live) : "–"}</div>
        <div className="tabular text-[10px] text-ink-400">{pct != null ? t("live.take.learningPct", { pct }) : job?.stage === "queued" ? t("live.take.waiting") : learning ? t("live.take.learning") : t("live.take.scoring")}</div>
      </>
    );
  const inner = (
    <div className="glass group relative flex min-w-[220px] items-center gap-3 overflow-hidden rounded-2xl px-3.5 py-2.5 animate-rise">
      {(learning || tk.status === "scoring") && pct == null && <div className="shimmer absolute inset-0" />}
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium">{tk.name}</div>
        <div className="truncate text-xs text-ink-400">{tk.status === "learned" ? t("live.take.learned") : tk.artist}</div>
      </div>
      <div className="relative text-right">{right}</div>
      {pct != null && <div className="absolute inset-x-0 bottom-0 h-0.5 bg-white/6">
        <div className="h-full bg-accent-400 transition-[width] duration-500 ease-out motion-reduce:transition-none" style={{ width: `${Math.max(2, pct)}%` }} /></div>}
    </div>
  );
  return tk.status === "done" && tk.report ? <Link href={`/take?id=${encodeURIComponent(tk.report)}`}>{inner}</Link> : inner;
}

// ------------------------------------------------------------------ status
/** New headphones that are likely Bluetooth (macOS says over 100 ms) and never checked: a quiet suggestion to check
 *  their timing in Settings (macOS's figure is often short over Bluetooth), until checked or dismissed. */
function TimingHint({ uid, name }: { uid: string; name: string }) {
  const t = useT();
  const [gone, setGone] = useState<string | null>(null);
  if (gone === uid) return null;
  return (
    <span className="flex items-center rounded-full text-xs text-ink-400">
      <Link href="/settings#output-timing" title={t("live.timing.title")}
            className="rounded-full px-2.5 py-1 transition hover:bg-white/6 hover:text-ink-100">
        {t.rich("live.timing.check", { name: name || t("live.timing.theseHeadphones"), hl: c => <span className="inline-block max-w-32 truncate align-bottom">{c}</span> })}
      </Link>
      <button onClick={() => { setGone(uid); void dismissTimingHint(uid); }} aria-label={t("live.timing.dismiss")}
              className="grid h-5 w-5 place-items-center rounded-full text-ink-500 transition hover:bg-white/8 hover:text-ink-100">×</button>
    </span>
  );
}

export function StatusChips() {
  const t = useT();
  const { status, connected } = useSession();
  const out = status.output_ms ?? 0;
  const chip = "flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs text-ink-300";
  return (
    <div className="flex flex-wrap items-center gap-1">
      {/* Live: pressed, OnKey stops listening (no take, nothing named or learned) until pressed again (also ⌘⇧P) */}
      {!connected
        ? <span className={chip} title={t("live.chips.connectionTitle")}><span className="h-1.5 w-1.5 rounded-full bg-off" />{t("live.chips.reconnecting")}</span>
        : status.listening_paused
          ? <button onClick={() => void setListeningPaused(false)} title={t("live.chips.resumeTitle")}
                    className="flex items-center gap-1.5 rounded-full bg-notice-fill/10 px-2.5 py-1 text-xs text-notice transition hover:bg-notice-fill/16">
              <span aria-hidden>⏸</span>{t("live.chips.paused")}</button>
          : <button onClick={() => void setListeningPaused(true)} title={t("live.chips.pauseTitle")}
                    className={`${chip} transition hover:bg-white/6 hover:text-ink-100`}>
              <span className="h-1.5 w-1.5 rounded-full bg-good" />{t("live.chips.live")}</button>}
      {/* where you hear the music: the device's name (else what kind it likely is); its delay in the tooltip */}
      <span className={chip} title={t("live.chips.outputTitle", { device: status.output?.name || t("live.chips.thisDevice"), ms: out, checked: status.output?.correction_ms != null ? "yes" : "no" })}>
        <Icon d={ICONS.head} size={14} />
        <span className="max-w-44 truncate">{status.output?.name || ((status.output?.reported_ms ?? out) > 100 ? t("live.chips.bluetooth") : t("live.chips.headphones"))}</span>
      </span>
      {TIMING_CHECK_VERIFIED && status.output?.suggest && <TimingHint uid={status.output.uid} name={status.output.name} />}
      {status.mic_device !== undefined && (
        <Link href="/settings" title={t("live.chips.micTitle")}
              className={`flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs transition hover:bg-white/6 ${status.mic_device === null ? "text-notice" : "text-ink-300"}`}>
          <Icon d={ICONS.mic} size={14} /><span className="max-w-44 truncate">{status.mic_device ?? t("live.chips.noMic")}</span>
        </Link>
      )}
    </div>
  );
}

/** The switches while singing, at the bottom right (status stays at the left, so each fits on one line on a laptop):
 *  Show notes, Echo cancel, and Dancer (under the dancer). */
export function SingSwitches() {
  const t = useT();
  const { status, song, blind } = useSession();
  return (
    <div className="flex flex-wrap items-center justify-end gap-1">
      {song?.mode === "sing" && (
        <button onClick={() => setBlind(!blind)} title={t("live.switches.blindTitle")}
                className={`flex items-center gap-2 whitespace-nowrap rounded-full px-2.5 py-1 text-xs transition hover:bg-white/6 ${blind ? "text-ink-100" : "text-ink-300"}`}>
          <span className={`relative h-4 w-7 rounded-full transition ${blind ? "bg-ink-500" : "bg-accent-500"}`}>
            <span className={`absolute top-0.5 h-3 w-3 rounded-full bg-white transition-all ${blind ? "left-0.5" : "left-3.5"}`} />
          </span>
          {t("live.switches.showNotes")}
        </button>
      )}
      {status.echo_cancel != null && (
        <button onClick={() => setEchoCancel(!status.echo_cancel)}
                title={t("live.switches.echoTitle")}
                className={`flex items-center gap-2 whitespace-nowrap rounded-full px-2.5 py-1 text-xs transition hover:bg-white/6 ${status.echo_cancel ? "text-ink-300" : "text-ink-100"}`}>
          <span className={`relative h-4 w-7 rounded-full transition ${status.echo_cancel ? "bg-accent-500" : "bg-ink-500"}`}>
            <span className={`absolute top-0.5 h-3 w-3 rounded-full bg-white transition-all ${status.echo_cancel ? "left-3.5" : "left-0.5"}`} />
          </span>
          {t("live.switches.echo")}
        </button>
      )}
      {song?.mode === "sing" && <DancerToggle />}
    </div>
  );
}

/** The Dancer switch, at the bottom right under the dancer (app/page.tsx), while a learned song is sung. */
export function DancerToggle() {
  const t = useT();
  const on = useDancer();
  return (
    <button onClick={() => setDancer(!on)} title={t("live.switches.dancerTitle")}
            className={`flex items-center gap-2 whitespace-nowrap rounded-full px-2.5 py-1 text-xs transition hover:bg-white/6 ${on ? "text-ink-300" : "text-ink-100"}`}>
      <span className={`relative h-4 w-7 rounded-full transition ${on ? "bg-accent-500" : "bg-ink-500"}`}>
        <span className={`absolute top-0.5 h-3 w-3 rounded-full bg-white transition-all ${on ? "left-3.5" : "left-0.5"}`} />
      </span>
      {t("live.switches.dancer")}
    </button>
  );
}

export function SongClock() {
  useTick(250);
  const song = live.song;
  if (!song) return null;
  return <span className="tabular text-sm text-ink-400">{clock(live.songTime(perfNow()))}{song.track.duration > 0 && ` / ${clock(song.track.duration)}`}</span>;
}

/** In an intro or instrumental break: a button to jump to just before the singing resumes. */
/** After the last lyric line: finish the play now and see the score (people often move on before the song's
 *  end, and a play only counts once it's finished). */
export function FinishSong() {
  const t = useT();
  useTick(500);
  const song = live.song;
  if (pausedEndsIn() != null) return null;     // (paused: PausedFinish says it instead)
  const lines = (song?.lyrics ?? []).filter(([, text]) => text.trim());
  // learned songs only: on a first play, finishing early would also stop the rest of the song being learned
  if (!song || song.mode !== "sing" || !song.cache || lines.length < 3) return null;
  const time = live.songTime(live.frozenPerf ?? performance.now());
  const last = lines[lines.length - 1][0];
  const barsAhead = live.bars().some(([s]) => s > live.now(live.frozenPerf ?? performance.now()));
  if (time < last + 8 || barsAhead) return null;
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-6 flex justify-center">
      <button onClick={() => finishTake()}
              className="pointer-events-auto glass flex animate-rise items-center gap-2 rounded-full px-4 py-2 text-sm font-medium text-ink-100 shadow-lg shadow-black/40 transition hover:border-accent-400/40 hover:text-white">
        {t("live.finish.lastLine")}
        <span className="text-ink-500">·</span>
        <span className="text-accent-300">{t("live.finish.seeScore")}</span>
      </button>
    </div>
  );
}

/** Seconds until a paused take ends and is scored (the engine's PAUSE_ENDS_S), while paused with one open. */
function pausedEndsIn(): number | null {
  const at = live.status.pause_ends_ms;
  if (!at || !live.song || live.ended || live.status.player?.state !== "paused") return null;
  return Math.max(0, (at - Date.now()) / 1000);
}

/** The take still open (History): it's listed once it's scored, so until then it shows here, singing or paused
 *  (with the countdown to its score and Finish now). */
export function OpenTake() {
  const t = useT();
  const { song, ended } = useSession();
  const [, set] = useState(0);
  const left = pausedEndsIn();
  const counting = left != null;
  useEffect(() => {
    if (!counting) return;
    const id = setInterval(() => set(x => x + 1), 1000);
    return () => clearInterval(id);
  }, [counting]);
  if (!song || ended || (song.mode !== "sing" && song.mode !== "learn")) return null;
  const learning = song.mode !== "sing";
  const paused = live.status.player?.state === "paused";
  const what = learning ? "yes" : "no";
  const status = paused && left != null
    ? (left > 0.5 ? t("live.openTake.countdown", { learning: what, time: clock(Math.ceil(left)) }) : learning ? t("live.openTake.learning") : t("live.openTake.scoring"))
    : t("live.openTake.state", { state: paused ? "paused" : learning ? "learning" : "singing", learning: what });
  // (shaped like History's song cards, first among them: the play under way, listed once it's scored)
  return (
    <div className="glass rounded-3xl border-accent-400/30 p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <Cover name={song.track.name} art={song.track.art} link={song.track.link} size={44} />
          <div className="min-w-0">
            <div className="truncate font-medium">{song.track.name}</div>
            <div className="truncate text-sm text-ink-400">{song.track.artist}</div>
          </div>
        </div>
        <div className="flex items-center gap-1.5 pt-1 text-[10px] uppercase tracking-wider text-ink-400">
          <span className="relative flex h-2 w-2">
            {!paused && <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent-400 opacity-60 motion-reduce:animate-none" />}
            <span className={`relative inline-flex h-2 w-2 rounded-full ${paused ? "bg-ink-400" : "bg-accent-400"}`} />
          </span>
          {paused ? t("live.openTake.paused") : t("live.openTake.now")}
        </div>
      </div>
      <div className="mt-3 flex min-h-9 items-end justify-between gap-3">
        <span className="tabular text-xs text-ink-300">{status}</span>
        {(!learning || paused) && (
          <button onClick={() => finishTake()}
                  className="shrink-0 rounded-full bg-white/6 px-3 py-1.5 text-xs font-medium text-accent-300 outline-none transition hover:bg-accent-500 hover:text-white focus-visible:ring-2 focus-visible:ring-accent-400/60">
            {t("live.openTake.finish")}
          </button>
        )}
      </div>
    </div>
  );
}

/** Paused mid-song: the take waits a minute in case you carry on, then is scored. Says so, with Finish to score
 *  it now. */
export function PausedFinish() {
  const t = useT();
  useSession();      // (re-rendered when the status says it's paused, and ticking each second while it counts down:
  const [, set] = useState(0);     // useTick stops while paused)
  const left = pausedEndsIn();
  const counting = left != null;
  useEffect(() => {
    if (!counting) return;
    const id = setInterval(() => set(x => x + 1), 1000);
    return () => clearInterval(id);
  }, [counting]);
  if (left == null) return null;
  const learning = live.song?.mode !== "sing";
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-6 flex justify-center">
      <button onClick={() => finishTake()}
              className="pointer-events-auto glass flex animate-rise items-center gap-2 rounded-full px-4 py-2 text-sm font-medium text-ink-100 shadow-lg shadow-black/40 transition hover:border-accent-400/40 hover:text-white">
        {t("live.finish.paused")}
        <span className="text-ink-500">·</span>
        <span className="tabular text-ink-300">{left > 0.5 ? t("live.finish.pausedIn", { learning: learning ? "yes" : "no", time: clock(Math.ceil(left)) }) : learning ? t("live.finish.learning") : t("live.finish.scoring")}</span>
        <span className="text-ink-500">·</span>
        <span className="text-accent-300">{t("live.finish.now")}</span>
      </button>
    </div>
  );
}

export function SkipToSinging() {
  const t = useT();
  useTick(250);
  const lull = live.lull(live.frozenPerf ?? performance.now());
  if (!lull || lull.wait < 6) return null;
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-6 flex justify-center">
      <button onClick={() => skipToSinging()}
              className="pointer-events-auto glass flex animate-rise items-center gap-2 rounded-full px-4 py-2 text-sm font-medium text-ink-100 shadow-lg shadow-black/40 transition hover:border-accent-400/40 hover:text-white">
        {t("live.skip.singingIn", { secs: Math.ceil(lull.wait) })}
        <span className="text-ink-500">·</span>
        <span className="text-accent-300">{t("live.skip.skipAhead")}</span>
        <Icon d={ICONS.next} size={14} />
      </button>
    </div>
  );
}

/** How close you are to the current note, in a small space: a dot on a ±1 semitone scale, green when within
 *  50 cents. Only on learned songs with the notes shown. */
export function PitchStrip() {
  const t = useT();
  useTick(50);
  const perf = live.frozenPerf ?? performance.now();
  // your pitch arrives a few hundred ms after you sing it (pitch tracking needs the sound first): judge the newest
  // frame that has arrived, within the last 0.6 s, against the note at that frame (looking at "now" found nothing)
  const nowF = live.now(perf);
  let at = -1;
  for (let f = Math.floor(nowF); f > nowF - 60; f--) if (!Number.isNaN(live.youAt(f))) { at = f; break; }
  // forgiving: the median of the last quarter second of you against the notes (so it glides, not darts), within
  // 25 cents counts as dead centre, and the bar's ends are two semitones out
  const errs: number[] = [];
  if (at >= 0 && live.showNotes()) {
    const bars = live.bars();
    for (let f = at; f > at - 25; f--) {
      const u = live.youAt(f);
      if (Number.isNaN(u)) continue;
      const b = bars.find(([s, e]) => s <= f && f < e);
      if (b) errs.push(fold(100 * (u - b[2])));
    }
  }
  const err = errs.length >= 3 ? errs.sort((p, q) => p - q)[errs.length >> 1] : NaN;
  const off = Number.isNaN(err) ? 0 : Math.sign(err) * Math.max(0, Math.abs(err) - 25);
  const pos = Number.isNaN(err) ? null : Math.max(-1, Math.min(1, off / 175));
  // a small grey dot that fades in as you near the note: nearly invisible when far out, clear when in tune
  const alpha = Number.isNaN(err) ? 0 : Math.max(0, 1 - Math.abs(err) / 200) ** 1.5;
  return (
    <div className="relative h-2 w-full rounded-full bg-white/6" title={t("live.pitchTitle")}>
      <div className="absolute inset-y-0 left-1/2 w-px bg-white/25" />
      {pos != null && <div className="absolute top-1/2 h-1.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-ink-200"
                           style={{ left: `${50 + pos * 50}%`, opacity: alpha, transition: "left 60ms linear, opacity 120ms linear" }} />}
    </div>
  );
}

/** When a take has been scored (or a song learned): a small notice that fades after a few seconds. The takes
 *  themselves live in History. */
export function TakeToast() {
  const t = useT();
  const { takes } = useSession();
  const seen = useRef<number | null>(null);
  const [shown, setShown] = useState<Take | null>(null);
  useEffect(() => {
    const finished = takes.filter(tk => tk.status === "done" || tk.status === "learned");
    if (seen.current === null) { seen.current = finished.length; return; }   // not the ones from before
    if (finished.length > seen.current) {
      seen.current = finished.length;
      setShown(finished[finished.length - 1]);
    }
  }, [takes]);
  // (its own timer, kept by what's shown: another takes update within the 7 s used to clear it, never set again,
  // so the notice stayed up for good)
  useEffect(() => {
    if (!shown) return;
    const id = setTimeout(() => setShown(null), 7000);
    return () => clearTimeout(id);
  }, [shown]);
  if (!shown) return null;
  const body = shown.status === "done"
    ? <><span className="tabular text-lg font-bold text-white">{shown.score}</span>
        {shown.report && <Link href={`/take?id=${encodeURIComponent(shown.report)}`} className="shrink-0 whitespace-nowrap text-accent-300 hover:text-white">{t("live.toast.view")}</Link>}</>
    : <span className="text-good">{t("live.toast.learned")}</span>;
  return (
    <div className="glass flex min-w-0 animate-rise items-center gap-3 rounded-2xl px-4 py-2 text-sm shadow-lg shadow-black/30">
      <span className="min-w-0 truncate text-ink-200">{shown.name}</span>
      {body}
    </div>
  );
}
