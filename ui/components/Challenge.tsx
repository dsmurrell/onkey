"use client";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { inApp } from "@/lib/session";
import { challengeLink, MAX } from "@/lib/challenge";
import { useT } from "@/lib/i18n";

const NAME_KEY = "onkey.challengeName";

/** "Challenge a friend": copies a link to singonkey.app that says "<name> scored 78 on <song>. Can you beat it?".
 *  The link holds only the title, artist, score and the name typed (lib/challenge.ts), and it's shown before it's
 *  copied. Self-contained: a button with its own small panel under it. */
export default function Challenge({ song, artist, score }: { song: string; artist: string; score: number }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [done, setDone] = useState<"" | "copied" | "failed">("");
  const box = useRef<HTMLSpanElement>(null);
  // the name typed last time (this Mac only)
  useEffect(() => { try { setName(localStorage.getItem(NAME_KEY) ?? ""); } catch {} }, []);
  useEffect(() => {
    if (!open) return;
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    const away = (e: PointerEvent) => { if (box.current && !box.current.contains(e.target as Node)) setOpen(false); };
    window.addEventListener("keydown", esc);
    window.addEventListener("pointerdown", away);
    return () => { window.removeEventListener("keydown", esc); window.removeEventListener("pointerdown", away); };
  }, [open]);

  const link = challengeLink(song, artist, score, name);
  const copy = async () => {
    try { localStorage.setItem(NAME_KEY, name.trim()); } catch {}
    let ok = false;
    if (inApp()) ok = await invoke<boolean>("copy_challenge", { link }).catch(() => false);
    else ok = await navigator.clipboard?.writeText(link).then(() => true, () => false) ?? false;
    setDone(ok ? "copied" : "failed");
    setTimeout(() => setDone(""), 2500);
  };

  return (
    <span ref={box} className="relative">
      <button onClick={() => { setOpen(v => !v); setDone(""); }} aria-expanded={open}
              className="inline-flex items-center gap-1.5 rounded-xl bg-white/6 px-3 py-2 text-sm font-medium text-ink-100 transition hover:bg-white/10">
        <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M8 21h8" /><path d="M12 17v4" /><path d="M7 4h10v5a5 5 0 0 1-10 0z" /><path d="M17 5h3v2a3 3 0 0 1-3 3" /><path d="M7 5H4v2a3 3 0 0 0 3 3" />
        </svg>
        {t("challenge.button")}
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-2 w-80 rounded-2xl border border-white/10 bg-ink-900/95 p-4 text-sm shadow-2xl backdrop-blur">
          <p className="text-ink-100">{t.rich("challenge.says", { score: Math.round(score), song, b: c => <b className="tabular">{c}</b> })}</p>
          <label className="mt-3 block text-xs text-ink-400">
            {t("challenge.nameLabel")}
            <input value={name} maxLength={MAX.name} onChange={e => setName(e.target.value)} placeholder={t("challenge.namePlaceholder")}
                   onKeyDown={e => { if (e.key === "Enter") void copy(); }}
                   className="mt-1 block w-full rounded-lg bg-white/6 px-3 py-2 text-sm text-ink-100 outline-none placeholder:text-ink-400 focus:bg-white/10" />
          </label>
          <button onClick={() => void copy()}
                  className="mt-3 w-full rounded-xl bg-accent-500 px-4 py-2 font-medium text-white shadow-lg shadow-accent-600/25 transition hover:bg-accent-400">
            {done === "copied" ? t("challenge.copied") : done === "failed" ? t("challenge.failed") : t("challenge.copy")}
          </button>
          <p className="mt-2 text-xs text-ink-400">{name.trim() ? t("challenge.holdsNamed") : t("challenge.holds")}</p>
          <p className="mt-1 select-all break-all font-mono text-[10px] leading-snug text-ink-400">{link}</p>
        </div>
      )}
    </span>
  );
}
