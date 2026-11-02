"use client";
import { useEffect, useState } from "react";
import { getSettings, live } from "@/lib/session";
import { useT } from "@/lib/i18n";

/** After this much singing in one sitting, a reminder to rest your voice; again after each further REPEAT_S. */
const BREAK_AFTER_S = 35 * 60;
const REPEAT_S = 30 * 60;

/** A quiet reminder to rest your voice after a long stretch of singing (the singing time is counted in
 *  live.countSinging; a 20-minute pause from singing resets it). Never blocks anything; off in Settings. */
export default function VoiceBreak() {
  const t = useT();
  const [on, setOn] = useState(false);
  const [next, setNext] = useState(BREAK_AFTER_S);
  const [show, setShow] = useState(false);
  useEffect(() => { getSettings().then(s => setOn(s?.voice_breaks !== false)).catch(() => setOn(true)); }, []);
  useEffect(() => {
    if (!on) return;
    const id = setInterval(() => {
      // a new sitting (the count started again): the first reminder is a full stretch away again
      if (live.sungS < next - REPEAT_S && next > BREAK_AFTER_S) setNext(BREAK_AFTER_S);
      if (!show && live.sungS >= next) setShow(true);
    }, 10_000);
    return () => clearInterval(id);
  }, [on, next, show]);
  if (!show) return null;
  const minutes = Math.round(live.sungS / 60);
  return (
    <div role="status" className="pointer-events-auto fixed bottom-24 left-1/2 z-30 flex max-w-[min(32rem,calc(100vw-2rem))] -translate-x-1/2 items-center gap-3 rounded-2xl border border-white/8 bg-ink-900/90 px-4 py-2.5 text-sm text-ink-200 shadow-lg backdrop-blur animate-rise">
      <span>{t("voiceBreak.text", { minutes })}</span>
      <button onClick={() => { setShow(false); setNext(live.sungS + REPEAT_S); }} aria-label={t("voiceBreak.dismiss")}
              className="shrink-0 rounded-lg px-2 py-0.5 text-ink-400 transition hover:bg-white/8 hover:text-ink-100">{t("voiceBreak.ok")}</button>
    </div>
  );
}
