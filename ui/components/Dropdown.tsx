"use client";
// A choice from a short list, in OnKey's own menu style (as the language menu, components/LanguageSelect.tsx, and
// Library's sort menu): a rounded button showing the choice, and under it a menu of rows, the chosen one ticked,
// in groups (each after the first behind a thin line, with an optional small heading). Arrow keys, Home/End and
// Esc work; a click outside closes it. Settings uses it for the microphone.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

/** An open menu, drawn over the whole page under its button (`anchor`), right edges lined up: inside a Settings
 *  box (rounded, clipped, frosted) it would be cut off at the box's edge. It flips above the button when there's
 *  more room there, closes when the page scrolls or resizes, and `inside` tells a click on it from one outside. */
export function FloatingMenu({ anchor, onClose, inside, className, children, ...rest }: {
  anchor: RefObject<HTMLElement | null>; onClose: () => void; inside: RefObject<HTMLDivElement | null>;
  className: string; children: ReactNode;
} & React.HTMLAttributes<HTMLDivElement>) {
  const [at, setAt] = useState<{ top?: number; bottom?: number; right: number; max: number } | null>(null);
  useLayoutEffect(() => {
    const r = anchor.current?.getBoundingClientRect();
    if (!r) return;
    const below = innerHeight - r.bottom - 16, above = r.top - 16;
    setAt(below >= 240 || below >= above
      ? { top: r.bottom + 8, right: innerWidth - r.right, max: below - 8 }
      : { bottom: innerHeight - r.top + 8, right: innerWidth - r.right, max: above - 8 });
  }, [anchor]);
  useEffect(() => {
    const close = (e: Event) => { if (!(e.target instanceof Node && inside.current?.contains(e.target))) onClose(); };
    addEventListener("scroll", close, true);
    addEventListener("resize", onClose);
    return () => { removeEventListener("scroll", close, true); removeEventListener("resize", onClose); };
  }, [onClose, inside]);
  if (!at) return null;
  return createPortal(
    <div ref={inside} {...rest} className={className}
         style={{ position: "fixed", top: at.top, bottom: at.bottom, right: at.right, maxHeight: Math.min(at.max, innerHeight * 0.7), zIndex: 60 }}>
      {children}
    </div>, document.body);
}

export type DropdownItem = { value: string; label: string; lang?: string };
export type DropdownGroup = { heading?: string; items: DropdownItem[] };

const Chevron = ({ open }: { open: boolean }) => (
  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden
       className={`shrink-0 transition ${open ? "rotate-180" : ""}`}><path d="M6 9l6 6 6-6" /></svg>
);
const Check = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden className="shrink-0 text-accent-400"><path d="M5 12l5 5 9-10" /></svg>
);

export default function Dropdown({ id, value, groups, onChange, label }: {
  id?: string; value: string; groups: DropdownGroup[]; onChange: (value: string) => void;
  /** the menu's name, for screen readers */
  label: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!ref.current?.contains(e.target as Node) && !menu.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") { setOpen(false); button.current?.focus(); } };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", esc);
    const at = items.current.findIndex(b => b?.getAttribute("aria-checked") === "true");
    items.current[at >= 0 ? at : 0]?.focus();
    return () => { document.removeEventListener("pointerdown", away); document.removeEventListener("keydown", esc); };
  }, [open]);

  const all = groups.flatMap(g => g.items);
  const shown = all.find(i => i.value === value) ?? all[0];
  const pick = (v: string) => { setOpen(false); button.current?.focus(); if (v !== value) onChange(v); };
  const move = (e: React.KeyboardEvent) => {
    const list = items.current.filter((b): b is HTMLButtonElement => !!b);
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    const to = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? list.length - 1 : null;
    if (to === null) return;
    e.preventDefault();
    list[(to + list.length) % list.length]?.focus();
  };
  items.current = [];

  return (
    <div ref={ref} className="relative shrink-0">
      <button ref={button} id={id} onClick={() => setOpen(o => !o)} aria-haspopup="menu" aria-expanded={open}
              className="mt-0.5 inline-flex max-w-56 shrink-0 items-center gap-2 rounded-full border border-white/8 bg-white/4 px-3 py-1.5 text-sm text-ink-100 transition hover:border-white/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400">
        <span className="truncate" lang={shown?.lang}>{shown?.label}</span>
        <Chevron open={open} />
      </button>
      {open && (
        <FloatingMenu anchor={button} inside={menu} onClose={() => setOpen(false)} role="menu" aria-label={label} onKeyDown={move}
             className="w-64 overflow-y-auto rounded-2xl border border-white/10 bg-ink-900/98 p-1 shadow-xl shadow-black/50">
          {groups.filter(g => g.items.length > 0).map((g, gi) => (
            <div key={gi}>
              {gi > 0 && <div className="mx-3 my-1 h-px bg-white/8" role="separator" />}
              {g.heading && <div className="px-3 pt-1.5 pb-1 text-[10px] font-medium uppercase tracking-wider text-ink-500">{g.heading}</div>}
              {g.items.map(it => {
                const on = it.value === value;
                return (
                  <button key={it.value} ref={b => { items.current.push(b); }} role="menuitemradio" aria-checked={on} lang={it.lang}
                          onClick={() => pick(it.value)}
                          className={`flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left text-sm transition hover:bg-white/6 focus:bg-white/6 focus:outline-none ${on ? "text-white" : "text-ink-300"}`}>
                    <span className="truncate">{it.label}</span>{on && <Check />}
                  </button>
                );
              })}
            </div>
          ))}
        </FloatingMenu>
      )}
    </div>
  );
}
