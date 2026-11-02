// Before `npm run build` (npm's prebuild): writes lib/shipped-languages.json, the languages other than English
// that have a catalog in messages/, for the Pages Function on / (functions/_middleware.ts), which can't read
// the folder. Never the pseudo-language. The file is committed (so tsc works before a build); a test checks
// it's current.
import { writeFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { catalogLocales } from "../lib/catalogs.mjs";

const file = join(process.cwd(), "lib", "shipped-languages.json");
const text = `${JSON.stringify(catalogLocales(false))}\n`;
let was = "";
try { was = readFileSync(file, "utf8"); } catch {}
if (was !== text) { writeFileSync(file, text); console.log(`[site] languages: ${text.trim()}`); }
