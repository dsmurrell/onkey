"use client";
import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import Script from "next/script";
import { FLOWMYNA_TRACKER_ID } from "@/lib/site";
import { FM_EVENTS, fm, isLiveSite, labelVisit, newPageView, resumeIfIdle } from "@/lib/flowmyna";

/** Landing-page sections whose first sight in a page view is an event (≥40% of the section in view). */
const SECTIONS: [string, string][] = [["how", FM_EVENTS.sawHow], ["privacy", FM_EVENTS.sawPrivacy], ["faq", FM_EVENTS.sawQuestions], ["get", FM_EVENTS.reachedGet]];

/** Flow Myna's script (cookieless, autocapture on) and the events autocapture can't see (lib/flowmyna.ts).
 *  Nothing at all when no tracker id is set. */
export function FlowMyna() {
  const path = usePathname();
  // the script loads only on the real site (singonkey.app), decided in the browser: local copies send nothing
  const [live, setLive] = useState(false);
  useEffect(() => { setLive(isLiveSite()); }, []);

  // each page view: events may be sent once again; the visit's label (fm.js keeps it in memory only)
  useEffect(() => {
    if (!FLOWMYNA_TRACKER_ID) return;
    newPageView();
    labelVisit();
  }, [path]);

  // the landing page's sections, and troubleshooting topics read (a heading in the top quarter of the window for 4 s)
  useEffect(() => {
    if (!FLOWMYNA_TRACKER_ID || typeof IntersectionObserver === "undefined") return;
    const seen = new IntersectionObserver((entries) => {
      for (const en of entries) {
        if (!en.isIntersecting) continue;
        const s = SECTIONS.find(([id]) => en.target.id === id || en.target.querySelector(`#${id}`));
        if (s) { fm(s[1]); seen.unobserve(en.target); }
      }
    }, { threshold: 0.4 });
    // only once the visitor scrolls: on a laptop the first steps are already in view on arrival, and counting
    // them then would mark every visit (bounces included) as having seen How it works
    const watch = () => {
      if (window.scrollY < 40) return;
      window.removeEventListener("scroll", watch);
      for (const [id] of SECTIONS) {
        const h = document.getElementById(id);
        const box = h?.closest("section") ?? h;
        if (box) seen.observe(box);
      }
    };
    window.addEventListener("scroll", watch, { passive: true });
    const timers = new Map<Element, number>();
    const topics = new IntersectionObserver((entries) => {
      for (const en of entries) {
        const t = timers.get(en.target);
        if (en.isIntersecting && t === undefined) {
          const topic = (en.target.textContent ?? "").trim().slice(0, 80);
          timers.set(en.target, window.setTimeout(() => fm(FM_EVENTS.readTopic, { topic }, `${FM_EVENTS.readTopic}:${topic}`), 4000));
        } else if (!en.isIntersecting && t !== undefined) {
          clearTimeout(t); timers.delete(en.target);
        }
      }
    }, { rootMargin: "0px 0px -75% 0px" });   // a topic is being read when its heading sits in the top quarter
    if (/^(\/[^/]+)?\/docs\/troubleshooting$/.test(path.replace(/\/+$/, ""))) {   // in any language: /es/docs/…
      document.querySelectorAll("main h2").forEach((h) => topics.observe(h));
    }
    return () => { window.removeEventListener("scroll", watch); seen.disconnect(); topics.disconnect(); timers.forEach((t) => clearTimeout(t)); };
  }, [path]);

  // coming back to a tab after Flow Myna's visit window: a fresh page view first, so the new visit starts on the
  // page (a click is caught before fm.js's own click capture sees it, so a Download after the gap follows the view)
  useEffect(() => {
    if (!FLOWMYNA_TRACKER_ID) return;
    const back = () => { if (document.visibilityState === "visible") resumeIfIdle(); };
    document.addEventListener("visibilitychange", back);
    window.addEventListener("pointerdown", resumeIfIdle, true);
    return () => { document.removeEventListener("visibilitychange", back); window.removeEventListener("pointerdown", resumeIfIdle, true); };
  }, []);

  // questions opened (FAQ answers, on the landing page or the FAQ page)
  useEffect(() => {
    if (!FLOWMYNA_TRACKER_ID) return;
    const onToggle = (ev: Event) => {
      const d = ev.target as HTMLDetailsElement;
      if (d instanceof HTMLDetailsElement && d.open && d.id.startsWith("faq-")) {
        fm(FM_EVENTS.openedQuestion, { question: d.id.slice(4) }, `${FM_EVENTS.openedQuestion}:${d.id}`);
      }
    };
    document.addEventListener("toggle", onToggle, true);          // (toggle doesn't bubble: caught on the way down)
    return () => document.removeEventListener("toggle", onToggle, true);
  }, []);

  if (!FLOWMYNA_TRACKER_ID || !live) return null;
  return (
    <>
      {/* calls made before the script loads are queued (Flow Myna's documented stub) */}
      <Script id="fm-stub" strategy="afterInteractive">
        {`window.FlowMyna=window.FlowMyna||{q:[],capture:function(){this.q.push(['capture',arguments])},register:function(){this.q.push(['register',arguments])}}`}
      </Script>
      <Script
        src={`https://flowmyna.com/fm.js?t=${encodeURIComponent(FLOWMYNA_TRACKER_ID)}`}
        data-privacy-mode="cookieless"
        strategy="afterInteractive"
      />
    </>
  );
}
