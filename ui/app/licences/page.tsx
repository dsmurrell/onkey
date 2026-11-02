"use client";
// Open-source licences: THIRD_PARTY_NOTICES.md as shipped inside the app (the models, data, fonts and libraries
// OnKey uses, with the copyright notices and licence texts their licences ask to go with every copy).
import Link from "next/link";
import { useEffect, useState } from "react";
import { Logo, Nav, WhatsNew } from "@/components/Nav";
import { thirdPartyNotices } from "@/lib/session";
import { useT } from "@/lib/i18n";

export default function LicencesPage() {
  const t = useT();
  const [text, setText] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { thirdPartyNotices().then(setText).catch(e => setError(String(e))); }, []);

  return (
    <div className="min-h-dvh bg-[radial-gradient(1200px_500px_at_50%_-10%,rgba(124,108,246,.10),transparent)]">
      <header data-tauri-drag-region className="app-header flex items-center gap-5 px-6 pt-4 pb-3"><Logo /><Nav /><span className="ml-auto"><WhatsNew /></span></header>
      <main className="mx-auto max-w-3xl px-6 pb-16">
        <Link href="/settings" className="mt-6 inline-block text-xs text-ink-400 hover:text-white">{t("licences.back")}</Link>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight">{t("licences.title")}</h1>
        <p className="mt-2 text-sm text-ink-400">{t("licences.intro")}</p>
        {error ? <p className="glass mt-6 rounded-3xl p-5 text-sm text-notice">{t("licences.error", { error })}</p>
          : text === undefined ? <div className="glass mt-6 h-40 rounded-3xl shimmer" />
          : text === null ? <p className="glass mt-6 rounded-3xl p-5 text-sm text-ink-300">{t("licences.missing")}</p>
          : <pre className="glass mt-6 overflow-x-auto whitespace-pre-wrap break-words rounded-3xl p-5 font-mono text-[11px] leading-relaxed text-ink-200 select-text">{text}</pre>}
      </main>
    </div>
  );
}
