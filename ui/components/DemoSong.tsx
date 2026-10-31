"use client";
import { useEffect, useState } from "react";
import { demoInfo, playTrack } from "@/lib/session";
import { demoCredit, type DemoSong as Demo } from "@/lib/demo";

/** "Try it now": the demo song bundled with OnKey, its notes already learned, so a new user can sing and be scored
 *  at once (src-tauri/src/demo.rs). Shows nothing when this build has no demo song. `onStart`: after it starts
 *  (setup closes). */
export default function DemoSong({ onStart, className = "" }: { onStart?: () => void; className?: string }) {
  const [demo, setDemo] = useState<Demo | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let gone = false;
    demoInfo().then(d => { if (!gone) setDemo(d); }).catch(() => {});
    return () => { gone = true; };
  }, []);
  if (!demo) return null;
  const start = async () => {
    setBusy(true);
    try { await playTrack(demo.id, true, demo.name); onStart?.(); } finally { setBusy(false); }
  };
  return (
    <div className={`mx-auto w-full max-w-sm rounded-xl border border-accent-500/25 bg-accent-500/8 px-4 py-3 text-left ${className}`}>
      <div className="flex items-center justify-between gap-3">
        <span className="text-sm font-medium text-ink-100">Try it now: sing a demo song</span>
        <button onClick={start} disabled={busy}
                className="shrink-0 rounded-lg bg-accent-500 px-3 py-1.5 text-xs font-semibold text-white transition hover:bg-accent-400 disabled:opacity-50">
          {busy ? "Starting…" : "Try it now"}
        </button>
      </div>
      <p className="mt-1.5 text-xs text-ink-300">
        An example song so you can see how OnKey works. Your own songs need one play to learn first.
      </p>
      <p className="mt-1 text-[11px] text-ink-500">{demoCredit(demo)}</p>
    </div>
  );
}
