"use client";
import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { listen } from "@tauri-apps/api/event";
import { useDevTools } from "@/components/Dev";
import { driveHref, type DriveEvent } from "@/lib/devDrive";

/** Development tools only (nothing shows): "Play with the song" driven without a click, for a coding agent testing it
 *  (the mirror's POST /dev/along, src-tauri/src/mirror.rs). A request opens the take's page, which then presses
 *  "Play with the song" from the place asked through the button's own code (app/take/page.tsx); a stop is passed
 *  to the take page as "onkey-dev-along-stop", which presses Stop. */
export default function DevDrive() {
  const on = useDevTools();
  const router = useRouter();
  useEffect(() => {
    if (!on) return;
    let off: (() => void) | undefined, gone = false;
    void listen<DriveEvent>("onkey-dev-along", e => {
      if (e.payload.stop) { window.dispatchEvent(new Event("onkey-dev-along-stop")); return; }
      const href = driveHref(e.payload);
      if (href) router.push(href);
    }).then(u => { if (gone) u(); else off = u; });
    return () => { gone = true; off?.(); };
  }, [on, router]);
  return null;
}
