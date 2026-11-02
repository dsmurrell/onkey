"use client";
import Link from "next/link";
import { useEffect, useMemo, useRef, useState } from "react";
import Cover from "@/components/Cover";
import DeleteButton from "@/components/DeleteButton";
import Heart from "@/components/Heart";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import LearnBar from "@/components/LearnBar";
import { OpenTake } from "@/components/LiveBits";
import { deleteTake, getLibrary, getTakes, learnLabel, learnOf, live, playTrack, setFavorite, useLearn, useSession, type TakeSummary } from "@/lib/session";
import { levelAbout, levelName, levelOf, scoreWord, when } from "@/lib/music";
import Trend from "@/components/Trend";
import { bestAtLatestLevel } from "@/components/LevelTag";
import { changeWords, vsBefore } from "@/lib/trend";
import VoiceRange from "@/components/VoiceRange";
import { deleteEach, readList } from "@/lib/dataSafety";
import ReadError from "@/components/ReadError";
import { DifficultyPicker, SortMenu, pill } from "@/components/ListControls";
import { HEART } from "@/components/Heart";
import TakeTags, { BIN } from "@/components/TakeTags";
import { useT } from "@/lib/i18n";

const TREND = "#8a78ff";

// filters and sort, kept on this Mac as Library's are
type Show = "all" | "favorites";
type Sort = "recent" | "best" | "most" | "improved" | "easy" | "hard" | "az";
const SORTS: Sort[] = ["recent", "best", "most", "improved", "easy", "hard", "az"];
type Col = "when" | "sung" | "pitch" | "score";
type View = "songs" | "takes";
const KEY = { show: "onkey.historyShow", levels: "onkey.historyLevels", sort: "onkey.historySort", view: "onkey.historyView" };
const PAGE = { songs: 24, takes: 50 };   // shown at first, and added by each "Show more"
const load = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
const save = (k: string, v: string) => { try { localStorage.setItem(k, v); } catch {} };   // validated for the dark surface (same as the chart in components/Trend)

