"use client";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { inApp } from "@/lib/session";

/** Development tools: shown only in a development build started by `./run` (ONKEY_DEV_TOOLS), never in a release.
 *  Each is marked in purple (dashed, "dev") so it's never mistaken for the app itself. */
let known: boolean | null = null;
export function useDevTools(): boolean {
  const [on, setOn] = useState(known ?? false);
  useEffect(() => {
    if (known !== null || !inApp()) return;
    invoke<boolean>("dev_tools").then(v => { known = v; setOn(v); }).catch(() => { known = false; });
  }, []);
  return on;
}

/** Development tools: the ear (src-tauri/src/ear.rs) measures the gap heard between the song and the voice while
 *  "Play with the song" plays this take (`on`), from what the music app and the window output; off, its summary. */
export async function devEar(take: string, on: boolean) {
  if (inApp()) await invoke("dev_ear", { take, on }).catch(() => {});
}

/** A take's link, for pinpointing it to Claude: onkey://take/<take id>?t=<s into the take>&song=<s into the song>,
 *  or, for the play under way, onkey://live/<engine version>?song=<s into the song>&track=<id>&at=<local time>.
 *  Paste it to a coding agent or into a bug report to point at the exact moment. */
export function takeLink(kind: "take" | "live", id: string | number, params: Record<string, string | number | null | undefined>): string {
  const q = Object.entries(params).filter(([, v]) => v != null && v !== "")
    .map(([k, v]) => `${k}=${typeof v === "number" ? v.toFixed(1) : encodeURIComponent(v as string)}`).join("&");
  return `onkey://${kind}/${encodeURIComponent(String(id))}${q ? `?${q}` : ""}`;
}

/** The local time now, as the app's log writes it (2026-10-20T09:14:42). */
export const logTime = () => {
  const d = new Date(), p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
};

/** A development tool's button: purple, dashed, labelled "dev". `run` returns the text to copy (or null). */
export function DevCopy({ label, text }: { label: string; text: () => string | null | Promise<string | null> }) {
  const on = useDevTools();
  const [done, setDone] = useState<"" | "copied" | "failed">("");
  if (!on) return null;
  return (
    <button
      onClick={async () => {
        const t = await text();
        const ok = t != null && await invoke<boolean>("dev_copy", { text: t }).catch(() => false);
        setDone(ok ? "copied" : "failed");
        setTimeout(() => setDone(""), 1500);
      }}
      title="Development tool (not in releases)"
      className="inline-flex items-center gap-1.5 rounded-lg border border-dashed border-fuchsia-400/70 bg-fuchsia-500/10 px-2.5 py-1 text-xs text-fuchsia-200 transition hover:bg-fuchsia-500/20">
      <span className="rounded bg-fuchsia-500/30 px-1 text-[10px] font-semibold uppercase tracking-wide text-fuchsia-100">dev</span>
      {done === "copied" ? "Copied" : done === "failed" ? "Couldn’t copy" : label}
    </button>
  );
}

