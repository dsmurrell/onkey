"use client";
import Link from "next/link";
import { Suspense, useEffect, useMemo, useRef, useState } from "react";

/** the take page's "Your voice" level (dB), kept on this Mac */
const VOICE_DB_KEY = "onkey.take.voiceDb";
import { useSearchParams } from "next/navigation";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import LearnBar from "@/components/LearnBar";
import LevelTag from "@/components/LevelTag";
import { reportProblem } from "@/components/ReportProblem";
import SharePanel from "@/components/SharePanel";
import Challenge from "@/components/Challenge";
import LeaveOutGuide from "@/components/guides/LeaveOutGuide";
import { DevCopy, devEar, takeLink, useDevTools } from "@/components/Dev";
import { driveGraphAt } from "@/lib/devDrive";
import { alongTiming } from "@/lib/alongTiming";
import { songAtTake, takeAtSong, type Place } from "@/lib/places";
import { allowControl, alongMode, type AlongMode, getSettings, getTake, joinAlong, setNotMelody, sharePrompt, sharePromptSeen, type SharePrompt, learnLabel, playAlong, playTrack, playerNow, type PlayerNow, stopPlayAlong, voiceClose, voiceGain, voiceOpen, voicePlay, voiceStop as voiceStopNative, takeAudioUrl, takePlaying, type Track, useLearn, useSession } from "@/lib/session";
import { ACCENT, INK } from "@/lib/colors";
import { COLORS, clock, creditAt, grade, levelName, levelOf, lineGrade, noteName, offBy, scoreWord, sourceName, when } from "@/lib/music";
import { useT, type Translator } from "@/lib/i18n";
import { appText, shareReason } from "@/lib/engineText";

type Line = { t0: number; t1: number; text: string; ref_frames: number; coverage: number; accuracy: number | null; bias: number | null };
type Result = {
  track: Track; recorded: string; score: number; accuracy: number; coverage: number; within50: number; bias_cents: number;
  latency_ms: number; frame_s: number; song_offset?: number | null; places?: Place[] | null; lines: Line[]; ref: (number | null)[]; usr: (number | null)[]; err: (number | null)[];
  notes?: [number, number, number, number][]; notes_hit?: number; notes_total?: number; live?: { score: number | null; cached: boolean } | null;
  mode?: "learn" | "sing"; blind?: boolean;
  /** the level it was scored at (none: from before levels, Relaxed) */
  level?: string;
  /** sung at another level than Settings' now: that level and its score then (store::take_summary) */
  sung_level?: string | null; sung_score?: number | null;
  /** false: an older take whose recording isn't kept (every take's is, until they pass 2 GB together) */
  audio_kept?: boolean;
  /** the learned song this take is of, and its stretches marked "Not the melody" (song seconds), left out of the score */
  song_id?: string; not_melody?: [number, number][];
  /** take frames left out of this take's score, its score as first scored, and when it was scored again */
  left_out?: [number, number][]; scored_first?: { score: number } | null; rescored?: string | null;
};

/** A stretch of the graph (s), [from, to). */
type Span = [number, number];

export default function TakePage() {
  return <Suspense><Take /></Suspense>;
}

/** How much to raise a recording so its singing plays at a listening level: the loud tenth of its 100 ms stretches
 *  (the singing, not the gaps) brought to -16 dBFS, by 0 to 30 dB. */
async function singingGain(b: Blob): Promise<number> {
  try {
    const ctx = new OfflineAudioContext(1, 1, 16000);
    const buf = await ctx.decodeAudioData(await b.arrayBuffer());
    const x = buf.getChannelData(0), n = Math.floor(buf.sampleRate / 10);
    const db: number[] = [];
    for (let i = 0; i + n <= x.length; i += n) {
      let e = 0;
      for (let k = i; k < i + n; k++) e += x[k] * x[k];
      db.push(10 * Math.log10(e / n + 1e-12));
    }
    if (!db.length) return 1;
    db.sort((p, q) => p - q);
    const loud = db[Math.floor(0.9 * (db.length - 1))];
    return Math.pow(10, Math.max(0, Math.min(30, -16 - loud)) / 20);
  } catch { return 1; }
}

// into the app's log (Tauri's ui_log), for diagnosing playback
function logUi(msg: string) {
  try { (window as any).__TAURI_INTERNALS__?.invoke("ui_log", { msg }); } catch {}
}

/** One take's page, made afresh for each take: another take opened in place (an onkey:// link while this page is
 *  open) never shows under the old one's graph, recording or loads still on their way. */
function Take() {
  const id = useSearchParams().get("id") ?? "";
  return <TakeOf key={id} />;
}

