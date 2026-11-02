# What the engine sends the window: kinds, codes and parameters

The engine (Rust) decides some of the lines the window shows. Each one carries a stable **kind** (or **code**) and
its **parameters**, so the window can say it in its own language (`ui/messages/<lang>.json`). Each also still carries
its English words (`text`, `reason` or `error`): for the log, a report, and a window that doesn't know the kind yet.
Never parse the English: map the kind. (The window: `ui/lib/engineText.ts`, messages under `engine.*` in
`ui/messages/en.json`; `ui/lib/engineText.test.mjs` checks each row below renders the same English.)

The words OnKey's Rust side shows itself (the menu bar, the share video's end card, the report's save panel, macOS's
permission prompts) are in this folder's `<lang>.json` files instead: see `src/messages.rs`.

Parameters are strings. Song and app names are as the music app gives them (never translated).

## The status line: `line`

Sent as `{"type": "listen", "line": …}` and inside every `{"type": "status", …}` event as `line`; `null` when
there's nothing to say. Shape:

```json
{"kind": "stopped", "params": {"song": "Yellow"}, "text": "Not learning Yellow"}
```

| kind | params | English (`text`) | sent by |
|---|---|---|---|
| `finding` | none | Hearing music… finding the song | listen.rs `Listener::status` |
| `new_song` | none | New song… finding it | listen.rs `Listener::status` |
| `unnamed` | none | Couldn’t name this song | listen.rs `Listener::status` |
| `no_shazam` | none | Shazam unavailable: only songs learned here are recognised | listen.rs `Listener::status`, `status_line` |
| `shazam_off` | none | Not a song learned here: Shazam is off in Settings | listen.rs `status_line` |
| `elsewhere` | `app` (e.g. "Spotify") | {app} is playing, but not on this Mac | listen.rs `status_line` |
| `learning` | none | Learning: sing along, it’s scored from the next play | engine.rs |
| `stopped` | `song` | Not learning {song} | engine.rs |
| `podcast` | none | Podcast playing: OnKey sings along to music | engine.rs |

## The share ask: `share_prompt`

The command returns `null` or:

```json
{"kind": "best", "params": {"song": "Yellow", "score": "72", "points": "12"}, "reason": "Your best on Yellow by 12 points"}
```

| kind | params | English (`reason`) |
|---|---|---|
| `first` | `song`, `score` | {score} on your first go at {song} |
| `over80` | `song`, `score`, `over` ("80") | Your first score over {over} on {song} |
| `best` | `song`, `score`, `points` | Your best on {song} by {points} point(s) |
| `up` | `song`, `score`, `points` | {points} point(s) up on your last take of {song} |

`song` is "this song" (English) when the take has no song name; plural `points` needs ICU plural rules.

## The output timing check: `output_timing_check`

A refused check returns `{"error": "<English>", "code": "<code>"}` (no params):

| code | English (`error`) |
|---|---|
| `output_changed` | The output changed during the check. Try again. |
| `few_taps` | Not enough taps in time with the clicks. Tap along in rhythm, once on each click. |
| `uneven` | Your taps were too uneven to measure. Try again, tapping along in rhythm, like clapping to a beat. |
| `reacting` | It looks like you waited for each click. Try tapping along in rhythm instead. |
| `unbelievable` | That doesn’t look right: did you wait for each click? Try again, tapping along in rhythm, like clapping to a beat. |
| `untimed` | The check couldn’t time its clicks. Try again. |
| `stalled` | The sound stalled or restarted during the check, so it can’t be timed. Try again. |
| `other` | (a refusal added later without a code: show `error`) |

## The learning models: `status.models`

