"use client";
import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { sizedArt, spotifySearch } from "@/components/Cover";
import { getSettings, inApp, openLink, playTrack, setSetting } from "@/lib/session";
import { useLocale, useT, type MessageKey } from "@/lib/i18n";
import { startersFor, type StarterList, type StarterSong } from "@/lib/starterSongs";

/** Songs that are good to start with, in the window's language (lib/starterSongs.ts: easy to hard, a reason each;
 *  English for a language without its own list). A cover is looked up on Apple's iTunes Search once and kept in the
 *  app's data folder (music.rs starter_cover). */
function useStarters(): StarterList & { list: string } {
  return startersFor(useLocale());
}

/** Apple Music's search for a song (a plain link to music.apple.com, no API; it goes to your own storefront). */
const appleSearch = (artist: string, title: string) =>
  `https://music.apple.com/search?term=${encodeURIComponent(`${artist} ${title}`)}`;

const HIDE_KEY = "onkey.hideStarters";   // outside the app (the browser preview), where there are no settings

/** Whether the suggestions are hidden ("Hide suggestions", kept in Settings), null until known, and a setter. */
export function useStartersHidden(): [boolean | null, (hide: boolean) => void] {
  const [hidden, setHidden] = useState<boolean | null>(null);
  useEffect(() => {
    if (!inApp()) { try { setHidden(localStorage.getItem(HIDE_KEY) === "1"); } catch { setHidden(false); } return; }
    getSettings().then(s => setHidden(!!s?.hide_starters)).catch(() => setHidden(false));
  }, []);
  const set = (hide: boolean) => {
    setHidden(hide);
    if (inApp()) void setSetting("hide_starters", hide);
    else try { localStorage.setItem(HIDE_KEY, hide ? "1" : "0"); } catch {}
  };
  return [hidden, set];
}

const open = (href: string) => (e: SyntheticEvent) => { e.preventDefault(); e.stopPropagation(); void openLink(href); };

function Out({ href, label, dim, title }: { href: string; label: string; dim?: boolean; title?: string }) {
  const t = useT();
  return (
    <a href={href} onClick={open(href)} title={title ?? t("starters.searchIn", { app: label })}
       className={`whitespace-nowrap underline-offset-2 transition hover:underline ${dim ? "text-ink-500 hover:text-ink-300" : "text-ink-400 hover:text-ink-200"}`}>{label} ↗</a>
  );
}

type StarterCover = { art: string; link: string | null };
/** Covers found this session (null: none found), so coming back to the page asks the app again for nothing. */
const covers = new Map<string, StarterCover | null>();
const songKey = (s: { artist: string; title: string }) => `${s.artist}\n${s.title}`;

/** The starter songs' covers, asked for one at a time (the app keeps them; the first time, it looks each up,
 *  spaced). Only in the app: the browser preview shows the drawn tiles. */
function useStarterCovers(songs: StarterSong[]): Map<string, StarterCover | null> {
  const [, redraw] = useState(0);
  useEffect(() => {
    if (!inApp()) return;
    let live = true;
    (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      for (const s of songs) {
        if (!live) return;
        if (covers.has(songKey(s))) continue;
        const c = await invoke<StarterCover | null>("starter_cover", { artist: s.artist, title: s.title }).catch(() => undefined);
        if (c === undefined) continue;   // asked again next time
        covers.set(songKey(s), c);
        if (live) redraw(n => n + 1);
      }
    })();
    return () => { live = false; };
  }, [songs]);
  return covers;
}

/** No cover (offline, not found, or outside the app): a tile coloured from the title with a little melody drawn in
 *  the lane's style (rounded note bars stepping up and down), different for every song. Drawn here: no one's art. */
