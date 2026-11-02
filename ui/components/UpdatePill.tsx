"use client";
// A quiet "Update available — Restart to update" in the corner once a new OnKey is downloaded (updater.rs).
// Never in mini mode; pressed mid-song, OnKey restarts once the song is over. Not pressed: it's installed on quit.
import { useState } from "react";
import { restartToUpdate, updateFailed, useUpdate } from "@/lib/update";
import { openLink } from "@/lib/session";
import { useT } from "@/lib/i18n";
import { updateWhy } from "@/lib/engineText";

export default function UpdatePill() {
  const t = useT();
  const [u, setU] = useUpdate();
  const [hidden, setHidden] = useState(false);
  if (!u || hidden) return null;
  // a download or install that failed: said (it used to vanish), with why in its tooltip; Settings can try again
  if (u.state === "error" && u.available && updateFailed(u) === "update")
    return (
      <div id="update-pill" role="status" title={updateWhy(u) ? t("update.failed.why", { error: updateWhy(u)! }) : t("update.failed.again")}
           className="in-[.mini]:hidden fixed bottom-3 left-3 z-40 flex items-center gap-2 rounded-full border border-white/10 bg-black/60 py-1 pl-3 pr-1 text-xs text-ink-200 shadow-lg backdrop-blur animate-rise">
        <span className="h-1.5 w-1.5 rounded-full bg-off" />
        <span>{t("update.failed.text", { version: String(u.available) })}</span>
        <button onClick={() => setHidden(true)} aria-label={t("update.hide")} className="rounded-full px-1.5 py-1 text-ink-400 transition hover:text-white">×</button>
      </div>
    );
  // a build from source (a fork's): a new official version is out; it's never installed over this one
  if (u.state === "official" && u.available)
    return (
      <div id="update-pill" role="status" title={t("update.official.why")}
           className="in-[.mini]:hidden fixed bottom-3 left-3 z-40 flex items-center gap-2 rounded-full border border-white/10 bg-black/60 py-1 pl-3 pr-1 text-xs text-ink-200 shadow-lg backdrop-blur animate-rise">
        <span className="h-1.5 w-1.5 rounded-full bg-accent-400" />
        <span>{t("update.official.text", { version: String(u.available) })}</span>
        <button onClick={() => void openLink("https://singonkey.app")} className="rounded-full bg-white/8 px-2.5 py-1 font-medium text-ink-100 transition hover:bg-white/14">
          {t("update.official.download")}
        </button>
        <button onClick={() => setHidden(true)} aria-label={t("update.hide")} className="rounded-full px-1.5 py-1 text-ink-400 transition hover:text-white">×</button>
      </div>
    );
  if (u.state !== "ready") return null;
  const restart = async () => { const v = await restartToUpdate(); if (v) setU(v); };
  return (
    <div id="update-pill" role="status"
         className="in-[.mini]:hidden fixed bottom-3 left-3 z-40 flex items-center gap-2 rounded-full border border-white/10 bg-black/60 py-1 pl-3 pr-1 text-xs text-ink-200 shadow-lg backdrop-blur animate-rise"
         title={t("update.ready.why", { version: String(u.available) })}>
      <span className="h-1.5 w-1.5 rounded-full bg-accent-400" />
      {u.waiting
        ? <span className="pr-2">{t("update.ready.waiting")}</span>
        : <>
            <span>{t("update.ready.available")}</span>
            <button onClick={restart} className="rounded-full bg-white/8 px-2.5 py-1 font-medium text-ink-100 transition hover:bg-white/14">
              {t("update.ready.restart")}
            </button>
            <button onClick={() => setHidden(true)} aria-label={t("update.ready.later")} title={t("update.ready.laterWhy")}
                    className="rounded-full px-1.5 py-1 text-ink-400 transition hover:text-white">×</button>
          </>}
    </div>
  );
}
