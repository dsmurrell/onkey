"use client";
// Using OnKey fairly: the README's section of the same name, for reading in the app (Settings links here). Keep the
// two saying the same things.
import Link from "next/link";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import { useT } from "@/lib/i18n";

export default function FairUsePage() {
  const t = useT();
  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-3xl px-6 pb-16">
        <Link href="/settings" className="mt-6 inline-block text-xs text-ink-400 hover:text-white">{t("fairUse.back")}</Link>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight">{t("fairUse.title")}</h1>
        <p className="mt-2 text-ink-300">{t("fairUse.intro")}</p>
        <ul className="glass mt-6 space-y-4 rounded-3xl p-6 text-sm leading-relaxed text-ink-200">
          <Rule title={t("fairUse.keep.title")}>
            {t("fairUse.keep.body")}
          </Rule>
          <Rule title={t("fairUse.share.title")}>
            {t("fairUse.share.body")}
          </Rule>
          <Rule title={t("fairUse.venue.title")}>
            {t("fairUse.venue.body")}
          </Rule>
          <Rule title={t("fairUse.modify.title")} />
        </ul>
        <p className="mt-4 px-1 text-sm text-ink-400">
          {t.rich("fairUse.contact", { email: c => <span className="select-text text-ink-200">{c}</span> })}
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