function StarterTile({ title, size }: { title: string; size: number }) {
  let h = 7;
  for (const c of title) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hue = h % 360;
  let seed = h || 1;
  const rnd = () => { seed = (Math.imul(seed, 1103515245) + 12345) >>> 0; return (seed >>> 8) / 0x1000000; };
  const bars: { x: number; y: number; w: number }[] = [];
  let x = 5, y = 16 + Math.round(rnd() * 12);
  while (x < 36) {
    const w = Math.min(39 - x, 3 + Math.round(rnd() * 5));
    if (w >= 3) bars.push({ x, y, w });
    x += w + 1.5;
    const step = (rnd() < 0.5 ? -1 : 1) * (3 + Math.round(rnd() * 5));
    y = y + step < 9 || y + step > 35 ? y - step : y + step;
  }
  return (
    <svg viewBox="0 0 44 44" width={size} height={size} aria-hidden className="shrink-0 shadow-lg shadow-black/30"
         style={{ borderRadius: Math.round(size * 0.28) }}>
      <defs>
        <linearGradient id={`starter-${h}`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor={`hsl(${hue} 55% 36%)`} /><stop offset="1" stopColor={`hsl(${(hue + 40) % 360} 50% 15%)`} />
        </linearGradient>
      </defs>
      <rect width="44" height="44" fill={`url(#starter-${h})`} />
      {[14, 22, 30].map(r => <rect key={r} x="0" y={r} width="44" height="0.6" fill="rgba(255,255,255,.08)" />)}
      {bars.map((b, i) => <rect key={i} x={b.x} y={b.y - 2.25} width={b.w} height="4.5" rx="2.25" fill={`hsl(${hue} 90% 87%)`} opacity={0.9} />)}
    </svg>
  );
}

/** A starter song's cover, linked to the song on Apple Music (as Apple's artwork terms ask), or the drawn tile. */
function StarterArt({ title, artist, cover, size = 44 }: { title: string; artist: string; cover?: StarterCover | null; size?: number }) {
  const t = useT();
  const [failed, setFailed] = useState(false);
  if (!cover || failed) return <StarterTile title={title} size={size} />;
  const img = (
    // eslint-disable-next-line @next/next/no-img-element
    <img src={cover.art.startsWith("data:") ? cover.art : sizedArt(cover.art, size * 2)} alt="" width={size} height={size}
         onError={() => setFailed(true)} className="block shrink-0 object-cover shadow-lg shadow-black/30 ring-1 ring-white/8"
         style={{ width: size, height: size, borderRadius: Math.round(size * 0.28) }} />
  );
  if (!cover.link) return img;
  return (
    <a href={cover.link} onClick={open(cover.link)} title={t("starters.openInAppleMusic")} aria-label={t("starters.openSongInAppleMusic", { title, artist })}
       className="shrink-0 transition hover:opacity-85">{img}</a>
  );
}

/** "Play the OnKey starter playlist": opens it in the Spotify app (or Apple Music), where OnKey hears it. (Not
 *  the services' embedded players: those played inside OnKey's own window, which OnKey doesn't listen to.) */
function PlaylistStrip({ playlist, playlistApple }: { playlist?: string; playlistApple?: string }) {
  const t = useT();
  const href = playlist || playlistApple || "";
  const app = playlist ? "Spotify" : "Apple Music";
  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-2xl border border-accent-400/25 bg-accent-500/10 px-4 py-3">
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium text-ink-100">{t("starters.playlist.title")}</span>
        <span className="block text-xs text-ink-300">{t("starters.playlist.sub")}</span>
      </span>
      {playlist && playlistApple && (
        <Out href={playlistApple} label={t("starters.playlist.orAppleMusic")} dim title={t("starters.playlist.openInAppleMusic")} />
      )}
      <a href={href} onClick={open(href)} title={t("starters.playlist.open", { app })}
         className="inline-flex shrink-0 items-center gap-1.5 rounded-xl bg-accent-500 px-3.5 py-1.5 text-sm font-medium text-white transition hover:bg-accent-400">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden><path d="M7 4.5v15l13-7.5z" /></svg>
        {t("starters.playlist.playIn", { app })}
      </a>
    </div>
  );
}

/** The cards in one row that scrolls sideways: the edge fades where there's more, and ‹ › buttons that scroll by a
 *  card (each hidden at its end). */
