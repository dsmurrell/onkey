import Link from "next/link";
import { LogoMark } from "./Logo";
import { GITHUB_URL, NOT_AFFILIATED_SHORT, DISCORD_URL } from "@/lib/site";

export function Footer() {
  return (
    <footer className="border-t border-line">
      <div className="mx-auto grid max-w-6xl gap-8 px-4 py-10 text-sm text-muted sm:px-6 md:grid-cols-[1fr_auto]">
        <div className="grid max-w-xl gap-3">
          <div className="flex items-center gap-2.5 text-fg">
            <LogoMark size={24} />
            <span className="font-semibold tracking-tight">OnKey</span>
            <span className="text-muted">· free and open source (MIT)</span>
          </div>
          <p className="text-xs italic text-muted">Saving shower singers from themselves since 2026</p>
          <p className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted">
            <img src="/dsmurrell.png" alt="" width={16} height={16} className="rounded-sm" />
            Made by <a href="https://dsmurrell.com" data-track="Daniel's site" className="text-fg underline-offset-2 hover:underline">Daniel Murrell</a>
          </p>
          <p className="text-xs leading-relaxed text-faint">{NOT_AFFILIATED_SHORT}</p>
        </div>
        <nav className="flex flex-wrap gap-x-6 gap-y-2" aria-label="Footer">
          <Link href="/docs/" className="inline-block py-1.5 hover:text-fg">Getting started</Link>
          <Link href="/docs/how-it-works/" className="inline-block py-1.5 hover:text-fg">How it works</Link>
          <Link href="/docs/privacy/" className="inline-block py-1.5 hover:text-fg">Privacy &amp; legal</Link>
          <Link href="/docs/faq/" className="inline-block py-1.5 hover:text-fg">FAQ</Link>
          <Link href="/changelog/" className="inline-block py-1.5 hover:text-fg">What&rsquo;s new</Link>
          <a href={GITHUB_URL} data-track="GitHub" className="inline-block py-1.5 hover:text-fg">GitHub</a>
          {DISCORD_URL && <a href={DISCORD_URL} data-track="Discord" className="inline-block py-1.5 hover:text-fg">Discord</a>}
        </nav>
      </div>
    </footer>
  );
}
