// The checks on the catalogs (i18n.test.mjs, scripts/english-same.mjs; never in the window): a message read with
// the same ICU parser use-intl formats with, the values and tags it takes, and the missing-key check.
import { parse as icuParse, TYPE, type MessageFormatElement } from "@formatjs/icu-messageformat-parser";
import { flatten, type Catalog } from "./i18n";

/** A message's parts. Throws on a message that isn't well formed. */
export function parse(msg: string): MessageFormatElement[] {
  return icuParse(msg, { requiresOtherClause: true });
}

/** The text runs of a message, as they're shown (between values, tags and plural/select syntax; ICU's quoting
 *  undone). */
export function runs(msg: string): string[] {
  const out: string[] = [];
  const walk = (els: MessageFormatElement[]) => {
    for (const e of els) {
      if (e.type === TYPE.literal) out.push(e.value);
      else if (e.type === TYPE.tag) walk(e.children);
      else if (e.type === TYPE.plural || e.type === TYPE.select) Object.values(e.options).forEach(o => walk(o.value));
    }
  };
  walk(parse(msg));
  return out;
}

/** The parameter names a message uses: {name}, plural and select values, and <tag>s (as "<tag>"). Sorted. */
export function paramNames(msg: string): string[] {
  const names = new Set<string>();
  const walk = (els: MessageFormatElement[]) => {
    for (const e of els) {
      if (e.type === TYPE.literal || e.type === TYPE.pound) continue;
      if (e.type === TYPE.tag) { names.add(`<${e.value}>`); walk(e.children); continue; }
      names.add(e.value);
      if (e.type === TYPE.plural || e.type === TYPE.select) Object.values(e.options).forEach(o => walk(o.value));
    }
  };
  walk(parse(msg));
  return [...names].sort();
}

/** The missing-key check (i18n.test.mjs, npm test): what's wrong with a translation against English, one line per
 *  problem (none: it's whole). It must have every key English has, no key English no longer has, and the same
 *  values, plurals and tags in each message, so nothing is left in English by mistake and nothing is put in that the
 *  window doesn't give. */
export function catalogProblems(english: Catalog, other: Catalog, code = "translation"): string[] {
  const want = flatten(english);
  const have = flatten(other);
  const out: string[] = [];
  const names = (key: string, msg: string, who: string) => {
    try { return paramNames(msg).join(", "); } catch (e) { out.push(`${who}: "${key}" isn't well formed: ${(e as Error).message}`); return null; }
  };
  for (const [k, msg] of want) {
    const theirs = have.get(k);
    if (theirs == null) { out.push(`${code}: missing "${k}"`); continue; }
    const a = names(k, msg, "en");
    const b = names(k, theirs, code);
    if (a != null && b != null && a !== b) out.push(`${code}: "${k}" has {${b}} where English has {${a}}`);
  }
  for (const k of have.keys()) if (!want.has(k)) out.push(`${code}: "${k}" isn't in English any more`);
  return out;
}