function Scroller({ children }: { children: React.ReactNode }) {
  const t = useT();
  const ref = useRef<HTMLUListElement>(null);
  const [ends, setEnds] = useState({ start: true, end: false });
  useEffect(() => {
    const ul = ref.current;
    if (!ul) return;
    // each end is reached when its card is wholly in view: watched, not worked out from scroll events (the app's
    // WebKit didn't always send one for where a smooth scroll stopped, leaving › up and the last card faded)
    const first = ul.firstElementChild, last = ul.lastElementChild;
    if (!first || !last) return;
    const seen = new Map<Element, boolean>();
    const io = new IntersectionObserver(entries => {
      for (const e of entries) seen.set(e.target, e.intersectionRatio >= 0.97);
      setEnds({ start: seen.get(first) ?? true, end: seen.get(last) ?? false });
    }, { root: ul, threshold: [0, 0.5, 0.97, 1] });
    io.observe(first);
    io.observe(last);
    return () => io.disconnect();
  }, []);
  const by = (dir: 1 | -1) => {
    const ul = ref.current, card = ul?.firstElementChild as HTMLElement | null;
    if (!ul || !card) return;
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    // stops at each card's edge, plus the two ends; a card edge within half a card of an end is dropped, so the last
    // step forward lands on the very end (not 24 px short of it, leaving the last card cut off and › up) and the
    // last step back on the very start
    const step = card.offsetWidth + 8, max = ul.scrollWidth - ul.clientWidth, sl = ul.scrollLeft;
    const stops = [0];
    for (let x = step; x < max - step / 2; x += step) stops.push(x);
    if (max > 0) stops.push(max);
    const to = dir > 0 ? stops.find(x => x > sl + 2) ?? max : [...stops].reverse().find(x => x < sl - 2) ?? 0;
    ul.scrollTo({ left: to, behavior: still ? "auto" : "smooth" });
  };
  const fade = `linear-gradient(to right, ${ends.start ? "black" : "transparent"}, black 2.5rem, black calc(100% - 2.5rem), ${ends.end ? "black" : "transparent"})`;
  const arrow = "absolute top-1/2 z-10 grid h-8 w-8 -translate-y-1/2 place-items-center rounded-full border border-white/10 bg-ink-900/90 text-lg leading-none text-ink-200 shadow-lg shadow-black/40 transition hover:bg-ink-850 hover:text-white";
  return (
    <div className="relative">
      <ul ref={ref} className="flex gap-2 overflow-x-auto pb-2 select-none [scrollbar-width:none]" style={{ maskImage: fade, WebkitMaskImage: fade }}>
        {children}
      </ul>
      {!ends.start && <button onClick={() => by(-1)} aria-label={t("starters.earlier")} className={`${arrow} -left-3`}>‹</button>}
      {!ends.end && <button onClick={() => by(1)} aria-label={t("starters.more")} className={`${arrow} -right-3`}>›</button>}
    </div>
  );
}

