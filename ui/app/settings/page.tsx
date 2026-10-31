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
import { allowControl, getMics, getOutputTiming, getSettings, openLink, setMic, setScoringLevel, setSetting, setupControl, setupStatus, setYouTube, type Mics, type OutputTiming, type Settings } from "@/lib/session";
import { LEVELS, levelOf, type Level } from "@/lib/music";
import { errorText } from "@/lib/dataSafety";

/** Audio: the output you hear the music on, which microphone OnKey listens to (automatic: the Mac's input, as chosen
 *  in System Settings → Sound, or one picked here; Setup's mic check and your takes use the same one), and setup again
 *  (the Sing page opens it with ?setup). */
function Audio() {
  const [m, setM] = useState<Mics | null>(null);
  const [out, setOut] = useState<OutputTiming | null>(null);
  useEffect(() => {
    let alive = true;
    const load = () => {
      getMics().then(v => { if (alive) setM(v); }).catch(() => {});
      getOutputTiming().then(v => { if (alive) setOut(v); }).catch(() => {});
    };
    load();
    const t = window.setInterval(() => { if (document.visibilityState === "visible") load(); }, 3000);   // a mic or headphones plugged in or out shows up
    return () => { alive = false; clearInterval(t); };
  }, []);
  const missing = !!m && m.preferred != null && !m.devices.some(d => d.uid === m.preferred);
  const pick = async (uid: string) => { await setMic(uid === "" ? null : uid); setM(await getMics()); };
  return (
    <>
      <h2 className="mt-6 mb-2 px-1 text-sm font-medium text-ink-200">Audio</h2>
      <section className="glass overflow-hidden rounded-3xl">
        {m && (
          <div className="flex items-start justify-between gap-6 border-b border-white/6 px-5 py-4">
            <div className="min-w-0">
              <label htmlFor="set-mic" className="text-sm font-medium text-ink-100">Microphone</label>
              <p className="mt-1 text-xs text-ink-400">
                {m.chosen ? <>Now: {m.chosen.name}.</> : <span className="text-amber-200">No microphone is connected.</span>}
                {missing && " The one you chose isn’t connected, so OnKey uses the Mac’s own input until it is."}
              </p>
              <More>
                Automatic uses your Mac’s input (System Settings → Sound), but not Bluetooth headphones’ mic while the Mac
                has its own, so the music stays at full quality in them. Setup’s mic check and your takes use the same one.
                Virtual devices other apps add (Krisp, Loopback, BlackHole, Zoom, Teams) are listed apart: pick one only to
                sing through it on purpose, as it gives OnKey that app’s sound, not your mic’s.
              </More>
            </div>
            <select id="set-mic" value={m.preferred ?? ""} onChange={e => pick(e.target.value)}
                    className="mt-0.5 max-w-56 shrink-0 rounded-lg border border-white/8 bg-white/4 px-2.5 py-1.5 text-sm text-ink-100 outline-none focus:border-accent-400">
              <option value="">Automatic</option>
              {m.devices.filter(d => !d.virtual).map(d => <option key={d.uid} value={d.uid}>{d.name}</option>)}
              {m.devices.some(d => d.virtual) && (
                <optgroup label="Virtual (from other apps)">
                  {m.devices.filter(d => d.virtual).map(d => <option key={d.uid} value={d.uid}>{d.name}</option>)}
                </optgroup>
              )}
              {missing && <option value={m.preferred!}>Not connected</option>}
            </select>
          </div>
        )}
        {out && (
          <div className="border-b border-white/6 px-5 py-4">
            <div className="text-sm font-medium text-ink-100">Output</div>
            <p className="mt-1 text-xs text-ink-400">Now: {out.name}. Change it in System Settings → Sound.</p>
          </div>
        )}
        <Link href="/?setup" className="flex items-center justify-between gap-6 px-5 py-4 text-sm text-ink-100 hover:bg-white/4">
          <span>
            Run setup again
            <span className="mt-1 block text-xs text-ink-400">Permissions and the mic check, as on the first day.</span>
          </span>
          <span aria-hidden className="text-ink-400">›</span>
        </Link>
      </section>
    </>
  );
}

/** Keyboard shortcuts on the Sing page (LiveBits.tsx Transport and app/page.tsx), not while typing; ⌘⇧P, from the
 *  app menu, on every page. */
