"use client";
import { useEffect, useState } from "react";
import Link from "next/link";
import Lane, { KEYS_W, NOW_AT } from "@/components/Lane";
import Dancer, { useDancer } from "@/components/Dancer";
import Cover, { SourceLinks } from "@/components/Cover";
import Setup from "@/components/Setup";
import StarterSongs, { StarterSheet, useStartersHidden } from "@/components/StarterSongs";
import VoiceBreak from "@/components/VoiceBreak";
import DemoSong from "@/components/DemoSong";
import RecentSongs from "@/components/RecentSongs";
import { offerDemo } from "@/lib/demo";
import Heart from "@/components/Heart";
import GuideFade from "@/components/GuideFade";
import { DevCopy, logTime, takeLink } from "@/components/Dev";
import { SongDevTools } from "@devhooks/hooks";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import { FinishSong, PausedFinish, SingSwitches, Icon, Lyrics, ScoreAndHint, SkipToSinging, SongClock, StatusChips, TakeToast, Transport, useTick } from "@/components/LiveBits";
import { errorText } from "@/lib/dataSafety";
import { getLibrary, inApp, learnAnyway, isFavorite, learnPct, learnStage, live, openApp, openInApp, clearStarting, useStarting, openPrivacySettings, playerNow, renameTrack, setFavorite, setListeningPaused, setMini, setupOpenSettings, setupStatus, setYouTube, usePlaying, useSession, type LearnJob, type Line, type SongState, type Status } from "@/lib/session";
import type { StartingView } from "@/lib/starting";
import { typing } from "@/lib/keys";
import { clock, sourceName } from "@/lib/music";

