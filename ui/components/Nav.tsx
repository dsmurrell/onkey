"use client";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { live, openLink, setLanguage, setListeningPaused, useSession } from "@/lib/session";
import { updateStatus } from "@/lib/update";
import { markSeen, seenRelease, showWhatsNewDot } from "@/lib/whatsNew";
import { useLanguageSetting, useT } from "@/lib/i18n";
import LanguageSelect from "@/components/LanguageSelect";
import { openFeedback } from "@/components/Feedback";

export function Logo() {
  const t = useT();
  const path = usePathname();
  // a preview (CI's Windows and Linux test builds, 0.3.0-preview.4): "Test build" beside the name, so a screenshot
  // or a tester's window can't be taken for a release
  const [preview, setPreview] = useState(false);
  useEffect(() => { updateStatus().then(u => setPreview(!!u.preview)).catch(() => {}); }, []);
  // on Sing already, the logo opens the songs to try over the page (StarterSheet) instead of going nowhere
  const onClick = (e: React.MouseEvent) => {
    if (path === "/") { e.preventDefault(); window.dispatchEvent(new CustomEvent("onkey-songs-to-try")); }
  };
  return (
    <Link href="/" onClick={onClick} title={path === "/" ? t("nav.songsToTry") : t("nav.sing")} className="flex items-center gap-2.5 rounded-xl transition hover:opacity-80">
      <span className="relative grid h-8 w-8 place-items-center rounded-xl bg-gradient-to-br from-accent-400 to-accent-600 shadow-lg shadow-accent-600/30">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2.2" strokeLinecap="round" aria-hidden>
          <path d="M3 14c2.5 0 2.5-6 5-6s2.5 8 5 8 2.5-10 5-10 2 4 3 4" />
        </svg>
      </span>
      <span className="text-[15px] font-semibold tracking-tight">{t("nav.appName")}</span>
      {preview && <span title={t("nav.testBuildTitle")} className="rounded-full bg-notice-fill/12 px-2 py-0.5 text-[11px] font-medium text-notice">{t("nav.testBuild")}</span>}
    </Link>
  );
}

export function Nav() {
  const t = useT();
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
      <nav className="flex items-center gap-0.5 lg:gap-1">{item("/", t("nav.sing"))}{item("/library", t("nav.library"))}{item("/history", t("nav.history"))}{item("/settings", t("nav.settings"))}</nav>
      <PausedPill />
    </>
  );
}

/** While listening is paused, on every page but Sing: "Listening paused · Resume" (⌘⇧P, or the Sing page's Live chip, too). */
function PausedPill() {
  const t = useT();
  useEffect(() => { live.start(); }, []);
  const { status, connected } = useSession();
  const path = usePathname();
  // (not on the Sing page: it says so there already, in the middle and on the Live chip)
  if (!connected || !status.listening_paused || path === "/") return null;
  return (
    <span role="status" className="glass flex shrink-0 items-center gap-1 rounded-full py-0.5 pl-2.5 pr-0.5 text-xs text-notice animate-rise">
      {t("nav.paused")}<span className="text-ink-500">·</span>
      <button onClick={() => void setListeningPaused(false)} title={t("nav.resumeWhy")}
              className="rounded-full px-2 py-1 font-medium text-ink-100 transition hover:bg-white/10">{t("nav.resume")}</button>
    </span>
  );
}

/** A present, at the right of each page's header (the Sing page: beside the score): opens singonkey.app/changelog/,
 *  every feature and fix and the release that brought it. Its dot: after an update, until opened once (lib/whatsNew.ts;
 *  this OnKey's version from the updater's status, so nothing to bump per release). */
export function WhatsNew() {
  const t = useT();
  const [version, setVersion] = useState<string | null>(null);
  const [fresh, setFresh] = useState(false);
  useEffect(() => {
    let alive = true;
    updateStatus().then(s => { if (alive) { setVersion(s.version); setFresh(showWhatsNewDot(s.version, seenRelease())); } }).catch(() => {});
    return () => { alive = false; };
  }, []);
  const open = () => {
    if (version) markSeen(version);
    setFresh(false);
    void openLink("https://singonkey.app/changelog/");
  };
  return (
    <span className="inline-flex items-center gap-1">
    <FeedbackBubble />
    <LanguageGlobe />
    <button onClick={open} title={t("nav.whatsNew")} aria-label={fresh ? t("nav.whatsNewFresh") : t("nav.whatsNew")}
            className="relative grid h-8 w-8 shrink-0 place-items-center rounded-full text-ink-300 transition hover:bg-white/8 hover:text-white">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <rect x="3.5" y="8.5" width="17" height="4" rx="1" />
        <path d="M5 12.5v7a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-7M12 8.5v12" />
        <path d="M12 8.5C10.5 5 7 4.6 7 6.6 7 8.2 9.6 8.5 12 8.5ZM12 8.5c1.5-3.5 5-3.9 5-1.9 0 1.6-2.6 1.9-5 1.9Z" />
      </svg>
      {fresh && <span className="absolute right-1 top-1 h-2 w-2 rounded-full bg-accent-400 ring-2 ring-ink-900" />}
    </button>
    </span>
  );
}

/** The top bar's speech bubble, before the globe: "Send feedback" (components/Feedback.tsx), in plain sight while
 *  OnKey is new. The email says it came from here ("bar"), so it can go back to Settings only if nobody uses it. */
function FeedbackBubble() {
  const t = useT();
  return (
    <button onClick={() => openFeedback("bar")} title={t("nav.feedback")} aria-label={t("nav.feedback")}
            className="grid h-8 w-8 shrink-0 place-items-center rounded-full text-ink-300 transition hover:bg-white/8 hover:text-white">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M20 11.5c0 4.1-3.6 7.5-8 7.5-1.2 0-2.3-.2-3.3-.6L4 20l1.3-3.6C4.5 15 4 13.3 4 11.5 4 7.4 7.6 4 12 4s8 3.4 8 7.5Z" />
      </svg>
    </button>
  );
}

/** The top bar's globe, beside the present: the language list (components/LanguageSelect.tsx); nothing while
 *  OnKey has only English. */
function LanguageGlobe() {
  const choice = useLanguageSetting();
  const choose = (code: string) => { void setLanguage(code).catch(() => {}); };
  return <LanguageSelect globe choice={choice} onChange={choose} />;
}