`{"state": "ready"}`, or `{"state": "missing" | "broken", "code", "error"}`. `error` is technical detail in English
(which file, the loader's message), for a report: show it as is, if at all. The one code whose `error` is plain words
is said from the catalog (`engine.models.<code>`, lib/engineText.ts `modelsError`):

| state | code | meaning | English (`error`) |
|---|---|---|---|
| `missing` | `files_bad` | a model file is missing or the wrong size (a damaged install): reinstall | (technical: the file) |
| `missing` | `unloadable` | the files are there but couldn't be loaded | they couldn't be loaded |
| `broken` | `pitch_model` | the pitch model couldn't load: OnKey can't do anything | (technical: the loader's message) |

## Updates: `onkey-update` events, `update_status`, `update_check`, `update_restart`

`state` is already a kind: `idle`, `checking`, `current`, `official` (a build from source: `available`), `downloading`
(`available`), `ready` (`available`), `error`. An `error` state now has a `code`; `error` is the updater's own
message (English, technical) and `available` the version, when known.

| code | params | meaning |
|---|---|---|
| `check_failed` | `error` | couldn't check for updates |
| `download_failed` | `available`, `error` | the new version couldn't be downloaded |
| `install_failed` | `available` | the update couldn't be installed |

`check_failed` and `download_failed` carry the updater's technical message in `error` (shown as sent, in brackets);
`install_failed`'s `error` is only "the update couldn't be installed", which the window's own sentence already says,
so the window shows no `error` for it (lib/engineText.ts `updateWhy`).

`update_restart` with nothing downloaded returns `{"ok": false, "code": "not_ready", "error": "no update is ready"}`
(not shown: the window keeps the status it had).

## Settings that couldn't be saved

`set_setting`, `set_listening_paused` and `set_language` reject only when the settings file couldn't be written:
the rejection (a string) is always that, in English: "Couldn’t save the setting ({OS error}). It holds until OnKey
quits." Treat any rejection of these commands as code `setting_unsaved`.

## Already kinds (no English sent)

`{"type": "problem", "kind": null | "permission"}`, `{"type": "phase", "phase": …}`, `{"type": "not_a_song", …}`.

## A song's fit with your range: Library rows' `fit`

Each Library row (the `library` command, range.rs `annotate`) with notes and a range of yours carries `fit` (the key),
`fit_label` (its English) and `fit_suits`. The window says it from `engine.fit.<key>` (lib/engineText.ts `fitText`);
`fit_label` only for a key it doesn't know.

| key | English (`fit_label`) |
|---|---|
| `fits` | Fits your range |
| `few_above` | A few notes above your range |
| `few_below` | A few notes below your range |
| `octave_lower` | Mostly above you: sing it an octave lower |
| `octave_higher` | Mostly below you: sing it an octave higher |
| `above` | Mostly above your range |
| `below` | Mostly below your range |
| `wider` | Wider than your range |

## A problem report's files: `report_prepare`

Each file listed (`files[]`, report.rs) carries `what` (English), `what_kind` and `what_params` (`artist`, `name`:
the song, never translated). The window says it from `engine.reportWhat.<kind>` with `os` ("pc" or "mac") for "this
PC" / "this Mac" (lib/engineText.ts `reportWhat`). `{machine}` below is the engine's word: "Mac" on macOS, else "PC"
(the Linux summary says "computer").

| kind | params | English (`what`) |
|---|---|---|
| `log` | none | OnKey's log for that day |
| `take` | `artist`, `name` | a play's summary: {artist} - {name} |
| `take_reported` | `artist`, `name` | the take reported: a play's summary: {artist} - {name} |
| `builds` | none | the OnKey builds that ran on this {machine} |
| `summary` | none | the logs summed up: this {machine}, audio, music apps, learning, takes, errors |
| `about` | none | OnKey's version, this {machine}, the settings, and what you wrote |

## The browser setting YouTube needs: `status.youtube_blocked`

`{"browser", "setting", "setting_kind"}` (macOS only; platform/mac/youtube.rs `js_setting`). `setting` is the
English path to the setting; `setting_kind` picks the catalog's (`engine.youtubeSetting.<kind>`, lib/engineText.ts
`youtubeSetting`), whose translations name the browser's menus as that language's browser shows them.

| kind | English (`setting`) |
|---|---|
| `safari` | Safari ▸ Settings ▸ Advanced: tick "Show features for web developers", then Develop ▸ Allow JavaScript from Apple Events |
| `chromium` | View ▸ Developer ▸ Allow JavaScript from Apple Events |

## An app with no name of its own: `engine.app.<kind>`

App names ("Spotify", "Apple Music", "YouTube", a browser's) go to the window as they are, never translated. Where
the engine has no name it sends `null` and the window says what it is (lib/engineText.ts `appText`):
`status.player.app` and `status.music_quiet.app` (null: "your music app"), and `along_mode`'s `app` with `app_kind`.

| kind | English |
|---|---|
| `music_app` | your music app |
| `browser` | your browser |
