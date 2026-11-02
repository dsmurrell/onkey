import type { TakeSummary } from "@/lib/session";
import { useT } from "@/lib/i18n";

/** A tag for the takes that aren't like the rest: the first play (sung while the song was learned), or sung blind.
 *  Nothing for the usual take. */
export default function TakeTags({ t: tk }: { t: Pick<TakeSummary, "mode" | "blind"> }) {
  const t = useT();
  const tag = tk.mode === "learn" ? t("takeTags.firstPlay") : tk.blind ? t("takeTags.blind") : null;
  return tag ? <span className="rounded-full bg-white/6 px-1.5 py-px text-[10px] font-medium text-ink-300">{tag}</span> : null;
}

/** A table row's bin: shown on hover, or while anything in the row has keyboard focus (so Tab still reaches it),
 *  and while it asks "Delete?" (two buttons). The row needs the `group` class. */
export const BIN = "opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100 has-[button~button]:opacity-100";
