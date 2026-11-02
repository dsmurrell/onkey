"use client";
// The language list Settings, first-run setup and the top bar's globe share, a menu in OnKey's own style (as
// Library's sort menu, components/ListControls.tsx). At the top, the computer's own language marked as such
// ("Português (your Mac's language)"): that row is Automatic, so OnKey follows the computer if its language changes
// (when the computer's language isn't one OnKey ships, the row is just the language Automatic falls back to,
// English, unmarked). Then the region's language when it's another (an English Mac set to Mexico: Español
// (Latinoamérica); lib/languages.ts `welcomeOrder`), the languages OnKey fully supports, and the Early ones under their
// own small heading. Nothing shows while there's only English (lib/languages.ts `pickerCodes`).
//   Settings: a button showing the choice, beside the setting's words.
//   Welcome (`welcome`): a small globe + the language shown + ▾ at the top right of the step.
//   Top bar (`globe`): just the globe, beside the What's new present.
import { useEffect, useRef, useState } from "react";
import { locales, pickLocale, systemLanguages, useLocale, useSystemRegion, useT } from "@/lib/i18n";
import { isEarly, nativeName, pickerCodes, welcomeOrder } from "@/lib/languages";
import { useWindows } from "@/lib/platform";
import { FloatingMenu } from "@/components/Dropdown";

const Globe = ({ size }: { size: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3c3 3.3 3 14.7 0 18M12 3c-3 3.3-3 14.7 0 18" />
  </svg>
);
const Chevron = ({ open }: { open: boolean }) => (
  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden
       className={`transition ${open ? "rotate-180" : ""}`}><path d="M6 9l6 6 6-6" /></svg>
);
const Check = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden className="shrink-0 text-accent-400"><path d="M5 12l5 5 9-10" /></svg>
);

export default function LanguageSelect({ choice, onChange, id, welcome, globe }: {
  choice: string; onChange: (code: string) => void; id?: string; welcome?: boolean;
  /** the top bar's globe: just the icon, beside the What's new present; the list as Welcome's */
  globe?: boolean;
}) {
  const t = useT();
  const shown = useLocale();
  const win = useWindows();
  const region = useSystemRegion();
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
    // the chosen language gets focus, so arrows move from there
    const at = items.current.findIndex(b => b?.getAttribute("aria-checked") === "true");
    items.current[at >= 0 ? at : 0]?.focus();
    return () => { document.removeEventListener("pointerdown", away); document.removeEventListener("keydown", esc); };
  }, [open]);

  const available = pickerCodes(locales());
  if (!available) return null;
  const auto = pickLocale("auto", systemLanguages(), available);
  // Automatic isn't a row of its own: it's the row of the language it comes to, marked as the computer's when the
  // computer asked for it (not when OnKey fell back to English for a language it doesn't ship). Choosing that row
  // saves "auto", so OnKey follows the computer if its language changes.
  const computers = systemLanguages().some(l => l.toLowerCase().split("-")[0] === auto.toLowerCase().split("-")[0]);
  const rest = welcomeOrder(available, auto, region).filter(c => c !== auto);
  const full = rest.filter(c => !isEarly(c));
  const early = rest.filter(c => isEarly(c));
  const current = available.includes(choice) ? choice : "auto";
  const autoRow = computers ? t("settings.language.autoOs", { language: nativeName(auto), os: win ? "pc" : "mac" }) : nativeName(auto);
  const autoLabel = welcome || globe ? autoRow : t("settings.language.autoNamed", { language: nativeName(auto) });

  const pick = (code: string) => { setOpen(false); button.current?.focus(); if (code !== current) onChange(code); };
  const move = (e: React.KeyboardEvent) => {
    const list = items.current.filter((b): b is HTMLButtonElement => !!b);
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    const to = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? list.length - 1 : null;
    if (to === null) return;
    e.preventDefault();
    list[(to + list.length) % list.length]?.focus();
  };
  items.current = [];
  const item = (code: string, label: string, langAttr?: string) => {
    const on = current === code || (code === "auto" && current === auto);
    return (
      <button key={code} ref={b => { items.current.push(b); }} role="menuitemradio" aria-checked={on} lang={langAttr}
              onClick={() => pick(code)}
              className={`flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left text-sm transition hover:bg-white/6 focus:bg-white/6 focus:outline-none ${on ? "text-white" : "text-ink-300"}`}>
        <span className="min-w-0">{label}</span>{on && <Check />}
      </button>
    );
  };

  const trigger = globe ? (
    <button ref={button} id={id} onClick={() => setOpen(o => !o)} aria-haspopup="menu" aria-expanded={open}
            title={t("settings.language.label")} aria-label={t("settings.language.label")}
            className={`grid h-8 w-8 shrink-0 place-items-center rounded-full transition hover:bg-white/8 hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400 ${open ? "bg-white/8 text-white" : "text-ink-300"}`}>
      <Globe size={16} />
    </button>
  ) : welcome ? (
    <button ref={button} id={id} onClick={() => setOpen(o => !o)} aria-haspopup="menu" aria-expanded={open}
            aria-label={t("settings.language.label")}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-lg px-2 py-1 text-xs text-ink-300 transition hover:bg-white/4 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400">
      <Globe size={14} /><span lang={shown}>{nativeName(shown)}</span><Chevron open={open} />
    </button>
  ) : (
    <button ref={button} id={id} onClick={() => setOpen(o => !o)} aria-haspopup="menu" aria-expanded={open}
            className="mt-0.5 inline-flex max-w-56 shrink-0 items-center gap-2 rounded-full border border-white/8 bg-white/4 px-3 py-1.5 text-sm text-ink-100 transition hover:border-white/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400">
      <span className="truncate" lang={current === "auto" ? undefined : current}>{current === "auto" ? autoLabel : nativeName(current)}</span>
      <Chevron open={open} />
    </button>
  );

  return (
    <div ref={ref} className="relative shrink-0">
      {trigger}
      {open && (
        <FloatingMenu anchor={button} inside={menu} onClose={() => setOpen(false)} role="menu" aria-label={t("settings.language.label")} onKeyDown={move}
             className="w-64 overflow-y-auto rounded-2xl border border-white/10 bg-ink-900/98 p-1 shadow-xl shadow-black/50">
          {item("auto", autoRow, auto)}
          <div className="mx-3 my-1 h-px bg-white/8" role="separator" />
          {full.map(c => item(c, nativeName(c), c))}
          {early.length > 0 && (
            <>
              <div className="mx-3 my-1 h-px bg-white/8" role="separator" />
              <div className="px-3 pt-1.5 pb-1 text-[10px] font-medium uppercase tracking-wider text-ink-500">{t("settings.language.early")}</div>
              {early.map(c => item(c, nativeName(c), c))}
            </>
          )}
        </FloatingMenu>
      )}
    </div>
  );
}
