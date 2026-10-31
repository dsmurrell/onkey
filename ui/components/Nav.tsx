"use client";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { live, openLink, setListeningPaused, useSession } from "@/lib/session";

export function Logo() {
  const path = usePathname();
  // on Sing already, the logo opens the songs to try over the page (StarterSheet) instead of going nowhere
  const onClick = (e: React.MouseEvent) => {
    if (path === "/") { e.preventDefault(); window.dispatchEvent(new CustomEvent("onkey-songs-to-try")); }
  };
  return (
    <Link href="/" onClick={onClick} title={path === "/" ? "Songs to try" : "Sing"} className="flex items-center gap-2.5 rounded-xl transition hover:opacity-80">
      <span className="relative grid h-8 w-8 place-items-center rounded-xl bg-gradient-to-br from-accent-400 to-accent-600 shadow-lg shadow-accent-600/30">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2.2" strokeLinecap="round" aria-hidden>
          <path d="M3 14c2.5 0 2.5-6 5-6s2.5 8 5 8 2.5-10 5-10 2 4 3 4" />
        </svg>
      </span>
      <span className="text-[15px] font-semibold tracking-tight">OnKey</span>
    </Link>
  );
}

export function Nav() {
  const path = usePathname();
  const item = (href: string, label: string) => {
    const on = href === "/" ? path === "/" : path.startsWith(href);
    return (
      <Link href={href} className={`shrink-0 rounded-full px-2.5 py-1.5 text-sm transition lg:px-3 ${on ? "bg-white/8 text-white" : "text-ink-300 hover:text-white"}`}>
        {label}
      </Link>
    );
  };
  // tighter on a narrow window, so all four always fit beside the logo
  return (
    <>
      <nav className="flex items-center gap-0.5 lg:gap-1">{item("/", "Sing")}{item("/library", "Library")}{item("/history", "History")}{item("/settings", "Settings")}</nav>
      <PausedPill />
    </>
  );
}

/** While listening is paused, on every page but Sing: "Listening paused · Resume" (⌘⇧P, or the Sing page's Live chip, too). */
function PausedPill() {
  useEffect(() => { live.start(); }, []);
  const { status, connected } = useSession();
  const path = usePathname();
  // (not on the Sing page: it says so there already, in the middle and on the Live chip)
  if (!connected || !status.listening_paused || path === "/") return null;
  return (
    <span role="status" className="glass flex shrink-0 items-center gap-1 rounded-full py-0.5 pl-2.5 pr-0.5 text-xs text-amber-200 animate-rise">
      Listening paused<span className="text-ink-500">·</span>
      <button onClick={() => void setListeningPaused(false)} title="Resume listening (⌘⇧P)"
              className="rounded-full px-2 py-1 font-medium text-ink-100 transition hover:bg-white/10">Resume</button>
    </span>
  );
}

/** The release the site's What's new page was last updated for: bump it with each release, and the present
 *  shows a dot until it's opened once. */
const WHATS_NEW = "0.2.0";
const SEEN_KEY = "onkey-whats-new-seen";

/** A present, at the right of each page's header (the Sing page: beside the score): opens singonkey.app/changelog/, every feature and fix and the release that brought it. */
export function WhatsNew() {
  const [fresh, setFresh] = useState(false);
  useEffect(() => {
    try { setFresh(localStorage.getItem(SEEN_KEY) !== WHATS_NEW); } catch {}
  }, []);
  const open = () => {
    try { localStorage.setItem(SEEN_KEY, WHATS_NEW); } catch {}
    setFresh(false);
    void openLink("https://singonkey.app/changelog/");
  };
  return (
    <button onClick={open} title="What’s new in OnKey" aria-label={fresh ? "What’s new in OnKey (new)" : "What’s new in OnKey"}
            className="relative grid h-8 w-8 shrink-0 place-items-center rounded-full text-ink-300 transition hover:bg-white/8 hover:text-white">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <rect x="3.5" y="8.5" width="17" height="4" rx="1" />
        <path d="M5 12.5v7a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-7M12 8.5v12" />
        <path d="M12 8.5C10.5 5 7 4.6 7 6.6 7 8.2 9.6 8.5 12 8.5ZM12 8.5c1.5-3.5 5-3.9 5-1.9 0 1.6-2.6 1.9-5 1.9Z" />
      </svg>
      {fresh && <span className="absolute right-1 top-1 h-2 w-2 rounded-full bg-accent-400 ring-2 ring-[#0b0c12]" />}
    </button>
  );
}
