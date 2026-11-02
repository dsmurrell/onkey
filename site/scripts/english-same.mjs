// The English pages read exactly as before: compares two static exports (copies of site/out/) page by page.
//
//   node scripts/english-same.mjs <before-out> <after-out> [--allow-additions]
//
// Every .html page in <before-out> must exist in <after-out> with the same markup once build noise is taken out:
// - the build id comment and the hashed chunk file names;
// - the inline <script>s (React's copy of the page for hydration, which names files and component ids);
// - React's useId values (made from the component tree's shape, which the per-language layouts change; they only
//   pair a <label> with its input).
// React's comment markers are counted separately: a sentence that was split around a value in JSX
// ("Step <!-- -->1") is one string from a message ("Step 1"), which reads the same, and the 404 page loses an
// empty Suspense boundary (<!--$--><!--/$-->) now that it has its own layout.
// Pages only in <after-out> (other languages) are listed, not compared.
// --allow-additions: also accept the language markup that appears once a second language exists, or once a page
// is translated (hreflang links and the footer's language switcher): it's taken out of both builds before they're
// compared, and what the new build gained is listed. Exits 1 on any other difference.
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative } from "node:path";

const [before, after] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const allowAdditions = process.argv.includes("--allow-additions");
if (!before || !after) { console.error("usage: node scripts/english-same.mjs <before-out> <after-out> [--allow-additions]"); process.exit(2); }

const pages = (dir) => {
  const out = [];
  const walk = (d) => { for (const f of readdirSync(d)) { const p = join(d, f); if (statSync(p).isDirectory()) { if (f !== "_next") walk(p); } else if (f.endsWith(".html")) out.push(relative(dir, p)); } };
  walk(dir);
  return out.sort();
};

const ADDITIONS = [
  [/<link rel="alternate" hrefLang="[^"]+" href="[^"]+"\/>/g, "hreflang link"],
  [/<nav[^>]*aria-label="Language"[\s\S]*?<\/nav>/g, "language switcher"],
];

/** React's markers in server-rendered HTML: `<!-- -->` between two runs of text, `<!--$-->…<!--/$-->` around a
 *  Suspense boundary. Not shown, and not part of the page's text. */
const MARKERS = /<!--( |\$|\/\$)-->/g;

/** The page without build noise. */
function normalise(html, additions) {
  let s = html
    .replace(/^<!DOCTYPE html><!--[^>]*?-->/, "<!DOCTYPE html>")
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/g, "")
    .replace(/<link rel="preload" as="script"[^>]*\/>/g, "")
    .replace(/(\/_next\/static\/[^"]*?)[-.][0-9a-f]{8,}(\.\w+")/g, "$1$2")
    .replace(/_R_[0-9a-z]+_/g, "_R_id_");
  if (additions) for (const [re, name] of ADDITIONS) s = s.replace(re, () => { additions.push(name); return ""; });
  return s;
}

let failed = 0, identical = 0, markersOnly = 0;
const added = {};
for (const page of pages(before)) {
  const p = join(after, page);
  if (!existsSync(p)) { console.log(`MISSING  ${page}`); failed++; continue; }
  const had = [], found = [];
  const a = normalise(readFileSync(join(before, page), "utf8"), allowAdditions ? had : undefined);
  const b = normalise(readFileSync(p, "utf8"), allowAdditions ? found : undefined);
  for (const name of had) { const i = found.indexOf(name); if (i >= 0) found.splice(i, 1); }
  if (found.length) added[page] = found;
  if (a === b) { identical++; continue; }
  const strip = (s) => s.replace(MARKERS, "");
  const count = (s) => (s.match(MARKERS) ?? []).length;
  if (strip(a) === strip(b)) { markersOnly++; console.log(`same apart from React's comment markers (${count(a)} before, ${count(b)} after)  ${page}`); continue; }
  failed++;
  let i = 0;
  while (i < a.length && a[i] === b[i]) i++;
  console.log(`DIFFERENT  ${page}\n  before: …${a.slice(Math.max(0, i - 120), i + 200)}…\n  after:  …${b.slice(Math.max(0, i - 120), i + 200)}…`);
}
const extra = pages(after).filter((p) => !existsSync(join(before, p)));
for (const [page, list] of Object.entries(added)) {
  const counts = list.reduce((m, n) => ({ ...m, [n]: (m[n] ?? 0) + 1 }), {});
  console.log(`added  ${page}: ${Object.entries(counts).map(([n, c]) => `${c} ${n}`).join(", ")}`);
}
console.log(`\n${identical} identical, ${markersOnly} identical apart from React's comment markers, ${failed} different; ${extra.length} new pages${extra.length ? ` (${extra.slice(0, 6).join(", ")}${extra.length > 6 ? ", …" : ""})` : ""}`);
process.exit(failed ? 1 : 0);
