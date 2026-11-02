// Cloudflare Pages Function: a visitor to / whose browser prefers a language the site has goes to /<lang>/, unless
// the language switcher's cookie says otherwise. Only /: public/_routes.json limits Functions to it, so every
// other page and file is served straight from the static export, and deep links are never redirected.
// Search engines (no Accept-Language, or a crawler's user agent) get English. lib/choose-language.ts decides.
// https://developers.cloudflare.com/pages/functions/middleware/
import { LANG_COOKIE, chooseLanguage } from "../lib/choose-language";
import shipped from "../lib/shipped-languages.json";

type Context = { request: Request; next: () => Promise<Response> };

/** Which answer depends on these, so no cache in between hands one visitor's language to another. */
const VARY = "Accept-Language, Cookie";

export async function onRequest({ request, next }: Context): Promise<Response> {
  const url = new URL(request.url);
  const to = chooseLanguage(
    {
      path: url.pathname,
      acceptLanguage: request.headers.get("Accept-Language"),
      cookie: request.headers.get("Cookie"),
      userAgent: request.headers.get("User-Agent"),
    },
    shipped,
    LANG_COOKIE,
  );
  if (to) {
    return new Response(null, { status: 302, headers: { Location: `/${to}/${url.search}`, Vary: VARY, "Cache-Control": "private, no-store" } });
  }
  const res = await next();
  if (url.pathname !== "/") return res;
  const out = new Response(res.body, res);
  out.headers.append("Vary", VARY);
  return out;
}
