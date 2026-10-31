// Which computer the window is on, for what it says (System Settings or Windows Settings, this Mac or this PC) and
// which setup steps it shows. Read from the window's own browser engine: WebView2 on Windows names Windows in its
// user agent, the Mac's WebKit names Macintosh, WebKitGTK on Linux names Linux (or X11). The Mac whenever it can't
// tell (and while the page is built), so the Mac's window is never changed by it. Linux is a PC like Windows (no
// macOS permissions, the window's own title bar, reports saved to the Desktop): `useWindows` is true there too, and
// `usePc` gives the words that differ (Windows Settings or the sound settings, File Explorer or the file manager).
import { useSyncExternalStore } from "react";

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

/** The words a PC's window says that differ between Windows and Linux. */
export function pcWords(linux: boolean) {
  return linux
    ? { os: "Linux", settings: "Sound Settings", sound: "your system’s Sound settings", soundInput: "your system’s Sound settings → Input",
        defaultInput: "the system’s default input", files: "your file manager",
        apps: "Spotify or YouTube in your browser" }
    : { os: "Windows", settings: "Windows Settings", sound: "Windows Settings → System → Sound", soundInput: "Windows Settings → System → Sound → Input",
        defaultInput: "Windows’ default input", files: "File Explorer",
        apps: "Spotify, Apple Music or YouTube in your browser" };
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

/** Where a saved file went, in words, from the path the app gives: "your Desktop" (Windows always; Linux when there
 *  is one), "your home folder" (Linux with no Desktop folder: "~/…"), else the folder's name. */
export function savedFolder(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  const dir = parts.length > 1 ? parts[parts.length - 2] : "";
  if (dir === "Desktop") return "your Desktop";
  if (dir === "~" || dir === "") return "your home folder";
  return dir;
}

/** A saved file's name from the path the app gives ("~\\Desktop\\OnKey-report-….zip" or "~/Desktop/…"). */
export function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
