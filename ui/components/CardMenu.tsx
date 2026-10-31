"use client";
// A small ⋯ menu on a card, for its less used actions (open in its app, delete). Esc, a click elsewhere or tabbing
// away closes it; it never triggers the card it sits on.
import { useEffect, useRef, useState } from "react";

export default function CardMenu({ label, children }: { label: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", away);
    // focus the first item, for the keyboard
    box.current?.querySelector<HTMLElement>("[role=menu] button, [role=menu] a")?.focus();
    return () => document.removeEventListener("mousedown", away);
  }, [open]);
  return (
    <span ref={box} className="relative" onClick={e => e.stopPropagation()}
          onKeyDown={e => {
            e.stopPropagation();
            if (e.key === "Escape" && open) { setOpen(false); button.current?.focus(); }
          }}
          // focus moved elsewhere (tabbing away); not when it's simply lost, as when Delete turns into its confirm
          onBlur={e => { if (e.relatedTarget && !box.current?.contains(e.relatedTarget as Node)) setOpen(false); }}>
      <button ref={button} type="button" aria-haspopup="menu" aria-expanded={open} aria-label={label} title={label}
              onClick={() => setOpen(o => !o)}
              className={`grid h-7 w-7 place-items-center rounded-full text-ink-400 outline-none transition hover:bg-white/8 hover:text-white focus-visible:ring-2 focus-visible:ring-accent-400/60 ${open ? "bg-white/8 text-white" : ""}`}>
        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
          <circle cx="5" cy="12" r="1.7" /><circle cx="12" cy="12" r="1.7" /><circle cx="19" cy="12" r="1.7" /></svg>
      </button>
      {open && (
        // a press keeps focus where it is: WebKit (the app's window) doesn't focus a pressed button but moves focus to
        // the card around it, which would close the menu (onBlur above) before the item's click lands
        <div role="menu" onMouseDown={e => e.preventDefault()}
             onClick={e => { if ((e.target as HTMLElement).closest("[data-close]")) setOpen(false); }}
             className="absolute top-8 right-0 z-30 flex w-max min-w-44 animate-rise flex-col rounded-2xl border border-white/10 bg-ink-900 p-1 text-sm shadow-2xl shadow-black/50">
          {children}
        </div>
      )}
    </span>
  );
}

/** A plain item in a CardMenu; it closes the menu once chosen. */
export function MenuItem({ onSelect, children }: { onSelect: () => void; children: React.ReactNode }) {
  return (
    <button type="button" role="menuitem" data-close onClick={onSelect}
            className="rounded-xl px-3 py-1.5 text-left text-ink-200 outline-none transition hover:bg-white/8 hover:text-white focus-visible:bg-white/8 focus-visible:text-white">
      {children}
    </button>
  );
}