// a song's key for its heart: the same artist and title is the same song (a take may be by Spotify's id, the song
// kept by its ISRC)
const songKey = (t: { name: string; artist: string }) => `${t.artist}\u0000${t.name}`.toLowerCase().replace(/[\s!-/:-@[-`{-~]/g, "");

export default function History() {
  const t = useT();
  const [takes, setTakes] = useState<TakeSummary[] | null>(null);
  // takes from this session still being scored or learned; the saved list reloads as each one finishes
  useEffect(() => { live.start(); }, []);
  const { takes: session, learning } = useSession();
  const pending = session.filter(tk => tk.status === "scoring" || tk.status === "learning");
  const finished = session.filter(tk => tk.status === "done").length;
  // a read that fails shows as that (with a retry), never as "No takes yet"
  const [readErr, setReadErr] = useState<{ error: string; detail: string } | null>(null);
  const [tries, setTries] = useState(0);
  useEffect(() => {
    readList(getTakes, "takes").then(r => { if ("items" in r) { setTakes(r.items); setReadErr(null); } else setReadErr(r); });
  }, [finished, tries]);
  // hearts, from the library (by id, and by artist and title)
  const [hearts, setHearts] = useState<Map<string, boolean>>(new Map());
  const [diffs, setDiffs] = useState<Map<string, number>>(new Map());
  useEffect(() => {
    getLibrary().then(lib => {
      const m = new Map<string, boolean>(), d = new Map<string, number>();
      for (const s of lib) if (s?.track?.id) {
        m.set(s.track.id, !!s.favorite); m.set(songKey(s.track), !!s.favorite);
        if (s.difficulty) { d.set(s.track.id, s.difficulty); d.set(songKey(s.track), s.difficulty); }
      }
      setHearts(m); setDiffs(d);
    }).catch(() => {});
  }, [finished]);
  const difficulty = (tk: TakeSummary["track"]) => diffs.get(tk.id) ?? diffs.get(songKey(tk));

  // search, filters and sort
  const [q, setQ] = useState("");
  const [show, setShowState] = useState<Show>("all");
  const [levels, setLevelsState] = useState<number[]>([]);
  const [sort, setSortState] = useState<Sort>("recent");
  const [col, setCol] = useState<{ by: Col; down: boolean }>({ by: "when", down: true });
  const [view, setViewState] = useState<View>("songs");
  useEffect(() => {
    if (load(KEY.show) === "favorites") setShowState("favorites");
    if (load(KEY.view) === "takes") setViewState("takes");
    try { const l = JSON.parse(load(KEY.levels) ?? "[]"); if (Array.isArray(l)) setLevelsState(l.filter(n => [1, 2, 3, 4, 5].includes(n))); } catch {}
    const o = load(KEY.sort);
    if (SORTS.some(k => k === o)) setSortState(o as Sort);
  }, []);
  const setShow = (v: Show) => { setShowState(v); save(KEY.show, v); };
  const setLevels = (l: number[]) => { setLevelsState(l); save(KEY.levels, JSON.stringify(l)); };
  const toggleLevel = (n: number) => setLevels(levels.includes(n) ? levels.filter(x => x !== n) : [...levels, n].sort());
  const setSort = (v: Sort) => { setSortState(v); save(KEY.sort, v); };
  const setView = (v: View) => { setViewState(v); save(KEY.view, v); };
  // how many pages of songs or takes are shown; back to one when what's shown changes
  const [pages, setPages] = useState(1);
  useEffect(() => setPages(1), [q, show, levels, sort, col, view]);
  const narrowed = q.trim() !== "" || show !== "all" || levels.length > 0;
  const clearAll = () => { setQ(""); setShow("all"); setLevels([]); };
  const hearted = (tk: TakeSummary["track"]) => hearts.get(tk.id) ?? hearts.get(songKey(tk)) ?? false;
  function heart(tk: TakeSummary["track"], on: boolean) {
    setHearts(h => new Map(h).set(tk.id, on).set(songKey(tk), on));
    void setFavorite(tk, on);
  }
  const real = useMemo(() => (takes ?? []).filter(tk => tk.coverage >= 0.1), [takes]);
  // deleted one by one; if one can't go to the Trash it stops there (the error shows by the button) and only those
  // deleted leave the list
  async function remove(ids: string[]) {
    await deleteEach(ids, deleteTake, gone => setTakes(ts => (ts ?? []).filter(tk => !gone.includes(tk.id))));
  }

  const allSongs = useMemo(() => {
    const by = new Map<string, TakeSummary[]>();
    for (const tk of real) by.set(tk.track.id, [...(by.get(tk.track.id) ?? []), tk]);
    return [...by.values()].map(ts => {
      const asc = ts.slice().sort((a, b) => a.recorded.localeCompare(b.recorded));
      const b = bestAtLatestLevel(ts)!;   // like with like: the best at the level of your latest take
      return { track: ts[0].track, takes: asc, best: b.best, bestLevel: b.level, last: asc[asc.length - 1] };
    }).sort((a, b) => b.last.recorded.localeCompare(a.last.recorded));
  }, [real]);
  // a song shown: matches the search, hearted (Favourites), at a difficulty picked
  const keep = (tk: TakeSummary["track"]) => {
    const d = difficulty(tk);
    return `${tk.name} ${tk.artist}`.toLowerCase().includes(q.trim().toLowerCase())
      && (show !== "favorites" || hearted(tk)) && (!levels.length || (d != null && levels.includes(d)));
  };
  const songs = useMemo(() => {
    const s = allSongs.filter(x => keep(x.track));
    const d = (x: typeof s[number]) => difficulty(x.track);
    const gain = (x: typeof s[number]) => x.takes.length > 1 ? x.best - x.takes[0].score : -1e9;
    switch (sort) {
      case "best": return s.sort((a, b) => b.best - a.best);
      case "most": return s.sort((a, b) => b.takes.length - a.takes.length);
      case "improved": return s.sort((a, b) => gain(b) - gain(a));
      case "easy": return s.sort((a, b) => (d(a) ?? 9) - (d(b) ?? 9));
      case "hard": return s.sort((a, b) => (d(b) ?? 0) - (d(a) ?? 0));
      case "az": return s.sort((a, b) => a.track.name.localeCompare(b.track.name));
      default: return s;
    }
  }, [allSongs, q, show, levels, sort, hearts, diffs]); // eslint-disable-line react-hooks/exhaustive-deps
  const perLevel = [1, 2, 3, 4, 5].map(n => allSongs.filter(x => difficulty(x.track) === n).length);
  const rows = useMemo(() => {
    const r = real.filter(tk => keep(tk.track));
    const v = (tk: TakeSummary) => col.by === "score" ? tk.score : col.by === "pitch" ? tk.accuracy
      : col.by === "sung" ? tk.coverage : 0;
    const by = col.by === "when" ? (a: TakeSummary, b: TakeSummary) => a.recorded.localeCompare(b.recorded) : (a: TakeSummary, b: TakeSummary) => v(a) - v(b);
    return r.sort((a, b) => (col.down ? -1 : 1) * by(a, b));
  }, [real, q, show, levels, col, hearts, diffs]); // eslint-disable-line react-hooks/exhaustive-deps

  // each play's number for its song (1 = the first time you sang it)
  const playNo = useMemo(() => {
    const m = new Map<string, number>();
    for (const s of allSongs) s.takes.forEach((tk, i) => m.set(tk.id, i + 1));
    return m;
  }, [allSongs]);

  const recent10 = real.slice(0, 10);
  const avg = recent10.length ? Math.round(recent10.reduce((s, tk) => s + tk.score, 0) / recent10.length) : null;
  const vs = vsBefore(real.map(tk => tk.score));   // your last 10 against the 10 before (with 20 takes or more)
  const level = levelOf(real[0]?.level);   // every score is shown at the level chosen in Settings
  const best = real.length ? Math.max(...real.map(tk => tk.score)) : null;

  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-6xl px-6 pb-16">
        <div className="mt-6 mb-8 flex flex-wrap items-end justify-between gap-4">
          <div>
            <h1 className="text-3xl font-semibold tracking-tight">{t("history.title")}</h1>
            <p className="mt-1 text-ink-300">{t("history.intro")}</p>
            {real.length > 0 && (
              <p className="mt-1 text-xs text-ink-400" title={levelAbout(level)}>{t.rich("history.scoresAt", { level: levelName(level), link: c => <Link href="/settings"
                 className="text-accent-300 underline-offset-2 hover:underline">{c}</Link> })}</p>
            )}
          </div>
          {!readErr && real.length > 0 && (
            <input value={q} onChange={e => setQ(e.target.value)} placeholder={t("history.search")}
                   className="w-64 rounded-xl border border-white/8 bg-white/4 px-3.5 py-2 text-sm text-ink-100 placeholder:text-ink-400 outline-none transition focus:border-accent-400/50 focus:bg-white/6" />
          )}
        </div>

        {pending.length > 0 && (
          <section className="mb-4 space-y-2">
            {pending.map((tk, i) => {
              // how far its learning has got (a take only scored has no learn going on)
              const job = learnOf(learning, [], { id: tk.track_id ?? "", name: tk.name, artist: tk.artist });
              return (
              <div key={i} className="glass flex items-center gap-4 rounded-2xl px-5 py-3.5">
                <span className="relative flex h-2.5 w-2.5"><span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent-400 opacity-60 motion-reduce:animate-none" /><span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-accent-400" /></span>
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">{tk.name}</div>
                  <div className="truncate text-xs text-ink-400">{tk.artist}</div>
                </div>
                {job && job.fraction != null
                  ? <LearnBar job={job} className="w-56 shrink-0" />
                  : <div className="text-sm text-ink-300">{tk.status === "learning" ? t("history.pending.learning") : t("history.pending.scoring")}</div>}
              </div>
            ); })}
          </section>
        )}
        {readErr ? <ReadError {...readErr} onRetry={() => setTries(n => n + 1)} /> : takes === null ? <Skeleton /> : real.length === 0 ? (pending.length ? null : <Empty />) : (
          <>
            <section className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <Stat label={t("history.stat.takes")} value={real.length} />
              <Stat label={t("history.stat.average")} value={avg ?? "–"} sub={vs ? changeWords(vs.change) : avg != null ? scoreWord(avg) : ""} />
              <Stat label={t("history.stat.best")} value={best ?? "–"} sub={best != null ? real.find(tk => tk.score === best)?.track.name : ""} />
              <Stat label={t("history.stat.songs")} value={allSongs.length} />
            </section>

            <VoiceRange refresh={finished} />

            <section className="glass mt-4 rounded-3xl p-5">
              <div className="mb-3 flex items-baseline justify-between">
                <h2 className="text-sm font-medium text-ink-200">{t("history.chart.title")}</h2>
                <span className="text-xs text-ink-400">{t("history.chart.axis")}</span>
              </div>
              <Trend takes={real.slice().reverse()} />
            </section>

            <div role="radiogroup" aria-label={t("history.view.label")} className="mt-8 inline-flex rounded-full border border-white/8 bg-white/4 p-1 text-sm">
              {(["songs", "takes"] as const).map(v => (
                <button key={v} role="radio" aria-checked={view === v} onClick={() => setView(v)}
                        className={`rounded-full px-3.5 py-1.5 outline-none transition focus-visible:ring-2 focus-visible:ring-accent-400/60
                                    ${view === v ? "bg-white/10 text-white" : "text-ink-300 hover:text-white"}`}>{t(`history.view.${v}`)}</button>
              ))}
            </div>

            <div className="mt-4 flex flex-wrap items-center gap-x-4 gap-y-3">
              <div role="radiogroup" aria-label={t("history.show.label")} className="inline-flex rounded-full border border-white/8 bg-white/4 p-1 text-sm">
                {(["all", "favorites"] as const).map(v => (
                  <button key={v} role="radio" aria-checked={show === v} onClick={() => setShow(v)}
                          className={`flex items-center gap-1.5 rounded-full px-3.5 py-1.5 outline-none transition focus-visible:ring-2 focus-visible:ring-accent-400/60
                                      ${show === v ? "bg-white/10 text-white" : "text-ink-300 hover:text-white"}`}>
                    {v === "favorites" && <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor" aria-hidden className={show === v ? "text-accent-400" : ""}><path d={HEART} /></svg>}
                    {t(`history.show.${v}`)}
                  </button>
                ))}
              </div>
              <DifficultyPicker levels={levels} onToggle={toggleLevel} onClear={() => setLevels([])} counts={perLevel} />
              {narrowed && <button onClick={clearAll} className={pill(false)}>{t("history.clearFilters")}</button>}
              <SortMenu sorts={SORTS.map(k => [k, t(`history.sort.${k}`)] as [Sort, string])} sort={sort} onSort={setSort} />
              <span className="tabular text-sm text-ink-500">{t("history.counts", { songs: songs.length, takes: rows.length })}</span>
            </div>
            {songs.length === 0 && (
              <p className="px-1 py-10 text-center text-ink-400">
                {q.trim() ? t("history.none.search", { q: q.trim() }) : show === "favorites" && !levels.length ? t("history.none.favourites") : t("history.none.filters")}
                <button onClick={clearAll} className="ml-2 text-accent-300 underline-offset-2 hover:underline">{t("history.none.clear")}</button>
              </p>
            )}
            {songs.length > 0 && view === "songs" && (
            <section className="mt-4">
              <h2 className="sr-only">{t("history.view.songs")}</h2>
              <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
                <OpenTake />
                {songs.slice(0, pages * PAGE.songs).map(s => <SongCard key={s.track.id} s={s} fav={hearted(s.track)} onFav={on => heart(s.track, on)} />)}
              </div>
              <More left={songs.length - pages * PAGE.songs} onMore={() => setPages(n => n + 1)} />
            </section>
            )}

            {songs.length > 0 && view === "takes" && (
            <section className="mt-4">
              <h2 className="sr-only">{t("history.view.takes")}</h2>
              <div className="glass overflow-hidden rounded-3xl">
                <table className="w-full text-sm">
                  <thead className="text-left text-xs text-ink-400">
                    <tr className="border-b border-white/6">
                      <th className="px-5 py-3 font-medium">{t("history.col.song")}</th>
                      <ColHead c="when" label={t("history.col.when")} col={col} onCol={setCol} />
                      <ColHead c="sung" label={t("history.col.sung")} col={col} onCol={setCol} right />
                      <ColHead c="pitch" label={t("history.col.pitch")} col={col} onCol={setCol} right />
                      <ColHead c="score" label={t("history.col.score")} col={col} onCol={setCol} right /><th className="w-12 pr-3" />
                    </tr>
                  </thead>
                  <tbody>
                    {rows.slice(0, pages * PAGE.takes).map(tk => (
                      <tr key={tk.id} className="group border-b border-white/4 transition last:border-0 hover:bg-white/3">
                        <td className="px-5 py-3"><Link href={`/take?id=${encodeURIComponent(tk.id)}`} className="block">
                          <div className="font-medium">{tk.track.name} <span className="tabular text-xs font-normal text-ink-400">#{playNo.get(tk.id)}</span></div>
                          <div className="flex items-center gap-2 text-xs text-ink-400">{tk.track.artist}
                            <TakeTags t={tk} />
                            {tk.rescored && <span className="rounded-full bg-white/6 px-1.5 py-px text-[10px] font-medium text-ink-300"
                                                 title={t("history.rescoredTitle")}>{t("history.rescored")}</span>}</div></Link></td>
                        <td className="px-3 py-3 text-ink-300">{when(tk.recorded)}</td>
                        <td className="tabular px-3 py-3 text-right text-ink-200">{Math.round(100 * tk.coverage)}%</td>
                        <td className="tabular px-3 py-3 text-right text-ink-200">{Math.round(100 * tk.accuracy)}%</td>
                        <td className="tabular px-3 py-3 text-right text-lg font-semibold">{tk.score}</td>
                        <td className="relative pr-3 text-right">
                          <span className={`inline-flex justify-end ${BIN}`}>
                            <DeleteButton what={t("history.deleteWhat")} confirm={t("history.deleteConfirm")} onDelete={() => remove([tk.id])} />
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <More left={rows.length - pages * PAGE.takes} onMore={() => setPages(n => n + 1)} />
            </section>
            )}
          </>
        )}
      </main>
    </div>
  );
}

function More({ left, onMore }: { left: number; onMore: () => void }) {
  const t = useT();
  if (left <= 0) return null;
  return (
    <div className="mt-4 text-center">
      <button onClick={onMore} className={pill(false)}>{t.rich("history.showMore", { left, n: c => <span className="tabular text-ink-400">{c}</span> })}</button>
    </div>
  );
}

/** A column of All takes to sort by: click for highest (or newest) first, again for the other way. */
function ColHead({ c, label, col, onCol, right }: { c: Col; label: string; col: { by: Col; down: boolean }; onCol: (v: { by: Col; down: boolean }) => void; right?: boolean }) {
  const on = col.by === c;
  return (
    <th className={`px-3 py-3 font-medium ${right ? "text-right" : ""}`} aria-sort={on ? (col.down ? "descending" : "ascending") : "none"}>
      <button onClick={() => onCol({ by: c, down: on ? !col.down : true })}
              className={`inline-flex items-center gap-1 transition hover:text-white ${on ? "text-ink-100" : ""}`}>
        {label}
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" aria-hidden
             className={`transition ${on ? "opacity-100" : "opacity-0"} ${on && !col.down ? "rotate-180" : ""}`}><path d="M6 9l6 6 6-6" /></svg>
      </button>
    </th>
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

function SongCard({ s, fav, onFav }: { s: { track: TakeSummary["track"]; takes: TakeSummary[]; best: number; last: TakeSummary };
                                       fav: boolean; onFav: (on: boolean) => void }) {
  const t = useT();
  const pts = s.takes.map(tk => tk.score);
  // the song page's line, small: every play, first to latest, across the room the text leaves (x in % of it)
  const H = 34;
  const sx = (i: number) => (pts.length <= 1 ? 50 : (i / (pts.length - 1)) * 100), sy = (v: number) => 4 + (1 - v / 100) * (H - 8);
  const job = useLearn(s.track);     // still being learned: it can be sung again once that's done
  // the whole card links to the song's page; its buttons (heart, sing it again) do their own thing
  return (
    <Link href={`/song?id=${encodeURIComponent(s.track.id)}`}
          className="glass group block rounded-3xl p-4 outline-none transition hover:-translate-y-0.5 hover:border-accent-400/30 focus-visible:ring-2 focus-visible:ring-accent-400/60">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <Cover name={s.track.name} art={s.track.art} link={s.track.link} size={44} />
          <div className="min-w-0">
            <div className="truncate font-medium">{s.track.name}</div>
            <div className="truncate text-sm text-ink-400">{s.track.artist}</div>
          </div>
        </div>
        <div className="flex items-start gap-1">
          <Heart on={fav} onToggle={onFav} subtle />
          <div className="text-right">
            <div className="tabular text-2xl font-bold leading-none">{s.best}</div>
            <div className="text-[10px] uppercase tracking-wider text-ink-400">{t("history.card.best")}</div>
          </div>
        </div>
      </div>
      <div className="mt-3 flex items-end justify-between gap-3">
        {/* (from two plays, as on the song page; the dot is drawn outside the stretched line so it stays round,
            and the line stops short of the edge so the dot isn't cut off) */}
        {pts.length >= 2 ? (
          <div className="relative mr-2 ml-1 h-[34px] min-w-0 flex-1" aria-label={t("history.card.scores", { list: pts.join(", ") })}>
            <svg className="absolute inset-0 h-full w-full overflow-visible" viewBox={`0 0 100 ${H}`} preserveAspectRatio="none" aria-hidden>
              <path d={pts.map((v, i) => `${i ? "L" : "M"}${sx(i)},${sy(v)}`).join(" ")} fill="none" stroke={TREND} strokeWidth={2}
                    strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
            </svg>
            <span className="absolute h-[7px] w-[7px] -translate-x-1/2 -translate-y-1/2 rounded-full"
                  style={{ left: `${sx(pts.length - 1)}%`, top: sy(pts[pts.length - 1]), background: TREND }} />
          </div>
        ) : <span />}
        <div className="flex shrink-0 items-center gap-3">
          <div className="whitespace-nowrap text-right text-xs text-ink-400 transition group-hover:text-ink-100">{t("history.card.plays", { count: s.takes.length, last: s.last.score })}</div>
          <button onClick={e => { e.preventDefault(); e.stopPropagation(); if (!job) void playTrack(s.track.id, true, s.track.name); }}
                  onKeyDown={e => e.stopPropagation()} disabled={!!job}
                  title={job ? t("history.card.whenLearned", { label: learnLabel(job) }) : t("history.card.singAgain")} aria-label={job ? learnLabel(job) : t("history.card.singAgain")}
                  className="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-white/6 text-ink-100 outline-none transition hover:bg-accent-500 hover:text-white
                             focus-visible:ring-2 focus-visible:ring-accent-400/60 disabled:cursor-default disabled:text-ink-500 disabled:hover:bg-white/6">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden><path d="M8 5.5v13l11-6.5z" /></svg>
          </button>
        </div>
      </div>
      {job && <LearnBar job={job} showPct={false} className="mt-3" />}
    </Link>
  );
}

function Empty() {
  const t = useT();
  return (
    <div className="glass grid place-items-center rounded-3xl px-6 py-20 text-center">
      <div>
        <h2 className="text-xl font-semibold">{t("history.empty.title")}</h2>
        <p className="mt-1 text-ink-300">{t("history.empty.body")}</p>
        <Link href="/" className="mt-5 inline-block rounded-xl bg-accent-500 px-4 py-2 font-medium text-white transition hover:bg-accent-400">{t("history.empty.start")}</Link>
      </div>
    </div>
  );
}
function Skeleton() {
  return <div className="grid grid-cols-2 gap-3 md:grid-cols-4">{[0, 1, 2, 3].map(i => <div key={i} className="glass h-[104px] rounded-2xl shimmer" />)}</div>;
}
