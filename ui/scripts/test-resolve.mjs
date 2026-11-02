// npm test: lets node's test runner load the window's TypeScript as the app's build does: "./i18n" (no extension)
// and "@/lib/i18n" (tsconfig's paths) find i18n.ts. Only for tests; the build uses Next's own resolver.
import { register } from "node:module";

register("data:text/javascript," + encodeURIComponent(`
  import { existsSync } from "node:fs";
  import { fileURLToPath, pathToFileURL } from "node:url";
  const root = ${JSON.stringify(new URL("..", import.meta.url).href)};
  export async function resolve(spec, ctx, next) {
    let s = spec.startsWith("@/") ? new URL(spec.slice(2), root).href : spec;
    if ((s.startsWith(".") || s.startsWith("file:")) && ctx.parentURL?.startsWith("file:") && !/\\.[cm]?[jt]sx?$|\\.json$/.test(s)) {
      const url = new URL(s, ctx.parentURL);
      for (const ext of [".ts", ".tsx", "/index.ts"]) {
        if (existsSync(fileURLToPath(url.href + ext))) return next(url.href + ext, ctx);
      }
    }
    return next(s, ctx);
  }
`));
