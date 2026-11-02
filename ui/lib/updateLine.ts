// Settings' line under "OnKey <version>": what the updater is doing, or why this build doesn't update itself, in the
// window's language. (Apart from lib/update.ts so it can be unit-tested: that file imports the app's session.)
import type { Translator } from "./i18n";
import type { UpdateStatus } from "./update";
import { updateWhy } from "./engineText";

/** An error state's kind: the engine's code, else (an older engine) whether a version was found. */
export const updateFailed = (u: Pick<UpdateStatus, "code" | "available">): "check" | "update" =>
  u.code === "check_failed" ? "check" : u.code ? "update" : u.available ? "update" : "check";

/** `win`: on Windows (lib/platform useWindows), where an unsupported build is a tester's; a preview (CI's Windows
 *  and Linux test builds) is a tester's on any platform. */
export function updateLine(t: Translator, u: UpdateStatus, win: boolean): string {
  const version = String(u.available);
  const why = updateWhy(u);
  return !u.supported ? (u.preview || win ? t("settings.updates.unsupportedPc") : t("settings.updates.unsupportedMac"))
    : u.waiting ? t("settings.updates.waiting", { version })
    : u.state === "ready" ? t("settings.updates.ready", { version })
    : u.state === "downloading" ? t("settings.updates.downloading", { version })
    : u.state === "checking" ? t("settings.updates.checking")
    : u.state === "official" ? t("settings.updates.official", { version })
    : u.state === "current" ? (u.official === false ? t("settings.updates.currentSource") : t("settings.updates.current"))
    : u.state === "error" ? (updateFailed(u) === "update"
        ? (why ? t("settings.updates.updateFailedWhy", { version, error: why }) : t("settings.updates.updateFailed", { version }))
        : why ? t("settings.updates.checkFailedWhy", { error: why }) : t("settings.updates.checkFailed"))
    : "";
}
