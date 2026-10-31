"use client";
// "Report a problem": OnKey's records (logs, each play's summary, versions; never lyrics or audio) put together as a
// zip you save and attach to a GitHub issue. Shows exactly what's in it before saving, with a box to say what went
// wrong. Opened from Settings, the Help menu, and a take's page ("Report a problem with this take"). Nothing is
// uploaded: the issue page opens in your browser and you attach the zip yourself. On Windows it's saved straight to
// the Desktop and shown selected in File Explorer (one click for a tester), with a summary.txt of the logs inside.
import { useEffect, useState } from "react";
import { fileName, savedFolder, usePc, useWindows } from "@/lib/platform";
import { openReportFolder, openReportIssue, prepareReport, saveReport, showInFinder, type ReportContents } from "@/lib/session";

/** Open the report dialog (about a take, when given). */
export function reportProblem(take?: string) {
  window.dispatchEvent(new CustomEvent("onkey-report", { detail: { take: take ?? null } }));
}

const size = (b: number) => b < 1024 ? `${b} B` : b < 1 << 20 ? `${(b / 1024).toFixed(0)} KB` : `${(b / (1 << 20)).toFixed(1)} MB`;

export default function ReportProblem() {
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
           className="glass flex max-h-[85vh] w-full max-w-xl flex-col rounded-3xl p-6 shadow-2xl animate-rise">
        <h2 id="report-title" className="text-lg font-semibold tracking-tight">
          {open.take ? "Report a problem with this take" : "Report a problem"}
        </h2>
        {saved ? (
          <>
            {win ? (
              <p className="mt-2 text-sm text-ink-300">
                Saved to {savedFolder(saved)} as <span className="font-mono text-xs text-ink-100">{fileName(saved)}</span>, and shown
                in {pc.files}. Attach that one file to your message (or to a GitHub issue). Nothing has been sent anywhere.
              </p>
            ) : (
              <p className="mt-2 text-sm text-ink-300">
                Saved to <span className="font-mono text-xs text-ink-100">{saved}</span>. To send it, open an issue on GitHub
                and attach the zip (drag it into the box). Nothing has been sent anywhere.
              </p>
            )}
            <div className="mt-5 flex flex-wrap justify-end gap-2">
              <button onClick={() => showInFinder(saved)} className="rounded-xl bg-white/8 px-4 py-2 text-sm text-ink-100 transition hover:bg-white/12">{win ? `Show in ${pc.files.replace("your ", "the ")}` : "Show in Finder"}</button>
              <button onClick={() => openReportIssue(open.take)} className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400">Open a GitHub issue</button>
              <button onClick={() => setOpen(null)} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">Done</button>
            </div>
          </>
        ) : (
          <>
            {win ? (
              <p className="mt-2 text-sm text-ink-300">
                OnKey puts its records in one zip file on your Desktop for you to attach to a message: its log for the last
                three days, a summary of each play, and a summary of this PC, its sound and the music apps it followed. No
                lyrics, no audio, no song files, and no user name or home folder. Nothing is uploaded.
              </p>
            ) : (
              <p className="mt-2 text-sm text-ink-300">
                OnKey puts its records together in a zip for you to save and attach to a bug report: its log for the last
                three days and a summary of each play (timings, CPU, which build, which Mac). No lyrics, no audio, no song
                files, and no user name or home folder. Nothing is uploaded.
              </p>
            )}
            <label htmlFor="report-text" className="mt-4 text-sm font-medium text-ink-100">What went wrong? <span className="font-normal text-ink-400">(optional)</span></label>
            <textarea id="report-text" value={text} onChange={e => setText(e.target.value)} rows={3} maxLength={4000}
                      placeholder="e.g. the lyrics were late in Bad Day, or it froze at Starting OnKey"
                      className="mt-1.5 resize-none rounded-xl border border-white/8 bg-white/4 px-3 py-2 text-sm text-ink-100 outline-none placeholder:text-ink-500 focus:border-accent-400" />
            <div className="mt-4 min-h-0 flex-1 overflow-y-auto rounded-xl bg-white/3 px-3 py-2 text-xs text-ink-300">
              {error ? <p className="text-amber-200">{error}</p> : !contents ? <p className="text-ink-400">Putting it together…</p> : (
                <>
                  <p className="mb-1.5 text-ink-200">In the zip ({contents.files.length} files, {size(contents.bytes)}):</p>
                  <ul className="space-y-1">
                    {other.map(f => <li key={f.name}><span className="font-mono text-ink-100">{f.name}</span> · {size(f.bytes)} · {f.what}</li>)}
                    {logs.length > 0 && <li><span className="font-mono text-ink-100">logs/</span> · {logs.length} day{logs.length === 1 ? "" : "s"} of OnKey’s log ({size(logs.reduce((n, f) => n + f.bytes, 0))})</li>}
                    {takes.map(f => <li key={f.name}><span className="font-mono text-ink-100">{f.name}</span> · {size(f.bytes)} · {f.what}</li>)}
                  </ul>
                </>
              )}
            </div>
            <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
              <button onClick={() => openReportFolder()} disabled={!contents}
                      className="mr-auto rounded-xl px-3 py-2 text-sm text-ink-300 transition hover:text-white disabled:opacity-40">Show the files</button>
              <button onClick={() => setOpen(null)} className="rounded-xl px-4 py-2 text-sm text-ink-300 transition hover:text-white">Cancel</button>
              <button onClick={save} disabled={!contents || busy}
                      className="rounded-xl bg-accent-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-accent-400 disabled:opacity-50">{win ? "Save to Desktop" : "Save…"}</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
