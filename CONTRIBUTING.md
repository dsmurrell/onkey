# Contributing to OnKey

Thanks for wanting to help. Bug reports, songs where the notes or lyrics go wrong, translations and small focused
pull requests are all welcome. Pull requests work the normal GitHub way: fork, change, open a pull request, and when
it's merged your commit stays in this repo's history under your name.

## Reporting a problem

Use **Report a problem** in the app (Settings → About, the Help menu, or a take's page). It shows what goes in a zip
of numbers and logs (no audio, no lyrics, no recordings of you), saves it on your Mac, and you can attach it to a
[bug report](../../issues/new/choose). Say which music app and output device you used: most timing problems depend
on them.

## Translations

OnKey's words live in one JSON file per language, in three places:

| File | What it translates |
|---|---|
| `ui/messages/<lang>.json` | the app: Settings, the song pages, buttons, notes |
| `site/messages/<lang>.json` | the website: home, download, FAQ, troubleshooting |
| `src-tauri/messages/<lang>.json` | the Mac's menus and system dialogs |

`<lang>` is the language's code: `es` (Latin America), `es-ES` (Spain), `pt` (Brazil), `it`, `fr`, `de`, `ja`,
`zh-Hans`, `zh-Hant`, `ko`, `hi`, `ru`, `tr`, `pl`, `nl`, `id`, `vi`, `th` (the list: `i18n/languages.json`). `en.json` is the original: every other file has the same keys, with the values
translated. The first versions were machine-translated, so fixes from people who speak the language are very welcome.

**To fix or add a translation**, one pull request is enough:

1. Edit the values in `<lang>.json` (you can do it in GitHub's web editor: open the file, press the pencil).
   Change only the text on the right of each `:`, never the keys on the left.
2. Missing a line? Copy its key from `en.json` and translate the value. A key a language doesn't have shows in English.
3. To add a whole new language, copy `en.json` to `<lang>.json` and translate it. Right-to-left languages can't be
   added yet (the singing lanes run left to right).
4. Add your GitHub username to that language in `ui/messages/CREDITS.json` (see [Credit](#credit)).
5. If the English itself is wrong or unclear, say so in a separate pull request or an issue.

The values use the ICU message format. What to keep exactly as it is:

- **Placeholders** in braces, like `{song}` or `{count}`: OnKey fills them in. Move them to wherever your grammar
  needs them, but don't translate the name inside.
- **Tags** like `<b>…</b>` or `<link>…</link>`: keep the tags, translate the words between them.
- **Plurals**: `{count, plural, one {# song} other {# songs}}`. Keep `{count, plural,`, the `#` (the number) and the
  category words, translate the text inside each `{…}`. Use the categories your language needs: `zero`, `one`,
  `two`, `few`, `many`, `other` (Polish and Russian use `few` and `many`; Japanese, Chinese and Korean need only
  `other`). `other` is always required.
- **Choices**: `{kind, select, mac {…} windows {…} other {…}}`: translate only inside the inner braces.
- Apostrophes: use the typographic `’`. In ICU a plain `'` next to `{`, `}` or `#` starts quoted text and can hide
  a placeholder.

The checks below catch broken message syntax, and placeholders or tags that don't match the English.

## Credit

Everyone whose pull request is merged is credited:

- in GitHub's history (your commits, under your name) and on the pull request;
- translators by GitHub username in the app (Settings, under the language picker), on the website's credits page
  and in the changelog of the release that ships the change.

Translators' names come from `ui/messages/CREDITS.json`: language code to a list of GitHub usernames, in order of
first contribution.

```json
{
  "es": ["octocat"],
  "pt": ["octocat", "hubot"]
}
```

Add yourself in the same pull request as your translation. Only codes that have a `<lang>.json` and valid GitHub
usernames are allowed; the checks test that.

## Running the checks

Every pull request runs the same checks on GitHub (the **Checks** workflow): no secrets, a few minutes. A first-time
contributor's run may wait for a maintainer to approve it. To run them yourself first, with Node 22:

```sh
cd ui && npm ci && npm test && npx tsc --noEmit      # the app's tests (translations included) and type check
cd ../site && npm ci && npm run build                # the website builds (and type-checks)
```

## Building from source

You need an Apple Silicon Mac with macOS 14.2 or later, the Xcode command line tools, Rust and Node.

```sh
npm install && npm --prefix ui install
scripts/fetch-models.sh        # the large models, checked by SHA-256 (Python setup below)
npx tauri build
```

`scripts/fetch-models.sh` exports two of the models from their published weights, so it needs Python 3 with a few
packages, at the versions that reproduce the files exactly. Set them up once, then fetch:

```sh
python3 -m venv .venv && .venv/bin/pip install -r scripts/requirements-models.txt
PYTHON=.venv/bin/python scripts/fetch-models.sh
```
 A build signed ad hoc runs on your own Mac; naming songs through Shazam's
catalogue needs a Developer ID signature with a provisioning profile for an App ID that has the ShazamKit service.

## Tests

```sh
cd src-tauri
cargo test                     # unit tests: scoring, placement, lyrics, storage, the report's redaction, … (--release needs ~4 GB more disk)
cd ../ui && npm run build      # the interface builds and type-checks
```

A **sync suite** replays scripted sessions (starts, skips, pauses, autoplay, Bluetooth delay) through the real
listener and checks OnKey knows where it is in the song. It and a few other harnesses (track replay, self-scoring)
need recordings of real music, which can't be shared, so they aren't in the repo. If your change touches
placement, timing or scoring, say so in the pull request and I'll run them.

## Checking a release

Each release lists the SHA-256 of its files in `SHA256SUMS`, attached to the release. To check a downloaded disk
image matches:

```sh
shasum -a 256 ~/Downloads/OnKey.dmg
```

If you'd rather not trust my build, build your own from the release's tagged commit. It's the same code, but it
won't match mine byte for byte. Mine carries my code signature and Apple's notarisation, and builds aren't
reproducible across machines yet.

## Pull requests

- One change per pull request, with a sentence on what it fixes and how you checked it. Translations can be one
  language per pull request.
- Keep the privacy promises: the music's audio is never saved or sent, and nothing new goes over the network without
  being listed in the README's Privacy section, the site's Privacy page, and [scripts/release-hosts.txt](scripts/release-hosts.txt) (the release
  check fails a build with a host that isn't).
- Match the style around you: plain names, comments that say why.

What happens next: I read every pull request, usually within a few days. The checks have to pass; for a translation
I may ask another speaker of the language to look, and for code that touches timing or scoring I run the sync suite
described above. I might suggest changes in the review; once it's ready I merge it here on GitHub, and it ships in
the next release. (OnKey is developed in a separate working copy, so after merging I bring your commit over with you
as its author; you don't need to do anything for that.)

## Licence

OnKey is MIT licensed ([LICENSE](LICENSE)). By contributing, you agree your contribution, translations included, is
released under the same licence, and that it's yours to give (not copied from somewhere with other terms).

## Be kind

Assume good intent, be patient with people new to this, and keep criticism about the code. Harassment of any kind
isn't welcome here.
