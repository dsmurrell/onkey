"use client";
// Listening mode: OnKey follows songs by listening and controls no app until you allow it. When you use a
// control (start a song, skip to the singing, pause from OnKey) for an app you haven't allowed, this asks:
// allow control of that app (macOS then asks once too), or open the song there and press play yourself.
import { useEffect, useState } from "react";
import { allowControl, goHome, openInApp, type ControlNeeded } from "@/lib/session";

export default function ControlPrompt() {
  const [ask, setAsk] = useState<ControlNeeded | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const on = (e: Event) => setAsk((e as CustomEvent<ControlNeeded>).detail);
    window.addEventListener("onkey-control-needed", on);
    return () => window.removeEventListener("onkey-control-needed", on);
  }, []);
  useEffect(() => {
    if (!ask) return;
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setAsk(null); };
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [ask]);
  if (!ask) return null;
  const browser = ask.key === "youtube";
  const canAllow = ask.key !== "";

  async function allow() {
    if (!ask) return;
    setBusy(true);
    await allowControl(ask.key);
    setAsk(null);
    setBusy(false);
    await ask.retry();
  }
  async function open() {
    if (!ask?.track) return;
    await openInApp(ask.track);
    const home = ask.home;
    setAsk(null);
    if (home) goHome();
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/65 p-6" onClick={() => setAsk(null)}>
      <div role="dialog" aria-modal="true" aria-labelledby="control-title" onClick={e => e.stopPropagation()}
           className="glass w-full max-w-md rounded-3xl p-6 shadow-2xl animate-rise">
        <h2 id="control-title" className="text-lg font-semibold tracking-tight">
          {canAllow ? `Let OnKey control ${ask.app}?` : `OnKey can't control ${ask.app}`}
        </h2>
        <p className="mt-2 text-sm text-ink-300">
          {canAllow
            ? `OnKey is listening to ${ask.app}, not controlling it. To start songs, skip to the singing and pause from OnKey, it needs your permission. macOS will ask you once too.`
            : `OnKey follows ${ask.app} by listening only. Play, pause and skip there.`}
        </p>
        {browser && (
          <p className="mt-2 rounded-xl bg-amber-400/10 px-3 py-2 text-xs text-amber-200">
            This is the bigger permission: OnKey can then read and control the YouTube tab in your browser, and listens to
            your browser’s sound, whatever it is (calls too): sound it doesn’t recognise may go to Shazam as a short
            fingerprint, never the audio. Your browser also needs “Allow JavaScript from Apple Events” turned on.
          </p>
        )}
        <div className="mt-5 flex flex-wrap justify-end gap-2">
          <button onClick={() => setAsk(null)} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">Not now</button>
          {ask.track && (
            <button onClick={open} className="rounded-xl bg-white/8 px-4 py-2 text-sm text-ink-100 transition hover:bg-white/12">
              Open in {browser ? "your browser" : ask.app}
            </button>
          )}
          {canAllow && (
            <button disabled={busy} onClick={allow}
                    className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400 disabled:opacity-50">
              Allow control
            </button>
          )}
        </div>
        <p className="mt-4 text-[11px] text-ink-500">You can change this any time in Settings.</p>
      </div>
    </div>
  );
}
