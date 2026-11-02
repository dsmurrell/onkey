# Translations

English is the source of truth. One tool, `i18n/translate.mjs`, translates all four message catalogs with
Claude:

| Catalog | English | Used by |
|---|---|---|
| `ui` | `ui/messages/en.json` | the app window (use-intl, ICU MessageFormat, nested keys) |
| `site` | `site/messages/en.json` | singonkey.app |
| `changelog` | `site/messages/changelog/en.json` | singonkey.app/changelog (a release adds entries; see site/README.md) |
| `native` | `src-tauri/messages/en.json` | the menu and macOS/Windows prompts |

Each language is `<code>.json` next to `en.json` (codes and Full/Early levels in `languages.json`).

## Everyday use: you changed some English
```sh
node i18n/translate.mjs ui --dry-run            # what would be sent, and why (no API call)
ANTHROPIC_API_KEY=… node i18n/translate.mjs ui --write
node i18n/translate.mjs ui --check              # every language complete and valid (also for CI)
```
Then commit `en.json`, the language files, `.sources.json` and `needs-review.md` (if any) together.

Only keys that are **new**, or whose **English changed** since the tool translated them, are sent. Everything
else is left alone, so a one-word change costs one short request per language.

Options: `--langs es,pt` or `full` / `early` / `all` (default), `--keys a.b,c.d`, `--batch 40`,
`--concurrency 4`, `--show-prompt` (dry run: the full prompt). Model: `ONKEY_TRANSLATE_MODEL` (default
`claude-fable-5-1`, Fable 5.1). The tool asks for the API's server-side refusal fallback on models that support it; set
`ONKEY_TRANSLATE_FALLBACK=0` to leave it out.

## What gets checked
Every message that comes back is parsed as ICU and compared with its English:
- same `{arguments}`, with the same kind (plain, number, date, plural, select);
- plurals keep their `=0`/`=1` cases and `#`, and use exactly the plural forms the language needs
  (Russian/Polish: one, few, many, other; Japanese/Chinese/Korean/Thai/Indonesian/Vietnamese: other only);
- `select` keeps English's option keys;
- the same rich-text tags (`<b>`, `<link>`…), the same number of times.

A message that fails is sent back once with the problem named. If it fails again it isn't written; it's listed
in `needs-review.md` and the key stays missing, so `--check` fails until it's fixed.

The parser is `@formatjs/icu-messageformat-parser` (what use-intl runs), pinned in `i18n/package.json`; run
`npm install` in `i18n/` once. Without it, a built-in parser covering the same subset is used.

## How contributors' edits are protected
`.sources.json` (next to `en.json`) records, for each language and key:
`en` = hash of the English it was translated from, `out` = hash of what the tool wrote, `human` = a person edited it.

- **A person edits a translation** (a PR on GitHub, a reviewer's fix): on the next run the text no longer matches
  `out`, so the key is marked `human: true`. From then on the tool **never overwrites it**.
- **A translation the tool didn't write** (no lock entry, e.g. an imported translation): treated the same, as a
  person's work.
- **English changes under a person's translation**: the tool doesn't send it. It lists the key in
  `<catalog>/needs-review.md` with the new English and the current translation. A reviewer then either
  - edits the translation if needed and runs `node i18n/translate.mjs ui --langs zh-Hans --mark-reviewed --keys some.key`
    (accepts it for the new English; still protected), or
  - hands it back to the tool: `--retranslate --keys some.key` (it becomes machine-owned again).
- English keys that are deleted are removed from every language and from the lock.

Contributors only edit `<code>.json`. They never need to touch `.sources.json`.

## Translations written outside the tool (an agent, a batch from a translator)
Write the values into `<code>.json`, then record them in the lock, without the API:
```sh
node i18n/translate.mjs ui --langs es,es-ES --adopt --as machine   # as if the tool wrote them
node i18n/translate.mjs ui --langs ja --adopt --as human            # a person's: protected from then on
```
- `--as machine`: the tool owns them, so when their English changes the next `--write` translates them again
  (with the old text as `previous`). It records keys with no lock entry and the tool's own keys, never a person's:
  a key marked `human`, or a tool translation edited since (someone's fix) stays a person's unless `--keys` names
  it. Run it right after writing the values, before merging anyone else's edits to that file.
- `--as human`: like a contributor's edit (`--mark-reviewed` for every key).
- Each value gets the same ICU checks as the tool's own; one that fails isn't recorded and the run exits 1.

## Where reviewers look
- `i18n/style/<code>.md`: register (tú/du/tu, です・ます, 해요체…), punctuation, plural forms and the agreed
  term for each OnKey word. Reviewers improve these; the next run uses them.
- `i18n/glossary.md`: what each OnKey term means, and what's kept in English.
- `<catalog>/needs-review.md`: what needs a person right now.
- `git log -p ui/messages/<code>.json`: what changed.

## Tests
```sh
cd i18n && npm install && npm test     # or: node --test 'i18n/test/*.test.mjs'
```
Offline: the API is a fake `fetch`. The ICU checks run with both the formatjs parser and the built-in one.
