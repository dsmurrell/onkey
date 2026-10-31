"use client";
import { useEffect, useState } from "react";
import { shareTake, shareWindow, type ShareWindow } from "@/lib/session";
import SpeakersGuide from "@/components/guides/SpeakersGuide";

/** "Share": a 20-second vertical video of your voice and the notes, to post with the song added in Instagram from
 *  its own music library (OnKey adds no music: the voice is the take as recorded). Starts on a whole second of the song so it lines up. */
export default function SharePanel({ id, song, t, onClose, onShow, onPlay }:
    { id: string; song: string; t: number; onClose: () => void; onShow: (graphS: number) => void; onPlay: (graphS: number) => void }) {
  const [win, setWin] = useState<ShareWindow | null>(null);
  const [mode, setMode] = useState<"playhead" | "best">("playhead");
  const [noBest, setNoBest] = useState(false);
  const [state, setState] = useState<"idle" | "making" | "done" | "failed">("idle");
  const [msg, setMsg] = useState("");
  const [why, setWhy] = useState(false);
  // no clip could be found in this take (an older take kept without its notes, say): said, rather than "Finding…" for good
  const [noClip, setNoClip] = useState(false);
  useEffect(() => {
    setNoClip(false);
    shareWindow(id, t).then(w => { setWin(w); setNoClip(!w); }).catch(() => setNoClip(true));
  }, [id]); // eslint-disable-line react-hooks/exhaustive-deps
  // the chosen clip is shown on the take's graph
  useEffect(() => { if (win) onShow(win.graph); }, [win?.start]); // eslint-disable-line react-hooks/exhaustive-deps

  const fromPlayhead = async () => {
    const w = await shareWindow(id, t).catch(() => null);
    if (w) { setWin(w); setNoBest(false); setMode("playhead"); }
  };
  const best = async () => {
    const w = await shareWindow(id).catch(() => null);
    setNoBest(!w);
    if (w) { setWin(w); setMode("best"); }
  };
  const make = async () => {
    if (!win) return;
    setState("making"); setMsg("");
    try {
      const r = await shareTake(id, win.start);
      setState("done");
      setMsg(`Saved to Downloads. In Instagram, add ${song} and start it at ${r.at} (copied).`);
    } catch (e) {
      setState("failed");
      setMsg(`Couldn’t make the video: ${String(e)}`);
    }
  };

  return (
    <div className="mt-3 rounded-2xl border border-white/8 bg-white/4 px-4 py-3 text-sm">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <span className="text-ink-100">
          {win ? <><b className="tabular">{win.at}–{win.to}</b><span className="text-ink-400"> · {win.hits} of {win.notes} notes hit</span></> : noClip ? "This take can’t be shared" : "Finding the clip…"}
        </span>
        <span className="flex rounded-lg bg-white/6 p-0.5" role="radiogroup" aria-label="Which 20 seconds">
          {([["playhead", "From the playhead", fromPlayhead], ["best", "Best bit", best]] as const).map(([k, label, run]) => (
            <button key={k} role="radio" aria-checked={mode === k} onClick={() => void run()}
                    className={`rounded-md px-2.5 py-1 transition ${mode === k ? "bg-white/12 text-white" : "text-ink-300 hover:text-white"}`}>
              {label}
            </button>
          ))}
        </span>
        <button disabled={!win} onClick={() => win && onPlay(win.graph)} className="inline-flex items-center gap-1 text-ink-200 transition hover:text-white disabled:opacity-50">
          <svg viewBox="0 0 24 24" className="h-3.5 w-3.5" fill="currentColor" aria-hidden><path d="M8 5v14l11-7z" /></svg>
          Play it
        </button>
        <span className="ml-auto flex items-center gap-2">
          <button disabled={!win || state === "making"} onClick={() => void make()}
                  className="rounded-xl bg-accent-500 px-4 py-2 font-medium text-white shadow-lg shadow-accent-600/25 transition hover:bg-accent-400 disabled:opacity-60">
            {state === "making" ? "Making video…" : "Make video"}
          </button>
          <button onClick={onClose} className="px-1 text-ink-400 transition hover:text-white" aria-label="Close">✕</button>
        </span>
      </div>
      <p className={`mt-2 text-xs ${state === "failed" ? "text-red-300" : state === "done" ? "text-accent-200" : "text-ink-400"}`}>
        {msg || (noBest ? "No notes hit in this take, so there’s no best bit: it starts at the playhead."
          : <>Your voice as recorded, the notes and your score, with no music added. Add the song in Instagram from its own music library, starting at the time shown. Sung with the song on speakers? Then some of it is in your recording: headphone takes are best for sharing.{" "}
              <button onClick={() => setWhy(v => !v)} aria-expanded={why} className="text-ink-300 underline underline-offset-2 transition hover:text-white">{why ? "Hide" : "Why headphones?"}</button>{" "}
              Please don’t post screen recordings with the music playing.</>)}
      </p>
      {why && <div className="mt-3 max-w-[300px]"><SpeakersGuide /></div>}
    </div>
  );
}
