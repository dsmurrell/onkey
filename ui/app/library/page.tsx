"use client";
import Link from "next/link";
import LearnSongGuide from "@/components/guides/LearnSongGuide";
import { isDemo } from "@/lib/demo";
import { useEffect, useMemo, useState } from "react";
import Cover from "@/components/Cover";
import CardMenu, { MenuItem } from "@/components/CardMenu";
import DeleteButton from "@/components/DeleteButton";
import Heart, { HEART } from "@/components/Heart";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import LearnBar from "@/components/LearnBar";
import StarterSongs, { useStartersHidden } from "@/components/StarterSongs";
import ReadError from "@/components/ReadError";
import { DifficultyPicker, SortMenu, difficultyWord } from "@/components/ListControls";
import { useT } from "@/lib/i18n";
import { fitText } from "@/lib/engineText";
import { readList, runDelete } from "@/lib/dataSafety";
import { firstPlayGoing, forgetSong, getLibrary, learnAgain, stopLearning, learnLabel, learnNow, live, type LearnJob, type LibrarySong, nowLocal, openInApp, playTrack, setFavorite, useSession } from "@/lib/session";

type Filter = "all" | "favorites" | "unscored" | "partial";
const FILTER_KEY = "onkey.libraryFilter";
type Sort = "recent" | "easy" | "hard" | "best" | "most" | "az";
const SORTS: Sort[] = ["recent", "easy", "hard", "best", "most", "az"];
const SORT_KEY = "onkey.librarySort";
const LEVELS_KEY = "onkey.libraryLevels";
const IN_RANGE_KEY = "onkey.libraryInRange";
const byName = (a: LibrarySong, b: LibrarySong) => a.track.name.localeCompare(b.track.name);
/** Songs in `sort` order; `shown` comes in recent order (or hearted order for Favourites), kept for ties. */
function sorted(songs: LibrarySong[], sort: Sort): LibrarySong[] {
  const s = [...songs];
  const d = (x: LibrarySong) => x.difficulty ?? null;
  switch (sort) {
    case "easy": return s.sort((a, b) => (d(a) ?? 9) - (d(b) ?? 9));
    case "hard": return s.sort((a, b) => (d(b) ?? 0) - (d(a) ?? 0));
    case "best": return s.sort((a, b) => (b.best ?? -1) - (a.best ?? -1));
    case "most": return s.sort((a, b) => (b.takes ?? 0) - (a.takes ?? 0));
    case "az": return s.sort(byName);
    default: return s;
  }
}

