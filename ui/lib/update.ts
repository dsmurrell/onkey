// Updates (src-tauri/src/updater.rs): OnKey's version, and whether a newer one is downloaded and waiting.
import { useEffect, useState } from "react";
import { inApp } from "@/lib/session";

export type UpdateStatus = {
  /** this OnKey's version */
  version: string;
  /** release builds only: development builds never check */
  supported: boolean;
  /** an official build (scripts/release.sh): installs updates. Any other release build (a fork's, or built from
   *  source) only says when a new official version is out ("official" state) */
  official?: boolean;
  /** "Check for updates automatically" */
  auto: boolean;
  state: "idle" | "checking" | "current" | "downloading" | "ready" | "error" | "official";
  /** the newer version, once found */
  available?: string;
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
export const restartToUpdate = () => invoke<UpdateStatus>("update_restart");

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
