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
import { deleteTake, getTakes, learnAgain, learnLabel, playTrack, type LearnAgainState, type TakeSummary, useLearn } from "@/lib/session";
import { LEVELS, levelOf, scoreWord, when } from "@/lib/music";
import { lastVsFirst } from "@/lib/trend";
import { readList } from "@/lib/dataSafety";
import ReadError from "@/components/ReadError";

export default function SongPage() {
  return <Suspense><Song /></Suspense>;
}

function Song() {
  const id = useSearchParams().get("id") ?? "";
  const [takes, setTakes] = useState<TakeSummary[] | null>(null);
  // a read that fails shows as that (with a retry), never as "No plays of this song yet"
  const [readErr, setReadErr] = useState<{ error: string; detail: string } | null>(null);
  const [tries, setTries] = useState(0);
  useEffect(() => {
    readList(getTakes, "takes").then(r => { if ("items" in r) { setTakes(r.items); setReadErr(null); } else setReadErr(r); });
  }, [tries]);
  // oldest first: play 1 is the first time you sang it
  const plays = useMemo(() => (takes ?? []).filter(t => t.track.id === id && t.coverage >= 0.1)
    .sort((a, b) => a.recorded.localeCompare(b.recorded)), [takes, id]);
  const track = plays[0]?.track;
  const job = useLearn(track);     // still being learned: it can be sung again once that's done
  // like with like: the best and the first play to compare with are of the plays at your latest play's level
  const level = levelOf(plays[plays.length - 1]?.level);
  const same = plays.filter(t => levelOf(t.level) === level);
  const best = same.length ? Math.max(...same.map(t => t.score)) : null;
  const last = plays[plays.length - 1]?.score ?? null;
  // how you sing it now against when you started: your last 3 plays against your first 3 (from 6 plays), else the
  // latest against the first
  const gain = lastVsFirst(same.map(t => t.score));
  // "Learn again": asked for, and where its learned copy looks off
  const [state, setState] = useState<LearnAgainState | null>(null);
  useEffect(() => { if (id) learnAgain(id).then(setState).catch(() => {}); }, [id]);
  // asked for: it plays now, as "Sing it again" does, and that play learns it afresh
  const again = (on: boolean) => learnAgain(id, on).then(st => {
    setState(st);
    if (st?.learn_again && track) void playTrack(track.id, true, track.name);
  }).catch(() => {});
  async function remove(tid: string) {
    await deleteTake(tid);
    setTakes(ts => (ts ?? []).filter(t => t.id !== tid));
  }

  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-5xl px-6 pb-16">
        <Link href="/history" className="mt-4 inline-block text-sm text-ink-400 transition hover:text-ink-100">← Your singing</Link>
        {readErr ? <div className="mt-6"><ReadError {...readErr} onRetry={() => setTries(n => n + 1)} /></div>
          : takes === null ? <div className="glass mt-6 h-40 rounded-3xl shimmer" /> : !track ? (
          <div className="glass mt-6 rounded-3xl px-6 py-16 text-center text-ink-300">No plays of this song yet.</div>
        ) : (
          <>
            <div className="mt-4 mb-6 flex items-center gap-4">
              <Cover name={track.name} art={track.art} link={track.link} size={64} />
              <div className="min-w-0 flex-1">
                <h1 className="truncate text-3xl font-semibold tracking-tight">{track.name}</h1>
                <div className="truncate text-ink-300">{track.artist}{track.link && <> <span className="text-ink-600">·</span> <AppleMusicLink link={track.link} className="text-sm" /></>}</div>
                <div className="mt-0.5 text-xs text-ink-400" title={LEVELS[level].about}>Scores at {LEVELS[level].name} · <Link href="/settings"
                     className="text-accent-300 underline-offset-2 hover:underline">change</Link></div>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-1.5">
                <button onClick={() => playTrack(track.id, true, track.name)} disabled={!!job} title={job ? "It can be sung once it’s learned" : undefined}
                        className="tabular rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400 disabled:cursor-default disabled:bg-white/6 disabled:text-ink-300">
                  {job ? learnLabel(job) : "Sing it again"}</button>
                {job && <LearnBar job={job} showPct={false} className="w-44" />}
                {!job && state?.learned && (state.learn_again
                  ? <span className="text-xs text-ink-400">Learns again on its next play · <button onClick={() => again(false)}
                      className="text-accent-300 underline-offset-2 hover:underline">Cancel</button></span>
                  : <button onClick={() => again(true)} className="text-xs text-ink-400 transition hover:text-white"
                            title="Plays it now and learns it afresh, as its first play did. Your plays and scores stay; stopped early, it’s left as it was.">
                      Learn again</button>)}
              </div>
            </div>
            {!job && state?.relearn && !state.learn_again && (
              <div className="glass mb-6 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-2xl px-4 py-3 text-sm">
                <span className="min-w-0 flex-1 text-ink-200">
                  The lyrics slip by about {state.relearn.by.toFixed(1)} s at {clock(state.relearn.at)} each time you play it. Learning it again should fix that.
                </span>
                <button onClick={() => again(true)} className="shrink-0 rounded-xl bg-white/8 px-3 py-1.5 font-medium text-white transition hover:bg-white/12">Learn it again</button>
              </div>
            )}

            <section className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <Stat label="Plays" value={plays.length} />
              <Stat label="Best" value={best ?? "–"} sub={best != null ? scoreWord(best) : ""} />
              <Stat label="Latest" value={last ?? "–"} sub={plays.length ? when(plays[plays.length - 1].recorded) : ""} />
              <Stat label={gain?.both ? "Last 3 vs first 3" : "Latest vs first play"} value={gain ? `${gain.change >= 0 ? "+" : ""}${gain.change}` : "–"}
                    sub={same.length > 1 && gain ? `${gain.both ? "average " : ""}from ${gain.from} to ${gain.to}` : "sing it again to see"} />
            </section>

            <section className="glass mt-4 rounded-3xl p-5">
              <div className="mb-3 flex items-baseline justify-between">
                <h2 className="text-sm font-medium text-ink-200">Score per play</h2>
                <span className="text-xs text-ink-400">first play → latest</span>
              </div>
              <Trend takes={plays} unit="play" />
            </section>

            <section className="mt-8">
              <h2 className="mb-3 text-sm font-medium text-ink-200">Every play</h2>
              <div className="glass overflow-x-auto rounded-3xl">
                <table className="w-full min-w-[560px] text-sm">
                  <thead className="text-left text-xs text-ink-400">
                    <tr className="border-b border-white/6">
                      <th className="px-5 py-3 font-medium">Play</th><th className="px-3 py-3 font-medium">When</th>
                      <th className="px-3 py-3 text-right font-medium" title="How much of the song you sang">Sung</th><th className="px-3 py-3 text-right font-medium">Pitch</th>
                      <th className="px-3 py-3 text-right font-medium">Score</th><th className="w-12 pr-3" />
                    </tr>
                  </thead>
                  <tbody>
                    {plays.slice().reverse().map(t => {
                      const n = plays.indexOf(t) + 1;
                      return (
                        <tr key={t.id} className="group border-b border-white/4 transition last:border-0 hover:bg-white/3">
                          <td className="px-5 py-3"><Link href={`/take?id=${encodeURIComponent(t.id)}`} className="flex items-center gap-2">
                            <span className="tabular font-medium">#{n}</span>
                            <TakeTags t={t} /></Link></td>
                          <td className="px-3 py-3 text-ink-300"><Link href={`/take?id=${encodeURIComponent(t.id)}`}>{when(t.recorded)}</Link></td>
                          <td className="tabular px-3 py-3 text-right text-ink-200">{Math.round(100 * t.coverage)}%</td>
                          <td className="tabular px-3 py-3 text-right text-ink-200">{Math.round(100 * t.accuracy)}%</td>
                          <td className="tabular px-3 py-3 text-right text-lg font-semibold">{t.score}</td>
                          <td className="pr-3 text-right"><span className={`inline-flex justify-end ${BIN}`}>
                            <DeleteButton what="Delete this play (it goes to the Trash)" confirm="Delete this play?" onDelete={() => remove(t.id)} />
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
  const t = Math.max(0, Math.round(s));
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
}