export default function Sing() {
  useEffect(() => { live.start(); }, []);
  const { song: playing, connected, status, ended, takes, problem, score, line, learning, drowned, notSong, blind } = useSession();
  // listening mode, the song changed (or isn't named yet): the old one goes at once, the status line says what's happening
  const finding = !!line && line.kind !== "learning";
  // a song just started from the Library, History or a take: shown as starting in its app at once, until it plays
  // (never the song before: its lane, or its score once it ended), or why the app couldn't start it
  const start = useStarting(playing?.version ?? null, finding);
  // listening paused: nothing is followed, so the page shows the pause (Waiting), not the song that was open, its
  // lane or "Session finished"
  const paused = !!status.listening_paused;
  const song = finding || start || paused ? null : playing;
  // the app the song plays in (listening mode: the one heard; a Shazam match's id doesn't say)
  // the app it opens in when it was kept by its ISRC (as "Open in your music app" does), else its id's
  const [opensIn, setOpensIn] = useState<{ id: string; app: string } | null>(null);
  const songId = song?.track.id;
  useEffect(() => {
    if (!songId || !inApp()) return;
    let gone = false;
    openApp(songId).then(app => { if (!gone && app) setOpensIn({ id: songId, app }); }).catch(() => {});
    return () => { gone = true; };
  }, [songId]);
  const source = status.player?.app && status.player.app !== "your music app" ? status.player.app
    : song ? (opensIn?.id === song.track.id ? opensIn.app : sourceName(song.track.id)) : "";
  const [mini, setMiniState] = useState(false);
  const [app, setApp] = useState(false);              // known only in the browser (the page is prerendered)
  useEffect(() => setApp(inApp()), []);
  // first-run setup: opens by itself until it's been finished (in the browser, with ?setup)
  const [setup, setSetup] = useState(false);
  useEffect(() => {
    if (new URLSearchParams(location.search).has("setup")) { setSetup(true); return; }
    if (inApp()) setupStatus().then(s => { if (!s.done) setSetup(true); }).catch(() => {});
  }, []);
  const toggleMini = (on: boolean) => { setMiniState(on); void setMini(on); };
  // the song's heart (F)
  const [fav, setFav] = useState<{ id: string; on: boolean } | null>(null);
  const track = song?.track;
  useEffect(() => {
    if (!track) return;
    let gone = false;
    isFavorite(track).then(on => { if (!gone) setFav({ id: track.id, on }); }).catch(() => {});
    return () => { gone = true; };
  }, [track?.id]);   // eslint-disable-line react-hooks/exhaustive-deps
  const favOn = !!track && fav?.id === track.id && fav.on;
  const toggleFav = (on: boolean) => { if (!track) return; setFav({ id: track.id, on }); void setFavorite(track, on); };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (typing(e) || e.metaKey || e.ctrlKey) return;
      if (e.key === "m" || e.key === "M") toggleMini(!mini);
      else if ((e.key === "f" || e.key === "F") && track) toggleFav(!favOn);
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  });
  if (mini) return <Mini onExpand={() => toggleMini(false)} />;
  return (
    <div className="flex h-dvh flex-col overflow-clip bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      {/* overflow-clip (not hidden) and minmax(0, ...) tracks: nothing can widen the page or scroll it sideways,
          so the lane, lyrics and controls stay centred in the window */}
      <VoiceBreak />
      <StarterSheet />
      {/* the left column is never narrower than the logo and menu (max-content), so they're never cut off on a narrow
          window: the middle (status line, or the song) gives way instead */}
      <header data-tauri-drag-region className="app-header grid grid-cols-[minmax(max-content,1fr)_minmax(0,auto)_minmax(0,1fr)] items-center gap-4 px-6 pt-4 pb-3">
        <div className="flex items-center gap-3 lg:gap-5"><Logo /><Nav /></div>
        <div className="min-w-0 text-center">
          {!song && line && <StatusLine line={line} />}
          {!song && <LearningLine jobs={learning} className={line ? "mt-1.5" : ""} />}
          {song && (
            <div key={song.version} className="flex animate-rise items-center justify-center gap-3">
              {/* (as tall as the two lines of text beside it) */}
              <Cover name={song.track.name} art={song.track.art} link={song.track.link} size={44} eager />
              <div className="min-w-0 text-left">
                <div className="flex items-center gap-1.5">
                  <span className="truncate text-[15px] font-semibold tracking-tight">{song.track.name}</span>
                  <Heart on={favOn} onToggle={toggleFav} size={14} className="-my-1.5 h-6 w-6" />
                  {song.track.id.startsWith("youtube:") && <RenameVideo id={song.track.id} artist={song.track.artist} name={song.track.name} />}
                </div>
                <div className="flex items-center gap-2 truncate text-sm text-ink-400">
                  {song.track.artist}<span className="text-ink-600">·</span><SongClock />
                  {/* the app it plays from, as the link to the song there; then the other service, dimmer */}
                  <span className="text-ink-600">·</span><SourceLinks source={source} artist={song.track.artist} name={song.track.name} appleLink={song.track.link} trackId={song.track.id} />
                </div>
                {/* (no "Learning: sing along…" line here: the lane's own card says it, and how the first play is scored) */}
                {/* (earlier songs still being learned aren't shown over a playing song: it crowded the header; the
                    Library shows their progress) */}
              </div>
            </div>
          )}
        </div>
        <div className="flex items-center justify-end gap-4">
          {/* development tools here, apart from the title, so they never move what people see */}
          {song && (
            <span className="flex items-center gap-1.5">
              <DevCopy label="Copy link" text={async () => {
                const n = await playerNow().catch(() => null);
                return takeLink("live", song.version, { song: n?.position, track: song.track.id, mode: song.mode, at: logTime() });
              }} />
              <SongDevTools track={song.track.id} mode={song.mode} />
            </span>
          )}
          <WhatsNew />
          {song && <ScoreAndHint />}
          {app && (
            <button onClick={() => toggleMini(true)} title="Mini mode: a small lyrics bar that stays on top of other apps (M)"
                    className="grid h-9 w-9 place-items-center rounded-full text-ink-300 transition hover:bg-white/8 hover:text-white">
              <Icon d="M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM12.5 11.5h5.5v4.5h-5.5z" size={18} />
            </button>
          )}
        </div>
      </header>

      <main className="relative mx-4 min-h-0 flex-1 overflow-clip rounded-3xl border border-white/6 bg-ink-900 shadow-2xl shadow-black/40">
        {!finding && !start && !paused && <Lane />}
        {!ended && !finding && !start && !paused && <Dancer />}
        {song && !ended && <SkipToSinging />}
        {song && !ended && <FinishSong />}
        {song && !ended && <PausedFinish />}
        <GuideFade song={song} blind={blind} ended={!!ended && !paused} />
        {song?.mode === "learn" && <Learning key={song.version} />}
        {!song && <Waiting start={start} connected={connected} line={line} player={status.player} youtube={!!status.youtube} listening={!!status.listening} paused={!!status.listening_paused} />}
        {!setup && <Notices status={status} song={song} />}
        {!setup && song?.mode === "sing" && !ended && drowned && <SpeakerHint key={song.version} id={song.track.id} />}
        {!setup && notSong && <NotASong key={notSong.track_id} id={notSong.track_id} name={notSong.name} />}
        {ended && !finding && !start && !paused && <Ended takes={takes} />}
        {(problem === "permission" || status.audio === "denied") && !setup && <NeedsPermission app={status.player?.app ?? "your music app"} />}
      </main>
      {setup && <Setup onClose={() => setSetup(false)} />}

      <section className="pt-5 pb-3">{song && <Lyrics />}</section>

      <footer className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-end gap-4 px-6 pb-5">
        <StatusChips />
        <Transport />
        <div className="flex min-w-0 items-end justify-end gap-2"><TakeToast />{!ended && !paused && <SingSwitches />}</div>
      </footer>
    </div>
  );
}