// Songs OnKey has learned. Click one to play it in its app (Spotify or Apple Music) and sing it with the notes ahead.
export default function Library() {
  const t = useT();
  const [songs, setSongs] = useState<LibrarySong[] | null>(null);
  const [q, setQ] = useState("");
  const [starting, setStarting] = useState<string | null>(null);
  const [startersHidden, hideStarters] = useStartersHidden();
  // All or Favourites, remembered between visits
  const [filter, setFilterState] = useState<Filter>("all");
  // the difficulty levels picked (none: every song) and the order, remembered too
  const [levels, setLevelsState] = useState<number[]>([]);
  const [sort, setSortState] = useState<Sort>("recent");
  // only songs that suit your voice, as they are or an octave away (range.rs), remembered too
  const [inRange, setInRangeState] = useState(false);
  useEffect(() => {
    try {
      const f = localStorage.getItem(FILTER_KEY);
      if (f === "favorites" || f === "unscored" || f === "partial") setFilterState(f);
      const l = JSON.parse(localStorage.getItem(LEVELS_KEY) ?? "[]");
      if (Array.isArray(l)) setLevelsState(l.filter(n => [1, 2, 3, 4, 5].includes(n)));
      const o = localStorage.getItem(SORT_KEY);
      if (SORTS.some(k => k === o)) setSortState(o as Sort);
      setInRangeState(localStorage.getItem(IN_RANGE_KEY) === "1");
    } catch {}
  }, []);
  const setFilter = (f: Filter) => { setFilterState(f); try { localStorage.setItem(FILTER_KEY, f); } catch {} };
  const setLevels = (l: number[]) => { setLevelsState(l); try { localStorage.setItem(LEVELS_KEY, JSON.stringify(l)); } catch {} };
  const toggleLevel = (n: number) => setLevels(levels.includes(n) ? levels.filter(x => x !== n) : [...levels, n].sort());
  const setInRange = (on: boolean) => { setInRangeState(on); try { localStorage.setItem(IN_RANGE_KEY, on ? "1" : "0"); } catch {} };
  const setSort = (o: Sort) => { setSortState(o); try { localStorage.setItem(SORT_KEY, o); } catch {} };
  // hearts given on this page (over the library as loaded): on, and when
  const [hearts, setHearts] = useState<Map<string, string | null>>(new Map());
  function heart(s: LibrarySong, on: boolean) {
    setHearts(h => new Map(h).set(s.track.id, on ? nowLocal() : null));
    void setFavorite(s.track, on);
  }
  // "Learn again" asked for or taken back on this page (over the library as loaded)
  const [again, setAgain] = useState<Map<string, boolean>>(new Map());
  // asked for: it plays now, as Sing does, and that play learns it afresh (taken back: it's left as it was)
  async function learnItAgain(s: LibrarySong, on: boolean) {
    const st = await learnAgain(s.track.id, on).catch(() => null);
    if (!st) return;
    setAgain(a => new Map(a).set(s.track.id, st.learn_again));
    if (st.learn_again) await sing(s);
  }
  async function forget(id: string) {
    await forgetSong(id);
    setSongs(s => (s ?? []).filter(x => x.track.id !== id));
  }
  // a song being learned (a video that isn't a song, say): its learning stops and it's deleted
  async function stop(track: LibrarySong["track"]) {
    await stopLearning(track);
    setSongs(s => (s ?? []).filter(x => x.track.id !== track.id));
  }
  // songs being learned right now show here too: the one playing, and any still finishing after their play
  useEffect(() => { live.start(); }, []);
  const { song: playing, takes, learning: jobs, ended } = useSession();
  // the play under way, while it is: once it ends (Stop learning and delete, say) its card goes, though the music
  // app may play on
  const song = ended ? null : playing;
  // newest activity first (top left): the song learning as it plays, then those finishing (the latest first), then
  // the library, the last played or learned first
  const learning = useMemo(() => {
    const m = new Map<string, { track: LibrarySong["track"] }>();
    if (song && firstPlayGoing(song, takes, song.track)) m.set(song.track.id, { track: song.track });
    for (const j of [...jobs].reverse()) if (!m.has(j.track_id))
      m.set(j.track_id, { track: { id: j.track_id, name: j.name, artist: j.artist, album: "", duration: 0 } });
    for (const tk of [...takes].reverse()) if (tk.status === "learning" && tk.track_id && !m.has(tk.track_id))
      m.set(tk.track_id, { track: { id: tk.track_id, name: tk.name, artist: tk.artist, album: "", duration: 0 } });
    return m;
  }, [song, takes, jobs]);
  // a song being learned (playing for the first time, or its learning still going) can't be started until it's done
  const learnFor = (track: LibrarySong["track"]): LearnJob | null =>
    learnNow(jobs, takes, song, track);   // (as useLearn)
  const settled = takes.filter(tk => tk.status !== "learning" && tk.status !== "scoring").length;
  // a read that fails shows as that (with a retry), never as "Nothing learned yet"
  const [readErr, setReadErr] = useState<{ error: string; detail: string } | null>(null);
  const [tries, setTries] = useState(0);
  useEffect(() => {
    readList(getLibrary, "songs").then(r => {
      if ("items" in r) { setSongs(r.items.filter(s => s?.track?.id)); setReadErr(null); } else setReadErr(r);
    });
  }, [settled, tries]);
  const all = useMemo(() => {
    if (songs === null) return null;
    const byId = new Map(songs.map(s => [s.track.id, s]));
    const now: LibrarySong[] = [...learning.values()]
      .map(l => byId.get(l.track.id) ?? { track: l.track, learned: null, fraction: 0, ready: false, best: null });
    return [...now, ...songs.filter(s => !learning.has(s.track.id))];
  }, [songs, learning]);
  // each song's heart: as hearted here, else as loaded
  const withHearts = useMemo(() => (all ?? []).map(s => hearts.has(s.track.id)
    ? { ...s, favorite: hearts.get(s.track.id) != null, favorited_at: hearts.get(s.track.id) ?? s.favorited_at } : s), [all, hearts]);
  // the list shows songs ready to sing (and any being learned now); ones only partly learned wait under "Partly
  // learned": they finish learning the next time they play, so there's nothing to sing from them yet
  const partly = (s: LibrarySong) => !s.ready && !learning.has(s.track.id);
  const main = useMemo(() => withHearts.filter(s => !partly(s)), [withHearts, learning]); // eslint-disable-line
  const favCount = withHearts.filter(s => s.favorite).length;
  const unscoredCount = main.filter(s => s.best == null).length;
  const partlyCount = withHearts.length - main.length;
  // songs at each difficulty level, for the level buttons
  const perLevel = useMemo(() => [1, 2, 3, 4, 5].map(n => main.filter(s => s.difficulty === n).length), [main]);
  // songs that suit your voice; none fitted yet (no range of yours yet) and the filter isn't offered
  const fitted = main.some(s => s.fit != null);
  const inRangeCount = main.filter(s => s.fit_suits).length;
  const rangeOn = inRange && fitted;
  // Favourites: the most recently hearted first (when sorted by Recent)
  const shown = useMemo(() => {
    let match = (filter === "partial" ? withHearts.filter(partly) : filter === "favorites" ? withHearts : main)
      .filter(s => `${s.track.name} ${s.track.artist}`.toLowerCase().includes(q.trim().toLowerCase()));
    if (levels.length) match = match.filter(s => s.difficulty != null && levels.includes(s.difficulty));
    if (rangeOn) match = match.filter(s => s.fit_suits);
    if (filter === "unscored") match = match.filter(s => s.best == null);
    if (filter === "favorites")
      match = match.filter(s => s.favorite).sort((a, b) => (b.favorited_at ?? "").localeCompare(a.favorited_at ?? ""));
    return sorted(match, sort);
  }, [withHearts, main, q, filter, levels, sort, rangeOn]); // eslint-disable-line
  const narrowed = levels.length > 0 || q.trim() !== "" || rangeOn;

  async function sing(s: LibrarySong) {
    setStarting(s.track.id);
    try { await playTrack(s.track.id, true, s.track.name); }     // or, in listening mode without control, the prompt shows
    catch {}
    finally { setStarting(null); }
  }

  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-6xl px-6 pb-16 min-[1400px]:max-w-[88rem]">
        <div className="mt-6 mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <h1 className="text-3xl font-semibold tracking-tight">{t("library.title")}</h1>
            <p className="mt-1 text-ink-300">{t("library.intro")}</p>
          </div>
          {!readErr && all && all.length > 0 && (
            <input value={q} onChange={e => setQ(e.target.value)} placeholder={t("library.search")}
                   className="w-64 rounded-xl border border-white/8 bg-white/4 px-3.5 py-2 text-sm text-ink-100 placeholder:text-ink-400 outline-none transition focus:border-accent-400/50 focus:bg-white/6" />
          )}
        </div>

        {readErr ? <ReadError {...readErr} onRetry={() => setTries(n => n + 1)} /> : all === null ? (
          <div className={GRID}>{[0, 1, 2, 3].map(i => <div key={i} className="glass h-[88px] rounded-3xl shimmer" />)}</div>
        ) : all.length === 0 ? (
          <div className={`glass rounded-3xl px-6 text-center ${startersHidden === false ? "pt-16 pb-6" : "py-20"}`}>
            <h2 className="text-xl font-semibold">{t("library.empty.title")}</h2>
            <p className="mx-auto mt-1 max-w-md text-ink-300">{t("library.empty.body")}</p>
            <div className="mx-auto mt-6 max-w-[340px] text-left"><LearnSongGuide /></div>
            <Link href="/" className="mt-5 inline-block rounded-xl bg-accent-500 px-4 py-2 font-medium text-white transition hover:bg-accent-400">{t("library.empty.start")}</Link>
            {startersHidden === false && <StarterSongs onHide={() => hideStarters(true)} className="mx-auto mt-12 max-w-4xl" />}
            {startersHidden && <button onClick={() => hideStarters(false)} className="mt-4 block w-full text-xs text-ink-500 transition hover:text-ink-200">{t("library.empty.showStarters")}</button>}
          </div>
        ) : (<>
          <div className="mb-4 flex flex-wrap items-center gap-x-4 gap-y-3">
          <div role="radiogroup" aria-label={t("library.filter.label")} className="inline-flex rounded-full border border-white/8 bg-white/4 p-1 text-sm">
            {([["all", main.length], ["favorites", favCount], ["unscored", unscoredCount],
               ...(partlyCount ? [["partial", partlyCount]] as const : [])] as const).map(([f, n]) => (
              <button key={f} role="radio" aria-checked={filter === f} onClick={() => setFilter(f)} title={t("library.filter.count", { count: n })}
                      className={`flex items-center gap-1.5 rounded-full px-3.5 py-1.5 outline-none transition focus-visible:ring-2 focus-visible:ring-accent-400/60
                                  ${filter === f ? "bg-white/10 text-white" : "text-ink-300 hover:text-white"}`}>
                {f === "favorites" && <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor" aria-hidden
                                           className={filter === f ? "text-accent-400" : ""}><path d={HEART} /></svg>}
                {t(`library.filter.${f}`)}
              </button>
            ))}
          </div>
          <DifficultyPicker levels={levels} onToggle={toggleLevel} onClear={() => setLevels([])} counts={perLevel} />
          {fitted && (
            <button aria-pressed={rangeOn} onClick={() => setInRange(!rangeOn)}
                    title={t("library.inRange.title", { count: inRangeCount })}
                    className={`rounded-full border border-white/8 px-3.5 py-2 text-sm outline-none transition focus-visible:ring-2 focus-visible:ring-accent-400/60
                                ${rangeOn ? "bg-white/10 text-white" : "bg-white/4 text-ink-300 hover:text-white"}`}>
              {t("library.inRange.label")}
            </button>
          )}
          <SortMenu sorts={SORTS.map(k => [k, t(`library.sort.${k}`)] as [Sort, string])} sort={sort} onSort={setSort} />
          <span className="tabular text-sm text-ink-500">{filter === "partial" ? t("library.shownPartly", { count: shown.length }) : t("library.shownLearned", { count: shown.length })}</span>
          </div>
          {filter === "partial" && partlyCount > 0 && (
            <div className="-mt-1 mb-4 flex flex-wrap items-center justify-between gap-x-4 gap-y-2 px-1 text-sm text-ink-400">
              <p>{t("library.partlyNote")}</p>
              <ClearPartly songs={shown.filter(x => !x.favorite)} hearted={shown.filter(x => x.favorite).length} narrowed={narrowed} onDelete={forget} />
            </div>
          )}
          {filter === "favorites" && favCount === 0 ? (
            <div className="glass rounded-3xl px-6 py-16 text-center">
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden className="mx-auto mb-3 text-accent-400"><path d={HEART} /></svg>
              <p className="text-ink-200">{t("library.noFavourites")}</p>
            </div>
          ) : shown.length === 0 ? (
            <p className="px-1 py-10 text-center text-ink-400">
              {filter === "partial" && !narrowed ? t("library.none.partly") : filter === "unscored" && !narrowed ? t("library.none.unscored") : q.trim() ? t("library.none.search", { q: q.trim() }) : t("library.none.filters")}
              {narrowed && <button onClick={() => { setQ(""); setLevels([]); setInRange(false); }} className="ml-2 text-accent-300 underline-offset-2 hover:underline">{t("library.none.clear")}</button>}
            </p>
          ) : (
          <div className={GRID}>
            {shown.map(s => { const job = learnFor(s.track); const being = !!job || learning.has(s.track.id);
                              const learnsAgain = again.get(s.track.id) ?? !!s.learn_again; return (
              <div key={s.track.id} role="button" tabIndex={0} aria-disabled={!!job || undefined}
                   aria-label={job ? undefined : t("library.card.sing", { name: s.track.name, artist: s.track.artist })}
                   title={job ? t("library.card.whenLearned") : !s.ready ? t("library.card.playThrough") : undefined}
                   onClick={() => !job && !starting && sing(s)}
                   onKeyDown={e => { if (e.key === "Enter" && !job && !starting) sing(s); }}
                   // over the cards after it while its menu is open
                   className={`glass group relative rounded-3xl p-3.5 text-left transition has-[[aria-expanded=true]]:z-20
                               ${job ? "cursor-default" : "cursor-pointer hover:-translate-y-0.5 hover:border-accent-400/35"} ${starting ? "opacity-60" : ""}`}>
                <div className="flex items-center gap-3.5">
                  <Cover name={s.track.name} art={s.track.art} link={s.track.link} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{s.track.name}</div>
                    <div className="truncate text-sm text-ink-400">{s.track.artist}</div>
                    <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 whitespace-nowrap text-xs">
                      {/* a status only when it isn't simply ready to sing */}
                      {job
                        ? <span className="flex items-center gap-1.5 rounded-full bg-accent-500/12 px-2 py-0.5 font-medium text-accent-300">
                            <span className="h-1.5 w-1.5 rounded-full bg-accent-400" />{t("library.card.learning")}</span>
                        : learnsAgain
                        ? <span className="rounded-full bg-accent-500/12 px-2 py-0.5 font-medium text-accent-300"
                                title={t("library.card.learnsAgainTitle")}>{t("library.card.learnsAgain")}</span>
                        : !s.ready
                        ? <span className="rounded-full bg-close/12 px-2 py-0.5 font-medium text-close">{t("library.card.pctLearned", { pct: Math.round(100 * s.fraction) })}</span>
                        : s.relearn
                        ? <span className="rounded-full bg-close/12 px-2 py-0.5 font-medium text-close"
                                title={t("library.card.relearnTitle", { at: clock(s.relearn.at) })}>{t("library.card.relearn")}</span>
                        : s.best == null
                        ? <span className="rounded-full bg-white/6 px-2 py-0.5 text-ink-300" title={t("library.card.notScoredTitle")}>{t("library.card.notScored")}</span>
                        : null}
                      {s.difficulty != null && <Difficulty level={s.difficulty} />}
                      {/* your plays of it (its page: best, trend, every take), once you've sung it */}
                      {(s.takes ?? 0) > 0 && (
                        <Link href={`/song?id=${encodeURIComponent(s.track.id)}`} onClick={e => e.stopPropagation()} onKeyDown={e => e.stopPropagation()}
                              title={t("library.card.playsTitle")} className="truncate text-ink-400 underline-offset-2 transition hover:text-ink-100 hover:underline">
                          {s.best != null
                            ? t.rich("library.card.playsBest", { count: s.takes ?? 0, best: s.best, n: c => <span className="tabular font-semibold text-ink-100">{c}</span> })
                            : t("library.card.plays", { count: s.takes ?? 0 })}
                        </Link>
                      )}
                      {isDemo(s.track.id) && <span className="text-ink-500" title={t("library.card.demoTitle")}>{t("library.card.demo")}</span>}
                    </div>
                    {/* how it suits your voice, when known */}
                    {!job && s.ready && fitText(t, s) && (
                      <div className={`mt-1 text-xs ${s.fit === "fits" ? "text-good/90" : "text-aside/90"}`}
                           title={s.range_names ? t("library.card.fitTitle", { label: fitText(t, s)!, low: s.range_names[0], high: s.range_names[1] }) : fitText(t, s)!}>{fitText(t, s)}</div>
                    )}
                  </div>
                  {/* the heart and the ⋯ menu (its app, delete) at the top; Sing below (the whole card sings it) */}
                  <div className="-mt-1 -mr-1 flex flex-col items-end justify-between self-stretch">
                    <span className="flex items-center" onClick={e => e.stopPropagation()} onKeyDown={e => e.stopPropagation()}>
                      <Heart on={!!s.favorite} onToggle={on => heart(s, on)} subtle />
                      <CardMenu label={t("library.menu.more", { name: s.track.name })}>
                        {s.app && !job && !isDemo(s.track.id) && <MenuItem onSelect={() => void openInApp(s.track.id)}>{t("library.menu.openIn", { app: s.app })}</MenuItem>}
                        {s.ready && !being && !isDemo(s.track.id) && (
                          <MenuItem onSelect={() => void learnItAgain(s, !learnsAgain)}>{learnsAgain ? t("library.menu.dontLearnAgain") : t("library.menu.learnAgain")}</MenuItem>
                        )}
                        {/* the row presses its trash button, which asks before deleting (the label goes while it asks) */}
                        <span onClick={e => { if (!(e.target as HTMLElement).closest("button")) e.currentTarget.querySelector("button")?.click(); }}
                              className="flex cursor-pointer items-center gap-1 rounded-xl py-0.5 pr-3 pl-1 text-ink-300 transition hover:bg-white/8 [&:has(>span:first-child)>[data-label]]:hidden">
                          {!being ? (
                            <DeleteButton what={t("library.menu.deleteWhat")} confirm={t("library.menu.deleteConfirm")}
                                          onDelete={() => forget(s.track.id)} />
                          ) : (
                            <DeleteButton what={t("library.menu.stopWhat")} confirm={t("library.menu.stopConfirm")}
                                          onDelete={() => stop(s.track)} />
                          )}
                          <span data-label>{being ? t("library.menu.stop") : t("library.menu.delete")}</span>
                        </span>
                      </CardMenu>
                    </span>
                    {!job && (
                      <span aria-hidden className="mr-1 mb-0.5 flex items-center gap-1 whitespace-nowrap rounded-full px-2 py-0.5 text-sm font-medium text-accent-300 transition group-hover:bg-accent-500/15 group-hover:text-white">
                        {starting === s.track.id ? t("library.card.starting") : t("library.card.singButton")}
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden><path d="M8 5.5v13l11-6.5z" /></svg>
                      </span>
                    )}
                  </div>
                </div>
                {job && (
                  <div className="mt-3 flex items-center gap-4 text-sm">
                    <LearnBar job={job} showPct={false} className="min-w-0 flex-1" />
                    <span className="tabular shrink-0 font-medium text-ink-400">{learnLabel(job)}</span>
                  </div>
                )}
              </div>
            ); })}
          </div>
          )}
          {/* songs to try, always reachable (collapsed to a link once hidden) */}
          <div className="mt-12">
            {startersHidden === false && <StarterSongs onHide={() => hideStarters(true)} className="mx-auto max-w-4xl" />}
            {startersHidden && <button onClick={() => hideStarters(false)} className="block w-full text-center text-xs text-ink-500 transition hover:text-ink-200">{t("library.startersLink")}</button>}
          </div>
        </>)}
        <p className="mt-12 max-w-2xl text-xs leading-relaxed text-ink-500">
          {t.rich("library.footer", { link: c => <a href="https://lrclib.net" target="_blank" rel="noreferrer" className="underline decoration-ink-600 underline-offset-2 hover:text-ink-300">{c}</a> })}
        </p>
      </main>
    </div>
  );
}