export default function StarterSongs({ onHide, onPlay, className = "", hideLabel }: {
  onHide: () => void;
  /** a song was started (the sheet over Sing closes) */
  onPlay?: () => void;
  className?: string; hideLabel?: string;
}) {
  const t = useT();
  const starters = useStarters();
  // A song with a Spotify id is sung as a Library card is: the same playTrack, which starts it in its app and goes to
  // Sing, showing it as starting there (or why the app couldn't, with a button to open it). Its Spotify and Apple
  // Music links stay for opening it there yourself. In the browser preview there's no app: the links only.
  const [starting, setStarting] = useState<string | null>(null);
  const sing = async (s: StarterSong) => {
    if (!s.spotify || starting) return;
    setStarting(s.spotify);
    onPlay?.();
    try { await playTrack(`spotify:track:${s.spotify}`, true, s.title); }
    catch {}
    finally { setStarting(null); }
  };
  const playable = (s: StarterSong) => !!s.spotify && inApp();
  const playlist = !!(starters.playlist || starters.playlistApple);
  const found = useStarterCovers(starters.songs);
  return (
    <section aria-label={t("starters.label")} className={`w-full text-left ${className}`}>
      <div className="mb-2 flex items-baseline justify-between gap-3 px-1 text-xs">
        <span className="font-medium text-ink-300">{t.rich("starters.heading", { dim: c => <span className="font-normal text-ink-500">{c}</span> })}</span>
        <button onClick={onHide} className="text-ink-500 transition hover:text-ink-200">{hideLabel ?? t("starters.hide")}</button>
      </div>
      <Scroller>
        {starters.songs.map(s => {
          const cover = found.get(songKey(s));
          return (
            <li key={songKey(s)}
                {...(playable(s) ? {
                  role: "button", tabIndex: 0, "aria-label": t("library.card.sing", { name: s.title, artist: s.artist }),
                  onClick: () => void sing(s), onKeyDown: (e: React.KeyboardEvent) => { if (e.key === "Enter") void sing(s); },
                } : {})}
                className={`glass group flex w-48 shrink-0 flex-col rounded-2xl p-3 transition ${playable(s) ? "cursor-pointer hover:border-accent-400/35" : ""} ${starting === s.spotify ? "opacity-60" : ""}`}>
              <span className="flex items-center gap-2.5">
                <StarterArt title={s.title} artist={s.artist} cover={cover} />
                <span className="flex min-w-0 flex-col">
                  <span className="truncate text-sm font-medium text-ink-100" title={s.title}>{s.title}</span>
                  <span className="truncate text-xs text-ink-400" title={s.artist}>{s.artist}</span>
                </span>
              </span>
              <span className="mt-2 flex-1 text-xs leading-snug text-ink-300">{t(`starters.why.${starters.list}.${s.why}` as MessageKey)}</span>
              {playable(s) ? (
              // sung from the card (it starts in Spotify, as a Library card does): its Sing ▶, and Apple Music's link
              <span className="mt-2 flex items-center justify-between gap-1.5 text-[11px]">
                <span aria-hidden className="-ml-2 flex items-center gap-1 whitespace-nowrap rounded-full px-2 py-0.5 text-sm font-medium text-accent-300 transition group-hover:bg-accent-500/15 group-hover:text-white">
                  {starting === s.spotify ? t("library.card.starting") : t("library.card.singButton")}
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden><path d="M8 5.5v13l11-6.5z" /></svg>
                </span>
                {cover?.link ? <Out href={cover.link} label={t("starters.appAppleMusic")} dim title={t("starters.openItInAppleMusic")} />
                  : <Out href={appleSearch(s.artist, s.title)} label={t("starters.appAppleMusic")} dim />}
              </span>
              ) : (
              <span className="mt-2 flex gap-1.5 text-[11px]">
                <span className="text-ink-500">{t("starters.in")}</span><Out href={spotifySearch(s.artist, s.title)} label={t("starters.appSpotify")} />
                <span className="text-ink-600">·</span>
                {cover?.link ? <Out href={cover.link} label={t("starters.appAppleMusic")} dim title={t("starters.openItInAppleMusic")} />
                  : <Out href={appleSearch(s.artist, s.title)} label={t("starters.appAppleMusic")} dim />}
              </span>
              )}
            </li>
          );
        })}
      </Scroller>
      {playlist && <PlaylistStrip playlist={starters.playlist} playlistApple={starters.playlistApple} />}
    </section>
  );
}

/** The songs to try over the Sing page (the logo, clicked on Sing, opens it: "onkey-songs-to-try"), even mid-song,
 *  without leaving the lane; Esc or a click outside closes it. */
export function StarterSheet() {
  const t = useT();
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const show = () => setOpen(true);
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    window.addEventListener("onkey-songs-to-try", show);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("onkey-songs-to-try", show); window.removeEventListener("keydown", key); };
  }, []);
  if (!open) return null;
  return (
    <div role="dialog" aria-modal="true" aria-label={t("starters.label")} onClick={() => setOpen(false)}
         className="fixed inset-0 z-40 grid place-items-start overflow-y-auto bg-ink-950/70 px-4 pt-24 pb-10 backdrop-blur-sm animate-rise">
      <div onClick={e => e.stopPropagation()} className="glass mx-auto w-full max-w-4xl rounded-3xl p-5">
        <StarterSongs onHide={() => setOpen(false)} onPlay={() => setOpen(false)} hideLabel={t("starters.close")} />
      </div>
    </div>
  );
}
