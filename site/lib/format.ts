// Messages formatted in the browser: for text a client component makes from a value it only knows there (a star
// count, a slider's position, which step is in view). The page passes the message as written in its language
// (raw ICU, `t.raw`), and this formats it with use-intl's core, the same formatter the pages are built with, so
// plurals and selects come out right in every language (Polish and Russian get their few/many forms).
import { createTranslator } from "use-intl/core";

/** `message` (raw ICU) with `values`. A message that doesn't parse throws, so a test can catch it. */
export function formatMessage(locale: string, message: string, values: Record<string, string | number>): string {
  return createTranslator({ locale, messages: { m: message }, onError: (e) => { throw e; } })("m", values);
}

/** The GitHub star count's label ("1 star on GitHub", "12 stars on GitHub"): `message` is the page's `common.stars`
 *  as written (an ICU plural on {n}), formatted once the count is known (components/GitHubStars.tsx). */
export function starsLabel(locale: string, message: string, n: number): string {
  return formatMessage(locale, message, { n });
}
