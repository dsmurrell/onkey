"use client";
// The live mirror's badge (development builds; see src-tauri/src/mirror.rs): says this tab only watches the
// running app, shows the song time and lyric line it draws and how late events reach it, and turns every
// control into a no-op with a tooltip. The readout is also in data attributes, for agents reading the page:
//   #mirror-badge[data-song-time] (s), [data-lyric-index] (-1: none yet), [data-lag-ms] (average), [data-connected]
import { useEffect, useRef, useState } from "react";
import { live, mirror, useSession } from "@/lib/session";

const TIP = "Live mirror (read-only): use the OnKey app to change anything";

export default function MirrorBadge() {
  const [on, setOn] = useState(false);
  useEffect(() => setOn(mirror()), []);
  useEffect(() => {
    if (!on) return;
    live.start();
    document.documentElement.classList.add("mirror");
    // controls do nothing here: clicks and keys stop before the page sees them; links (pages) still work
    const control = (t: EventTarget | null) => t instanceof Element
      ? t.closest("button, input, select, textarea, [role=button], [role=switch], [role=slider]") : null;
    const block = (e: Event) => {
      const c = control(e.target);
      if (!c || c.closest("#mirror-badge")) return;
      e.preventDefault(); e.stopPropagation();
      (c as HTMLElement).title = TIP;
    };
    const mark = (e: Event) => { const c = control(e.target); if (c && !c.closest("#mirror-badge")) (c as HTMLElement).title = TIP; };
    const keys = (e: KeyboardEvent) => { if (!e.metaKey && !e.ctrlKey) { e.preventDefault(); e.stopPropagation(); } };
    for (const ev of ["click", "pointerdown", "mousedown", "change", "input", "submit"]) document.addEventListener(ev, block, true);
    document.addEventListener("pointerover", mark, true);
    window.addEventListener("keydown", keys, true);
    return () => {
      for (const ev of ["click", "pointerdown", "mousedown", "change", "input", "submit"]) document.removeEventListener(ev, block, true);
      document.removeEventListener("pointerover", mark, true);
      window.removeEventListener("keydown", keys, true);
    };
  }, [on]);
  return on ? <Badge /> : null;
}

function Badge() {
  const { connected } = useSession();
  const ref = useRef<HTMLDivElement>(null);
  const [text, setText] = useState("");
  useEffect(() => {
    const id = setInterval(() => {
      const el = ref.current;
      if (!el) return;
      const t = live.song ? live.songTime(live.frozenPerf ?? performance.now()) : NaN;
      const i = live.song ? live.lyricIndex(t) : -1, lag = live.mirrorLag.avg;
      el.dataset.songTime = Number.isFinite(t) ? t.toFixed(3) : "";
      el.dataset.lyricIndex = String(i);
      el.dataset.lagMs = Number.isFinite(lag) ? lag.toFixed(1) : "";
      setText([Number.isFinite(t) ? `${t.toFixed(2)} s` : "no song", i >= 0 ? `line ${i}` : "",
               Number.isFinite(lag) ? `lag ${lag.toFixed(1)} ms` : ""].filter(Boolean).join(" · "));
    }, 100);
    return () => clearInterval(id);
  }, []);
  return (
    <div id="mirror-badge" ref={ref} data-connected={connected ? "1" : "0"} title={TIP}
         className="pointer-events-auto fixed bottom-3 right-3 z-[1000] flex items-center gap-2 rounded-full border border-white/10 bg-black/70 px-3 py-1.5 text-xs text-ink-200 shadow-lg backdrop-blur">
      <span className={`h-2 w-2 rounded-full ${connected ? "bg-good" : "bg-off"}`} />
      <span className="font-semibold">Live mirror (read-only)</span>
      {text && <span className="tabular text-ink-400">{text}</span>}
    </div>
  );
}
