"use client";
import { useEffect, useId, useRef, useState } from "react";
import { languageName } from "@/lib/languages";
import { useLanguageLinks } from "./LanguageSwitcher";

/** The header's language menu: a globe and the page's language (the name from lg up; the globe alone where the
 *  header is tighter), opening a list of every language in its own name. The same links and cookie as the footer's
 *  list (useLanguageLinks). Shown from sm up; below that the footer's list is the way. A menu button: Enter, Space
 *  or an arrow opens it with the first language focused; arrows move, Home and End jump, Escape closes and puts
 *  focus back on the button; a click elsewhere or focus leaving closes it. */
export function LanguageMenu({ locales, current, label }: { locales: string[]; current: string; label: string }) {
  const { href, remember } = useLanguageLinks(locales);
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLUListElement>(null);
  const id = useId();

  const items = () => Array.from(menu.current?.querySelectorAll<HTMLAnchorElement>("a[role=menuitem]") ?? []);
  const focusItem = (i: number) => { const list = items(); list[(i + list.length) % list.length]?.focus(); };
  const openAt = (i: number) => { setOpen(true); requestAnimationFrame(() => focusItem(i)); };
  const close = (refocus: boolean) => { setOpen(false); if (refocus) button.current?.focus(); };

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!menu.current?.contains(t) && !button.current?.contains(t)) setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [open]);

  const onButtonKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); openAt(e.key === "ArrowDown" ? 0 : -1); }
  };
  const onMenuKey = (e: React.KeyboardEvent) => {
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLAnchorElement);
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); focusItem(at + 1); break;
      case "ArrowUp": e.preventDefault(); focusItem(at - 1); break;
      case "Home": e.preventDefault(); focusItem(0); break;
      case "End": e.preventDefault(); focusItem(-1); break;
      case "Escape": e.preventDefault(); close(true); break;
      case "Tab": setOpen(false); break;
    }
  };
  const onBlur = (e: React.FocusEvent) => {
    const to = e.relatedTarget as Node | null;
    if (to && !menu.current?.contains(to) && !button.current?.contains(to)) setOpen(false);
  };

  return (
    <div className="relative" onBlur={onBlur}>
      <button
        ref={button}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={id}
        aria-label={`${label}: ${languageName(current)}`}
        onClick={() => (open ? close(false) : openAt(0))}
        onKeyDown={onButtonKey}
        className="inline-flex min-h-11 items-center gap-1.5 rounded-full px-3 text-sm text-muted transition hover:text-fg"
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3c3 3.3 3 14.7 0 18M12 3c-3 3.3-3 14.7 0 18" />
        </svg>
        <span lang={current} className="hidden lg:inline">{languageName(current)}</span>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>
      {open && (
        <ul
          ref={menu}
          id={id}
          role="menu"
          aria-label={label}
          onKeyDown={onMenuKey}
          className="absolute right-0 top-full z-40 mt-1 grid max-h-[70vh] min-w-52 grid-cols-1 gap-px overflow-y-auto rounded-xl border border-line bg-panel p-1.5 text-sm shadow-xl shadow-black/20"
        >
          {locales.map((l) => (
            <li key={l} role="none">
              <a
                role="menuitem"
                href={href(l)}
                hrefLang={l}
                lang={l}
                tabIndex={-1}
                aria-current={l === current ? "true" : undefined}
                onClick={() => remember(l)}
                className={`block rounded-lg px-3 py-1.5 hover:bg-accent-wash focus-visible:bg-accent-wash ${l === current ? "font-medium text-fg" : "text-muted hover:text-fg"}`}
              >
                {languageName(l)}
              </a>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
