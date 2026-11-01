# Releasing OnKey

Everything ships from the GitHub repo (github.com/dsmurrell/onkey): no servers. A release is a GitHub release
`vX.Y.Z` with four files:

| File | What it's for |
|---|---|
| `OnKey.dmg` | the download (signed, notarised, stapled). `…/releases/latest/download/OnKey.dmg` always gets the newest one |
| `OnKey_X.Y.Z_arm64.app.tar.gz` (+ `.sig`) | the update every installed OnKey downloads, signed with the updater key |
| `latest.json` | what installed OnKeys check: `…/releases/latest/download/latest.json` |
| `SHA256SUMS` | checksums of the two downloads |

The learning models ship inside the app (`Contents/Resources/models`, ≈380 MB; `tauri.conf.json`
`bundle.resources`), and OnKey never downloads them. So the DMG and every update carry them: an update replaces
the whole app, models included (≈350 MB compressed each time). `scripts/check-release.sh` checks every model is in
the built app with the SHA-256 `src-tauri/src/models.rs` expects, so a release without them can't ship.

## How updates reach people

`src-tauri/src/updater.rs`. A minute after launch and once a day after, if **Settings → Check for updates
automatically** is on (it is by default), OnKey reads `latest.json` from the latest release. A newer version is
downloaded in the background and its signature checked against the public key in `tauri.conf.json`. Then a small
**Update available · Restart to update** appears in the corner of the window (never in mini mode). Pressed during
a song, OnKey waits for the song to end (and for the song to be learned) before restarting. If nobody presses it,
the update is installed when OnKey quits. Development builds (the `devtest` feature) never update.

So a fix reaches most people within a day of publishing, with no prompt in the middle of a song.

**Only official builds install updates.** `scripts/release.sh` builds with `ONKEY_OFFICIAL=1` (`updater.rs`
`OFFICIAL`). Any other release build, such as a fork's or one built from source, still checks, but only says
"OnKey X is out from the original project · Download" (a link to singonkey.app). It never replaces itself with
this project's build, which would wipe out a fork's changes.

**Every release rehearses updating itself before it's published** (`scripts/update-rehearsal.sh`, run by
`release.sh` in dry runs and real ones):
- a copy of the exact app about to ship is offered a fake next version, served on this Mac;
- it must download it, check the signature and install it over itself;
- a second copy must refuse one signed with another key;
- the app must be an official build.

If any of that fails, nothing is published. A test (`updater.rs`) pins the update address, the public key and the
app ID, so none of them changes by accident. A day after a release, the downloads stats job fails, and GitHub
emails, if copies are reading the new `latest.json` but none has downloaded its update.

Only Apple Silicon builds are made (`darwin-aarch64` in `latest.json`), the same as the DMG.

## One-time setup

1. **The updater key: back it up now.** It was made on 16 Oct 2026:
   - private key: `~/.tauri/onkey-updater.key` (not in the repo, and never to be)
   - its password: login Keychain, service `onkey-updater-key`
     (`security find-generic-password -a onkey -s onkey-updater-key -w` shows it)
   - public key: `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` (key id `59F5303C6E58D8DD`)

   Put the key file and the password in your password manager. **If the key is lost, installed copies of OnKey
   can't be updated any more**: everyone would have to download the DMG again by hand. If it leaks, someone with
   the key and control of the release could push an "update": make a new key, ship it in a release signed with
   the old one, and treat the old one as gone.
2. **GitHub CLI:** `gh auth login` (GitHub.com, HTTPS, log in with a browser). `gh auth status` checks it.
3. **Notarisation** is already set up for `scripts/make-dmg.sh` (Developer ID certificate in the keychain,
   notarytool profile `onkey-notary`, the provisioning profile in `~/Documents/Apple Developer/`).

## Launch day

Development happens in a private checkout; this repository is its public copy, one commit per release. Every
build people download (releases and test builds alike) is made **from the public copy**, so what ships is exactly
the published code, and a file missing from the published list breaks the build loudly instead of shipping quietly.

```sh
# in the development checkout, on a clean master
scripts/release.sh 0.1.0 --prepare               # version bump + CHANGELOG entry, committed as "[release] v0.1.0"
# export that commit to the public copy, review what changed, commit it there (the maintainer's export script)
# then, in the public copy (branch main), with the models in src-tauri/models (scripts/fetch-models.sh)
gh auth status                                   # logged in
scripts/release.sh 0.1.0 --dry-run               # builds (ad hoc signed), makes + signs the update, prints what it would publish
scripts/release.sh 0.1.0                         # the release: tag, build, notarise, push, gh release create
```