function Learning() {
  useTick(500);
  const playing = usePlaying();     // the dot pulses only while the song plays
  const song = live.song!;
  const perf = live.frozenPerf ?? performance.now();
  const t = live.songTime(perf);
  const d = song.track.duration;     // 0 when unknown (YouTube can say so): no bar, and the time without a length
  const pct = d > 0 ? Math.max(0, Math.min(1, t / d)) : null;
  const dot = <span className="relative flex h-2 w-2 shrink-0"><span className={`absolute inline-flex h-full w-full rounded-full bg-accent-400 opacity-60 motion-reduce:hidden ${playing ? "animate-ping" : "hidden"}`} /><span className="relative inline-flex h-2 w-2 rounded-full bg-accent-400" /></span>;
  const bar = (h: string) => pct != null && (
    <div className={`${h} overflow-hidden rounded-full bg-white/9`}>
      <div className="h-full rounded-full bg-gradient-to-r from-accent-400 to-accent-600 transition-[width] duration-500" style={{ width: `${pct * 100}%` }} />
    </div>
  );
  // in the future side of the lane (right of the now line), where nothing is known yet
  return (
    <div className="pointer-events-none absolute inset-y-0 right-0" style={{ left: `calc(${KEYS_W}px + (100% - ${KEYS_W}px) * ${NOW_AT})` }}>
      <div className={`frost absolute top-1/2 left-1/2 w-[min(calc(100%-40px),300px)] rounded-[22px] px-[18px] pt-4 pb-3.5 text-center -translate-x-1/2 -translate-y-1/2`}>
        <div className="mx-auto mb-2.5 flex w-fit items-center gap-2 rounded-full bg-accent-500/14 px-3 py-1 text-xs font-medium text-accent-300">{dot}Learning this song</div>
        <h2 className="text-base font-semibold tracking-tight">Your blind take</h2>
        <p className="mt-1 text-[12.5px] leading-snug text-ink-200/85">Sing along, no hints. Next time you’ll see every note coming.</p>
        {bar("mt-3 h-1")}
        <div className="mt-2 text-[11.5px] text-ink-300"><span className="tabular">{clock(t)}{d > 0 && ` / ${clock(d)}`}</span> · score after the song</div>
      </div>
    </div>
  );
}

