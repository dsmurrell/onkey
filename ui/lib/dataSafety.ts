// Deleting and reading your takes and songs, safely: a failed delete says so (and the button works again), and a
// failed read is never shown as "nothing here". Pure (no React, no engine), so lib/dataSafety.test.mjs can check it
// with `npm test`.
import { t } from "./i18n";

/** What to show for an error from the engine (its own words when it gave some). */
export function errorText(e: unknown, fallback: string): string {
  if (typeof e === "string" && e.trim()) return e;
  if (e instanceof Error && e.message.trim()) return e.message;
  return fallback;
}

/** Run a confirmed delete: null when done, else what went wrong (to show by the button). Never throws, so the
 *  button that started it always becomes usable again. */
export async function runDelete(onDelete: () => Promise<void> | void): Promise<string | null> {
  try {
    await onDelete();
    return null;
  } catch (e) {
    return errorText(e, t("errors.deleteFailed"));
  }
}

/** Delete each of `ids` in turn, stopping at the first that fails. `gone` gets every id actually deleted (so the
 *  list drops those and only those), then the failure is thrown. */
export async function deleteEach(ids: string[], del: (id: string) => Promise<void>, gone: (ids: string[]) => void) {
  const done: string[] = [];
  try {
    for (const id of ids) { await del(id); done.push(id); }
  } finally {
    if (done.length) gone(done);
  }
}

/** A list read from the engine: the items, or that it couldn't be read and why (never an empty list in its place). */
export type Read<T> = { items: T[] } | { error: string; detail: string };

export async function readList<T>(get: () => Promise<T[]>, what: "takes" | "songs"): Promise<Read<T>> {
  const error = t("errors.readFailed", { what });
  try {
    const items = await get();
    return Array.isArray(items) ? { items } : { error, detail: "" };
  } catch (e) {
    return { error, detail: errorText(e, "") };
  }
}
