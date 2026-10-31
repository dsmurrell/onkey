"use client";
// Using OnKey fairly: the README's section of the same name, for reading in the app (Settings links here). Keep the
// two saying the same things.
import Link from "next/link";
import { Logo, Nav, WhatsNew } from "@/components/Nav";

export default function FairUsePage() {
  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-3xl px-6 pb-16">
        <Link href="/settings" className="mt-6 inline-block text-xs text-ink-400 hover:text-white">← Settings</Link>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight">Using OnKey fairly</h1>
        <p className="mt-2 text-ink-300">OnKey is for practising at home, with music you&rsquo;re entitled to play. Please:</p>
        <ul className="glass mt-6 space-y-4 rounded-3xl p-6 text-sm leading-relaxed text-ink-200">
          <Rule title="Keep what it learns to yourself.">
            Its transcription of each song&rsquo;s melody is for your practice, so don&rsquo;t share or publish it.
            Sharing OnKey itself is welcome.
          </Rule>
          <Rule title="To share a take, use Share on its page.">
            It makes a short video of your voice and your pitch line, with no music, and you can add the song from
            Instagram&rsquo;s own music library, which is licensed for this. Headphone takes are best: sung on speakers,
            some of the song is in your recording. Don&rsquo;t post screen recordings with the music playing.
          </Rule>
          <Rule title="Use a licensed karaoke service for a bar or an event.">
            OnKey&rsquo;s lyrics come from a free community database.
          </Rule>
          <Rule title="Don’t modify OnKey to save the music." />
        </ul>
        <p className="mt-4 px-1 text-sm text-ink-400">
          OnKey never saves the music it listens to. Rights holders can reach me
          at <span className="select-text text-ink-200">hello@singonkey.app</span>.
        </p>
      </main>
    </div>
  );
}

function Rule({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="mt-[0.55em] h-1.5 w-1.5 shrink-0 rounded-full bg-accent-400" aria-hidden />
      <span><b className="font-medium text-ink-100">{title}</b>{children && <> {children}</>}</span>
    </li>
  );
}