/** Listening mode's status line, in the header while no song is shown: what OnKey is doing about the sound. */
function StatusLine({ line }: { line: Line }) {
  const busy = line.kind === "finding" || line.kind === "new_song";
  return (
    <div key={line.text} role="status" aria-live="polite"
         className={`mx-auto flex w-fit animate-rise items-center gap-2 rounded-full px-3 py-1 text-xs font-medium ${busy ? "bg-accent-500/12 text-accent-300" : "bg-white/6 text-ink-300"}`}>
      {busy && <span className="relative flex h-2 w-2"><span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent-400 opacity-60 motion-reduce:animate-none" /><span className="relative inline-flex h-2 w-2 rounded-full bg-accent-400" /></span>}
      {line.text}
    </div>
  );
}

/** Songs whose play is over still being learned (the status line's learning part): which, what it's doing and how
 *  far it's got, as a thin bar that moves only when the engine reports (see LearnBar). The song being learned is
 *  the one shown; one waiting behind it (one is learned at a time) is only counted. */
function LearningLine({ jobs, className = "" }: { jobs: LearnJob[]; className?: string }) {
  const after = jobs.filter(j => j.fraction != null);
  if (!after.length) return null;
  const j = after.find(a => a.stage !== "queued") ?? after[0], pct = learnPct(j);
  return (
    <div role="status" aria-live="polite" className={`mx-auto w-72 max-w-full text-left text-xs ${className}`}
         title={after.length > 1 ? after.map(a => a.name).join(", ") : undefined}>
      <div className="flex items-baseline gap-1.5">
        {pct == null
          ? <span className="truncate text-ink-300">Waiting to learn <span className="text-ink-100">{j.name}</span></span>
          : <span className="truncate text-ink-300">Learning <span className="text-ink-100">{j.name}</span>: {learnStage(j).toLowerCase()}</span>}
        <span className="tabular ml-auto shrink-0 text-ink-400">{after.length > 1 ? `+${after.length - 1}` : ""}{after.length > 1 && pct != null ? " · " : ""}{pct != null ? `${pct}%` : ""}</span>
      </div>
      <div className="mt-1 h-0.5 overflow-hidden rounded-full bg-white/8">
        {pct != null && <div className="h-full rounded-full bg-accent-400 transition-[width] duration-500 ease-out motion-reduce:transition-none" style={{ width: `${Math.max(2, pct)}%` }} />}
      </div>
    </div>
  );
}

/** What the status line means, said under it while waiting. */
const LINE_SUB: Record<Line["kind"], string> = {
  finding: "OnKey is naming the song from its sound. It takes a few seconds, and the take starts from the song’s beginning.",
  new_song: "OnKey is naming the next song. Its take starts from the song’s beginning.",
  unnamed: "It isn’t one OnKey or Shazam knows. OnKey asks again when the sound changes.",
  no_shazam: "Shazam can’t be reached just now, so only songs learned on this Mac are recognised. OnKey keeps trying.",
  shazam_off: "Shazam is off (Settings), so from its sound alone OnKey recognises only songs already learned on this Mac. Turn Shazam on to name new ones.",
  podcast: "Play a song and OnKey follows along.",
  learning: "",
  stopped: "You stopped learning it, so OnKey leaves it be. Play another song and OnKey follows along.",
  elsewhere: "OnKey hears what plays on this Mac. If it's playing on another device (your phone, a speaker), switch it to this Mac in the app's device menu.",
};

/** First run (nothing learned yet) and still no song after this long: suggest songs to try, whatever the line says. */
const STARTERS_AFTER_MS = 45_000;

function Waiting({ start, connected, line, player, youtube, listening, paused }: { start: StartingView | null; connected: boolean; line: Line | null; player: null | { state: string; name: string; artist: string }; youtube: boolean; listening: boolean; paused: boolean }) {
  const [busy, setBusy] = useState(false);
  const said = line && line.kind !== "learning" ? line : null;
  // songs to try: while nothing plays, and on a first run when OnKey has heard no song for a while
  const [startersHidden, hideStarters] = useStartersHidden();
  const [firstRun, setFirstRun] = useState(false);
  // the demo song ("Try it now"), while you have no songs of your own
  const [demo, setDemo] = useState(false);
  const [waited, setWaited] = useState(false);
  useEffect(() => {
    let gone = false;
    getLibrary().then(l => { if (!gone) { setFirstRun(l.length === 0); setDemo(offerDemo(l)); } }).catch(() => {});
    const t = setTimeout(() => setWaited(true), STARTERS_AFTER_MS);
    return () => { gone = true; clearTimeout(t); };
  }, []);
  if (connected && paused) return <ListeningPaused />;
  const idle = !said && player?.state !== "playing" && !start;
  const starters = connected && startersHidden === false && (idle || (firstRun && waited && !start));
  const title = !connected ? "Starting OnKey…" : start ? start.title : said ? (said.kind === "finding" || said.kind === "new_song" ? "Getting ready…" : said.kind === "podcast" ? "Not a song" : said.kind === "elsewhere" || said.kind === "stopped" ? said.text : "Still listening") : player?.state === "playing" ? "Getting ready…"
    : "Play a song and sing along";
  const sub = !connected
    ? "Connecting to the engine on this Mac."
    : start
      ? start.sub
    : said
      ? LINE_SUB[said.kind]
      : player?.state === "playing"
        ? `Listening to ${player.name}. The lane appears in a moment.`
        : `In Spotify${youtube ? ", Apple Music or YouTube" : " or Apple Music"}. OnKey follows along, and every song becomes a take you can review.`;
  return (
    // my-auto, not place-items-center: when it's taller than the frame it scrolls from its top, never cut off there
    <div className="absolute inset-0 flex flex-col items-center overflow-y-auto p-6">
      <div className="my-auto flex w-full max-w-3xl flex-col items-center py-4">
      <div className="max-w-lg text-center animate-rise">
        <div className="mx-auto mb-5 grid h-16 w-16 place-items-center rounded-2xl bg-accent-500/12 text-accent-300">
          <svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden>
            <path d="M9 18V6l11-2v12M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z" />
          </svg>
        </div>
        <h1 className="text-3xl font-semibold tracking-tight">{title}</h1>
        <p className="mt-2 text-balance text-[15px] text-ink-300">{sub}</p>
        {start?.failed && (
          <div className="mt-5 flex justify-center gap-2">
            <button onClick={() => { void openInApp(start.id); clearStarting(); }}
                    className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400">
              Open in {start.app}</button>
            <button onClick={clearStarting} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">Not now</button>
          </div>
        )}
        <div className="mt-6 flex flex-wrap justify-center gap-x-4 gap-y-2 text-xs text-ink-400">
          <span><Kbd>Space</Kbd> play / pause</span><span><Kbd>R</Kbd> start over</span><span><Kbd>←</Kbd><Kbd>→</Kbd> skip</span><span><Kbd>F</Kbd> favourite</span>
        </div>
      </div>
      {/* your last songs first, then the demo and songs to try */}
      {connected && idle && <RecentSongs className="mt-10 animate-rise" />}
      {connected && idle && demo && <DemoSong className="mt-8 animate-rise" />}
      {starters && <StarterSongs onHide={() => hideStarters(true)} className="mt-8 animate-rise" />}
      {connected && idle && startersHidden && (
        <button onClick={() => hideStarters(false)} className="mt-6 text-xs text-ink-500 transition hover:text-ink-200">Songs to try</button>
      )}
      {/* the YouTube switch; when listening, only while it's off (songs in a browser aren't picked up until it's on) */}
      {connected && (!listening || !youtube) && (
        <div className="mt-8 w-full max-w-sm rounded-xl border border-white/6 px-4 py-3 text-left text-xs text-ink-400 animate-rise">
          <div className="flex items-center justify-between gap-3">
            <span className="font-medium text-ink-200">YouTube and YouTube Music <span className="ml-1 rounded-full bg-white/6 px-1.5 py-px text-[10px] font-normal text-ink-300">Experimental</span></span>
            <button disabled={busy} onClick={async () => { setBusy(true); await setYouTube(!youtube); setBusy(false); }}
                    className="shrink-0 rounded-lg bg-white/6 px-2.5 py-1 text-ink-100 transition hover:bg-white/10 disabled:opacity-50">
              {youtube ? "Turn off" : "Turn on"}</button>
          </div>
          <p className="mt-1.5">{youtube
            ? "On. macOS asks once to let OnKey control your browser, so it can read where the video is."
            : "Off, so songs playing in your browser aren’t picked up. Turn it on to sing along in Chrome, Safari, Brave or Edge: macOS asks once to let OnKey control the browser."}</p>
        </div>
      )}
      </div>
    </div>
  );
}

/** Listening paused (the Live chip, the header's pill, ⌘⇧P): nothing is followed, so no "Play a song…" here. */
function ListeningPaused() {
  return (
    <div className="absolute inset-0 grid place-items-center p-6">
      <div className="max-w-lg text-center animate-rise">
        <p className="text-[15px] text-ink-300">Listening is paused. Resume to sing along.</p>
        <button onClick={() => void setListeningPaused(false)} title="Resume listening (⌘⇧P)"
                className="mt-4 rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400">
          Resume</button>
      </div>
    </div>
  );
}

/** A YouTube video's artist and title are read from the video and can be wrong: fix them here (kept for that
 *  video, and its lyrics are looked up again). */
function RenameVideo({ id, artist, name }: { id: string; artist: string; name: string }) {
  const [open, setOpen] = useState(false);
  const [a, setA] = useState(artist);
  const [n, setN] = useState(name);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (!open) { setA(artist); setN(name); setError(null); } }, [open, artist, name]);
  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (!n.trim()) return;
    setBusy(true);
    setError(null);
    try { await renameTrack(id, a, n); setOpen(false); }
    catch (err) { setError(errorText(err, "Couldn’t save the name.")); }
    finally { setBusy(false); }
  }
  return (
    <span className="relative">
      <button onClick={() => setOpen(o => !o)} title="Wrong song name? Fix the artist and title (used to find the lyrics)"
              className="grid h-6 w-6 place-items-center rounded-full text-ink-500 transition hover:bg-white/8 hover:text-ink-100">
        <Icon d="M4 20h4l10.5-10.5a2.1 2.1 0 0 0-4-4L4 16v4zM13.5 6.5l4 4" size={13} />
      </button>
      {open && (
        <form onSubmit={save} onKeyDown={e => { if (e.key === "Escape") setOpen(false); }}
              className="glass absolute left-1/2 top-8 z-30 w-72 -translate-x-1/2 animate-rise rounded-2xl p-4 text-left shadow-2xl shadow-black/50">
          <div className="text-xs font-medium text-ink-200">Fix the song name</div>
          <p className="mt-0.5 text-xs text-ink-400">Read from the YouTube video. OnKey finds the lyrics by this name.</p>
          <label className="mt-3 block text-[11px] text-ink-400" htmlFor="rename-artist">Artist</label>
          <input id="rename-artist" value={a} onChange={e => setA(e.target.value)} autoFocus
                 className="mt-1 w-full rounded-lg border border-white/8 bg-white/4 px-2.5 py-1.5 text-sm text-ink-100 outline-none focus:border-accent-400" />
          <label className="mt-2 block text-[11px] text-ink-400" htmlFor="rename-title">Title</label>
          <input id="rename-title" value={n} onChange={e => setN(e.target.value)}
                 className="mt-1 w-full rounded-lg border border-white/8 bg-white/4 px-2.5 py-1.5 text-sm text-ink-100 outline-none focus:border-accent-400" />
          {error && <p role="alert" className="mt-2 text-xs text-off">{error}</p>}
          <div className="mt-3 flex justify-end gap-2">
            <button type="button" onClick={() => setOpen(false)} className="rounded-lg px-3 py-1.5 text-xs text-ink-300 transition hover:text-ink-100">Cancel</button>
            <button type="submit" disabled={busy || !n.trim()}
                    className="rounded-lg bg-accent-500 px-3 py-1.5 text-xs font-medium text-white transition hover:bg-accent-400 disabled:opacity-50">Save</button>
          </div>
        </form>
      )}
    </span>
  );
}

