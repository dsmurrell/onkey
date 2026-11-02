"use client";
// The window's language at launch: the Settings choice ("language", kept by settings.rs), else the computer's own
// (the engine's `ui_locale`, lib/i18n.ts), and the computer's region (`ui_region`) for the Welcome step's list.
// Renders nothing; the built page is English until this runs, then any other language shows.
import { useEffect } from "react";
import { getSettings, uiLocale, uiRegion } from "@/lib/session";
import { languageSetting, setLanguageSetting, setSystemLanguages, setSystemRegion } from "@/lib/i18n";

export default function LanguageSync() {
  useEffect(() => {
    let alive = true;
    const readOs = () => uiLocale().then(l => { if (alive && l) setSystemLanguages(l); }).catch(() => {});
    void uiRegion().then(r => { if (alive) setSystemRegion(r); }).catch(() => {});   // for the Welcome step's list
    void Promise.all([readOs(), getSettings().catch(() => null)]).then(([, s]) => {
      if (alive) setLanguageSetting(s?.language || languageSetting());
    });
    // the computer's language changed (matters when "auto"; the browser says so, the app's WebView may not)
    const changed = () => void readOs().then(() => setLanguageSetting(languageSetting()));
    window.addEventListener("languagechange", changed);
    return () => { alive = false; window.removeEventListener("languagechange", changed); };
  }, []);
  return null;
}
