"use client";
// Your takes or songs couldn't be read: said plainly, with a retry, and never shown as "nothing here" (they're
// most likely still there; nothing is cleaned up or written while this shows).
import { useT } from "@/lib/i18n";

export default function ReadError({ error, detail, onRetry }: { error: string; detail?: string; onRetry: () => void }) {
  const t = useT();
  return (
    <div role="alert" className="glass grid place-items-center rounded-3xl px-6 py-20 text-center">
      <div>
        <h2 className="text-xl font-semibold">{error}</h2>
        <p className="mx-auto mt-1 max-w-md text-ink-300">{t("readError.body")}</p>
        {detail && <p className="mx-auto mt-2 max-w-md text-xs text-ink-500">{detail}</p>}
        <button onClick={onRetry} className="mt-5 rounded-xl bg-accent-500 px-4 py-2 font-medium text-white transition hover:bg-accent-400">{t("readError.tryAgain")}</button>
      </div>
    </div>
  );
}