/** Something OnKey can't do until you change a setting, said with what to do: a permission macOS refused, the
 *  learning models missing, or a browser's JavaScript setting. Never a silent nothing. */
function Notices({ status, song }: { status: Status; song: SongState | null }) {
  const m = status.models;
  // the YouTube hint: gone once turned on (status follows within a few seconds), or for this browser when closed
  const [ytOn, setYtOn] = useState(false);
  const [ytClosed, setYtClosed] = useState<string | null>(null);
  const sounding = status.browser_sounding;
  useEffect(() => { if (status.youtube) setYtOn(false); }, [status.youtube]);
  const learning = !song || song.mode === "learn";
  const list = [
    (status.mic === "denied" || status.mic === "restricted") && (
      <Notice key="mic" title="OnKey can’t hear your microphone"
              action={status.mic === "denied" ? { label: "Open System Settings", run: () => setupOpenSettings("mic") } : undefined}>
        {status.mic === "denied"
          ? <>Your singing isn’t heard or scored until it can. In <b>System Settings → Privacy &amp; Security → Microphone</b>, turn on OnKey.</>
          : <>Your Mac’s settings don’t let OnKey use the microphone. Ask whoever manages this Mac.</>}
      </Notice>
    ),
    status.mic === "granted" && status.mic_device === null && (
      <Notice key="mic" title="OnKey can’t hear your microphone">
        No microphone is connected, so your singing isn’t heard or scored. Connect one, or choose one in{" "}
        <b>System Settings → Sound → Input</b>.
      </Notice>
    ),
    status.music_quiet && (
      <Notice key="music-quiet" title={`${status.music_quiet.app}’s volume is very low`}>
        OnKey can’t hear the song well{status.music_quiet.volume != null ? ` (${status.music_quiet.app} is at ${status.music_quiet.volume}%)` : ""}, so your
        singing isn’t scored. Turn {status.music_quiet.app} up and use your Mac’s volume instead.
      </Notice>
    ),
    status.automation_denied && (
      <Notice key="automation" title={`OnKey isn’t allowed to control ${status.automation_denied}`}
              action={{ label: "Open System Settings", run: () => setupOpenSettings("automation") }}>
        {status.listening ? "OnKey follows it by listening instead, and can’t start, pause or skip songs there." : "So it can’t tell what’s playing there."} In{" "}
        <b>System Settings → Privacy &amp; Security → Automation → OnKey</b>, turn on {status.automation_denied}{status.listening ? <>, or turn it off in OnKey’s Settings</> : null}.
      </Notice>
    ),
    !status.automation_denied && status.not_answering && (
      <Notice key="not-answering" title={`${status.not_answering} isn’t answering OnKey`}
              action={{ label: "Open System Settings", run: () => setupOpenSettings("automation") }}>
        macOS may be asking whether OnKey can control {status.not_answering}, in a window hidden behind others: look for
        it and choose <b>Allow</b>. Or turn {status.not_answering} on in <b>System Settings → Privacy &amp; Security →
        Automation → OnKey</b>. OnKey tries again in a minute.
      </Notice>
    ),
    m && m.state === "missing" && learning && (
      <Notice key="models" title="OnKey can’t learn new songs right now">
        Its learning models, which come inside the app, are missing or damaged{m.error ? ` (${m.error})` : ""}. Songs you’ve
        already learned still work. To fix it, download OnKey again and replace the app in Applications: your songs and
        takes are kept.
      </Notice>
    ),
    m && m.state === "broken" && (
      <Notice key="models-broken" title="OnKey can’t hear your singing right now">
        Its pitch model, which comes inside the app, couldn’t load{m.error ? ` (${m.error})` : ""}. To fix it, download
        OnKey again and replace the app in Applications: your songs and takes are kept.
      </Notice>
    ),
    !song && status.youtube_blocked && (
      <Notice key="youtube" title={`To sing along to YouTube, let OnKey read the video in ${status.youtube_blocked.browser}`}>
        In {status.youtube_blocked.browser}, choose <b>{status.youtube_blocked.setting}</b>. OnKey only reads the video’s position and the song’s title, nothing else on the page.
      </Notice>
    ),
    !song && !status.youtube && !ytOn && sounding && sounding.browser !== ytClosed && (sounding.works ? (
      <Notice key="youtube-off" title={`Playing YouTube or YouTube Music in ${sounding.browser}?`} onClose={() => setYtClosed(sounding.browser)}
              action={{ label: "Turn on YouTube", run: () => { setYtOn(true); void setYouTube(true).catch(() => setYtOn(false)); } }}>
        OnKey only picks up songs in your browser once YouTube is on. macOS then asks once to let OnKey control {sounding.browser}, so
        it can read where the video is.
      </Notice>
    ) : (
      <Notice key="youtube-off" title={`Playing YouTube or YouTube Music in ${sounding.browser}?`} onClose={() => setYtClosed(sounding.browser)}>
        OnKey can’t follow songs in {sounding.browser}. Play them in <b>Chrome, Safari, Brave or Edge</b> and turn on YouTube
        (here, or in Settings), or turn on <b>listening mode</b> in Settings, which works in any browser.
      </Notice>
    )),
  ].filter(Boolean);
  if (!list.length) return null;
  return <div className="absolute inset-x-0 bottom-6 z-10 flex flex-col items-center gap-3 px-6">{list}</div>;
}

