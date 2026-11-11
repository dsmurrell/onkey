"use client";
import { useEffect, useState } from "react";
import Link from "next/link";
import { DownloadButton, type DownloadText } from "@/components/DownloadButton";
import { REQUIREMENTS } from "@/lib/site";
import { type Challenge, STASH_KEY, checkChallenge, readChallenge } from "@/lib/challenge";

const btn = "inline-flex min-h-12 items-center justify-center gap-2 rounded-xl px-5 font-medium transition";
const primary = `${btn} bg-accent text-white shadow-lg shadow-accent/25 hover:bg-accent-strong`;

/** The fragment the page's first script kept (lib/challenge.ts STASH_SCRIPT), else the address's, else this tab's. */
function stashed(): string {
  const w = window as unknown as { __onkeyChallenge?: string };
  if (w.__onkeyChallenge) return w.__onkeyChallenge;
  if (location.hash.length > 1) return location.hash.slice(1);
  try { return sessionStorage.getItem(STASH_KEY) ?? ""; } catch { return ""; }
}

/** The invite: "<name> scored 78. Can you beat it?" over a card with the song and its artist, or a general one when the link has no
 *  challenge in it (or one that doesn't make sense). All text is set as text by React (no HTML from the link). */
export function ChallengeInvite({ download }: { download: DownloadText }) {
  const [c, setC] = useState<Challenge | null>(null);
  // whether the app's signature fits what the link says (null: being checked)
  const [checked, setChecked] = useState<boolean | null>(null);
  useEffect(() => {
    let alive = true;
    setChecked(null);
    if (c) void checkChallenge(c).then(ok => { if (alive) setChecked(ok); });
    return () => { alive = false; };
  }, [c]);
  useEffect(() => {
    setC(readChallenge(stashed()));
    // another challenge link opened in this tab: read it, then take it out of the address as the first script does
    const changed = () => {
      const h = location.hash.slice(1);
      if (!h) return;
      try { sessionStorage.setItem(STASH_KEY, h); } catch {}
      history.replaceState(history.state, "", location.pathname + location.search);
      setC(readChallenge(h));
    };
    window.addEventListener("hashchange", changed);
    return () => window.removeEventListener("hashchange", changed);
  }, []);

  const who = c?.name || "A friend";
  return (
    <section className="relative overflow-hidden">
      <div aria-hidden="true" className="pointer-events-none absolute inset-x-0 -top-40 h-[480px] bg-[radial-gradient(700px_300px_at_50%_30%,var(--accent-wash),transparent)]" />
      <div className="rise relative mx-auto grid max-w-3xl gap-6 px-4 pb-20 pt-12 text-center sm:px-6 md:pt-24">
        <p className="text-sm font-medium text-accent-soft">A singing challenge</p>
        {c ? (
          <>
            <div className={`mx-auto grid size-28 place-items-center rounded-full border-4 bg-panel shadow-lg ${checked === false ? "border-dashed border-line shadow-black/20" : "border-accent/60 shadow-accent/20"}`}>
              <span className={`text-5xl font-bold tabular-nums tracking-tight ${checked === false ? "text-muted" : "text-fg"}`}>{c.score}</span>
            </div>
            <h1 className="text-balance text-[2rem] font-semibold leading-[1.15] tracking-tight text-fg sm:text-5xl sm:leading-[1.1]">
              {checked === false ? `${who} says they scored ${c.score}. Can you beat it?` : `${who} scored ${c.score}. Can you beat it?`}
            </h1>
            {checked !== null && (
              <p className={`-mt-2 text-sm ${checked ? "text-accent-soft" : "text-faint"}`}>
                {checked ? "✓ Score checked: this link is as OnKey made it."
                         : "This score isn't checked: the link was changed, or it was made by an older version of OnKey."}
              </p>
            )}
            <div className="mx-auto flex max-w-full items-center gap-4 rounded-2xl border border-line bg-panel px-5 py-4 text-left shadow-lg shadow-black/20">
              <div aria-hidden="true" className="grid size-12 shrink-0 place-items-center rounded-xl bg-accent/15 text-accent-soft">
                <svg viewBox="0 0 24 24" className="size-6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" />
                </svg>
              </div>
              <div className="min-w-0">
                <p className="text-xl font-semibold leading-snug text-fg [overflow-wrap:anywhere]">{c.song}</p>
                {c.by && <p className="mt-0.5 text-base text-muted [overflow-wrap:anywhere]">{c.by}</p>}
              </div>
            </div>
          </>
        ) : (
          <h1 className="text-balance text-[2rem] font-semibold leading-[1.15] tracking-tight text-fg sm:text-5xl sm:leading-[1.1]">
            A friend has challenged you to sing. Can you beat their score?
          </h1>
        )}
        <p className="mx-auto max-w-[54ch] text-lg leading-relaxed text-muted">
          OnKey is a free singing trainer for the Mac: play a song you love, sing along, and it shows how close you are to the singer&apos;s notes and gives you a score.
        </p>
        <div className="flex flex-wrap items-center justify-center gap-3">
          <DownloadButton where="challenge" text={download} className={primary} />
          <Link href="/docs/how-it-works/" className="inline-flex min-h-12 items-center px-2 font-medium text-accent-soft underline-offset-4 hover:underline">
            How it works →
          </Link>
        </div>
        <p className="text-sm text-faint">
          {c ? `Play ${c.song} once so OnKey learns it, then sing it to get your score. ` : ""}Free and open source. {REQUIREMENTS.join(", ")}.
        </p>
      </div>
    </section>
  );
}