const SHORTCUTS: [string[], string][] = [
  [["⌘", "⇧", "P"], "Pause or resume listening (on any page)"],
  [["Space"], "Play or pause"],
  [["R"], "Start the song over"],
  [["←"], "Previous song"],
  [["→"], "Next song, or skip to the singing"],
  [["B"], "Show or hide the notes (Blind take)"],
  [["F"], "Favourite the song"],
  [["M"], "Mini mode: a small lyrics bar on top of other apps"],
];
function Shortcuts() {
  return (
    <>
      <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">Keyboard shortcuts</h2>
      <section className="glass overflow-hidden rounded-3xl px-5 py-2">
        {SHORTCUTS.map(([keys, what]) => (
          <div key={what} className="flex items-center justify-between gap-6 py-2 text-sm text-ink-100">
            <span>{what}</span>
            <span className="shrink-0">{keys.map(k => (
              <kbd key={k} className="mx-0.5 rounded-md border border-white/10 bg-white/5 px-1.5 py-0.5 font-sans text-[11px] text-ink-200">{k}</kbd>
            ))}</span>
          </div>
        ))}
      </section>
    </>
  );
}

/** Updates: this version, "Check for updates automatically" (on), and "Check now". */
function Updates() {
  const [u, setU] = useUpdate();
  const [busy, setBusy] = useState(false);
  if (!u) return null;
  const check = async () => { setBusy(true); try { setU(await checkForUpdate()); } finally { setBusy(false); } };
  const line = !u.supported ? "Development build: it doesn’t update itself."
    : u.waiting ? `OnKey ${u.available} is installed when this song is over, then OnKey restarts.`
    : u.state === "ready" ? `OnKey ${u.available} is ready. Restart to update, or it’s installed when you quit.`
    : u.state === "downloading" ? `Downloading OnKey ${u.available}…`
    : u.state === "checking" ? "Checking…"
    : u.state === "current" ? "You have the latest version."
    : u.state === "error" ? (u.available ? `Couldn’t update to OnKey ${u.available}${u.error ? ` (${u.error})` : ""}. Check now tries again.`
                                          : `Couldn’t check for updates just now${u.error ? ` (${u.error})` : ""}.`)
    : "";
  return (
    <>
      <div className="flex items-start justify-between gap-6 border-b border-white/6 px-5 py-4">
        <div className="min-w-0">
          <div className="text-sm font-medium text-ink-100">OnKey {u.version}</div>
          {line && <p className="mt-1 text-xs text-ink-400">{line}</p>}
        </div>
        {u.supported && (u.state === "ready" && !u.waiting
          ? <button onClick={async () => setU(await restartToUpdate())}
                    className="shrink-0 rounded-lg bg-accent-500 px-3 py-1.5 text-sm font-medium text-white transition hover:bg-accent-400">Restart to update</button>
          : <button onClick={check} disabled={busy || u.state === "checking" || u.state === "downloading" || u.waiting}
                    className="shrink-0 rounded-lg border border-white/8 bg-white/4 px-3 py-1.5 text-sm text-ink-100 transition hover:bg-white/8 disabled:opacity-40">Check now</button>)}
      </div>
      {u.supported && (
        <Row id="set-auto-update" title="Check for updates automatically" on={u.auto} onChange={async on => setU(await setAutoUpdate(on))}
             sub="Once a day, in the background. It never interrupts a song."
             more="OnKey asks GitHub for a newer version and downloads it in the background. It’s installed when you restart or quit." />
      )}
    </>
  );
}

/** "See how it looks": an animation under a setting, on request. */
function SeeHow({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(v => !v)} aria-expanded={open}
              className="mt-1.5 text-xs text-accent-300 underline-offset-2 transition hover:text-accent-200 hover:underline">
        {open ? "Hide" : "See how it looks"}
      </button>
      {open && <div className="mt-3 max-w-xl">{children}</div>}
    </>
  );
}

/** "More": the rest of a setting's explanation, on request (the row itself says it in one line). */
function More({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(v => !v)} aria-expanded={open}
              className="mt-1 text-xs text-ink-500 underline-offset-2 transition hover:text-ink-200 hover:underline">
        {open ? "Less" : "More"}
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
          {badge && <span className={`rounded-full px-1.5 py-px text-[10px] font-normal ${warn ? "bg-amber-400/12 text-amber-200" : "bg-white/6 text-ink-300"}`}>{badge}</span>}
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
  return (
    <section className="glass overflow-hidden rounded-3xl px-5 py-4">
      <div role="radiogroup" aria-label="Scoring level" className="flex gap-1 rounded-full bg-white/6 p-1">
        {(Object.keys(LEVELS) as Level[]).map(l => (
          <button key={l} id={`set-level-${l}`} role="radio" aria-checked={level === l} onClick={() => level !== l && onChange(l)}
                  className={`flex-1 rounded-full px-3 py-1.5 text-sm transition ${level === l ? "bg-accent-500 font-medium text-white" : "text-ink-300 hover:bg-white/6"}`}>
            {LEVELS[l].name}
          </button>
        ))}
      </div>
      <p className="mt-3 text-xs text-ink-400">{LEVELS[level].about}</p>
      <More>
        {level !== "relaxed" && "Vibrato is fine: your pitch is judged by the middle of its wobble. "}
        Every score in OnKey is shown at this level, your earlier takes too: they’re scored again from your singing as it was kept.
      </More>
    </section>
  );
}

