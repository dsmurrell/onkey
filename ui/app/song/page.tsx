"use client";
// One song's history: how your score has moved play by play, and every play listed (numbered, dated) with a
// link to its report.
import Link from "next/link";
import { Suspense, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import Cover, { AppleMusicLink } from "@/components/Cover";
import DeleteButton from "@/components/DeleteButton";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import Trend from "@/components/Trend";
import LearnBar from "@/components/LearnBar";
import TakeTags, { BIN } from "@/components/TakeTags";
import NoLyrics from "@/components/NoLyrics";
import { deleteTake, getTakes, learnAgain, learnLabel, live, playTrack, type LearnAgainState, type TakeSummary, useLearn, useSession } from "@/lib/session";
import { levelAbout, levelName, levelOf, scoreWord, when } from "@/lib/music";
import { lastVsFirst } from "@/lib/trend";
import { readList } from "@/lib/dataSafety";
import ReadError from "@/components/ReadError";
import { useT } from "@/lib/i18n";

export default function SongPage() {
  return <Suspense><Song /></Suspense>;
}

function Song() {
  const t = useT();
  const id = useSearchParams().get("id") ?? "";
  const [takes, setTakes] = useState<TakeSummary[] | null>(null);
  // a read that fails shows as that (with a retry), never as "No plays of this song yet"
  const [readErr, setReadErr] = useState<{ error: string; detail: string } | null>(null);
  const [tries, setTries] = useState(0);
  useEffect(() => {
    readList(getTakes, "takes").then(r => { if ("items" in r) { setTakes(r.items); setReadErr(null); } else setReadErr(r); });
  }, [tries]);
  // oldest first: play 1 is the first time you sang it
  const plays = useMemo(() => (takes ?? []).filter(tk => tk.track.id === id && tk.coverage >= 0.1)
    .sort((a, b) => a.recorded.localeCompare(b.recorded)), [takes, id]);
  const track = plays[0]?.track;
  const job = useLearn(track);     // still being learned: it can be sung again once that's done
  // like with like: the best and the first play to compare with are of the plays at your latest play's level
  const level = levelOf(plays[plays.length - 1]?.level);
  const same = plays.filter(tk => levelOf(tk.level) === level);
  const best = same.length ? Math.max(...same.map(tk => tk.score)) : null;
  const last = plays[plays.length - 1]?.score ?? null;
  // how you sing it now against when you started: your last 3 plays against your first 3 (from 6 plays), else the
  // latest against the first
  const gain = lastVsFirst(same.map(tk => tk.score));
  // "Learn again": asked for, and where its learned copy looks off
  const [state, setState] = useState<LearnAgainState | null>(null);
  useEffect(() => { if (id) learnAgain(id).then(setState).catch(() => {}); }, [id]);
  // a learned song LRCLIB had no lyrics for: where they come from, and Look again (not with "Show lyrics" off, nor
  // while LRCLIB is down); found, it has them from now on
  const { status } = useSession();
  const noLyrics = !job && !!state?.learned && state.lyrics === false && live.lyricsOn && !status.lyrics_down;
  // asked for: it plays now, as "Sing it again" does, and that play learns it afresh
  const again = (on: boolean) => learnAgain(id, on).then(st => {
    setState(st);
    if (st?.learn_again && track) void playTrack(track.id, true, track.name);
  }).catch(() => {});
  async function remove(tid: string) {
    await deleteTake(tid);
    setTakes(ts => (ts ?? []).filter(tk => tk.id !== tid));
  }

  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-5xl px-6 pb-16">
        <Link href="/history" className="mt-4 inline-block text-sm text-ink-400 transition hover:text-ink-100">{t("song.back")}</Link>
        {readErr ? <div className="mt-6"><ReadError {...readErr} onRetry={() => setTries(n => n + 1)} /></div>
          : takes === null ? <div className="glass mt-6 h-40 rounded-3xl shimmer" /> : !track ? (
          <div className="glass mt-6 rounded-3xl px-6 py-16 text-center text-ink-300">{t("song.none")}</div>
        ) : (
          <>
            <div className="mt-4 mb-6 flex items-center gap-4">
              <Cover name={track.name} art={track.art} link={track.link} size={64} />
              <div className="min-w-0 flex-1">
                <h1 className="truncate text-3xl font-semibold tracking-tight">{track.name}</h1>
                <div className="truncate text-ink-300">{track.artist}{track.link && <> <span className="text-ink-600">·</span> <AppleMusicLink link={track.link} className="text-sm" /></>}</div>
                <div className="mt-0.5 text-xs text-ink-400" title={levelAbout(level)}>{t.rich("song.scoresAt", { level: levelName(level), link: c => <Link href="/settings"
                     className="text-accent-300 underline-offset-2 hover:underline">{c}</Link> })}</div>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-1.5">
                <button onClick={() => playTrack(track.id, true, track.name)} disabled={!!job} title={job ? t("song.whenLearned") : undefined}
                        className="tabular rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400 disabled:cursor-default disabled:bg-white/6 disabled:text-ink-300">
                  {job ? learnLabel(job) : t("song.singAgain")}</button>
                {job && <LearnBar job={job} showPct={false} className="w-44" />}
                {!job && state?.learned && (state.learn_again
                  ? <span className="text-xs text-ink-400">{t.rich("song.learnsAgain", { cancel: c => <button onClick={() => again(false)}
                      className="text-accent-300 underline-offset-2 hover:underline">{c}</button> })}</span>
                  : <button onClick={() => again(true)} className="text-xs text-ink-400 transition hover:text-white"
                            title={t("song.learnAgainTitle")}>
                      {t("song.learnAgain")}</button>)}
              </div>
            </div>
            {noLyrics && <NoLyrics id={id} className="-mt-3 mb-6" onFound={() => setState(st => st && { ...st, lyrics: true })} />}
            {!job && state?.relearn && !state.learn_again && (
              <div className="glass mb-6 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-2xl px-4 py-3 text-sm">
                <span className="min-w-0 flex-1 text-ink-200">
                  {t("song.relearn", { by: state.relearn.by.toFixed(1), at: clock(state.relearn.at) })}
                </span>
                <button onClick={() => again(true)} className="shrink-0 rounded-xl bg-white/8 px-3 py-1.5 font-medium text-white transition hover:bg-white/12">{t("song.learnItAgain")}</button>
              </div>
            )}

            <section className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <Stat label={t("song.stat.plays")} value={plays.length} />
              <Stat label={t("song.stat.best")} value={best ?? "–"} sub={best != null ? scoreWord(best) : ""} />
              <Stat label={t("song.stat.latest")} value={last ?? "–"} sub={plays.length ? when(plays[plays.length - 1].recorded) : ""} />
              <Stat label={gain?.both ? t("song.stat.gainBoth") : t("song.stat.gainOne")} value={gain ? `${gain.change >= 0 ? "+" : ""}${gain.change}` : "–"}
                    sub={same.length > 1 && gain ? (gain.both ? t("song.stat.fromToAverage", { from: gain.from, to: gain.to }) : t("song.stat.fromTo", { from: gain.from, to: gain.to })) : t("song.stat.singToSee")} />
            </section>

            <section className="glass mt-4 rounded-3xl p-5">
              <div className="mb-3 flex items-baseline justify-between">
                <h2 className="text-sm font-medium text-ink-200">{t("song.chart.title")}</h2>
                <span className="text-xs text-ink-400">{t("song.chart.axis")}</span>
              </div>
              <Trend takes={plays} unit="play" />
            </section>

            <section className="mt-8">
              <h2 className="mb-3 text-sm font-medium text-ink-200">{t("song.table.title")}</h2>
              <div className="glass overflow-x-auto rounded-3xl">
                <table className="w-full min-w-[560px] text-sm">
                  <thead className="text-left text-xs text-ink-400">
                    <tr className="border-b border-white/6">
                      <th className="px-5 py-3 font-medium">{t("song.table.play")}</th><th className="px-3 py-3 font-medium">{t("song.table.when")}</th>
                      <th className="px-3 py-3 text-right font-medium" title={t("song.table.sungTitle")}>{t("song.table.sung")}</th><th className="px-3 py-3 text-right font-medium">{t("song.table.pitch")}</th>
                      <th className="px-3 py-3 text-right font-medium">{t("song.table.score")}</th><th className="w-12 pr-3" />
                    </tr>
                  </thead>
                  <tbody>
                    {plays.slice().reverse().map(tk => {
                      const n = plays.indexOf(tk) + 1;
                      return (
                        <tr key={tk.id} className="group border-b border-white/4 transition last:border-0 hover:bg-white/3">
                          <td className="px-5 py-3"><Link href={`/take?id=${encodeURIComponent(tk.id)}`} className="flex items-center gap-2">
                            <span className="tabular font-medium">#{n}</span>
                            <TakeTags t={tk} /></Link></td>
                          <td className="px-3 py-3 text-ink-300"><Link href={`/take?id=${encodeURIComponent(tk.id)}`}>{when(tk.recorded)}</Link></td>
                          <td className="tabular px-3 py-3 text-right text-ink-200">{Math.round(100 * tk.coverage)}%</td>
                          <td className="tabular px-3 py-3 text-right text-ink-200">{Math.round(100 * tk.accuracy)}%</td>
                          <td className="tabular px-3 py-3 text-right text-lg font-semibold">{tk.score}</td>
                          <td className="pr-3 text-right"><span className={`inline-flex justify-end ${BIN}`}>
                            <DeleteButton what={t("song.deleteWhat")} confirm={t("song.deleteConfirm")} onDelete={() => remove(tk.id)} />
                          </span></td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </section>
          </>
        )}
      </main>
    </div>
  );
}

function Stat({ label, value, sub }: { label: string; value: React.ReactNode; sub?: React.ReactNode }) {
  return (
    <div className="glass rounded-2xl px-5 py-4">
      <div className="text-xs text-ink-400">{label}</div>
      <div className="tabular mt-1 text-3xl font-bold tracking-tight">{value}</div>
      <div className="mt-0.5 truncate text-xs text-ink-300">{sub || " "}</div>
    </div>
  );
}

/** A song time as m:ss (2:06). */
function clock(s: number) {
  const secs = Math.max(0, Math.round(s));
  return `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
}
