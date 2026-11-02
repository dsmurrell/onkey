// Updates (src-tauri/src/updater.rs): OnKey's version, and whether a newer one is downloaded and waiting.
import { useEffect, useState } from "react";
import { inApp } from "@/lib/session";

export type UpdateStatus = {
  /** this OnKey's version */
  version: string;
  /** release builds only: development builds and previews never check */
  supported: boolean;
  /** a preview (a version like 0.2.0-preview.3: CI's Windows and Linux test builds): not supported, and the next
   *  preview is installed over it by hand */
  preview?: boolean;
  /** an official build (scripts/release.sh): installs updates. Any other release build (a fork's, or built from
   *  source) only says when a new official version is out ("official" state) */
  official?: boolean;
  /** "Check for updates automatically" */
  auto: boolean;
  state: "idle" | "checking" | "current" | "downloading" | "ready" | "error" | "official";
  /** the newer version, once found */
  available?: string;
  /** an error's kind (KINDS.md): check_failed, download_failed, install_failed. `error` is the updater's own
   *  message (English, technical), shown as the detail */
  code?: "check_failed" | "download_failed" | "install_failed";
  error?: string;
  /** "Restart to update" was pressed: restarting once the song playing is over */
  waiting: boolean;
  /** when it last checked (epoch s) */
  checked: number | null;
};

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const updateStatus = () => invoke<UpdateStatus>("update_status");
export const checkForUpdate = () => invoke<UpdateStatus>("update_check");
export const setAutoUpdate = (on: boolean) => invoke<UpdateStatus>("update_set_auto", { on });
/** "Restart to update": the status, or null when no update was ready after all (`{ok: false, code: "not_ready"}`:
 *  the status in hand stays). */
export const restartToUpdate = async (): Promise<UpdateStatus | null> => {
  const r = await invoke<UpdateStatus | { ok: false; code: string }>("update_restart");
  return "ok" in r && r.ok === false ? null : r as UpdateStatus;
};
export { updateFailed } from "./updateLine";

/** The update status, kept current (the engine says when it changes). Null outside the app. */
export function useUpdate(): [UpdateStatus | null, (s: UpdateStatus) => void] {
  const [s, setS] = useState<UpdateStatus | null>(null);
  useEffect(() => {
    if (!inApp()) return;
    let alive = true, unlisten: (() => void) | undefined;
    updateStatus().then(v => { if (alive) setS(v); }).catch(() => {});
    import("@tauri-apps/api/event").then(({ listen }) =>
      listen<UpdateStatus>("onkey-update", e => { if (alive) setS(e.payload); })).then(u => { if (alive) unlisten = u; else u(); });
    return () => { alive = false; unlisten?.(); };
  }, []);
  return [s, setS];
}