function TakeOf() {
  const params = useSearchParams();
  const id = params.get("id") ?? "";
  // development tools: "Play with the song" asked for without a click (components/DevDrive.tsx), from this place
  const driveAt = params.get("along"), driveN = params.get("n");
  // a link's moment (onkey://take/<id>?t=…, components/OpenLinks.tsx): gone to once the recording has loaded
  const tParam = params.get("t");
  const startAt = useRef<number | null>(null);
  useEffect(() => { const s = Number(tParam); startAt.current = tParam != null && Number.isFinite(s) ? s : null; }, [id, tParam]);
  const t = useT();
  const dev = useDevTools();
  const [r, setR] = useState<Result | null | undefined>(undefined);
  const [lyricsOff, setLyricsOff] = useState(false);
  // "Show lyrics" off (Settings): the lines are scored as ever, but their words aren't shown
  const load = () => Promise.all([getTake(id), getSettings().catch(() => null)])
    .then(([take, s]) => {
      const off = s?.show_lyrics === false;
      setLyricsOff(off);
      setR(take && off ? { ...take, lines: take.lines.map((l: Line) => ({ ...l, text: "" })) } : take);
    })
    .catch(() => setR(null));
  useEffect(() => { void load(); }, [id]); // eslint-disable-line
  // "Not the melody": choosing a stretch of the graph to leave out of the song's score (`sel`, graph s)
  const [marking, setMarking] = useState(false);
  const [sel, setSel] = useState<Span | null>(null);
  const [saving, setSaving] = useState(false);
  const [rescoredNote, setRescoredNote] = useState("");
  const job = useLearn(r?.track);     // the song still being learned: it can be sung again once that's done
  const [src, setSrc] = useState("");
  // the recording, loaded into memory once (a few MB), so jumping around in it is instant
  useEffect(() => {
    // (left before the fetch ends: its blob URL is never made, so none is left behind)
    let url = "", alive = true;
    // (an AAC recording typed audio/mp4 whatever the asset protocol guessed; an older take's WAV as served)
    takeAudioUrl(id).then(async u => {
      if (!u) return null;            // no recording kept: nothing to fetch
      const b = await (await fetch(u)).blob();
      return /\.m4a($|\?)/i.test(u) ? new Blob([b], { type: "audio/mp4" }) : b;
    })
      .then(b => {
        if (!alive) return;
        if (!b) { setSrc(""); return; }
        blob.current = b; url = URL.createObjectURL(b); setSrc(url);
        void singingGain(b).then(g => { boost.current.gain = g; applyGain(); });
      })
      .catch(() => takeAudioUrl(id).then(u => { if (alive) setSrc(u); }).catch(() => { if (alive) setSrc(""); }));
    return () => { alive = false; if (url) URL.revokeObjectURL(url); };
  }, [id]);
  const audio = useRef<HTMLAudioElement>(null);
  const blob = useRef<Blob | null>(null);
  // your recording is the raw microphone, quiet next to the music (singing ~-30 to -45 dBFS, Spotify near full
  // level: live, "Play with the song" let only the loudest notes through): it plays brought up to a listening level,
  // through a limiter (see singingGain), set up on the first play (sound may only start from a click)
  const boost = useRef<{ gain: number; node: GainNode | null; ctx: AudioContext | null }>({ gain: 1, node: null, ctx: null });
  // "Your voice": louder or quieter than that listening level (dB), your choice kept on this Mac
  const [voiceDb, setVoiceDb] = useState(0);
  const voiceDbRef = useRef(0);
  useEffect(() => { try { const v = Number(localStorage.getItem(VOICE_DB_KEY)); if (Number.isFinite(v)) { voiceDbRef.current = v; setVoiceDb(v); } } catch {} }, []);
  const applyGain = () => {
    const b = boost.current;
    if (b.node) b.node.gain.value = b.gain * Math.pow(10, voiceDbRef.current / 20);
    void voiceGain(20 * Math.log10(b.gain) + voiceDbRef.current);   // the same level, for "Play with the song"
  };
  // development tools: "Shift" moves your voice later (+) or earlier (-) against the song in "Play with the song", to
  // line it up by ear. Each setting goes to the log next to the ear's measured gap, to compare what you hear with what
  // it measures (kept per take on this Mac)
  const shiftKey = `onkey.dev.shift.${id}`;
  const [shiftMs, setShiftMs] = useState(0);
  const shiftRef = useRef(0);
  useEffect(() => { try { const v = Number(localStorage.getItem(shiftKey)); shiftRef.current = Number.isFinite(v) ? v : 0; setShiftMs(shiftRef.current); } catch {} }, [shiftKey]);
  // ✓: you hear it in sync at this shift (to the log too); moving the shift takes the mark off
  const [synced, setSynced] = useState(false);
  useEffect(() => { try { setSynced(localStorage.getItem(`${shiftKey}.ok`) === "1"); } catch {} }, [shiftKey]);
  const markSynced = (on: boolean) => {
    setSynced(on);
    try { if (on) localStorage.setItem(`${shiftKey}.ok`, "1"); else localStorage.removeItem(`${shiftKey}.ok`); } catch {}
  };
  const changeShift = (v: number) => {
    shiftRef.current = v; setShiftMs(v);
    try { localStorage.setItem(shiftKey, String(v)); } catch {}
    if (synced) markSynced(false);
  };
  const changeVoiceDb = (v: number) => {
    voiceDbRef.current = v; setVoiceDb(v); applyGain();
    try { localStorage.setItem(VOICE_DB_KEY, String(v)); } catch {}
  };
  // closed when the page goes: WebKit allows only a few AudioContexts, and after several take pages a new one
  // would fail (the voice boost silently gone)
  useEffect(() => () => {
    const b = boost.current;
    if (b.ctx) void b.ctx.close().catch(() => {});
    b.ctx = null; b.node = null;
  }, []);
  // called from every click that starts sound (WebKit only lets an AudioContext start from one): set up once, then
  // make sure it's running, or the recording plays into a suspended context and is silent
  const route = () => {
    const a = audio.current;
    if (!a) return;
    if (boost.current.ctx) {
      if (boost.current.ctx.state !== "running") void boost.current.ctx.resume().then(() => logUi(`take playback: sound ${boost.current.ctx?.state}`));
      return;
    }
    try {
      const ctx = new AudioContext();
      const node = ctx.createGain();
      node.gain.value = boost.current.gain * Math.pow(10, voiceDbRef.current / 20);
      const limit = ctx.createDynamicsCompressor();
      limit.threshold.value = -3; limit.knee.value = 0; limit.ratio.value = 20; limit.attack.value = 0.002; limit.release.value = 0.1;
      ctx.createMediaElementSource(a).connect(node).connect(limit).connect(ctx.destination);
      boost.current.node = node; boost.current.ctx = ctx;
      void ctx.resume().then(() => logUi(`take playback: voice raised ${(20 * Math.log10(boost.current.gain)).toFixed(1)} dB, sound ${ctx.state}`));
    } catch (e) { logUi(`take playback: no level boost (${e})`); }
  };
  const [head, setHead] = useState(0);
  const [sharing, setSharing] = useState(false);
  // the player: what the play button plays, and the dev tools tucked under "dev"
  // "With the song" first: hearing yourself against the track is what most people want; "Your voice only" when the
  // take can't be placed in the song (no place, so no switch shown)
  const [pick, setWithSong] = useState(true);
  const withSong = pick && r?.song_offset != null;
  const [devOpen, setDevOpen] = useState(false);
  // a proud moment (a best on the song, a clear step up, a first score over 80): "Share this take" is offered, once
  // per take (marked shown as soon as it's on screen; ✕ or Share puts it away)
  const [proud, setProud] = useState<SharePrompt | null>(null);
  useEffect(() => {
    void sharePrompt(id).then(p => { if (p) { setProud(p); void sharePromptSeen(id); } }).catch(() => {});
  }, [id]);
  const [playing, setPlaying] = useState(false);
  // "Play with the song" plays the recording natively (voice.rs), not in the window: WebKit holds its sound back
  // ~0.1-0.15 s more than it says, so the voice came out late. The native player says exactly where the voice is, read
  // with the song's place (player_now's `voice`), so the two are compared at one moment, as a take's bars are placed.
  // `at`/`from`: when and where it last started, for the playhead in between readings.
  const voice = useRef<{ dur: number | null; on: boolean; at: number; from: number }>({ dur: null, on: false, at: 0, from: 0 });
  const voicePos = () => {
    const v = voice.current;
    return v.on ? v.from + (performance.now() - v.at) / 1000 : (audio.current?.currentTime ?? 0);
  };
  // the playhead shows the moment you're hearing, on the graph's time line. Two delays put it behind the
  // recording's own clock:
  //  - the take's `latency_ms`: scoring moved your voice that much earlier to meet the singer's notes (how late the
  //    song reached you while singing, the mic's delay, how late you sang), and the graph draws it moved, so the
  //    voice playing at recording time τ is drawn at τ - latency
  //  - the output's delay now (Bluetooth headphones ~150-250 ms, Core Audio's figure, as live scoring allows for)
  // The sound needs nothing: in "Play with the song" your voice and the song share the output and reach you together.
  // Everything shown (the playhead, playing or paused, the line lit, where a click goes) is on that heard time line.
  const { status } = useSession();
  const heardMs = useRef(0), latency = useRef(0);
  latency.current = (r?.latency_ms ?? 0) / 1000;
  heardMs.current = (status.output_ms ?? 0) + (r?.latency_ms ?? 0);
  // where playback last started on the graph: until the first sound reaches you (the output's delay) the playhead
  // waits there, so a click puts it exactly where clicked and resuming doesn't jump back
  const heardFrom = useRef(0);
  const heardPos = () => Math.max(0, heardFrom.current, voicePos() - heardMs.current / 1000);
  // (the native player fades each start, stop and move: never a hard cut)
  const voiceStop = () => {
    const v = voice.current;
    if (!v.on) return;
    v.on = false;
    void voiceStopNative();
  };
  const voiceStart = (pos: number) => {
    const v = voice.current;
    if (v.dur == null) return;
    pos = Math.max(0, pos);
    void voicePlay(pos);
    v.on = true; v.at = performance.now(); v.from = pos;
    heardFrom.current = Math.max(0, pos - latency.current);
    setPlaying(true);
  };
  // another take in this page (an onkey:// link goes from take to take in place): let the last one's recording go, and
  // stop playing it, so the next "Play with the song" opens this take's
  useEffect(() => () => {
    voice.current = { dur: null, on: false, at: 0, from: 0 };
    void voiceClose();
    audio.current?.pause(); setAlong(false); setLining(false); setPlaying(false);
  }, [id]);
  // listening back with the song: the music app plays the track, your recording follows its position (lined up
  // the way the take was scored), corrected several times a second
  const [along, setAlong] = useState(false);
  // the share panel's "Play it": when its 20 s are up
  const clipStop = useRef<number | undefined>(undefined);
  // listening mode without control of the song's app: you play the song there and OnKey joins in (`joined`: it
  // has heard the song and the recording is following it)
  const [mode, setMode] = useState<AlongMode | null>(null);
  const [joined, setJoined] = useState(false);
  const [joinFrom, setJoinFrom] = useState(0);
  const [modeTried, setModeTried] = useState(false);
  useEffect(() => { if (r) alongMode(r.track.id).then(setMode).catch(() => setMode(null)).finally(() => setModeTried(true)); }, [r]);
  const holdUntil = useRef(0);        // after asking the app to jump, or a seek: readings settle for a moment
  const startingUntil = useRef(0);    // OnKey pausing, seeking and playing the song: lining up, not "paused"
  // waiting for the song to be placed before the recording plays (shown as "Lining up with the song…")
  const [lining, setLining] = useState(false);
  // One rule, with or without control of the app: the recording plays where the engine places the song (player_now's
  // `heard`), exactly as it places a take's bars (the app's position, then the music's own fingerprint), and waits,
  // silent, while it isn't placed. The last few (song - recording) differences: one reading can be a moment old, so
  // the median of five is what counts
  // (a take with a jump in it has a place per stretch: lib/places.ts)
  const recordingAt = (song: number) => takeAtSong(song, r?.places, r?.song_offset ?? 0) + (r?.latency_ms ?? 0) / 1000;
  const songAt = (g: number) => Math.max(0, songAtTake(g, r?.places, r?.song_offset ?? 0));  // song time at graph time g
  useEffect(() => {
    if (!along || !r) return;
    let alive = true;
    // each change in what the sync loop decides goes to the app's log, to diagnose listening back
    let said = "";
    const diffs: number[] = [];
    const say = (what: string) => { if (what !== said) { said = what; logUi(`play with the song: ${what}`); } };
    // timing, for the log (diagnostics: where the voice is against the song heard), at the start and every 2 s
    let timedAt = -Infinity;
    const timing = (n: PlayerNow, song: number, target: number) => {
      const now = performance.now();
      if (now - timedAt < 2000) return;
      timedAt = now;
      logUi(`play with the song: timing ${alongTiming(n, song, target, n.voice ?? null, Math.round(heardMs.current - latency.current * 1000))}`);
    };
    const tick = async () => {
      const n = await playerNow().catch(() => null);
      if (!alive) return;
      const v = voice.current;
      if (v.dur == null) { say("opening the recording"); return; }
      if (performance.now() < holdUntil.current) return;
      // (lining up again after a move keeps a join: the window doesn't ask you to play the song again)
      const quiet = (why: string, wait = false) => { say(why); setLining(wait); if (v.on) { voiceStop(); setPlaying(false); } if (!wait) setJoined(false); };
      if (!n) return quiet("no music app state");
      // (the player names the song by the id it's kept under, or, outside listening mode, by its app's own id)
      if (n.track !== r.track.id && n.track !== mode?.play_id) return quiet(`the music app plays another track (${n.track}, take ${r.track.id})`, !!mode?.control);
      if (!n.playing && performance.now() >= startingUntil.current) return quiet("the music is paused");
      if (n.heard == null) return quiet("lining up with the song (until it's placed)", true);
      setLining(false); setJoined(true);
      const song = n.heard;
      const target = recordingAt(song) - shiftRef.current / 1000;
      if (target < 0 || target > v.dur) return quiet(`outside the recording (song ${song.toFixed(1)} s, recording ${target.toFixed(1)} of ${v.dur.toFixed(1)} s)`);
      // (with how far the app's own clock is from the song heard, for the log: never used)
      if (!v.on) { say(`playing together, as placed (the app's clock ${(n.position - song).toFixed(2)} s ahead of it${shiftRef.current ? `; dev shift ${shiftRef.current > 0 ? "+" : ""}${shiftRef.current} ms` : ""})`); diffs.length = 0; voiceStart(target); holdUntil.current = performance.now() + 300; timedAt = -Infinity; timing(n, song, target); return; }
      setHead(heardPos());
      timing(n, song, target);
      if (n.voice == null) return;     // (just started: not rendering yet)
      diffs.push(target - n.voice);
      if (diffs.length > 5) diffs.shift();
      if (diffs.length < 3) return;
      const diff = [...diffs].sort((x, y) => x - y)[Math.floor(diffs.length / 2)];
      // the voice follows every move of the song (a jump, or the music's fingerprint placing it again): both places are
      // exact now, so anything over a few hundredths is a move, not noise (the audio clocks don't drift within a song)
      if (Math.abs(diff) > 0.05) {
        say(`moved the recording ${diff.toFixed(2)} s`);
        voiceStart(target); diffs.length = 0; holdUntil.current = performance.now() + 300;
      }
    };
    const id = setInterval(tick, 150);
    return () => { alive = false; clearInterval(id); voiceStop(); setPlaying(false); setLining(false); };
  }, [along, r, mode]);
  useEffect(() => () => { void stopPlayAlong(); }, []);
  // your recording playing here is never to be heard as music by listening mode
  useEffect(() => { void takePlaying(playing); }, [playing]);
  useEffect(() => () => { void takePlaying(false); }, []);
  const startAlong = (from: number) => {
    audio.current?.pause(); voiceStop();
    if (voice.current.dur == null)
      voiceOpen(id).then(d => {
        if (d == null) { logUi("play with the song: couldn't open the recording"); return; }
        voice.current.dur = d; applyGain(); logUi(`play with the song: recording opened (${d.toFixed(1)} s)`);
      }).catch(e => logUi(`play with the song: couldn't open the recording (${e})`));
    setAlong(true); setLining(true); holdUntil.current = performance.now() + 700;
    if (mode && !mode.control) {
      // join in: it plays once the song is heard
      setJoinFrom(songAt(from)); setJoined(false);
      void joinAlong(r!.track.id);
      return;
    }
    startingUntil.current = performance.now() + 4000;
    void playAlong(mode?.play_id ?? r!.track.id, songAt(from)); };
  const stopAlong = () => { setAlong(false); setLining(false); voiceStop(); setPlaying(false); audio.current?.pause(); void stopPlayAlong(); };
  // development tools: a driven request presses the buttons through their own code, once the page is as ready as
  // for a click (the take, its recording and how it plays along loaded): from a place, as a click on the graph then
  // "Play with the song" (or, playing, as a click on the graph); a driven stop, as "Stop"
  const driven = useRef<string | null>(null);
  useEffect(() => {
    if (!dev || driveAt == null || driveN == null || driven.current === driveN) return;
    if (!r || !src || !audio.current || !modeTried) return;
    driven.current = driveN;
    const g = driveGraphAt(driveAt, r.song_offset);
    if (g == null || r.song_offset == null || r.audio_kept === false) { logUi(`dev drive: can't play this take with the song (${driveAt})`); return; }
    logUi(`dev drive: Play with the song from ${g.toFixed(2)} s on the graph (song ${(r.song_offset + g).toFixed(2)} s)`);
    heardFrom.current = g; setHead(g);
    startAlong(g);
  }); // eslint-disable-line react-hooks/exhaustive-deps
  const devStop = useRef(() => {});
  devStop.current = () => { if (along) { logUi("dev drive: Stop"); stopAlong(); } };
  useEffect(() => {
    const on = () => devStop.current();
    window.addEventListener("onkey-dev-along-stop", on);
    return () => window.removeEventListener("onkey-dev-along-stop", on);
  }, []);
  // development tools: the ear measures what's heard while it plays (src-tauri/src/ear.rs)
  useEffect(() => {
    if (!along || !dev) return;
    void devEar(id, true);
    return () => { void devEar(id, false); };
  }, [along, id, dev]);

  if (r === undefined) return <Shell><div className="glass mt-8 h-64 rounded-3xl shimmer" /></Shell>;
  if (r === null) return <Shell><p className="mt-10 text-ink-300">{t("take.notFound")}</p></Shell>;
  // the song's "Not the melody" stretches in this take, on the graph (song s - the take's place in the song)
  const songNm = r.not_melody ?? [];
  const graphDur = r.ref.length * r.frame_s;
  const leftOut: Span[] = r.song_offset == null ? [] : songNm.map(([a, b]): Span => [takeAtSong(a, r.places, r.song_offset!), takeAtSong(b, r.places, r.song_offset!)])
    .filter(([a, b]) => b > 0 && a < graphDur);
  const canMark = r.song_offset != null && !!r.song_id;
  const keepNotMelody = async (next: [number, number][]) => {
    if (!r.song_id) return;
    setSaving(true);
    try {
      const done = await setNotMelody(r.song_id, next);
      setRescoredNote(!done ? t("take.notMelody.notLearned") : done.rescored === 0 ? t("take.notMelody.noChange") :
        t("take.notMelody.rescored", { count: done.rescored }));
      setMarking(false); setSel(null);
      await load();
    } finally { setSaving(false); }
  };
  const leaveOut = () => {
    if (!sel || r.song_offset == null) return;
    try { localStorage.setItem(LEFT_OUT_ONCE, "1"); } catch {}
    const at = songAt;
    void keepNotMelody([...songNm, [at(sel[0]), at(sel[1])]]);
  };
  const putBack = (i: number) => void keepNotMelody(songNm.filter((_, k) => k !== i));
  // s on the graph: you want to hear s, so the recording goes to s + latency (the graph draws your voice moved
  // that much earlier); the output's delay is taken care of by the playhead waiting at s (heardFrom)
  const seek = (s: number) => {
    if (!audio.current) return;
    s = Math.max(0, s);
    heardFrom.current = s; setHead(s);
    if (along) { startAlong(s); return; }
    // moved there; it plays on only if it was playing (press Play to start from here)
    audio.current.currentTime = s + latency.current;
    if (playing) { route(); void audio.current.play(); }
  };

  // an ended recording starts again from 0, so the playhead waits there, not at the end
  const play = () => {
    const a = audio.current;
    if (!a) return;
    const from = a.ended ? 0 : head;
    heardFrom.current = from; setHead(from);
    route(); void a.play();
  };

  return (
    <Shell>
      <audio ref={audio} src={src || undefined} preload="auto"
             onLoadedMetadata={() => { if (startAt.current != null) { seek(startAt.current); startAt.current = null; } }}
             onTimeUpdate={() => setHead(heardPos())} onPlay={() => { route(); setPlaying(true); }}
             onPause={() => { setHead(heardPos()); setPlaying(false); }} onEnded={() => setPlaying(false)} />
      {/* the ring stays beside the title at every width (smaller when narrow), so the graph stays on the first screen */}
      <section className="mt-6 grid grid-cols-[auto_1fr] items-start gap-4 md:items-center md:gap-6">
        <ScoreRing score={r.score} />
        <div className="min-w-0">
          <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
            <div className="min-w-0">
              <h1 className="truncate text-3xl font-semibold tracking-tight">{r.track.name}</h1>
              <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1">
                <span className="text-lg text-ink-300">{r.track.artist}</span>
                <span aria-hidden className="text-ink-500">·</span>
                <span className="text-sm text-ink-400">{when(r.recorded)}</span>
                <ModeBadge mode={r.mode} blind={!!r.blind} />
                <LevelTag level={r.level} />
              </div>
            </div>
            <div className="flex shrink-0 flex-wrap items-center gap-2">
              <button onClick={() => playTrack(r.track.id, true, r.track.name)} disabled={!!job} title={job ? t("take.header.singAgainBlocked") : undefined}
                      className="tabular rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white shadow-lg shadow-accent-600/25 transition hover:bg-accent-400
                                 disabled:cursor-default disabled:bg-white/6 disabled:text-ink-300 disabled:shadow-none">
                {job ? learnLabel(job) : t("take.header.singAgain")}
              </button>
              {r.song_offset != null && r.audio_kept !== false && (
                <button onClick={() => { setSharing(v => !v); setProud(null); }} aria-expanded={sharing} title={t("take.header.shareTitle")}
                        className={`inline-flex items-center gap-1.5 rounded-xl px-3 py-2 text-sm font-medium transition ${sharing || (proud && !sharing) ? "bg-accent-500/20 text-accent-100 ring-1 ring-accent-400/40 hover:bg-accent-500/30" : "bg-white/6 text-ink-100 hover:bg-white/10"}`}>
                  <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                    <path d="M12 3v12" /><path d="M7 8l5-5 5 5" /><path d="M5 13v6a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-6" />
                  </svg>
                  {t("take.header.share")}
                </button>
              )}
              {!job && <Challenge song={r.track.name} artist={r.track.artist} score={r.score} />}
            </div>
          </div>
          <p className="mt-3 max-w-2xl text-ink-200">{verdict(t, r)}</p>
          <p className="mt-2 max-w-2xl text-sm text-ink-300" title={t("take.header.within50Title", { pct: Math.round(100 * r.within50) })}>
            <ScoreStory r={r} />
          </p>
          {r.sung_level && r.sung_score != null && (
            <p className="mt-1 text-xs text-ink-400" title={t("take.header.sungLevelTitle")}>
              {t("take.header.sungLevel", { level: levelName(levelOf(r.level)), sungLevel: levelName(levelOf(r.sung_level)), score: r.sung_score })}
            </p>
          )}
          {r.left_out?.length && r.scored_first ? (
            <p className="mt-1 text-xs text-ink-400">
              {t("take.header.leftOut", { count: r.left_out.length, changed: r.scored_first.score !== r.score ? "yes" : "no", first: r.scored_first.score })}
            </p>
          ) : null}
          {job && <LearnBar job={job} className="mt-3 max-w-sm" />}
          {proud && !sharing && r.song_offset != null && r.audio_kept !== false && (
            <div className="mt-4 inline-flex max-w-full items-center gap-3 rounded-xl bg-accent-500/10 py-1.5 pl-3 pr-1.5 text-sm text-accent-100">
              <span className="min-w-0 truncate"><span aria-hidden className="mr-1.5 text-accent-300">✦</span>{shareReason(t, proud)}</span>
              <button onClick={() => { setSharing(true); setProud(null); }}
                      className="shrink-0 rounded-lg bg-accent-500 px-3 py-1 text-xs font-medium text-white transition hover:bg-accent-400">
                {t("take.proud.share")}
              </button>
              <button onClick={() => setProud(null)} aria-label={t("take.proud.notNow")} title={t("take.proud.notNow")}
                      className="shrink-0 px-1 text-accent-200/70 transition hover:text-white">✕</button>
            </div>
          )}
        </div>
      </section>

      <section className="glass mt-8 rounded-3xl p-4">
        {r.audio_kept === false ? (
          <p className="mb-3 px-1 text-xs text-ink-400">{t("take.player.notKept")}</p>
        ) : (
          <div className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-2 px-1">
            {/* one play button; what it plays is chosen beside it */}
            <button onClick={() => {
                      if (along) stopAlong();
                      else if (playing) audio.current?.pause();
                      else if (withSong) startAlong(head);
                      else play();
                    }}
                    aria-label={playing || along ? t("take.player.stop") : t("take.player.play")} title={playing || along ? t("take.player.stop") : withSong ? t("take.player.playWithSong") : t("take.player.playVoice")}
                    className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-accent-500 text-white shadow-lg shadow-accent-600/25 transition hover:bg-accent-400">
              {playing || along
                ? <svg viewBox="0 0 24 24" className="h-4 w-4" fill="currentColor" aria-hidden><rect x="6" y="5" width="4" height="14" rx="1" /><rect x="14" y="5" width="4" height="14" rx="1" /></svg>
                : <svg viewBox="0 0 24 24" className="ml-0.5 h-4 w-4" fill="currentColor" aria-hidden><path d="M8 5v14l11-7z" /></svg>}
            </button>
            {r.song_offset != null && (
              <span className="flex rounded-xl bg-white/6 p-0.5 text-sm" role="radiogroup" aria-label={t("take.player.whatPlays")}>
                {([[true, t("take.player.withSong")], [false, t("take.player.voiceOnly")]] as const).map(([k, label]) => (
                  <button key={String(k)} role="radio" aria-checked={withSong === k}
                          onClick={() => {
                            if (withSong === k) return;
                            setWithSong(k);
                            // switching while it plays carries on from here the other way
                            if (along) { stopAlong(); if (!k) play(); }
                            else if (playing) { audio.current?.pause(); if (k) startAlong(head); }
                          }}
                          className={`rounded-lg px-3 py-1.5 font-medium transition ${withSong === k ? "bg-white/12 text-white" : "text-ink-300 hover:text-white"}`}>
                    {label}
                  </button>
                ))}
              </span>
            )}
            <button onClick={() => { seek(0); if (audio.current && !along && playing) { route(); void audio.current.play(); } }}
                    disabled={head <= 1} title={t("take.player.backToStart")} aria-label={t("take.player.backToStart")}
                    className="grid h-8 w-8 place-items-center rounded-lg text-ink-300 transition hover:bg-white/6 hover:text-white disabled:opacity-30 disabled:hover:bg-transparent">
              <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M3 12a9 9 0 1 0 3-6.7" /><path d="M3 4v5h5" />
              </svg>
            </button>
            <span className="tabular text-sm text-ink-300">{clock(head)}<span className="text-ink-500"> / {clock(graphDur)}</span></span>
            {/* the recording waits, silent, until the song is placed (the same words as live singing's) */}
            {along && lining && !(mode && !mode.control && !joined) && (
              <span role="status" className="text-sm text-accent-200">{t("take.player.lining")}</span>
            )}
            <span className="ml-auto flex items-center gap-3">
              <label className="inline-flex items-center gap-2 text-ink-400" title={t("take.player.voiceTitle")}>
                <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <path d="M11 5 6 9H3v6h3l5 4z" /><path d="M15.5 8.5a5 5 0 0 1 0 7" /><path d="M18.5 5.5a9 9 0 0 1 0 13" />
                </svg>
                <span className="sr-only">{t("take.player.yourVoice")}</span>
                <input type="range" min={-6} max={12} step={1} value={voiceDb} onChange={e => changeVoiceDb(Number(e.target.value))}
                       aria-valuetext={voiceDb === 0 ? t("take.player.voiceNormal") : t("take.player.voiceDb", { db: `${voiceDb > 0 ? "+" : ""}${voiceDb}` })} className="w-20 accent-accent-400" />
                <span className="tabular w-9 text-xs">{voiceDb === 0 ? "" : t("take.player.voiceDb", { db: `${voiceDb > 0 ? "+" : ""}${voiceDb}` })}</span>
              </label>
              {dev && (
                <button onClick={() => setDevOpen(v => !v)} aria-expanded={devOpen} title="Development tools (not in releases)"
                        className={`rounded-md border border-dashed px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide transition ${devOpen || shiftMs !== 0 ? "border-fuchsia-400/70 bg-fuchsia-500/20 text-fuchsia-100" : "border-fuchsia-400/40 text-fuchsia-300/80 hover:text-fuchsia-100"}`}>
                  dev{shiftMs !== 0 ? ` ${shiftMs > 0 ? "+" : ""}${shiftMs}` : ""}
                </button>
              )}
              <span className="cursor-help text-ink-500 transition hover:text-ink-200" tabIndex={0}
                    title={mode && !mode.control ? t("take.player.aboutJoin", { song: r.track.name, app: appText(t, mode.app, mode.app_kind), ms: r.latency_ms })
                      : t("take.player.aboutPlay", { app: sourceName(r.track.id), ms: r.latency_ms })}>
                <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden>
                  <circle cx="12" cy="12" r="9" /><path d="M12 11v5" /><path d="M12 7.5v.01" />
                </svg>
              </span>
            </span>
            {dev && devOpen && (
              <div className="basis-full">
                <label title="Development tool (not in releases): line your voice up with the song by ear; the log keeps it next to the ear's gap"
                     className="inline-flex items-center gap-2 rounded-lg border border-dashed border-fuchsia-400/70 bg-fuchsia-500/10 px-2 py-1 text-xs text-fuchsia-200">
                <span className="rounded bg-fuchsia-500/30 px-1 text-[10px] font-semibold uppercase tracking-wide text-fuchsia-100">dev</span>
                Shift
                <input type="range" min={-500} max={500} step={10} value={shiftMs} onChange={e => changeShift(Number(e.target.value))}
                       onPointerUp={() => logUi(`dev shift: take ${id} voice ${shiftRef.current >= 0 ? "+" : ""}${shiftRef.current} ms (set by ear; + is later)`)}
                       onKeyUp={() => logUi(`dev shift: take ${id} voice ${shiftRef.current >= 0 ? "+" : ""}${shiftRef.current} ms (set by ear; + is later)`)}
                       className="w-28 accent-fuchsia-400" />
                <span className="tabular w-20">{shiftMs === 0 ? "0" : `${shiftMs > 0 ? "+" : ""}${shiftMs} ms`}{shiftMs > 0 ? " later" : shiftMs < 0 ? " earlier" : ""}</span>
                <button onClick={e => {
                          e.preventDefault(); const on = !synced; markSynced(on);
                          logUi(`dev shift: take ${id} ${on ? "marked in sync" : "unmarked"} at ${shiftRef.current >= 0 ? "+" : ""}${shiftRef.current} ms${along ? "" : " (not playing with the song)"}`);
                        }}
                        title={synced ? "Marked: it sounds in sync at this shift (click to unmark)" : "Mark it: it sounds in sync at this shift"}
                        aria-pressed={synced}
                        className={`rounded px-1.5 py-0.5 font-semibold transition ${synced ? "bg-fuchsia-400 text-ink-950" : "bg-fuchsia-500/20 text-fuchsia-200 hover:bg-fuchsia-500/35"}`}>✓</button>
              </label>
              </div>
            )}
          </div>
        )}
        {along && mode && !mode.control && !joined && (
          <div className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-xl bg-accent-500/10 px-3 py-2 text-sm text-accent-100">
              <span>{t(joinFrom < 5 ? "take.join.playFromStart" : "take.join.play", { song: r.track.name, app: appText(t, mode.app, mode.app_kind) })}</span>
              {joinFrom >= 5 && <span className="text-accent-200/80">{t("take.join.goTo", { time: clock(joinFrom), app: appText(t, mode.app, mode.app_kind) })}</span>}
              {mode.key && (
                <button onClick={async () => { await allowControl(mode.key!); const m = await alongMode(r.track.id); setMode(m); stopAlong(); }}
                        className="text-xs text-accent-300 underline-offset-2 hover:underline">{t("take.join.letStart")}</button>
              )}
            </div>
        )}
        {sharing && r.song_offset != null && (
          <div className="mb-3">
            <SharePanel id={id} song={r.track.name} t={head} onClose={() => setSharing(false)}
                        onShow={g => { if (!playing && !along) { heardFrom.current = g; setHead(g); } }}
                        onPlay={g => {
                          if (along) stopAlong();
                          seek(g);
                          if (audio.current) { route(); void audio.current.play(); }
                          // just the clip: it stops after its 20 s
                          window.clearTimeout(clipStop.current);
                          clipStop.current = window.setTimeout(() => audio.current?.pause(), 20_000);
                        }} />
          </div>
        )}
        <PitchView r={r} head={head} playing={playing} onSeek={seek} clock={heardPos} leftOut={leftOut}
                   marking={marking} sel={sel} onSel={setSel} />
        <div className="mt-2 flex flex-wrap items-center justify-between gap-3 px-1 text-xs text-ink-400">
          <div className="flex flex-wrap items-center gap-4">
            <Key color={ACCENT[600]} label={t("take.graph.singer")} bar /><Key color={COLORS.good} label={t("take.graph.within50")} />
            <Key color={COLORS.close} label={t("take.graph.within100")} /><Key color={COLORS.off} label={t("take.graph.further")} />
            {/* the take's details beside the graph they describe: held notes light up green */}
            <span className="flex items-center gap-x-4 text-ink-300">
              {!!r.notes_total && <span title={t("take.graph.heldTitle")}>
                {t.rich("take.graph.held", { hit: r.notes_hit ?? "", total: r.notes_total, n: c => <span className="tabular text-ink-100">{c}</span> })}</span>}
              <span title={t("take.graph.biasTitle")}>
                {Math.abs(r.bias_cents) < 10 ? t("take.graph.centred") : t("take.graph.bias", { off: offBy(r.bias_cents) })}</span>
            </span>
          </div>
          {canMark && !marking && (
            <button onClick={() => { setMarking(true); setSel(null); setRescoredNote(""); }}
                    title={t("take.notMelody.buttonTitle")}
                    className="rounded-lg px-2.5 py-1 font-medium text-ink-300 transition hover:bg-white/6 hover:text-white">
              {t("take.notMelody.button")}
            </button>
          )}
        </div>
        {marking && (
          <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-xl bg-accent-500/10 px-3 py-2 text-sm text-accent-100">
            {sel ? (
              <>
                <span className="tabular">{t("take.notMelody.span", { from: clock(sel[0]), to: clock(sel[1]) })}</span>
                <button onClick={leaveOut} disabled={saving}
                        className="rounded-lg bg-accent-500 px-3 py-1 text-sm font-medium text-white transition hover:bg-accent-400 disabled:opacity-60">
                  {saving ? t("take.notMelody.saving") : t("take.notMelody.leaveOut")}
                </button>
              </>
            ) : (
              <span>{t("take.notMelody.drag")}</span>
            )}
            {/* how it goes, until you've left a part out once */}
            {!sel && !leftOutBefore() && <div className="order-last basis-full"><div className="mt-1 max-w-[300px]"><LeaveOutGuide /></div></div>}
            <button onClick={() => { setMarking(false); setSel(null); }} className="text-xs text-accent-300 underline-offset-2 hover:underline">{t("take.notMelody.cancel")}</button>
            <span className="basis-full text-xs text-accent-200/80">{t("take.notMelody.note")}</span>
          </div>
        )}
        {(leftOut.length > 0 || rescoredNote) && !marking && (
          <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-xs text-ink-400">
            {leftOut.length > 0 && <span>{t("take.notMelody.leftOut")}</span>}
            {songNm.map(([a, b], i) => {
              if (r.song_offset == null || b - r.song_offset <= 0 || a - r.song_offset >= graphDur) return null;
              return (
                <span key={`${a}-${b}`} className="flex items-center gap-1.5">
                  <span className="tabular text-ink-200">{t("take.notMelody.span", { from: clock(a - r.song_offset), to: clock(b - r.song_offset) })}</span>
                  <button onClick={() => putBack(i)} disabled={saving} className="text-accent-300 underline-offset-2 hover:underline disabled:opacity-60">{t("take.notMelody.putBack")}</button>
                </span>
              );
            })}
            {rescoredNote && <span>{rescoredNote}</span>}
          </div>
        )}
      </section>
      <div className="mt-3 flex items-center justify-end gap-4">
        <DevCopy label="Copy take link" text={() => takeLink("take", id, { t: head, song: r.song_offset != null ? songAt(head) : null })} />
        <button onClick={() => reportProblem(id)} className="text-xs text-ink-400 underline-offset-2 transition hover:text-white hover:underline">
          {t("take.report")}
        </button>
      </div>

      <section className="mt-8">
        <h2 className="mb-3 text-sm font-medium text-ink-200">{t("take.lines.title")}</h2>
        {r.lines.length === 0 ? <p className="text-ink-400">{lyricsOff ? t("take.lines.lyricsOff") : t("take.lines.none")}</p> : (
          <div className="glass divide-y divide-white/5 overflow-hidden rounded-3xl">
            {r.lines.map((l, i) => <LineRow key={i} n={i + 1} l={l} level={r.level} active={head >= l.t0 && head < l.t1} onClick={() => seek(l.t0)} />)}
          </div>
        )}
      </section>
    </Shell>
  );
}