/** A gentle hint, once per song in a session: over speakers, when the song has been drowning you out (the engine's
 *  `drowned`: for 20 s of your part the mic heard a voice, but nearly none of it counted as singing). Turning the
 *  music down or headphones lets your voice through; nothing about the score changes. It goes by itself after
 *  20 s, or with ×; "Don't show again" is remembered (localStorage onkey.hideSpeakerHint). */
const speakerHinted = new Set<string>();
const HIDE_SPEAKER_HINT = "onkey.hideSpeakerHint";
/** A YouTube video that sounded like talk, not a song: OnKey stopped learning it (and won't again), unless it's a
 *  song after all. Goes by itself after a while. */
function NotASong({ id, name }: { id: string; name: string }) {
  useEffect(() => {
    const t = setTimeout(() => live.dismissNotSong(), 30000);
    return () => clearTimeout(t);
  }, [id]);
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-6 z-10 flex justify-center px-6">
      <div role="status" className="glass pointer-events-auto w-full max-w-md animate-rise rounded-2xl px-4 py-3 shadow-2xl shadow-black/40">
        <div className="flex items-start justify-between gap-3">
          <p className="text-sm text-ink-200"><span className="text-ink-100">{name}</span> sounds like talking, not a song, so OnKey stopped learning it.</p>
          <button onClick={() => live.dismissNotSong()} aria-label="Dismiss" className="-mr-1 shrink-0 rounded-md px-1.5 text-ink-400 transition hover:text-ink-100">×</button>
        </div>
        <button onClick={() => void learnAnyway(id)} className="mt-1.5 text-xs text-ink-400 underline-offset-2 transition hover:text-ink-200 hover:underline">It’s a song: learn it next time it plays</button>
      </div>
    </div>
  );
}