export default function SettingsPage() {
  const [s, setS] = useState<Settings | null>(null);
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
  const change = async (key: Exclude<keyof Settings, "small_mac" | "listen_only" | "scoring_level">, on: boolean) => {
    setUnsaved(null);
    try { await changeSetting(key, on); }
    catch (e) { setUnsaved(errorText(e, "Couldn’t save the setting.")); }
    setS(await getSettings());
  };
  const changeSetting = async (key: Exclude<keyof Settings, "small_mac" | "listen_only" | "scoring_level">, on: boolean) => {
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
        <h1 className="mt-6 text-3xl font-semibold tracking-tight">Settings</h1>
        {unsaved && <p role="alert" className="mt-3 rounded-xl bg-off/10 px-4 py-2 text-sm text-off">{unsaved}</p>}
        <Audio />
        {!s ? <div className="glass mt-8 h-40 rounded-3xl shimmer" /> : (
          <>
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">Let OnKey control</h2>
            <p className="mb-3 px-1 text-xs text-ink-400">
              {s.listening
                ? "On by default: OnKey reads what’s playing and where (exact, instant positions), and can play, pause, seek and start songs. Nothing else. macOS asks once for each."
                : "OnKey follows Spotify and Apple Music by asking them, so it controls them already."}
            </p>
            <section className="glass overflow-hidden rounded-3xl">
              <Row id="set-spotify" title="Spotify" on={!missing.spotify && (!s.listening || s.control_spotify)} disabled={missing.spotify}
                   onChange={on => change("control_spotify", on)}
                   sub={missing.spotify ? "Spotify isn’t installed on this Mac." : "Start, pause and skip songs in Spotify from OnKey."} />
              <Row id="set-music" title="Apple Music" on={!missing.music && (!s.listening || s.control_music)} disabled={missing.music}
                   onChange={on => change("control_music", on)}
                   sub={missing.music ? "The Music app isn’t on this Mac." : "The same for the Music app (songs in your library)."} />
              <Row id="set-youtube" title="YouTube in your browser" badge="Bigger permission" warn on={s.youtube} onChange={on => change("youtube", on)}
                   sub="Read and control the YouTube tab in Chrome, Brave, Edge or Safari."
                   more="Needs “Allow JavaScript from Apple Events” turned on in the browser. While it’s on (and for 10 minutes after you open a YouTube song from OnKey), OnKey listens to your browser’s sound, whatever it is, calls and podcasts included: sound it doesn’t recognise may go to Shazam as a short fingerprint (never the audio) unless Shazam is off below." />
            </section>
            {s.listening && (
              <p role="status" className="mt-3 px-1 text-xs text-ink-400">
                {listenOnly
                  ? <>OnKey is listening only: it follows songs by their sound, and doesn’t touch any app. Positions are a little
                      less exact, and it can’t start or restart songs. Turning an app off here doesn’t take back macOS’s
                      permission: that’s in System Settings → Privacy &amp; Security → Automation.</>
                  : "With all of these off, OnKey follows songs by their sound alone."}
              </p>
            )}
            <section className="glass mt-8 overflow-hidden rounded-3xl">
              <Row id="set-shazam" title="Use Shazam to name new songs" on={s.shazam !== false} onChange={on => change("shazam", on)}
                   sub="Names new songs from a short fingerprint of the sound, not the audio."
                   more="Songs you’ve sung before are recognised on this Mac. New songs from Spotify and Apple Music are named by the app itself while OnKey may control it; Shazam names the rest. When off, nothing is sent to Shazam and only songs already learned on this Mac are recognised there. Album covers are still looked up by artist and title in Apple’s iTunes catalogue." />
              {s.listening && !s.control_spotify && !s.control_music && s.shazam === false && (
                <p role="status" className="bg-amber-400/8 px-5 py-3 text-xs text-amber-200">
                  With Spotify and Apple Music off, new songs can only be learned with Shazam on. Songs already learned still work.
                </p>
              )}
            </section>
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">Lyrics</h2>
            <section className="glass overflow-hidden rounded-3xl">
              <Row id="set-show-lyrics" title="Show lyrics" on={s.show_lyrics !== false} onChange={on => change("show_lyrics", on)}
                   sub="From LRCLIB, a free community lyrics database."
                   more="When off, OnKey doesn’t ask LRCLIB for any, and you sing to the notes alone: the pitch guide and your score work as usual." />
            </section>
          </>
        )}
        {s && (
          <>
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">Scoring</h2>
            <ScoringLevel level={levelOf(s.scoring_level)} onChange={async l => { await setScoringLevel(l); setS(await getSettings()); }} />
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">Your voice</h2>
            <section className="glass overflow-hidden rounded-3xl">
              <Row id="set-voice-breaks" title="Remind me to rest my voice" on={s.voice_breaks !== false} onChange={on => change("voice_breaks", on)}
                   sub="A quiet reminder to take a break after about 35 minutes of singing."
                   more="A 20-minute pause from singing starts the count again." />
              <Row id="set-fade-guide" title="Fade the guide on songs you know" on={!!s.fade_guide} onChange={on => change("fade_guide", on)}
                   sub="Your pitch line fades while you’re on the note, so you listen to yourself more."
                   more="It starts once you’ve sung a song well (your best 80 or more, or your last two takes 75 or more). The better you know it, the fainter it gets; more than a semitone off, it shows as usual. The notes always stay, and you can bring the full guide back for any take."
                   extra={<SeeHow><FadeGuideDemo /></SeeHow>} />
            </section>
            <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">Performance</h2>
            <section className="glass overflow-hidden rounded-3xl">
              <Row id="set-battery-30" title="Save battery while singing" on={s.battery_30fps !== false} onChange={on => change("battery_30fps", on)}
                   sub="On battery, the notes are drawn a little less often. Still smooth."
                   more="30 times a second instead of 60: half the drawing work for your Mac." />
              {!s.small_mac && <Row id="set-keep-models" title="Keep learning models ready" on={!!s.keep_models} onChange={on => change("keep_models", on)}
                   sub="New songs are learned faster, but OnKey uses more memory."
                   more="About 1.5 GB more, also while you sing songs OnKey already knows. Off, the models are loaded when a new song is learned and let go after. On by default on Macs with 32 GB or more." />}
            </section>
          </>
        )}
        <Shortcuts />
        <h2 className="mt-8 mb-2 px-1 text-sm font-medium text-ink-200">About</h2>
        <section className="glass overflow-hidden rounded-3xl">
          <Updates />
          <button onClick={() => reportProblem()} className="flex w-full items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <span>
              Report a problem
              <span className="mt-1 block text-xs text-ink-400">Save OnKey’s logs and timings as a zip to attach to a GitHub issue. You see what’s in it first; no lyrics or audio, nothing uploaded.</span>
            </span>
            <span aria-hidden className="text-ink-400">›</span>
          </button>
          <button onClick={() => void openLink("https://singonkey.app/docs/")}
                  className="flex w-full items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <span>
              Guides and help
              <span className="mt-1 block text-xs text-ink-400">How OnKey works, scoring, fixing problems and singing comfortably, on singonkey.app.</span>
            </span>
            <span aria-hidden className="text-ink-400">↗</span>
          </button>
          <button onClick={() => void openLink("https://discord.gg/afHvqTNS6r")}
                  className="flex w-full items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <span>
              Join OnKey’s Discord
              <span className="mt-1 block text-xs text-ink-400">Ask questions, report a song that’s out of step, and suggest ideas.</span>
            </span>
            <span aria-hidden className="text-ink-400">↗</span>
          </button>
          <button onClick={() => void openLink("https://dsmurrell.com")}
                  className="flex w-full items-center gap-3 border-b border-white/6 px-5 py-4 text-left text-sm text-ink-100 hover:bg-white/4">
            <img src="/dsmurrell.png" alt="" width={20} height={20} className="rounded" />
            <span className="flex-1">
              Made by Daniel Murrell
              <span className="mt-1 block text-xs italic text-ink-500">Saving shower singers from themselves since 2026</span>
            </span>
            <span aria-hidden className="text-ink-400">↗</span>
          </button>
          <Link href="/fair-use" className="flex items-center justify-between gap-6 border-b border-white/6 px-5 py-4 text-sm text-ink-100 hover:bg-white/4">
            <span>
              Using OnKey fairly
              <span className="mt-1 block text-xs text-ink-400">What OnKey is for, and how to share your singing without sharing the music.</span>
            </span>
            <span aria-hidden className="text-ink-400">›</span>
          </Link>
          <Link href="/licences" className="flex items-center justify-between gap-6 px-5 py-4 text-sm text-ink-100 hover:bg-white/4">
            <span>
              Open-source licences
              <span className="mt-1 block text-xs text-ink-400">The models, data, fonts and libraries OnKey is built on, and their licences.</span>
            </span>
            <span aria-hidden className="text-ink-400">›</span>
          </Link>
        </section>
        <p className="mt-3 px-1 text-xs text-ink-400">
          For personal practice at home. What OnKey learns stays on this Mac: please keep it to yourself, and don’t post
          takes or screen recordings with the music in them.
        </p>
      </main>
    </div>
  );
}