/** You've left a part out before (localStorage): the "Not the melody?" animation isn't shown any more. */
const LEFT_OUT_ONCE = "onkey.leftOutOnce";
const leftOutBefore = () => { try { return localStorage.getItem(LEFT_OUT_ONCE) === "1"; } catch { return false; } };

// the normal way (notes shown) has no tag
function ModeBadge({ mode, blind }: { mode?: string; blind: boolean }) {
  const t = useT();
  const [label, why] = mode === "learn" ? [t("take.mode.firstPlay"), t("take.mode.firstPlayWhy")]
    : blind ? [t("take.mode.blind"), t("take.mode.blindWhy")] : [null, null];
  return label ? <span className="rounded-full bg-white/6 px-2 py-0.5 text-xs font-medium text-ink-200" title={why!}>{label}</span> : null;
}

function Shell({ children }: { children: React.ReactNode }) {
  const t = useT();
  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-6xl px-6 pb-16">
        <Link href="/history" className="text-sm text-ink-400 transition hover:text-white">{t("take.allTakes")}</Link>
        {children}
      </main>
    </div>
  );
}

function ScoreRing({ score }: { score: number }) {
  const R = 54, C = 2 * Math.PI * R;
  return (
    <div className="relative grid h-24 w-24 place-items-center md:h-[148px] md:w-[148px]">
      <svg viewBox="0 0 148 148" className="h-full w-full -rotate-90">
        <circle cx={74} cy={74} r={R} fill="none" stroke="rgba(255,255,255,.07)" strokeWidth={10} />
        <circle cx={74} cy={74} r={R} fill="none" stroke="url(#ring)" strokeWidth={10} strokeLinecap="round"
                strokeDasharray={`${(C * score) / 100} ${C}`} />
        <defs><linearGradient id="ring" x1="0" x2="1"><stop offset="0" stopColor={ACCENT[300]} /><stop offset="1" stopColor={ACCENT[600]} /></linearGradient></defs>
      </svg>
      <div className="absolute text-center">
        <div className="tabular text-3xl font-bold tracking-tight md:text-5xl">{score}</div>
        <div className="text-[10px] font-medium text-accent-300 md:text-xs">{scoreWord(score)}</div>
      </div>
    </div>
  );
}