function SpeakerHint({ id }: { id: string }) {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (speakerHinted.has(id)) return;
    try { if (localStorage.getItem(HIDE_SPEAKER_HINT) === "1") return; } catch {}
    speakerHinted.add(id);
    setShown(true);
  }, [id]);
  useEffect(() => {
    if (!shown) return;
    const t = setTimeout(() => setShown(false), 20000);
    return () => clearTimeout(t);
  }, [shown]);
  if (!shown) return null;
  const never = () => { try { localStorage.setItem(HIDE_SPEAKER_HINT, "1"); } catch {} setShown(false); };
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-6 z-10 flex justify-center px-6">
      <div role="status" className="glass pointer-events-auto w-full max-w-md animate-rise rounded-2xl px-4 py-3 shadow-2xl shadow-black/40">
        <div className="flex items-start justify-between gap-3">
          <p className="text-sm text-ink-200">OnKey can’t hear you over the music. If you’re singing, turn it down a little or use headphones for a fair score.</p>
          <button onClick={() => setShown(false)} aria-label="Dismiss" className="-mr-1 shrink-0 rounded-md px-1.5 text-ink-400 transition hover:text-ink-100">×</button>
        </div>
        <button onClick={never} className="mt-1.5 text-xs text-ink-400 underline-offset-2 transition hover:text-ink-200 hover:underline">Don’t show again</button>
      </div>
    </div>
  );
}

function Notice({ title, action, onClose, children }: { title: string; action?: { label: string; run: () => void }; onClose?: () => void; children: React.ReactNode }) {
  return (
    <div role="status" className="glass w-full max-w-lg animate-rise rounded-2xl px-5 py-4 shadow-2xl shadow-black/40">
      <div className="flex items-start justify-between gap-3">
        <div className="text-sm font-medium text-ink-100">{title}</div>
        <div className="flex shrink-0 items-start gap-1">
          {action && (
            <button onClick={action.run} className="shrink-0 rounded-lg bg-white/8 px-3 py-1.5 text-xs font-medium text-ink-100 transition hover:bg-white/12">
              {action.label}
            </button>
          )}
          {onClose && <button onClick={onClose} aria-label="Dismiss" className="-mr-1.5 rounded-md px-1.5 py-1 text-ink-400 transition hover:text-ink-100">×</button>}
        </div>
      </div>
      <p className="mt-1 text-sm text-ink-300 [&_b]:font-medium [&_b]:text-ink-100">{children}</p>
    </div>
  );
}

function Ended({ takes }: { takes: { name: string; artist: string; score: number | null; report: string | null }[] }) {
  return (
    <div className="absolute inset-0 grid place-items-center bg-ink-950/90 p-6">
      <div className="w-full max-w-md animate-rise text-center">
        <h2 className="text-2xl font-semibold tracking-tight">Session finished</h2>
        <p className="mt-1 text-ink-300">{takes.length ? "Here’s how you did." : "No takes long enough to score this time."}</p>
        <div className="mt-6 space-y-2 text-left">
          {takes.map((t, i) => (
            <Link key={i} href={t.report ? `/take?id=${encodeURIComponent(t.report)}` : "/history"}
                  className="glass flex items-center justify-between rounded-2xl px-4 py-3 transition hover:border-accent-400/40">
              <div className="min-w-0"><div className="truncate font-medium">{t.name}</div><div className="truncate text-sm text-ink-400">{t.artist}</div></div>
              <div className="tabular text-2xl font-bold">{t.score ?? "–"}</div>
            </Link>
          ))}
        </div>
      </div>
    </div>
  );
}

function Kbd({ children }: { children: React.ReactNode }) {
  return <kbd className="mx-0.5 rounded-md border border-white/10 bg-white/5 px-1.5 py-0.5 font-sans text-[11px] text-ink-200">{children}</kbd>;
}

