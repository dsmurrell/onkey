"use client";
// Settings: every switch is something you allow. "Let OnKey control" Spotify, Apple Music (on by default) and YouTube in
// your browser (opt-in): all three off is listening only (settings.rs `listen_only`; there's no switch of its own). Then
// "Use Shazam to name new songs" (on by default; off, nothing is sent to Shazam), "Show lyrics" (on by default; off, LRCLIB
// isn't asked) and the microphone. (The output timing check, components/TimingCheck.tsx, is retired: see
// timing::CHECK_RETIRED.)
import Link from "next/link";
import FadeGuideDemo from "@/components/guides/FadeGuideDemo";
import { useEffect, useState } from "react";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import { reportProblem } from "@/components/ReportProblem";
import { checkForUpdate, restartToUpdate, setAutoUpdate, useUpdate } from "@/lib/update";
import { updateLine } from "@/lib/updateLine";
import { allowControl, getMics, getOutputTiming, getSettings, openLink, setMic, setLanguage, setScoringLevel, setSetting, setupControl, setupStatus, setYouTube, type Mics, type OutputTiming, type Settings } from "@/lib/session";
import { LEVELS, levelAbout, levelName, levelOf, type Level } from "@/lib/music";
import { usePc, useWindows } from "@/lib/platform";
import { engineErrorText } from "@/lib/engineText";
import { useT } from "@/lib/i18n";
import Dropdown from "@/components/Dropdown";
import LanguagePicker from "@/components/LanguagePicker";

/** Audio: the output you hear the music on, which microphone OnKey listens to (automatic: the Mac's input, as chosen
 *  in System Settings → Sound, or one picked here; Setup's mic check and your takes use the same one), and setup again
 *  (the Sing page opens it with ?setup). */
function Audio() {
  const t = useT();
  const [m, setM] = useState<Mics | null>(null);
  const win = useWindows();
  const pc = usePc();
  const [out, setOut] = useState<OutputTiming | null>(null);
  useEffect(() => {
    let alive = true;
    const load = () => {
      getMics().then(v => { if (alive) setM(v); }).catch(() => {});
      getOutputTiming().then(v => { if (alive) setOut(v); }).catch(() => {});
    };
    load();
    const timer = window.setInterval(() => { if (document.visibilityState === "visible") load(); }, 3000);   // a mic or headphones plugged in or out shows up
    return () => { alive = false; clearInterval(timer); };
  }, []);
  const missing = !!m && m.preferred != null && !m.devices.some(d => d.uid === m.preferred);
  const pick = async (uid: string) => { await setMic(uid === "" ? null : uid); setM(await getMics()); };
  return (
    <>
      <h2 className="mt-6 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.audio.title")}</h2>
      <section className="glass overflow-hidden rounded-3xl">
        {m && (
          <div className="flex items-start justify-between gap-6 border-b border-white/6 px-5 py-4">
            <div className="min-w-0">
              <div className="text-sm font-medium text-ink-100">{t("settings.audio.mic.label")}</div>
              <p className="mt-1 text-xs text-ink-400">
                {m.chosen
                  ? (!missing ? t("settings.audio.mic.now", { name: m.chosen.name })
                    : win ? t("settings.audio.mic.nowMissingPc", { name: m.chosen.name, input: pc.defaultInput })
                    : t("settings.audio.mic.nowMissingMac", { name: m.chosen.name }))
                  : !missing ? <span className="text-notice">{t("settings.audio.mic.none")}</span>
                  : t.rich(win ? "settings.audio.mic.noneMissingPc" : "settings.audio.mic.noneMissingMac",
                           { input: pc.defaultInput, warn: c => <span className="text-notice">{c}</span> })}
              </p>
              <More>
                {win ? t("settings.audio.mic.morePc", { input: pc.defaultInput, soundInput: pc.soundInput })
                  : t("settings.audio.mic.moreMac")}
              </More>
            </div>
            <Dropdown id="set-mic" value={m.preferred ?? ""} onChange={pick} label={t("settings.audio.mic.label")}
                      groups={[
                        { items: [{ value: "", label: t("settings.audio.mic.automatic") },
                                  ...m.devices.filter(d => !d.virtual).map(d => ({ value: d.uid, label: d.name }))] },
                        { heading: t("settings.audio.mic.virtual"), items: m.devices.filter(d => d.virtual).map(d => ({ value: d.uid, label: d.name })) },
                        { items: missing ? [{ value: m.preferred!, label: t("settings.audio.mic.notConnected") }] : [] },
                      ]} />
          </div>
        )}
        {out && (
          <div className="border-b border-white/6 px-5 py-4">
            <div className="text-sm font-medium text-ink-100">{t("settings.audio.output.label")}</div>
            <p className="mt-1 text-xs text-ink-400">{t("settings.audio.output.now", { name: out.name, os: win ? "pc" : "mac", sound: pc.sound })}</p>
          </div>
        )}
        <Link href="/?setup" className="flex items-center justify-between gap-6 px-5 py-4 text-sm text-ink-100 hover:bg-white/4">
          <span>
            {t("settings.audio.setup.title")}
            <span className="mt-1 block text-xs text-ink-400">{t("settings.audio.setup.sub")}</span>
          </span>
          <span aria-hidden className="text-ink-400">›</span>
        </Link>
      </section>
    </>
  );
}

