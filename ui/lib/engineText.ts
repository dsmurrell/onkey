// The lines the engine decides (the status line, the share ask, a refused timing check, a setting it couldn't
// save), said in the window's language: each comes with a stable kind or code and its values
// (src-tauri/messages/KINDS.md), looked up in the catalog under engine.*. A kind the catalog doesn't know (an
// engine newer than the window) is shown in the engine's own English, as it sent it. Never parse the English.
import type { MessageKey, Translator } from "./i18n";

/** An error the engine gave a code for: `english` is its own words (the fallback, and for the log). */
export class EngineError extends Error {
  code: string;
  english: string;
  constructor(code: string, english: string) {
    super(english);
    this.code = code;
    this.english = english;
  }
}

/** Values as the catalog's messages take them: `points` is a number (it picks the plural); the rest stay text,
 *  as the engine sent them (a song called "1999" isn't a number). */
const values = (params: Record<string, string> | undefined) => {
  const out: Record<string, string | number> = { ...params };
  if (params?.points != null && /^\d+$/.test(params.points)) out.points = Number(params.points);
  return out;
};

/** The message for a kind, or null when the catalog doesn't have it (or lacks a value it needs). */
function say(t: Translator, key: string, params?: Record<string, string>): string | null {
  const k = key as MessageKey;
  if (!t.has(k)) return null;
  const said = t(k, values(params) as never);
  return said === key ? null : said;
}

/** Listening's status line. */
export const lineText = (t: Translator, line: { kind: string; params?: Record<string, string>; text: string }): string =>
  say(t, `engine.line.${line.kind}`, line.params) ?? line.text;

/** The take page's share ask ("Your best on Yellow by 12 points"). */
export const shareReason = (t: Translator, p: { kind: string; params?: Record<string, string>; reason: string }): string =>
  (p.params ? say(t, `engine.share.${p.kind}`, p.params) : null) ?? p.reason;

/** How a song sits against your range, on its Library card (range.rs Fit: `fit` is its key, `fit_label` the
 *  engine's English, shown only for a key the catalog doesn't know). */
export const fitText = (t: Translator, s: { fit?: string | null; fit_label?: string | null }): string | null =>
  (s.fit ? say(t, `engine.fit.${s.fit}`) : null) ?? s.fit_label ?? null;

/** What a file in a problem report is (report.rs: `what_kind` and its values; `what` is the engine's English).
 *  `os`: "pc" or "mac", for "this PC" / "this Mac". */
export const reportWhat = (t: Translator, f: { what: string; what_kind?: string; what_params?: Record<string, string> }, os: "pc" | "mac"): string =>
  (f.what_kind ? say(t, `engine.reportWhat.${f.what_kind}`, { ...f.what_params, os }) : null) ?? f.what;

/** The learning models' trouble, for the notice: a code the catalog has words for (unloadable), else the engine's
 *  technical detail as sent (a file name, the loader's message), else nothing. */
export const modelsError = (t: Translator, m: { code?: string; error?: string }): string =>
  (m.code ? say(t, `engine.models.${m.code}`) : null) ?? m.error ?? "";

/** Where the browser setting OnKey needs lives (`setting_kind`: safari, chromium), else the engine's English. */
export const youtubeSetting = (t: Translator, b: { setting: string; setting_kind?: string }): string =>
  (b.setting_kind ? say(t, `engine.youtubeSetting.${b.setting_kind}`) : null) ?? b.setting;

/** An app the engine names, or what it is when it has no name ("your music app", "your browser": `kind`
 *  music_app, browser), in the window's language. */
export const appText = (t: Translator, app: string | null | undefined, kind?: string | null): string =>
  app || say(t, `engine.app.${kind ?? "music_app"}`) || say(t, "engine.app.music_app") || "";

/** Why an update failed, for "({error})": the updater's own message for a check or download that failed
 *  (technical, as sent); none for install_failed, whose English is only "the update couldn't be installed", which the
 *  sentence around it already says. */
export const updateWhy = (u: { code?: string; error?: string }): string | null =>
  u.code === "install_failed" ? null : u.error || null;

/** A timing check the engine refused (`code`; "other" or none: its English). */
export const timingError = (t: Translator, r: { code?: string; error?: string }): string =>
  (r.code && r.code !== "other" ? say(t, `engine.timing.${r.code}`) : null) ?? r.error ?? "";

/** What to show for an error: an EngineError's code in the window's language, else the error's own words, else
 *  `fallback`. setting_unsaved keeps the system's reason (in the engine's English: "No space left on device"),
 *  which it carries in brackets. */
export function engineErrorText(t: Translator, e: unknown, fallback: string): string {
  if (e instanceof EngineError) {
    if (e.code === "setting_unsaved") {
      const why = /^Couldn’t save the setting \(([\s\S]*)\)\. It holds until OnKey quits\.$/.exec(e.english);
      if (why) return say(t, "engine.settingUnsaved", { error: why[1] }) ?? e.english;
    }
    return e.english.trim() || fallback;
  }
  if (typeof e === "string" && e.trim()) return e;
  if (e instanceof Error && e.message.trim()) return e.message;
  return fallback;
}
