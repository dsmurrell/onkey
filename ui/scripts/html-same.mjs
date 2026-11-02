// The second check that English is unchanged: the built pages' text, before and after.
//   npm run build, then: node scripts/html-same.mjs <before-out-dir> [after-out-dir: out]
// (<before-out-dir>: `out/` from a build of the commit before the move.) For every page in the static export, the
// text it shows (text between tags, and title/aria-label/placeholder/alt attributes, outside scripts and styles)
// must be the same, in the same order. It sees only what the page shows as it's built (before any session data), so
// it complements english-same.mjs, which checks every string in the source.
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const [before, after = "out"] = process.argv.slice(2);
if (!before) { console.error("usage: node scripts/html-same.mjs <before-out-dir> [after-out-dir]"); process.exit(2); }
const ENT = { amp: "&", lt: "<", gt: ">", quot: "\"", apos: "'", nbsp: " ", "#39": "'" };
const decode = s => s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e) =>
  e[0] === "#" ? String.fromCodePoint(e[1].toLowerCase() === "x" ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10)) : ENT[e] ?? m);
function texts(html) {
  const body = html.replace(/<script[\s\S]*?<\/script>/gi, "").replace(/<style[\s\S]*?<\/style>/gi, "").replace(/<!--[\s\S]*?-->/g, "");
  const out = [];
  for (const m of body.matchAll(/<[^>]+>|[^<]+/g)) {
    const s = m[0];
    if (s.startsWith("<")) {
      for (const a of s.matchAll(/\s(title|aria-label|placeholder|alt|content)="([^"]*)"/g)) out.push(`[${a[1]}] ${decode(a[2])}`);
    } else {
      const v = decode(s).replace(/\s+/g, " ").trim();
      if (v) out.push(v);
    }
  }
  return out;
}
const pages = dir => readdirSync(dir).filter(f => f.endsWith(".html")).sort();
let bad = 0, n = 0;
for (const p of pages(before)) {
  if (!existsSync(join(after, p))) { console.log(`${p}: missing after`); bad++; continue; }
  const a = texts(readFileSync(join(before, p), "utf8")), b = texts(readFileSync(join(after, p), "utf8"));
  n += a.length;
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i++) if (a[i] !== b[i]) {
    console.log(`${p}: text ${i} differs:\n  before: ${JSON.stringify(a[i])}\n  after:  ${JSON.stringify(b[i])}`);
    bad++;
    break;
  }
}
console.log(`${pages(before).length} pages, ${n} pieces of text: ${bad ? `${bad} page(s) differ` : "the same"}.`);
process.exit(bad ? 1 : 0);