/** Keyboard shortcuts on the Sing page (LiveBits.tsx Transport and app/page.tsx), not while typing; ⌘⇧P, from the
 *  app menu, on every page. */
// (the keys as shown: symbols and letters, except "space", which is a word: settings.shortcuts.space)
const SHORTCUTS: [string[], "pause" | "playPause" | "restart" | "previous" | "next" | "blind" | "favourite" | "mini"][] = [
  [["⌘", "⇧", "P"], "pause"],
  [["space"], "playPause"],
  [["R"], "restart"],
  [["←"], "previous"],
  [["→"], "next"],
  [["B"], "blind"],
  [["F"], "favourite"],
  [["M"], "mini"],
];
function Shortcuts() {
  const t = useT();
  return (
    <>
      <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.shortcuts.title")}</h2>
      <section className="glass overflow-hidden rounded-3xl px-5 py-2">
        {SHORTCUTS.map(([keys, what]) => (
          <div key={what} className="flex items-center justify-between gap-6 py-2 text-sm text-ink-100">
            <span>{t(`settings.shortcuts.${what}`)}</span>
            <span className="shrink-0">{keys.map(k => (
              <kbd key={k} className="mx-0.5 rounded-md border border-white/10 bg-white/5 px-1.5 py-0.5 font-sans text-[11px] text-ink-200">{k === "space" ? t("settings.shortcuts.space") : k}</kbd>
            ))}</span>
          </div>
        ))}
      </section>
    </>
  );
}

/** Updates: this version, "Check for updates automatically" (on), and "Check now". */
function Updates() {
  const t = useT();
  const [u, setU] = useUpdate();
  const [busy, setBusy] = useState(false);
  const win = useWindows();
  if (!u) return null;
  const check = async () => { setBusy(true); try { setU(await checkForUpdate()); } finally { setBusy(false); } };
  const line = updateLine(t, u, win);
  return (
    <>
      <div className="flex items-start justify-between gap-6 border-b border-white/6 px-5 py-4">
        <div className="min-w-0">
          <div className="text-sm font-medium text-ink-100">{t("settings.updates.version", { version: String(u.version) })}</div>
          {line && <p className="mt-1 text-xs text-ink-400">{line}</p>}
        </div>
        {u.supported && (u.state === "ready" && !u.waiting
          ? <button onClick={async () => { const v = await restartToUpdate(); if (v) setU(v); }}
                    className="shrink-0 rounded-lg bg-accent-500 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-accent-400">{t("settings.updates.restart")}</button>
          : <button onClick={check} disabled={busy || u.state === "checking" || u.state === "downloading" || u.waiting}
                    className="shrink-0 rounded-lg border border-white/8 bg-white/4 px-3 py-1.5 text-sm text-ink-100 transition hover:bg-white/8 disabled:opacity-40">{t("settings.updates.checkNow")}</button>)}
      </div>
      {u.supported && (
        <Row id="set-auto-update" title={t("settings.updates.auto.title")} on={u.auto} onChange={async on => setU(await setAutoUpdate(on))}
             sub={t("settings.updates.auto.sub")}
             more={t("settings.updates.auto.more")} />
      )}
    </>
  );
}

/** "See how it looks": an animation under a setting, on request. */
function SeeHow({ children }: { children: React.ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(v => !v)} aria-expanded={open}
              className="mt-1.5 text-xs text-accent-300 underline-offset-2 transition hover:text-accent-200 hover:underline">
        {open ? t("settings.seeHow.hide") : t("settings.seeHow.show")}
      </button>
      {open && <div className="mt-3 max-w-xl">{children}</div>}
    </>
  );
}

