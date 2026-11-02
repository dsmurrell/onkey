// Which computer the window is on, for what it says (System Settings or Windows Settings, this Mac or this PC) and
// which setup steps it shows. Read from the window's own browser engine: WebView2 on Windows names Windows in its
// user agent, the Mac's WebKit names Macintosh, WebKitGTK on Linux names Linux (or X11). The Mac whenever it can't
// tell (and while the page is built), so the Mac's window is never changed by it. Linux is a PC like Windows (no
// macOS permissions, the window's own title bar, reports saved to the Desktop): `useWindows` is true there too, and
// `usePc` gives the words that differ (Windows Settings or the sound settings, File Explorer or the file manager).
import { useSyncExternalStore } from "react";
import { t } from "./i18n";

/** Is this user agent Windows'? */
export function isWindowsAgent(ua: string | undefined | null): boolean {
  return !!ua && /\bWindows\b/.test(ua);
}

/** Is this user agent Linux's (WebKitGTK: "X11; Linux x86_64")? Never Windows' or Android's. */
export function isLinuxAgent(ua: string | undefined | null): boolean {
  return !!ua && !isWindowsAgent(ua) && /\b(Linux|X11)\b/.test(ua) && !/Android/.test(ua);
}

/** On a PC: Windows, or Linux, which the window treats as Windows (see the note at the top). False on the Mac, and
 *  when there's no window to ask. */
export function onWindows(): boolean {
  return typeof navigator !== "undefined" && (isWindowsAgent(navigator.userAgent) || isLinuxAgent(navigator.userAgent));
}

/** On Linux? */
export function onLinux(): boolean {
  return typeof navigator !== "undefined" && isLinuxAgent(navigator.userAgent);
}

const never = () => () => {};

/** `onWindows()` for components: false while the page is put together (hydration), then the real answer. */
export function useWindows(): boolean {
  return useSyncExternalStore(never, onWindows, () => false);
}

/** The words a PC's window says that differ between Windows and Linux (in the window's language, read when called). */
export function pcWords(linux: boolean) {
  const k = linux ? "linux" : "windows";
  return { os: t(`platform.${k}.os`), settings: t(`platform.${k}.settings`), sound: t(`platform.${k}.sound`),
           soundInput: t(`platform.${k}.soundInput`), defaultInput: t(`platform.${k}.defaultInput`),
           files: t(`platform.${k}.files`), filesThe: t(`platform.${k}.filesThe`), apps: t(`platform.${k}.apps`) };
}

/** `pcWords` for this PC (Windows' while the page is put together). */
export function usePc() {
  return pcWords(useSyncExternalStore(never, onLinux, () => false));
}

/** First-run setup's steps. Windows has nothing to allow beyond the microphone (its switch is shown in the mic
 *  check) and no Apple Events: no Permissions or Music apps step there. */
export function setupSteps(windows: boolean) {
  return windows ? ["Welcome", "Mic check", "Ready"] as const
    : ["Welcome", "Permissions", "Music apps", "Mic check", "Ready"] as const;
}
export type SetupStep = ReturnType<typeof setupSteps>[number];
const STEP_KEY = { "Welcome": "welcome", "Permissions": "permissions", "Music apps": "musicApps", "Mic check": "micCheck", "Ready": "ready" } as const;
/** A setup step's name as shown (the step itself, SetupStep, is an id and stays English). */
export const setupStepLabel = (step: SetupStep) => t(`platform.setupStep.${STEP_KEY[step]}`);

/** Where a saved file went, in words, from the path the app gives: "your Desktop" (Windows always; Linux when there
 *  is one), "your home folder" (Linux with no Desktop folder: "~/…"), else "the folder <name>" (Linux names the
 *  Desktop in its own language: "Рабочий стол", "Bureau"). Always a fragment from the catalog, so every language's
 *  "Saved to {folder} as …" reads right with a bare name too. */
export function savedFolder(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  const dir = parts.length > 1 ? parts[parts.length - 2] : "";
  if (dir === "Desktop") return t("platform.savedFolder.desktop");
  if (dir === "~" || dir === "") return t("platform.savedFolder.home");
  return t("platform.savedFolder.named", { folder: dir });
}

/** A saved file's name from the path the app gives ("~\\Desktop\\OnKey-report-….zip" or "~/Desktop/…"). */
export function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
