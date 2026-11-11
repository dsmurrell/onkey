"use client";
// "Send feedback": a thumb and a few words for OnKey's maker, sent only when you press Send (feedback.rs says exactly
// what goes, and the window says it too). Opened from the top bar's speech bubble (components/Nav.tsx), Settings →
// About and the take page's "How's OnKey working for you?" card (after the 3rd sung take, then the 20th), with the thumb already chosen there.
import { useEffect, useState } from "react";
import { sendFeedback, type Mood } from "@/lib/session";
import { useLocale, useT } from "@/lib/i18n";

export type From = "take" | "settings" | "bar";
type Open = { mood: Mood | null; from: From };

/** Open the feedback window (with a thumb chosen, from the take page's card). */
export function openFeedback(from: From, mood: Mood | null = null) {
  window.dispatchEvent(new CustomEvent("onkey-feedback", { detail: { mood, from } }));
}

const thumb = (on: boolean) =>
  `inline-flex items-center gap-1.5 rounded-xl px-3 py-2 text-sm transition ${on ? "bg-accent-500/25 text-white ring-1 ring-accent-400/60" : "bg-white/6 text-ink-200 hover:bg-white/10"}`;

export default function Feedback() {
  const t = useT();
  const locale = useLocale();
  const [open, setOpen] = useState<Open | null>(null);
  const [mood, setMood] = useState<Mood | null>(null);
  const [text, setText] = useState("");
  const [email, setEmail] = useState("");
  const [state, setState] = useState<"idle" | "sending" | "sent" | "failed">("idle");
  useEffect(() => {
    const on = (e: Event) => {
      const d = (e as CustomEvent<Open>).detail;
      setOpen({ mood: d?.mood ?? null, from: d?.from ?? "settings" });
    };
    window.addEventListener("onkey-feedback", on);
    return () => window.removeEventListener("onkey-feedback", on);
  }, []);
  useEffect(() => {
    if (!open) return;
    setMood(open.mood); setText(""); setEmail(""); setState("idle");
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(null); };
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [open]);
  if (!open) return null;

  const canSend = (mood !== null || text.trim() !== "") && state !== "sending";
  async function send() {
    if (!open || !canSend) return;
    setState("sending");
    try {
      await sendFeedback({ mood, text, email, from: open.from, language: locale });
      setState("sent");
    } catch { setState("failed"); }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/65 p-6" onClick={() => setOpen(null)}>
      <div role="dialog" aria-modal="true" aria-labelledby="feedback-title" onClick={e => e.stopPropagation()}
           className="flex max-h-[85vh] w-full max-w-lg flex-col overflow-y-auto rounded-3xl border border-white/10 bg-ink-900 p-6 shadow-2xl animate-rise">
        <h2 id="feedback-title" className="text-lg font-semibold tracking-tight">{t("feedback.title")}</h2>
        {state === "sent" ? (
          <>
            <p className="mt-2 text-sm text-ink-300">{t("feedback.sent")}</p>
            <div className="mt-5 flex justify-end">
              <button onClick={() => setOpen(null)} className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400">{t("feedback.done")}</button>
            </div>
          </>
        ) : (
          <>
            <p className="mt-2 text-sm text-ink-300">{t("feedback.intro")}</p>
            <p className="mt-4 text-sm font-medium text-ink-100">{t("feedback.how")}</p>
            <div className="mt-2 flex flex-wrap gap-2" role="group" aria-label={t("feedback.how")}>
              <button onClick={() => setMood(mood === "up" ? null : "up")} aria-pressed={mood === "up"} className={thumb(mood === "up")}>
                <span aria-hidden>👍</span>{t("feedback.up")}
              </button>
              <button onClick={() => setMood(mood === "down" ? null : "down")} aria-pressed={mood === "down"} className={thumb(mood === "down")}>
                <span aria-hidden>👎</span>{t("feedback.down")}
              </button>
            </div>
            <label htmlFor="feedback-text" className="mt-4 text-sm font-medium text-ink-100">
              {t.rich("feedback.label", { muted: c => <span className="font-normal text-ink-400">{c}</span> })}
            </label>
            <textarea id="feedback-text" value={text} onChange={e => setText(e.target.value)} rows={4} maxLength={4000} autoFocus
                      placeholder={t("feedback.placeholder")}
                      className="mt-1.5 resize-none rounded-xl border border-white/8 bg-white/4 px-3 py-2 text-sm text-ink-100 outline-none placeholder:text-ink-500 focus:border-accent-400" />
            <label htmlFor="feedback-email" className="mt-4 text-sm font-medium text-ink-100">
              {t.rich("feedback.emailLabel", { muted: c => <span className="font-normal text-ink-400">{c}</span> })}
            </label>
            <input id="feedback-email" type="email" value={email} onChange={e => setEmail(e.target.value)} maxLength={200}
                   placeholder={t("feedback.emailPlaceholder")} autoComplete="email"
                   className="mt-1.5 rounded-xl border border-white/8 bg-white/4 px-3 py-2 text-sm text-ink-100 outline-none placeholder:text-ink-500 focus:border-accent-400" />
            <p className="mt-4 text-xs text-ink-400">{t("feedback.sends")}</p>
            {state === "failed" && <p className="mt-3 text-sm text-notice">{t("feedback.failed")}</p>}
            <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
              <button onClick={() => setOpen(null)} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">{t("feedback.cancel")}</button>
              <button onClick={send} disabled={!canSend}
                      className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400 disabled:opacity-50">
                {state === "sending" ? t("feedback.sending") : t("feedback.send")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