/** macOS hasn't let OnKey hear other apps yet. */
function NeedsPermission({ app }: { app: string }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center bg-ink-950/85 px-6">
      <div className="glass w-full max-w-md animate-rise rounded-3xl px-7 py-6 shadow-2xl shadow-black/40">
        <h2 className="text-lg font-semibold tracking-tight">OnKey can’t hear {app} yet</h2>
        <p className="mt-2 text-sm leading-relaxed text-ink-300">
          To follow the song, OnKey listens to {app}’s audio on your Mac. The audio isn’t saved or sent; in listening mode a short fingerprint of it (not the audio) goes to Apple’s Shazam to name the song.
        </p>
        <ol className="mt-4 space-y-1.5 text-sm text-ink-200">
          <li>1. Open <span className="font-medium">System Settings → Privacy &amp; Security</span></li>
          <li>2. Choose <span className="font-medium">Screen &amp; System Audio Recording</span></li>
          <li>3. Under <span className="font-medium">System Audio Recording Only</span>, turn on OnKey</li>
        </ol>
        <button onClick={() => openPrivacySettings()}
                className="mt-5 w-full rounded-xl bg-accent-500 px-4 py-2.5 text-sm font-semibold text-white transition hover:bg-accent-400">
          Open System Settings
        </button>
      </div>
    </div>
  );
}

/** Mini mode: the current and next lyric line, your live score, a progress line and the dancer, on a small
 *  rounded card floating above other apps. Drag it anywhere (text doesn't catch the mouse, so the whole card
 *  is a handle); the button opens the full window again. */
function Mini({ onExpand }: { onExpand: () => void }) {
  const { song, score } = useSession();
  const dancerOn = useDancer();
  // the dancer can be hidden from this card alone (the full window's switch is separate), remembered
  const [miniDancer, setMiniDancerState] = useState(true);
  useEffect(() => { try { setMiniDancerState(localStorage.getItem("onkey-mini-dancer") !== "0"); } catch {} }, []);
  const setMiniDancer = (on: boolean) => { setMiniDancerState(on); try { localStorage.setItem("onkey-mini-dancer", on ? "1" : "0"); } catch {} };
  const canDance = dancerOn && song?.mode === "sing" && !!song?.beats?.at.length;
  const dancing = canDance && miniDancer;
  useEffect(() => {
    document.documentElement.classList.add("mini");
    return () => document.documentElement.classList.remove("mini");
  }, []);
  return (
    <div className="h-dvh p-1.5 select-none">
      <div data-tauri-drag-region
           className="relative flex h-full cursor-grab items-stretch gap-2 overflow-hidden rounded-2xl border border-white/10 bg-ink-900/95 py-2 pr-2 pl-4 shadow-xl shadow-black/50 active:cursor-grabbing">
        <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col justify-center gap-1.5">
          <div data-tauri-drag-region className="flex items-center gap-3">
            <div className="pointer-events-none min-w-0 flex-1">
              {song ? <Lyrics compact /> : <p className="text-sm text-ink-400">Play a song in Spotify or Apple Music</p>}
            </div>
            <div className="pointer-events-none text-right">
              <div className="tabular text-xl font-bold leading-none">{song?.mode === "learn" ? "–" : score ?? "–"}</div>
              <div className="text-[9px] uppercase tracking-wider text-ink-500">score</div>
            </div>
          </div>
        </div>
        {dancing && (
          <div className="group relative -my-2 w-[50px] shrink-0">
            <Dancer className="absolute inset-0 h-full w-full" />
            <button onClick={() => setMiniDancer(false)} title="Hide the dancer here"
                    className="absolute -top-0.5 -right-0.5 grid h-5 w-5 place-items-center rounded-full text-xs text-ink-400 opacity-0 transition group-hover:opacity-100 hover:bg-white/10 hover:text-white">×</button>
          </div>
        )}
        <div className="flex shrink-0 flex-col items-center justify-between">
          <button onClick={onExpand} title="Open the full window (M)"
                  className="grid h-7 w-7 place-items-center rounded-full text-ink-300 transition hover:bg-white/8 hover:text-white">
            <Icon d="M14 4h6v6M20 4l-8 8M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4" size={15} />
          </button>
          {canDance && !miniDancer && (
            <button onClick={() => setMiniDancer(true)} title="Show the dancer"
                    className="grid h-7 w-7 place-items-center rounded-full text-ink-400 transition hover:bg-white/8 hover:text-white">
              <Icon d="M12 4.5a1.5 1.5 0 1 0 0 .01M12 7v6M8 9l4 1 4-1M9 21l3-8 3 8" size={15} />
            </button>
          )}
        </div>
        {song && <MiniProgress />}
      </div>
    </div>
  );
}

/** How far through the song: a hairline along the mini card's bottom edge. */
function MiniProgress() {
  useTick(500);
  const song = live.song;
  const d = song?.track.duration ?? 0;
  if (!song || !d) return null;
  const f = Math.max(0, Math.min(1, live.songTime(live.frozenPerf ?? performance.now()) / d));
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-0 h-[3px] bg-white/5" title="Through the song">
      <div className="h-full bg-accent-400/70 transition-[width] duration-500 ease-linear" style={{ width: `${f * 100}%` }} />
    </div>
  );
}