/** The cards: up to four across on a wide window. */
const GRID = "grid gap-3 sm:grid-cols-2 lg:grid-cols-3 min-[1400px]:grid-cols-4";

/** How hard a song is to sing, as 1-5 dots (store::difficulty): mostly the range of its melody. */
function Difficulty({ level }: { level: number }) {
  const t = useT();
  const word = difficultyWord(t, level);
  return (
    <span className="tabular rounded-md border border-white/10 px-1.5 py-px text-ink-300" aria-label={t("library.difficulty.label", { level, word })}
          title={t("library.difficulty.title", { level, word })}>
      {level}<span className="text-ink-500">/5</span>
    </span>
  );
}

/** The order, as a pill like the filters beside it, opening a small menu. */
/** The Partly learned tab's "Delete all": every song shown there (not hearted ones), after asking. Each goes to the
 *  Trash as one deleted from its card does; takes stay in History. */
function ClearPartly({ songs, hearted, narrowed, onDelete }: { songs: LibrarySong[]; hearted: number; narrowed: boolean; onDelete: (id: string) => Promise<void> }) {
  const t = useT();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  // a delete that failed (the songs before it are gone, it and those after stay): said here, and it can be tried again
  const [error, setError] = useState<string | null>(null);
  async function deleteAll() {
    setBusy(true);
    setError(null);
    const failed = await runDelete(async () => { for (const x of songs) await onDelete(x.track.id); });
    setBusy(false);
    if (failed) setError(failed);
    else setAsking(false);
  }
  if (songs.length === 0) return null;
  const n = songs.length;
  if (!asking) {
    return (
      <button onClick={() => setAsking(true)} className="shrink-0 text-ink-400 underline-offset-2 transition hover:text-white hover:underline">
        {t("library.clearPartly.button", { count: n })}
      </button>
    );
  }
  return (
    <span className="flex shrink-0 items-center gap-3" role="group" aria-label={t("library.clearPartly.label")}>
      {error
        ? <span className="text-off" role="alert">{error}</span>
        : <span className="text-ink-200">{t("library.clearPartly.ask", { count: n, shown: narrowed ? "yes" : "no", hearted })}</span>}
      <button disabled={busy} onClick={deleteAll}
              className="rounded-lg bg-off/80 px-3 py-1 font-medium text-white transition hover:bg-off disabled:opacity-60">
        {busy ? t("library.clearPartly.deleting") : error ? t("library.clearPartly.tryAgain") : t("library.clearPartly.deleteAll")}
      </button>
      <button disabled={busy} onClick={() => { setAsking(false); setError(null); }} className="text-ink-400 transition hover:text-white">{t("library.clearPartly.cancel")}</button>
    </span>
  );
}

/** A song time as m:ss (2:06). */
function clock(s: number) {
  const secs = Math.max(0, Math.round(s));
  return `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
}
