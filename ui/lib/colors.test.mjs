// npm test: lib/colors.ts (hex for canvas and SVG) carries exactly the colour tokens of app/globals.css's @theme
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { TOKENS } from "./colors.ts";

test("lib/colors.ts mirrors every --color-* token in globals.css, with the same value", () => {
  const css = readFileSync(new URL("../app/globals.css", import.meta.url), "utf8");
  const theme = css.slice(css.indexOf("@theme {"), css.indexOf("\n}\n", css.indexOf("@theme {")));
  const fromCss = Object.fromEntries([...theme.matchAll(/--color-([a-z0-9-]+):\s*([^;]+);/g)].map(m => [m[1], m[2].trim().toLowerCase()]));
  assert.ok(Object.keys(fromCss).length >= 20, "found the theme's colour tokens");
  assert.deepEqual(TOKENS, fromCss);
});