/** The score from its parts, as the engine makes it (dsp::score): how in tune what you sang was, counted in full once
 *  you sing 40% of the singer's part, less below that. */
function ScoreStory({ r }: { r: Result }) {
  const t = useT();
  const sang = Math.round(100 * r.coverage), tune = Math.round(100 * r.accuracy);
  if (r.coverage <= 0) return <>{t("take.story.none")}</>;
  return <>{t.rich("take.story.main", { sang, tune, full: r.score >= tune ? "yes" : "no",
    n: c => <span className="tabular font-semibold text-ink-100">{c}</span> })}</>;
}

function Key({ color, label, bar }: { color: string; label: string; bar?: boolean }) {
  return <span className="flex items-center gap-1.5"><span className={bar ? "h-2 w-4 rounded-full" : "h-2 w-2 rounded-full"} style={{ background: color }} />{label}</span>;
}

// a click plays from the line: ▶ by its time on hover and keyboard focus, and on the line playing
function LineRow({ l, n, level, active, onClick }: { l: Line; n: number; level?: string; active: boolean; onClick: () => void }) {
  const t = useT();
  const scored = l.accuracy != null && l.ref_frames > 20 && l.coverage > 0.2;
  const b = l.bias == null ? 0 : Math.round(l.bias);
  const note = l.ref_frames <= 20 ? t("take.lines.noLead") : l.coverage <= 0.2 ? t("take.lines.notSung") : Math.abs(b) >= 25 ? offBy(b) : "";
  const col = !scored ? "" : COLORS[lineGrade(l.accuracy!, levelOf(level))];
  return (
    <button onClick={onClick} title={t("take.lines.playFrom", { time: clock(l.t0) })} aria-current={active || undefined}
            className={`group grid w-full grid-cols-[60px_1fr_minmax(80px,180px)_140px] items-center gap-4 px-5 py-3 text-left transition
                        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-400
                        ${active ? "bg-accent-500/15 shadow-[inset_3px_0_0_var(--color-accent-400)]" : "hover:bg-white/4"}`}>
      <span className={`tabular flex items-center gap-1.5 text-sm ${active ? "text-accent-200" : "text-ink-400 group-hover:text-ink-200"}`}>
        <svg viewBox="0 0 24 24" className={`h-2.5 w-2.5 shrink-0 text-accent-300 transition ${active ? "opacity-100" : "opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100"}`} fill="currentColor" aria-hidden>
          <path d="M7 4v16l13-8z" />
        </svg>
        {clock(l.t0)}
      </span>
      <span className={`truncate ${active ? "font-medium text-white" : "text-ink-100"}`}>{l.text || <span className="text-ink-400">{t("take.lines.line", { n })}</span>}</span>
      <span className="h-1.5 overflow-hidden rounded-full bg-white/6">
        {scored && <span className="block h-full rounded-full" style={{ width: `${100 * l.accuracy!}%`, background: col }} />}
      </span>
      <span className="text-right text-xs text-ink-300"><span className="tabular font-semibold text-ink-100">{scored ? `${Math.round(100 * l.accuracy!)}%` : ""}</span> {note}</span>
    </button>
  );
}

