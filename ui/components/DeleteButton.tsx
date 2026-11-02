"use client";
// A small trash button that asks before deleting (inline, no dialog). Deleted things go to the Trash; if that
// fails nothing is deleted, and the error shows here with the button ready to try again.
import { useState } from "react";
import { runDelete } from "@/lib/dataSafety";
import { useT } from "@/lib/i18n";

export default function DeleteButton({ what, confirm, onDelete, className = "" }:
    { what: string; confirm: string; onDelete: () => Promise<void> | void; className?: string }) {
  const t = useT();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function confirmed() {
    setBusy(true);
    setError(null);
    const failed = await runDelete(onDelete);
    setBusy(false);
    if (failed) setError(failed);
    else setAsking(false);
  }
  if (asking)
    return (
      <span className={`flex items-center gap-1.5 rounded-full bg-ink-950/90 p-1 pl-3 text-xs whitespace-nowrap shadow-lg ${className}`}
            onClick={e => { e.preventDefault(); e.stopPropagation(); }}>
        <span className={error ? "text-off" : "text-ink-300"} role={error ? "alert" : undefined}>{error ?? confirm}</span>
        <button disabled={busy} onClick={confirmed}
                className="rounded-full bg-off/20 px-2.5 py-1 font-medium text-off transition hover:bg-off/30 disabled:opacity-50">{error ? t("deleteButton.tryAgain") : t("deleteButton.delete")}</button>
        <button onClick={() => { setAsking(false); setError(null); }} className="rounded-full px-2.5 py-1 text-ink-300 transition hover:bg-white/8">{t("deleteButton.cancel")}</button>
      </span>
    );
  return (
    <button onClick={e => { e.preventDefault(); e.stopPropagation(); setAsking(true); }} title={what} aria-label={what}
            className={`grid h-7 w-7 place-items-center rounded-full text-ink-400 transition hover:bg-white/8 hover:text-off ${className}`}>
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" /></svg>
    </button>
  );
}