/** "More": the rest of a setting's explanation, on request (the row itself says it in one line). */
function More({ children }: { children: React.ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(v => !v)} aria-expanded={open}
              className="mt-1 mr-3 text-xs text-ink-500 underline-offset-2 transition hover:text-ink-200 hover:underline">   {/* (mr-3: apart from "See how it looks" beside it) */}
        {open ? t("settings.more.less") : t("settings.more.more")}
      </button>
      {open && <p className="mt-1 text-xs text-ink-400">{children}</p>}
    </>
  );
}

function Row({ id, title, sub, more, on, onChange, badge, warn, disabled, extra }: {
  id: string; title: string; sub: string; on: boolean; onChange: (on: boolean) => void; badge?: string; warn?: boolean; disabled?: boolean;
  /** the rest of the explanation, behind "More" */
  more?: React.ReactNode;
  /** under the description (e.g. "See how it looks") */
  extra?: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-6 border-b border-white/6 px-5 py-4 last:border-0">
      <div className="min-w-0">
        <label htmlFor={id} className="flex items-center gap-2 text-sm font-medium text-ink-100">
          {title}
          {badge && <span className={`rounded-full px-1.5 py-px text-[10px] font-normal ${warn ? "bg-notice-fill/12 text-notice" : "bg-white/6 text-ink-300"}`}>{badge}</span>}
        </label>
        <p className="mt-1 text-xs text-ink-400">{sub}</p>
        {more && <More>{more}</More>}
        {extra}
      </div>
      <button id={id} role="switch" aria-checked={on} disabled={disabled} onClick={() => onChange(!on)}
              className={`relative mt-0.5 h-6 w-10 shrink-0 rounded-full transition disabled:opacity-40 ${on ? "bg-accent-500" : "bg-white/12"}`}>
        <span className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all ${on ? "left-[18px]" : "left-0.5"}`} />
      </button>
    </div>
  );
}

/** How strictly takes are scored: Relaxed (the default), Steady or Pro, from the next take on. */
function ScoringLevel({ level, onChange }: { level: Level; onChange: (l: Level) => void }) {
  const t = useT();
  return (
    <section className="glass overflow-hidden rounded-3xl px-5 py-4">
      <div role="radiogroup" aria-label={t("settings.scoring.label")} className="flex gap-1 rounded-full bg-white/6 p-1">
        {(Object.keys(LEVELS) as Level[]).map(l => (
          <button key={l} id={`set-level-${l}`} role="radio" aria-checked={level === l} onClick={() => level !== l && onChange(l)}
                  className={`flex-1 rounded-full px-3 py-1.5 text-sm transition ${level === l ? "bg-accent-500 font-medium text-white" : "text-ink-300 hover:bg-white/6"}`}>
            {levelName(l)}
          </button>
        ))}
      </div>
      <p className="mt-3 text-xs text-ink-400">{levelAbout(level)}</p>
      <More>
        {level !== "relaxed" ? t("settings.scoring.more") : t("settings.scoring.moreRelaxed")}
      </More>
    </section>
  );
}

export default function SettingsPage() {
  const t = useT();
  const [s, setS] = useState<Settings | null>(null);
  const win = useWindows();
  const pc = usePc();
  // a setting that couldn't be saved (a full disk, say): said, rather than seeming to stick and reverting at restart
  const [unsaved, setUnsaved] = useState<string | null>(null);
  useEffect(() => { getSettings().then(setS).catch(() => setS(null)); }, []);
  // apps that aren't installed: their switch shows off and can't be turned on. Checked again when the window comes
  // back to the front, in case one was installed meanwhile (the check never asks macOS anything)
  const [missing, setMissing] = useState<{ spotify?: boolean; music?: boolean }>({});
  useEffect(() => {
    const check = () => setupStatus()
      .then(st => setMissing({ spotify: st.control?.spotify === "not_installed", music: st.control?.music === "not_installed" }))
      .catch(() => {});
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, []);
  // listening only: Spotify, Apple Music and YouTube all off (settings.rs `listen_only`)
  const listenOnly = !!s?.listen_only;
  const change = async (key: Exclude<keyof Settings, "small_mac" | "listen_only" | "scoring_level" | "language">, on: boolean) => {
    setUnsaved(null);
    try { await changeSetting(key, on); }
    catch (e) { setUnsaved(engineErrorText(t, e, t("settings.saveFailed"))); }
    setS(await getSettings());
  };
  const changeSetting = async (key: Exclude<keyof Settings, "small_mac" | "listen_only" | "scoring_level" | "language">, on: boolean) => {
    if (key === "youtube") await setYouTube(on);
    else if (key === "keep_models" || key === "battery_30fps" || key === "voice_breaks" || key === "shazam" || key === "show_lyrics" || key === "fade_guide") await setSetting(key, on);
    else if ((key === "control_spotify" || key === "control_music") && !on && s && !s.listening) {
      // the older way (following the apps by asking them, which controls both): turning one off moves to listening,
      // with the other app still allowed, as the old "Listen only" switch's way out did
      await setupControl(true);
      await setSetting(key, false);
    }
    else if (on && key !== "listening") await allowControl(key);
    else await setSetting(key, on);
  };

  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-2xl px-6 pb-16">
        <h1 className="mt-6 text-3xl font-semibold tracking-tight">{t("settings.title")}</h1>
        {unsaved && <p role="alert" className="mt-3 rounded-xl bg-off/10 px-4 py-2 text-sm text-off">{unsaved}</p>}
        <Audio />
        {!s ? <div className="glass mt-8 h-40 rounded-3xl shimmer" /> : (
          <>
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.control.title")}</h2>
            <p className="mb-3 px-1 text-xs text-ink-400">
              {s.listening
                ? win ? t("settings.control.introPc") : t("settings.control.introMac")
                : t("settings.control.introAsking")}
            </p>
            <section className="glass overflow-hidden rounded-3xl">
              <Row id="set-spotify" title={t("settings.control.spotify.title")} on={!missing.spotify && (!s.listening || s.control_spotify)} disabled={missing.spotify}
                   onChange={on => change("control_spotify", on)}
                   sub={missing.spotify ? t("settings.control.spotify.missing", { os: win ? "pc" : "mac" }) : t("settings.control.spotify.sub")} />
              <Row id="set-music" title={t("settings.control.music.title")} on={!missing.music && (!s.listening || s.control_music)} disabled={missing.music}
                   onChange={on => change("control_music", on)}
                   sub={missing.music ? (win ? t("settings.control.music.missingPc") : t("settings.control.music.missingMac"))
                                      : win ? t("settings.control.music.subPc") : t("settings.control.music.subMac")} />
              {/* (Windows: followed through the browser's media session, nothing for the browser or Windows to allow) */}
              {win
                ? <Row id="set-youtube" title={t("settings.control.youtube.title")} on={s.youtube} onChange={on => change("youtube", on)}
                       sub={t("settings.control.youtube.subPc")}
                       more={t("settings.control.youtube.morePc", { os: pc.os })} />
                : <Row id="set-youtube" title={t("settings.control.youtube.title")} badge={t("settings.control.youtube.badge")} warn on={s.youtube} onChange={on => change("youtube", on)}
                       sub={t("settings.control.youtube.subMac")}
                       more={t("settings.control.youtube.moreMac")} />}
            </section>
            {s.listening && (
              <p role="status" className="mt-3 px-1 text-xs text-ink-400">
                {listenOnly && win
                  ? t("settings.control.listenOnlyPc", { os: pc.os, apps: pc.apps })
                  : listenOnly
                  ? t("settings.control.listenOnlyMac")
                  : t("settings.control.allOff")}
              </p>
            )}
            {/* Windows has no Shazam: nothing to switch, only what happens */}
            {win ? (
              <section className="glass mt-8 overflow-hidden rounded-3xl px-5 py-4">
                <h3 className="text-sm font-medium text-ink-100">{t("settings.naming.title")}</h3>
                <p className="mt-1 text-xs text-ink-400">
                  {t("settings.naming.pc")}
                </p>
              </section>
            ) : <section className="glass mt-8 overflow-hidden rounded-3xl">
              <Row id="set-shazam" title={t("settings.naming.shazam.title")} on={s.shazam !== false} onChange={on => change("shazam", on)}
                   sub={t("settings.naming.shazam.sub")}
                   more={t("settings.naming.shazam.more")} />
              {s.listening && !s.control_spotify && !s.control_music && s.shazam === false && (
                <p role="status" className="bg-notice-fill/8 px-5 py-3 text-xs text-notice">
                  {t("settings.naming.shazam.warn")}
                </p>
              )}
            </section>}
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.lyrics.title")}</h2>
            <section className="glass overflow-hidden rounded-3xl">
              <Row id="set-show-lyrics" title={t("settings.lyrics.show.title")} on={s.show_lyrics !== false} onChange={on => change("show_lyrics", on)}
                   sub={t("settings.lyrics.show.sub")}
                   more={t("settings.lyrics.show.more")} />
            </section>
          </>
        )}
        {s && (
          <>
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.scoring.title")}</h2>
            <ScoringLevel level={levelOf(s.scoring_level)} onChange={async l => { await setScoringLevel(l); setS(await getSettings()); }} />
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.voice.title")}</h2>
            <section className="glass overflow-hidden rounded-3xl">
              <Row id="set-voice-breaks" title={t("settings.voice.breaks.title")} on={s.voice_breaks !== false} onChange={on => change("voice_breaks", on)}
                   sub={t("settings.voice.breaks.sub")}
                   more={t("settings.voice.breaks.more")} />
              <Row id="set-fade-guide" title={t("settings.voice.fadeGuide.title")} on={!!s.fade_guide} onChange={on => change("fade_guide", on)}
                   sub={t("settings.voice.fadeGuide.sub")}
                   more={t("settings.voice.fadeGuide.more")}
                   extra={<SeeHow><FadeGuideDemo /></SeeHow>} />
            </section>
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.performance.title")}</h2>
            <section className="glass overflow-hidden rounded-3xl">
              {/* (not on Windows: the engine doesn't see the battery there yet) */}
              {!win && <Row id="set-battery-30" title={t("settings.performance.battery.title")} on={s.battery_30fps !== false} onChange={on => change("battery_30fps", on)}
                   sub={t("settings.performance.battery.sub")}
                   more={t("settings.performance.battery.more")} />}
              {!s.small_mac && <Row id="set-keep-models" title={t("settings.performance.keepModels.title")} on={!!s.keep_models} onChange={on => change("keep_models", on)}
                   sub={t("settings.performance.keepModels.sub")}
                   more={t("settings.performance.keepModels.more", { os: win ? "pc" : "mac" })} />}
            </section>
          </>
        )}
        {s && <LanguagePicker choice={s.language ?? "auto"} onChange={async code => {
          setUnsaved(null);
          try { await setLanguage(code); } catch (e) { setUnsaved(engineErrorText(t, e, t("settings.saveFailed"))); }
          setS(await getSettings());
        }} />}
        <Shortcuts />
        <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">{t("settings.about.title")}</h2>
        <section className="glass overflow-hidden rounded-3xl">
          <Updates />
          <button onClick={() => reportProblem()} className="flex w-full items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <span>
              {t("settings.about.report.title")}
              <span className="mt-1 block text-xs text-ink-400">{win
                ? t("settings.about.report.subPc")
                : t("settings.about.report.subMac")}</span>
            </span>
            <span aria-hidden className="text-ink-400">›</span>
          </button>
          <button onClick={() => void openLink("https://singonkey.app/docs/")}
                  className="flex w-full items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <span>
              {t("settings.about.guides.title")}
              <span className="mt-1 block text-xs text-ink-400">{t("settings.about.guides.sub")}</span>
            </span>
            <span aria-hidden className="text-ink-400">↗</span>
          </button>
          <button onClick={() => void openLink("https://discord.gg/afHvqTNS6r")}
                  className="flex w-full items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <span>
              {t("settings.about.discord.title")}
              <span className="mt-1 block text-xs text-ink-400">{t("settings.about.discord.sub")}</span>
            </span>
            <span aria-hidden className="text-ink-400">↗</span>
          </button>
          <button onClick={() => void openLink("https://dsmurrell.com")}
                  className="flex w-full items-center gap-3 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <img src="/dsmurrell.png" alt="" width={20} height={20} className="rounded" />
            <span className="flex-1">
              {t("settings.about.madeBy.title")}
              <span className="mt-1 block text-xs italic text-ink-500">{t("settings.about.madeBy.tagline")}</span>
            </span>
            <span aria-hidden className="text-ink-400">↗</span>
          </button>
          <Link href="/fair-use" className="flex items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-sm text-ink-100 hover:bg-white/4">
            <span>
              {t("settings.about.fairUse.title")}
              <span className="mt-1 block text-xs text-ink-400">{t("settings.about.fairUse.sub")}</span>
            </span>
            <span aria-hidden className="text-ink-400">›</span>
          </Link>
          <Link href="/licences" className="flex items-center justify-between gap-6 px-5 py-4 text-sm text-ink-100 hover:bg-white/4">
            <span>
              {t("settings.about.licences.title")}
              <span className="mt-1 block text-xs text-ink-400">{t("settings.about.licences.sub")}</span>
            </span>
            <span aria-hidden className="text-ink-400">›</span>
          </Link>
        </section>
        <p className="mt-3 px-1 text-xs text-ink-400">
          {t("settings.about.footer", { os: win ? "pc" : "mac" })}
        </p>
      </main>
    </div>
  );
}