In the public copy `release.sh` finds the version already prepared (the version files say 0.1.0 and CHANGELOG.md
has its entry), so it doesn't bump or commit: it tags the export commit, builds, and publishes. A test build for a
friend is just `scripts/make-dmg.sh` in the public copy.

A dry run touches no network: no GitHub, no Apple (it signs ad hoc and skips notarisation). `--dry-run --notarise` signs with the Developer ID and notarises as a release does; the release
can then publish exactly those files with `--no-build`, run straight after with nothing merged in between (it
reuses `src-tauri/target-dist/release-v0.1.0/OnKey.dmg` and the app beside it, checks the version, the Developer
ID signature, the stapled tickets and that there's no development-only code; an ad hoc dry-run build is refused).
Each build takes about 15–25 minutes. The first release's changelog entry is "First public release."; later ones list
the tagged merges since the last release.

Then check:

- `https://github.com/dsmurrell/onkey/releases/latest` shows `OnKey 0.1.0` with the four files.
- `curl -sL https://github.com/dsmurrell/onkey/releases/latest/download/latest.json` shows version `0.1.0`.
- Download `OnKey.dmg` on another Mac (or a new user account): it opens with no warning, and a new song is
  learned (the models are in the app: nothing is downloaded).

`release.sh` stops before changing anything if: the tree isn't clean, it isn't on `master` or `main`, `gh` isn't logged in,
the version isn't newer than the current one, the tag exists, or the updater key or its password isn't there.
After building it stops if the app has development-only code or is missing a learning model (or has one whose
SHA-256 isn't the expected one) (`scripts/check-release.sh`), or the update isn't signed by the key OnKey trusts
(`scripts/verify-update-sig.py`). The models must be in `src-tauri/models` to build (`scripts/fetch-models.sh`).

## A hotfix

1. Fix it on a branch as usual, tests passing, merged into `master` with a tagged merge commit
   (`Merge [fix-name]: what's fixed`): that line becomes the changelog entry.
2. `scripts/release.sh 0.1.1 --prepare` in the development checkout, then export and commit to the public copy.
3. In the public copy: `scripts/release.sh 0.1.1 --dry-run`: read the CHANGELOG entry and the `gh` command it prints.
4. `scripts/release.sh 0.1.1 --no-build` publishes what the dry run built (or drop `--no-build` to build again).

Installed copies pick it up within a day (on next launch for anyone who quits OnKey). To reach someone sooner:
Settings → Check now.

### If a release is bad

Updates only move forward: an OnKey that installed 0.1.1 won't go back to 0.1.0 on its own. So fix forward with
0.1.2. To stop a bad update spreading in the meantime, unpublish it (keeps the tag):

```sh
gh release edit v0.1.1 --repo dsmurrell/onkey --draft
```

`latest` then points at the previous release again, for new downloads and for update checks.

## The pieces

| | |
|---|---|
| `scripts/release.sh` | version bump (tauri.conf.json, Cargo.toml/.lock, ui and site package.json) and CHANGELOG.md (`--prepare`), build, updater bundle, latest.json, publish |
| `scripts/make-dmg.sh` | release build, signed, notarised, stapled DMG; runs `check-release.sh` before signing and `check-dmg.sh` on the finished DMG |
| `scripts/check-release.sh <OnKey.app> [version]` | fails if the app has development-only code: the `devtest` hooks (simulated singer and player, event log, kept audio) or the live mirror; or the debugging entitlement; or the wrong version; or a learning model missing or not matching its SHA-256 in `models.rs` |
| `scripts/check-dmg.sh <OnKey.dmg or .app> [version]` | what people download: the DMG holds only the app and an Applications link; the app only its expected files; no data files built into the window's pages; no build-machine paths, keys or tokens; every web host in `scripts/release-hosts.txt` (what OnKey contacts, links it opens, library text); Developer ID, hardened runtime, exactly the expected entitlements, notarised and stapled. Runs `check-release.sh` too. Anyone can run it on a downloaded DMG from a checkout of the same version |
| `scripts/verify-update-sig.py <bundle.tar.gz>` | checks the update's signature as OnKey will |
| `scripts/update-rehearsal.sh <OnKey.app>` | the app updates itself from a fake next version served on this Mac, refuses one signed with another key, and is an official build |
| `src-tauri/src/updater.rs` | the in-app updater |
