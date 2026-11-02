"use client";
// "Report a problem": OnKey's records (logs, each play's summary, versions; never lyrics or audio) put together as a
// zip you save and attach to a GitHub issue. Shows exactly what's in it before saving, with a box to say what went
// wrong. Opened from Settings, the Help menu, and a take's page ("Report a problem with this take"). Nothing is
// uploaded: the issue page opens in your browser and you attach the zip yourself. On Windows it's saved straight to
// the Desktop and shown selected in File Explorer (one click for a tester), with a summary.txt of the logs inside.
import { useEffect, useState } from "react";
import { fileName, savedFolder, usePc, useWindows } from "@/lib/platform";
import { openReportFolder, openReportIssue, prepareReport, saveReport, showInFinder, type ReportContents } from "@/lib/session";
import { useT, type Translator } from "@/lib/i18n";
import { reportWhat } from "@/lib/engineText";

/** Open the report dialog (about a take, when given). */
export function reportProblem(take?: string) {
  window.dispatchEvent(new CustomEvent("onkey-report", { detail: { take: take ?? null } }));
}

const size = (t: Translator, b: number) => b < 1024 ? t("report.size.b", { n: b }) : b < 1 << 20 ? t("report.size.kb", { n: (b / 1024).toFixed(0) }) : t("report.size.mb", { n: (b / (1 << 20)).toFixed(1) });

export default function ReportProblem() {
  const t = useT();
  const [open, setOpen] = useState<{ take: string | null } | null>(null);
  const [contents, setContents] = useState<ReportContents | null>(null);
  const [error, setError] = useState("");
  const [text, setText] = useState("");
  const [saved, setSaved] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const win = useWindows();
  const pc = usePc();
  useEffect(() => {
    const on = (e: Event) => setOpen({ take: (e as CustomEvent<{ take: string | null }>).detail?.take ?? null });
    window.addEventListener("onkey-report", on);
    // the Help menu's "Report a Problem…" (the app sends it as an event)
    let off: (() => void) | undefined;
    if ("__TAURI_INTERNALS__" in window) {
      import("@tauri-apps/api/event").then(({ listen }) => listen("onkey-menu", e => { if (e.payload === "report") setOpen({ take: null }); }))
        .then(u => { off = u; }).catch(() => {});
    }
    return () => { window.removeEventListener("onkey-report", on); off?.(); };
  }, []);
  useEffect(() => {
    if (!open) return;
    setContents(null); setError(""); setSaved(null); setText("");
    prepareReport(open.take).then(setContents).catch(e => setError(String(e)));
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(null); };
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [open]);
  if (!open) return null;

  async function save() {
    setBusy(true); setError("");
    try {
      const path = await saveReport(text);
      if (path) setSaved(path);
    } catch (e) { setError(String(e)); }
    setBusy(false);
  }

  const takes = contents?.files.filter(f => f.name.startsWith("takes/")) ?? [];
  const logs = contents?.files.filter(f => f.name.startsWith("logs/")) ?? [];
  const other = contents?.files.filter(f => !f.name.startsWith("takes/") && !f.name.startsWith("logs/")) ?? [];
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/65 p-6" onClick={() => setOpen(null)}>
      <div role="dialog" aria-modal="true" aria-labelledby="report-title" onClick={e => e.stopPropagation()}
           className="flex max-h-[85vh] w-full max-w-xl flex-col rounded-3xl border border-white/10 bg-ink-900 p-6 shadow-2xl animate-rise [&>*]:shrink-0">
        <h2 id="report-title" className="text-lg font-semibold tracking-tight">
          {open.take ? t("report.titleTake") : t("report.title")}
        </h2>
        {saved ? (
          <>
            {win ? (
              <p className="mt-2 text-sm text-ink-300">
                {t.rich("report.savedPc", { folder: savedFolder(saved), name: fileName(saved), files: pc.files,
                                            file: c => <span className="font-mono text-xs text-ink-100">{c}</span> })}
              </p>
            ) : (
              <p className="mt-2 text-sm text-ink-300">
                {t.rich("report.savedMac", { path: saved, file: c => <span className="font-mono text-xs text-ink-100">{c}</span> })}
              </p>
            )}
            <div className="mt-5 flex flex-wrap justify-end gap-2">
              <button onClick={() => showInFinder(saved)} className="rounded-xl bg-white/8 px-4 py-2 text-sm text-ink-100 transition hover:bg-white/12">{win ? t("report.showInPc", { files: pc.filesThe }) : t("report.showInFinder")}</button>
              <button onClick={() => openReportIssue(open.take)} className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400">{t("report.openIssue")}</button>
              <button onClick={() => setOpen(null)} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">{t("report.done")}</button>
            </div>
          </>
        ) : (
          <>
            {win ? (
              <p className="mt-2 text-sm text-ink-300">
                {t("report.introPc")}
              </p>
            ) : (
              <p className="mt-2 text-sm text-ink-300">
                {t("report.introMac")}
              </p>
            )}
            <label htmlFor="report-text" className="mt-4 text-sm font-medium text-ink-100">{t.rich("report.askLabel", { muted: c => <span className="font-normal text-ink-400">{c}</span> })}</label>
            <textarea id="report-text" value={text} onChange={e => setText(e.target.value)} rows={3} maxLength={4000}
                      placeholder={t("report.placeholder")}
                      className="mt-1.5 resize-none rounded-xl border border-white/8 bg-white/4 px-3 py-2 text-sm text-ink-100 outline-none placeholder:text-ink-500 focus:border-accent-400" />
            <div className="mt-4 min-h-0 flex-1 !shrink overflow-y-auto rounded-xl bg-white/3 px-3 py-2 text-xs text-ink-300">
              {error ? <p className="text-notice">{error}</p> : !contents ? <p className="text-ink-400">{t("report.preparing")}</p> : (
                <>
                  <p className="mb-1.5 text-ink-200">{t("report.inZip", { count: contents.files.length, size: size(t, contents.bytes) })}</p>
                  <ul className="space-y-1">
                    {other.map(f => <li key={f.name}><span className="font-mono text-ink-100">{f.name}</span> · {size(t, f.bytes)} · {reportWhat(t, f, win ? "pc" : "mac")}</li>)}
                    {logs.length > 0 && <li><span className="font-mono text-ink-100">logs/</span> · {t("report.logs", { count: logs.length, size: size(t, logs.reduce((n, f) => n + f.bytes, 0)) })}</li>}
                    {takes.map(f => <li key={f.name}><span className="font-mono text-ink-100">{f.name}</span> · {size(t, f.bytes)} · {reportWhat(t, f, win ? "pc" : "mac")}</li>)}
                  </ul>
                </>
              )}
            </div>
            <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
              <button onClick={() => openReportFolder()} disabled={!contents}
                      className="mr-auto rounded-xl px-3 py-2 text-sm text-ink-300 transition hover:text-white disabled:opacity-40">{t("report.showFiles")}</button>
              <button onClick={() => setOpen(null)} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">{t("report.cancel")}</button>
              <button onClick={save} disabled={!contents || busy}
                      className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400 disabled:opacity-50">{win ? t("report.savePc") : t("report.saveMac")}</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
