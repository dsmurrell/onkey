"use client";
// One quiet line for a song LRCLIB has no lyrics for: where lyrics come from (LRCLIB, which anyone can add to,
// linked), and "Look again", which asks LRCLIB about the song once more, forgetting the "none" answer OnKey kept
// (lyrics someone has added since would otherwise show up only a day later). Shown under the lane on the Sing page
// while such a song plays, and on a learned song's page. Not with "Show lyrics" off, and not while LRCLIB is
// down (the Sing page has its own notice for that): the callers check both.
import { useEffect, useRef, useState } from "react";
import { lyricsLookAgain, openLink, type LookAgain } from "@/lib/session";
import { useT } from "@/lib/i18n";

const LRCLIB = "https://lrclib.net";

export default function NoLyrics({ id, className = "", onFound }: { id: string; className?: string;
                                   /** they were found: the caller may hide this line (the song's page) */
                                   onFound?: () => void }) {
  const t = useT();
  const [state, setState] = useState<"idle" | "looking" | LookAgain>("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  async function look() {
    if (state === "looking") return;
    setState("looking");
    const r = await lyricsLookAgain(id);
    setState(r);
    if (r === "found") onFound?.();
    // the answer shows for a moment, then the button is back (another look is fine: LRCLIB is asked politely)
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setState("idle"), 4000);
  }
  const link = (c: React.ReactNode) => <button type="button" onClick={() => void openLink(LRCLIB)}
                                                className="underline decoration-ink-600 underline-offset-2 transition hover:text-ink-200">{c}</button>;
  return (
    <p className={`text-sm text-ink-400 ${className}`} data-no-lyrics>
      {t.rich("noLyrics.line", { link })}{" "}
      {state === "idle" ? (
        <button type="button" onClick={look} className="text-accent-300 underline-offset-2 transition hover:underline">{t("noLyrics.lookAgain")}</button>
      ) : (
        <span className="text-ink-300" role="status">
          {state === "looking" ? t("noLyrics.looking") : state === "found" ? t("noLyrics.found") : state === "none" || state === "unknown" ? t("noLyrics.stillNone") : t("noLyrics.notAnswering")}
        </span>
      )}
    </p>
  );
}