function verdict(t: Translator, r: Result) {
  const parts: string[] = [];
  // the words loosen with the level, as the line colours do (lineGrade): Pro credits less per note
  const lv = levelOf(r.level), d = lv === "pro" ? 0.2 : lv === "steady" ? 0.1 : 0, a = r.accuracy + d;
  parts.push(a > 0.85 ? t("take.verdict.great") : a > 0.7 ? t("take.verdict.mostly") :
             a > 0.5 ? t("take.verdict.often") : t("take.verdict.away"));
  if (Math.abs(r.bias_cents) >= 20) parts.push(t("take.verdict.bias", { off: offBy(r.bias_cents) }));
  const worst = r.lines.filter(l => l.accuracy != null && l.coverage > 0.3 && l.ref_frames > 30).sort((a, b) => a.accuracy! - b.accuracy!)[0];
  if (worst && worst.accuracy! + d < 0.75) parts.push(worst.text ? t("take.verdict.worstLine", { text: worst.text }) : t("take.verdict.worstAt", { time: clock(worst.t0) }));
  return parts.join(" ");
}

// ------------------------------------------------------------------ the pitch view
function PitchView({ r, head, playing, onSeek, clock, leftOut, marking, sel, onSel }: { r: Result; head: number; playing: boolean; onSeek: (s: number) => void;
                     clock: () => number; leftOut: Span[]; marking: boolean; sel: Span | null; onSel: (s: Span | null) => void }) {
  const t = useT();
  const notMelody = t("take.graph.notMelody");
  const cv = useRef<HTMLCanvasElement>(null), ov = useRef<HTMLCanvasElement>(null);
  const dt = r.frame_s, N = r.ref.length, dur = N * dt;
  const WIN = 12;
  // open where the singing starts (not on the intro)
  const [view0, setView0] = useState(() => {
    const first = r.usr.findIndex((u, i) => u != null && r.ref[i] != null);
    const sing = first >= 0 ? first : r.ref.findIndex(v => v != null);
    return Math.max(0, Math.min(dur - WIN, (Math.max(0, sing) * dt) - 1));
  });
  const range = useMemo(() => {
    const v = r.ref.filter((x): x is number => x != null).sort((a, b) => a - b);
    return v.length ? [Math.floor(v[Math.floor(v.length * 0.01)] - 3), Math.ceil(v[Math.floor(v.length * 0.99)] + 3)] : [48, 72];
  }, [r]);
  // follow playback (only while your take is playing, so browsing isn't yanked back)
  useEffect(() => {
    if (playing && (head > view0 + WIN * 0.8 || head < view0)) setView0(Math.max(0, Math.min(dur - WIN, head - WIN * 0.2)));
  }, [head, playing]); // eslint-disable-line

  useEffect(() => {
    const c = cv.current!, g = c.getContext("2d")!, dpr = devicePixelRatio || 1;
    const W = c.clientWidth, H = c.clientHeight;
    c.width = W * dpr; c.height = H * dpr; g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const FONT = getComputedStyle(document.body).fontFamily;
    const [lo, hi] = range, L = 40, B = 22, plotH = H - B;
    const x = (s: number) => L + ((s - view0) / WIN) * (W - L);
    const y = (m: number) => plotH - ((m - lo) / (hi - lo)) * (plotH - 6);
    g.clearRect(0, 0, W, H);
    for (let m = Math.ceil(lo); m <= hi; m++) {
      if (m % 12 !== 0 && m % 12 !== 7) continue;
      g.fillStyle = "rgba(255,255,255,.05)"; g.fillRect(L, y(m), W - L, 1);
      g.fillStyle = INK[400]; g.font = `11px ${FONT}`; g.textBaseline = "middle"; g.fillText(noteName(m), 4, y(m));
    }
    const f0 = Math.max(0, Math.floor(view0 / dt)), f1 = Math.min(N, Math.ceil((view0 + WIN) / dt));
    // singer's notes as bars, lit by how much of each you held
    const rowH = (plotH - 6) / (hi - lo);
    for (const [a, b, m, held] of r.notes ?? []) {
      if (b < f0 || a > f1) continue;
      const x0 = x(a * dt), x1 = x(b * dt), h = Math.max(5, rowH * 0.7);
      g.fillStyle = "#2a2f6b"; roundRect(g, x0, y(m) - h / 2, x1 - x0, h, h / 2); g.fill();
      g.fillStyle = held >= 0.5 ? "rgba(52,210,123,.35)" : "rgba(124,108,246,.35)";
      roundRect(g, x0, y(m) - h / 2, (x1 - x0) * Math.min(1, held), h, h / 2); g.fill();
    }
    // singer's exact pitch, faint
    g.strokeStyle = "rgba(181,168,255,.45)"; g.lineWidth = 1.5; g.beginPath();
    let pen = false;
    for (let i = f0; i < f1; i++) {
      const m = r.ref[i];
      if (m == null || (pen && Math.abs(m - (r.ref[i - 1] ?? m)) > 1.5)) { pen = false; if (m == null) continue; }
      if (pen) g.lineTo(x(i * dt), y(m)); else g.moveTo(x(i * dt), y(m));
      pen = true;
    }
    g.stroke();
    // you, coloured by closeness
    g.lineWidth = 3; g.lineCap = "round";
    for (let i = f0 + 1; i < f1; i++) {
      const a = r.usr[i - 1], b = r.usr[i], e = r.err[i];
      if (a == null || b == null || Math.abs(a - b) > 1.5) continue;
      g.strokeStyle = COLORS[grade(e == null ? NaN : e)];
      g.beginPath(); g.moveTo(x((i - 1) * dt), y(a)); g.lineTo(x(i * dt), y(b)); g.stroke();
    }
    // the song's stretches marked "Not the melody": dimmed, and left out of the score
    for (const [a, b] of leftOut) {
      if (b < view0 || a > view0 + WIN) continue;
      const x0 = Math.max(L, x(a)), x1 = Math.min(W, x(b));
      g.fillStyle = "rgba(11,13,24,.62)"; g.fillRect(x0, 0, x1 - x0, plotH);
      g.fillStyle = "rgba(255,255,255,.12)"; g.fillRect(x0, 0, 1, plotH); g.fillRect(x1 - 1, 0, 1, plotH);
      g.save(); g.beginPath(); g.rect(x0, 0, Math.max(0, x1 - x0 - 6), plotH); g.clip();
      g.fillStyle = INK[300]; g.font = `11px ${FONT}`; g.textBaseline = "top"; g.fillText(notMelody, x0 + 6, 6); g.restore();
    }
    // lyrics under the plot
    g.fillStyle = INK[300]; g.font = `12px ${FONT}`; g.textBaseline = "alphabetic";
    for (const l of r.lines) {
      if (l.t1 < view0 || l.t0 > view0 + WIN) continue;
      const xs = Math.max(L, x(l.t0));
      g.save(); g.beginPath(); g.rect(xs, plotH, Math.max(0, x(l.t1) - xs - 8), B); g.clip();
      g.fillText(l.text, xs + 2, H - 6); g.restore();
    }
  }, [r, view0, range, dt, N, leftOut.map(s => s.join()).join(), notMelody]); // eslint-disable-line

  // playhead on its own layer: while playing, redrawn every frame from the heard time (only this small canvas,
  // never the page: React re-rendering at 60 fps made playback stutter); otherwise at `t` (heard time too)
  useEffect(() => {
    const c = ov.current!, g = c.getContext("2d")!, dpr = devicePixelRatio || 1;
    const W = c.clientWidth, H = c.clientHeight;
    c.width = W * dpr; c.height = H * dpr; g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const draw = (at: number) => {
      g.clearRect(0, 0, W, H);
      if (sel) {
        const X = (s: number) => Math.max(40, Math.min(W, 40 + ((s - view0) / WIN) * (W - 40)));
        const x0 = X(Math.min(sel[0], sel[1])), x1 = X(Math.max(sel[0], sel[1]));
        g.fillStyle = "rgba(124,108,246,.22)"; g.fillRect(x0, 0, x1 - x0, H - 22);
        g.fillStyle = "rgba(181,168,255,.9)"; g.fillRect(x0, 0, 1.5, H - 22); g.fillRect(x1 - 1.5, 0, 1.5, H - 22);
      }
      const px = 40 + ((at - view0) / WIN) * (W - 40);
      if (px >= 40 && px <= W) { g.fillStyle = "rgba(255,255,255,.8)"; g.fillRect(px - 1, 0, 2, H - 22); }
    };
    draw(head);
    if (!playing) return;
    let raf = 0;
    const step = () => { draw(clock()); raf = requestAnimationFrame(step); };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [head, view0, playing, sel]); // eslint-disable-line

  // drag to scroll, click to seek; choosing a stretch ("Not the melody"), drag across it
  const drag = useRef<{ x: number; v: number; moved: boolean } | null>(null);
  const pick = useRef<number | null>(null);
  const at = (e: React.PointerEvent) => {
    const r0 = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.max(0, Math.min(dur, view0 + ((e.clientX - r0.left - 40) / (r0.width - 40)) * WIN));
  };
  return (
    <div>
      <div className={`relative h-[300px] select-none ${marking ? "cursor-col-resize" : "cursor-grab active:cursor-grabbing"}`}
           onPointerDown={e => {
             (e.target as HTMLElement).setPointerCapture(e.pointerId);
             if (marking) { pick.current = at(e); onSel(null); return; }
             drag.current = { x: e.clientX, v: view0, moved: false };
           }}
           onPointerMove={e => {
             if (marking) {
               if (pick.current == null) return;
               const g = at(e), a = pick.current;
               onSel(Math.abs(g - a) >= 0.2 ? [Math.min(a, g), Math.max(a, g)] : null);
               return;
             }
             if (!drag.current) return;
             const dx = e.clientX - drag.current.x;
             if (Math.abs(dx) > 3) drag.current.moved = true;
             const w = (e.currentTarget as HTMLElement).clientWidth - 40;
             setView0(Math.max(0, Math.min(dur - WIN, drag.current.v - (dx / w) * WIN)));
           }}
           onPointerUp={e => {
             if (marking) { pick.current = null; return; }
             if (drag.current && !drag.current.moved) {
               const r0 = (e.currentTarget as HTMLElement).getBoundingClientRect();
               onSeek(view0 + ((e.clientX - r0.left - 40) / (r0.width - 40)) * WIN);
             }
             drag.current = null;
           }}>
        <canvas ref={cv} className="absolute inset-0 h-full w-full" />
        <canvas ref={ov} className="pointer-events-none absolute inset-0 h-full w-full" />
      </div>
      {/* clicking the strip plays from there (and shows that part of the chart) */}
      <Overview r={r} view0={view0} win={WIN} leftOut={leftOut} onPick={s => { setView0(Math.max(0, Math.min(dur - WIN, s - WIN / 2))); onSeek(s); }} />
    </div>
  );
}

function Overview({ r, view0, win, leftOut, onPick }: { r: Result; view0: number; win: number; leftOut: Span[]; onPick: (s: number) => void }) {
  const N = r.ref.length, dur = N * r.frame_s, bins = 160;
  const outKey = leftOut.map(s => s.join()).join();
  const cells = useMemo(() => Array.from({ length: bins }, (_, b) => {
    let n = 0, s = 0, rv = 0;
    for (let i = Math.floor((b / bins) * N); i < Math.floor(((b + 1) / bins) * N); i++) {
      if (leftOut.some(([a, e]) => i * r.frame_s >= a && i * r.frame_s < e)) continue;     // not the melody: not scored
      if (r.ref[i] != null) rv++;
      const e = r.err[i];
      if (e != null) { n++; s += creditAt(e, levelOf(r.level)); }
    }
    return n ? s / n : rv ? -1 : null;
  }), [r, N, outKey]); // eslint-disable-line
  return (
    <div className="relative mt-3 h-8 cursor-pointer" onClick={e => { const rc = e.currentTarget.getBoundingClientRect(); onPick(((e.clientX - rc.left) / rc.width) * dur); }}>
      <div className="flex h-full items-center gap-px">
        {cells.map((c, i) => (
          <div key={i} className="flex-1 rounded-sm" style={{
            height: c == null ? 0 : c < 0 ? 6 : 26,
            background: c == null ? "transparent" : c < 0 ? "rgba(255,255,255,.08)" : COLORS[lineGrade(c, levelOf(r.level))],
            opacity: c != null && c >= 0 ? 0.85 : 1,
          }} />
        ))}
      </div>
      <div className="pointer-events-none absolute inset-y-0 rounded-md border border-accent-300/70" style={{ left: `${(view0 / dur) * 100}%`, width: `${(win / dur) * 100}%` }} />
    </div>
  );
}

function roundRect(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  r = Math.min(r, w / 2, h / 2);
  g.beginPath(); g.moveTo(x + r, y); g.arcTo(x + w, y, x + w, y + h, r); g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r); g.arcTo(x, y, x + w, y, r); g.closePath();
}
