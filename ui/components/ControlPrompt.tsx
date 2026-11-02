"use client";
// Listening mode: OnKey follows songs by listening and controls no app until you allow it. When you use a
// control (start a song, skip to the singing, pause from OnKey) for an app you haven't allowed, this asks:
// allow control of that app (macOS then asks once too), or open the song there and press play yourself.
import { useEffect, useState } from "react";
import { useWindows } from "@/lib/platform";
import { useT } from "@/lib/i18n";
import { allowControl, goHome, openInApp, type ControlNeeded } from "@/lib/session";

export default function ControlPrompt() {
  const t = useT();
  const [ask, setAsk] = useState<ControlNeeded | null>(null);
  const [busy, setBusy] = useState(false);
  const win = useWindows();
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
          {canAllow ? t("controlPrompt.titleAllow", { app: ask.app }) : t("controlPrompt.titleCant", { app: ask.app })}
        </h2>
        <p className="mt-2 text-sm text-ink-300">
          {canAllow
            ? t("controlPrompt.bodyAllow", { app: ask.app, os: win ? "pc" : "mac" })
            : t("controlPrompt.bodyCant", { app: ask.app })}
        </p>
        {browser && win && (
          <p className="mt-2 rounded-xl bg-white/6 px-3 py-2 text-xs text-ink-300">
            {t("controlPrompt.browserPc")}
          </p>
        )}
        {browser && !win && (
          <p className="mt-2 rounded-xl bg-notice-fill/10 px-3 py-2 text-xs text-notice">
            {t("controlPrompt.browserMac")}
          </p>
        )}
        <div className="mt-5 flex flex-wrap justify-end gap-2">
          <button onClick={() => setAsk(null)} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">{t("controlPrompt.notNow")}</button>
          {ask.track && (
            <button onClick={open} className="rounded-xl bg-white/8 px-4 py-2 text-sm text-ink-100 transition hover:bg-white/12">
              {t("controlPrompt.openIn", { app: browser ? t("controlPrompt.yourBrowser") : ask.app })}
            </button>
          )}
          {canAllow && (
            <button disabled={busy} onClick={allow}
                    className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400 disabled:opacity-50">
              {t("controlPrompt.allow")}
            </button>
          )}
        </div>
        <p className="mt-4 text-[11px] text-ink-500">{t("controlPrompt.changeLater")}</p>
      </div>
    </div>
  );
}
