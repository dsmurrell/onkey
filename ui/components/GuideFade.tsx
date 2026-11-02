"use client";
// "Fade the guide" on the Sing screen (Settings, off by default): decides this take's fade from your past takes
// of the song (lib/guideFade.ts), hands it to the lane and the pitch hint, and says so in a small note with a
// one-click way back to the full guide for this take.
import { useEffect, useState } from "react";
import { getSettings, getTakes, type SongState } from "@/lib/session";
import { fadeLevel, setGuideLevel, songScores, type FadeLevel } from "@/lib/guideFade";
import { useT } from "@/lib/i18n";

export default function GuideFade({ song, blind, ended }: { song: SongState | null; blind: boolean; ended: boolean }) {
  const t = useT();
  // the fade this song has earned, per play (a new play counts the takes since); null until known
  const [earned, setEarned] = useState<{ version: number; level: FadeLevel } | null>(null);
  const [fullFor, setFullFor] = useState<number | null>(null);    // "Show full guide": this play's version
  const [quiet, setQuiet] = useState(false);
  const version = song?.version, sing = song?.mode === "sing" && !!song.cache;
  const track = song?.track;
  useEffect(() => {
    if (version == null || !track || !sing) return;
    let gone = false;
    (async () => {
      const s = await getSettings().catch(() => null);
      if (!s?.fade_guide) { if (!gone) setEarned({ version, level: 0 }); return; }
      const takes = await getTakes().catch(() => []);
      if (!gone) setEarned({ version, level: fadeLevel(songScores(takes, track)) });
    })();
    return () => { gone = true; };
  }, [version, sing]);   // eslint-disable-line react-hooks/exhaustive-deps

  const level: FadeLevel = earned && earned.version === version && sing && !blind && !ended && fullFor !== version ? earned.level : 0;
  useEffect(() => { setGuideLevel(level); }, [level]);
  useEffect(() => () => setGuideLevel(0), []);
  // the note steps back after a few seconds (still there, still one click)
  useEffect(() => {
    setQuiet(false);
    if (!level) return;
    const timer = setTimeout(() => setQuiet(true), 8000);
    return () => clearTimeout(timer);
  }, [level, version]);

  if (!level) return null;
  return (
    <div role="status" className={`absolute top-3 right-4 z-10 flex animate-rise items-center gap-2 rounded-full bg-ink-950/70 py-1 pr-1 pl-3 text-xs text-ink-300 shadow-lg shadow-black/30 ring-1 ring-white/6 backdrop-blur transition-opacity duration-700 hover:opacity-100 ${quiet ? "opacity-55" : ""}`}
         title={t("guideFade.title", { amount: ["", "little", "more", "fully"][level] })}>
      <span aria-hidden className="flex gap-0.5">
        {[1, 2, 3].map(k => <span key={k} className={`h-1.5 w-1.5 rounded-full ${k <= level ? "bg-accent-300" : "bg-white/12"}`} />)}
      </span>
      {t("guideFade.note")}
      <button onClick={() => setFullFor(version!)}
              className="rounded-full bg-white/6 px-2.5 py-1 font-medium text-ink-100 transition hover:bg-white/12 hover:text-white">{t("guideFade.showFull")}</button>
    </div>
  );
}
