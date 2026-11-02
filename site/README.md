# OnKey website

The landing page and docs for OnKey: Next.js (App Router) with Tailwind, built as a fully static site.

```sh
cd site
npm install
npm run dev       # http://localhost:3080
npm run build     # static files in site/out/
npm run serve     # serve site/out/ on http://localhost:3080
```

## Deploying

The site is on Cloudflare Pages: project root `site`, build command `npm run build`, output directory `out`.
Pages also deploys `functions/_middleware.ts`, a Pages Function that sends visitors of `/` to their browser's
language (see Languages); `public/_routes.json` limits it to `/`, so everything else is plain static files.
Any static host serves the export the same way, just without that redirect (visitors pick a language in the
footer). For a project site served under `/<repo>/` (GitHub Pages without a custom domain), build with
`BASE_PATH=/<repo> npm run build`.

```sh
npm test          # catalogs complete, the language choice for /, the Pages Function
```

## Languages

English is the source: `messages/en.json` holds every word of the translated pages, and English stays at the
site's root with its URLs unchanged (`/`, `/docs/faq/`, `/changelog/`: the app and release notes link to them).
Each other language is one more file, `messages/<code>.json`, with the same keys, and gets its pages at
`/<code>/`: the home page, every docs page and the changelog (`TRANSLATED_PAGES` in `lib/languages.ts`). A key a
catalog doesn't have yet is shown in English on that language's page, so new English text can land before its
translations. The changelog's words are a catalog of their own, `messages/changelog/<code>.json` (see Changelog
below), so a release adds keys there and the pages' catalog is left alone.

- Messages are ICU (use-intl, the same library as the app): `{n}` is a value, `<link>…</link>` a tag. Keep both
  exactly as in English, and translate the words around and inside them. An empty tag (`<wave></wave>` in
  `docs.howItWorks`) marks where a small picture goes; a file tag (`<dsp>dsp.rs</dsp>`) links to that source file
  and always shows the file's own name.
- The diagrams' words are messages too (`docs.diagrams`): a label with `\n` is drawn as that many lines (SVG text
  doesn't wrap), and `docs.diagrams.phrase` is the made-up lyric the diagrams sing: seven syllables separated by
  spaces, one per note (a translation with a different count falls back to English).
- `/docs/privacy/` in another language opens with `docs.privacy.translationNote`: the English page applies where
  they differ. Not shown on the English page.
- `/` follows the browser's language (`Accept-Language`) to a language the site has, unless the footer's
  language switcher has set the `lang` cookie. Never by location; search engines and other paths aren't
  redirected (`lib/choose-language.ts`, `functions/_middleware.ts`).
- Translated pages carry `hreflang` links to each other, with English as `x-default`, and `<html lang>`.
- Levels (`LANGUAGES` in `lib/languages.ts`): an "early" language's pages say, where they talk about lyrics, that
  songs in it show their lyrics a line at a time for now (`common.early`).
- `SITE_PSEUDO=1 npm run build` adds `/en-XA/`, English with every message accented and bracketed (never
  shipped): any plain English left on those pages isn't in the catalog yet.
- `node scripts/english-same.mjs <old out/> <new out/>` checks the English pages are unchanged between two builds
  (`--allow-additions` when a page has just been translated: it gains hreflang links).

To add a language (say Spanish, `es`):

1. Copy `messages/en.json` to `messages/es.json` and translate the values (not the keys). The code must be in
   `LANGUAGES` in `lib/languages.ts`; add it there with its own name if it isn't.
2. `npm test`: it fails on an extra key or a lost `{value}` or `<tag>`, and counts the keys still missing (shown
   in English until translated; `node i18n/translate.mjs site --check` from the repo root lists them).
3. `npm run build`: it writes `lib/shipped-languages.json` (the Pages Function's list) and builds `out/es/…`.
   Look at `npm run serve` → http://localhost:3080/es/.
4. Commit `messages/es.json` and `lib/shipped-languages.json` together.

## Changelog

`/changelog/` (What's new) is built from two files: `lib/changelog.ts` holds what isn't words (each release's
version and date, each entry's id, date, version and kind), and `messages/changelog/en.json` holds the words.
The other languages are `messages/changelog/<code>.json`, written by `i18n/translate.mjs`; an entry a language
doesn't have yet reads in English. Dates are formatted by Intl in the page's language.

For a release X.Y.Z:

1. `lib/changelog.ts`: add `{ version: "X.Y.Z", date: "YYYY-MM-DD" }` at the top of `RELEASES`, and one `ENTRIES`
   row per feature or fix, newest first: `{ id, date, version: "X.Y.Z", kind: "new" | "better" | "fix" }`. The
   `id` is the English title in lower case with dashes (`slug("Pause listening")` = `pause-listening`); it's the
   entry's anchor and its key in every language, so it never changes once published.
2. `messages/changelog/en.json`: `releases.<X-Y-Z>.note` (one line under the release; `.link` for a linked page's
   label, with the link itself in `RELEASES`), and for each entry `entries.<id>.title`, `.summary` and `.p1`,
   `.p2`… for its points, in order. Written for people who sing.
3. `npm test` checks the two agree (every entry has its words, every word has its entry, points are numbered in
   order) and that every message parses. Read `/changelog/` on the dev server.
4. Translate, from the repo root: `node i18n/translate.mjs changelog --dry-run`, then with `ANTHROPIC_API_KEY`
   set, `node i18n/translate.mjs changelog --write`. Commit `en.json`, the `<code>.json` files and
   `messages/changelog/.sources.json` together. `node i18n/translate.mjs changelog --check` says what's missing.

## Settings

Everything that changes between releases is in the `LAUNCH` block at the top of `lib/site.ts`.
`npm run build` lists any that are empty.

| Key | What | When empty |
|---|---|---|
| `githubUrl` | the GitHub repository | (required) |
| `downloadUrl` | the notarised `OnKey.dmg` | the repo's latest-release page |
| `windowsIssueUrl` | the "Want Windows?" issue | the repo's issue list |
| `contactEmail` | the contact address | "(address to come)" |
| `siteUrl` | the site's own address | no link previews |
| `plausibleDomain` | Plausible's site name: cookieless page counts | no Plausible script |
| `flowMynaTrackerId` | Flow Myna website tracker (`fm_…`): cookieless visit paths, only on the live domain | no Flow Myna script |
| `discordUrl` | the Discord invite | no Discord links |
| `demoVideo` (+ `demoPoster`) | the demo video's address and its still | the lane illustration |
| `notarised` | whether the download is notarised by Apple | install steps explain "Open Anyway" |

Keep the numbers on `docs/scoring` in step with the latest measurements.
