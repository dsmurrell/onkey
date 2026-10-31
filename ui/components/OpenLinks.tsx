"use client";
import { useEffect } from "react";
import { useRouter } from "next/navigation";

/** Opens the take an onkey:// link asks for (src-tauri/src/links.rs), in place: when the window loads (a link can
 *  start OnKey) and whenever another link is opened. */
export default function OpenLinks() {
  const router = useRouter();
  useEffect(() => {
    const w = window as unknown as { __TAURI_INTERNALS__?: { invoke: (cmd: string) => Promise<string | null> } };
    if (!w.__TAURI_INTERNALS__) return;
    const go = () => w.__TAURI_INTERNALS__!.invoke("link_take").then(p => { if (p) router.push(p); }).catch(() => {});
    go();
    let off: (() => void) | undefined, gone = false;
    import("@tauri-apps/api/event").then(({ listen }) => listen("onkey-link", go)).then(u => { if (gone) u(); else off = u; });
    return () => { gone = true; off?.(); };
  }, [router]);
  return null;
}
