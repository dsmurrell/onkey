"use client";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

/** The demo video, set up to work across browsers and phones:
 *  - the <video> is always in the page (search engines don't scroll, so a lazily mounted one isn't found), but with
 *    preload="none" no video bytes load until play; the poster shows meanwhile
 *  - iOS Safari: the address on the element itself (not <source>), playsInline and webkit-playsinline
 *  - starts muted and unmutes on the press (a user gesture), falling back to muted if sound is refused
 *  - lightbox: the press opens it large over a dimmed page (the app's detail is small at half-page width);
 *    Esc, the backdrop or ✕ closes it */
export type DemoText = { label: string; video: string; play: string; dialog: string; close: string };

/** `text`: the words in the page's language (home.video in messages). */
export function DemoVideo({ src, poster, lightbox = true, text }:
    { src: string; poster?: string; lightbox?: boolean; text: DemoText }) {
  const inline = useRef<HTMLVideoElement>(null);
  const big = useRef<HTMLVideoElement>(null);
  const closer = useRef<HTMLButtonElement>(null);
  const [overlay, setOverlay] = useState(true);
  const [open, setOpen] = useState(false);

  const playWithSound = (v: HTMLVideoElement | null) => {
    if (!v) return;
    v.muted = false;
    v.play().catch(() => { v.muted = true; void v.play(); });
  };

  // inline playback: the overlay comes back when it ends
  useEffect(() => {
    const v = inline.current;
    if (!v) return;
    const onPlay = () => setOverlay(false), onEnd = () => setOverlay(true);
    v.addEventListener("play", onPlay); v.addEventListener("ended", onEnd);
    return () => { v.removeEventListener("play", onPlay); v.removeEventListener("ended", onEnd); };
  }, []);

  // the lightbox: plays with sound on open, Esc closes, the page doesn't scroll behind it
  useEffect(() => {
    if (!open) return;
    playWithSound(big.current);
    closer.current?.focus();
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("keydown", onKey);
    const was = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => { document.removeEventListener("keydown", onKey); document.body.style.overflow = was; };
  }, [open]);

  const press = () => (lightbox ? setOpen(true) : playWithSound(inline.current));

  return (
    <div className="relative aspect-[1150/720] bg-black">
      <video
        ref={inline} src={src} poster={poster} preload="none" playsInline muted controls={!overlay}
        {...{ "webkit-playsinline": "" }}
        className="absolute inset-0 h-full w-full object-cover"
        aria-label={text.video}
      />
      {overlay && (
        <button onClick={press} aria-label={text.play}
                className="group absolute inset-0 grid place-items-center bg-black/25 transition hover:bg-black/15">
          <span className="grid h-20 w-20 place-items-center rounded-full bg-white/90 shadow-xl shadow-black/40 transition group-hover:scale-105">
            <svg viewBox="0 0 24 24" className="ml-1 h-9 w-9 text-black" fill="currentColor" aria-hidden><path d="M8 5v14l11-7z" /></svg>
          </span>
          <span className="absolute bottom-4 rounded-full bg-black/60 px-3 py-1 text-sm text-white">{text.label}</span>
        </button>
      )}
      {lightbox && open && typeof document !== "undefined" && createPortal(
        <div role="dialog" aria-modal="true" aria-label={text.dialog} onClick={() => setOpen(false)}
             className="fixed inset-0 z-[100] flex items-center justify-center bg-black/90 p-4 backdrop-blur-sm sm:p-10">
          <button ref={closer} onClick={() => setOpen(false)} aria-label={text.close}
                  className="absolute right-4 top-4 grid h-10 w-10 place-items-center rounded-full bg-white/10 text-white transition hover:bg-white/20">✕</button>
          <video
            ref={big} src={src} poster={poster} preload="auto" playsInline controls
            {...{ "webkit-playsinline": "" }}
            onClick={e => e.stopPropagation()}
            className="aspect-[1150/720] max-h-[90vh] w-full max-w-[1500px] rounded-xl bg-black object-contain shadow-2xl"
          />
        </div>,
        document.body)}
    </div>
  );
}
