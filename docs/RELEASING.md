# Releasing OnKey

Everything ships from the GitHub repo (github.com/dsmurrell/onkey): no servers. A release is a GitHub release
`vX.Y.Z` with these files:

| File | What it's for |
|---|---|
| `OnKey.dmg` | the Mac download (signed, notarised, stapled). `…/releases/latest/download/OnKey.dmg` always gets the newest one |
| `OnKey_X.Y.Z_arm64.app.tar.gz` (+ `.sig`) | the update every installed Mac OnKey downloads, signed with the updater key |
| `OnKey_X.Y.Z_x64-setup.exe` (+ `.sig`) | the Windows download (an NSIS installer), and the update every installed Windows OnKey downloads: the same file, signed with the updater key |
| `OnKey_X.Y.Z_amd64.AppImage` (+ `.sig`) | the Linux download, and the update every Linux OnKey downloads: the same file, signed with the updater key |
| `latest.json` | what installed OnKeys check: `…/releases/latest/download/latest.json`, one entry per platform (`darwin-aarch64`, `windows-x86_64`, `linux-x86_64`) |
| `SHA256SUMS` | checksums of the four downloads |

The Mac files are built on the maintainer's Mac (notarisation needs its keychain); the Windows and Linux files are
built by CI from the tagged public commit, which signs them with the same updater key (its secrets) and rehearses
the update on the runner before uploading them. One `latest.json` names all three: see "Windows and Linux" below.

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

**Every release rehearses updating itself before it's published**, on each platform (`scripts/update-rehearsal.sh`
on the Mac, run by `release.sh` in dry runs and real ones; `scripts/update-rehearsal-ci.sh` and
`update-rehearsal-ci.ps1` on the Linux and Windows CI runners that built those files):
- a copy of the exact app about to ship is offered a fake next version (a second build of the same source with the
  version raised by one), served on the same machine over `http://127.0.0.1`, the only address the app takes for
  this (`updater.rs` `rehearsal_url`; the signature is still checked);
- it must download it, check the signature and install it over itself, and what's installed must be the next
  version (the Mac's Info.plist; the Windows `onkey.exe`'s version, and the installer must relaunch OnKey, whose
  window must load; the Linux AppImage's bytes, which must then open);
- a second copy must refuse the same next version signed with another key, unchanged;
- the app must be an official build.

If any of that fails, nothing is published. A test (`updater.rs`) pins the update address, the public key and the
app ID, so none of them changes by accident. A day after a release, the downloads stats job fails, and GitHub
emails, if copies are reading the new `latest.json` but none has downloaded its update.

How an update installs differs by platform (`updater.rs`, top): on the Mac the new `OnKey.app` is unpacked over the
old and the running app restarts or quits; on Windows the new installer is written to a temporary folder and run
passively (a progress window, no questions; it ends OnKey if it still runs, installs over it, and relaunches it only
after "Restart to update", never after a quit) and the running OnKey exits as it starts, having saved what it was
learning; on Linux the AppImage file is replaced in place (the old one is moved aside on the same disk first and
put back if the write fails), keeping its permissions, and the running copy carries on until it restarts or quits.
Builds: Apple Silicon (`darwin-aarch64`), Windows x64 (`windows-x86_64`), Linux x64 (`linux-x86_64`).

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
4. **CI secrets for the Windows and Linux builds** (once; by the maintainer, by hand, never by a script or an agent).
   The Windows and Linux updater files are signed on GitHub's runners, so the private repo's Actions need the
   updater key, as encrypted secrets. On github.com, in the **development** repo (`dsmurrell/onkey-dev`, where
   `windows.yml` and `linux.yml` live): Settings → Secrets and variables → Actions → New repository secret:
   - `TAURI_SIGNING_PRIVATE_KEY`: the whole contents of `~/.tauri/onkey-updater.key` (both lines; `pbcopy <
     ~/.tauri/onkey-updater.key` puts it on the clipboard without showing it, then paste). Tauri reads the key
     from this variable directly.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the key's password (`security find-generic-password -a onkey -s
     onkey-updater-key -w | pbcopy`).
   - `ONKEY_PUBLIC_RELEASE_TOKEN`: a fine-grained personal access token (Settings → Developer settings →
     Fine-grained tokens) for the one repository `dsmurrell/onkey`, with repository permission **Contents: Read
     and write** only (that's what uploading release assets needs), a short expiry (renew it when a release
     fails to upload). The workflow uses it only to upload to a *draft* release; it refuses a published one.

   Secrets are encrypted at rest, shown to no one after saving, and masked in logs; the workflows pass the key
   straight to `tauri build` and never write it to disk or print it. Anyone with write access to `onkey-dev` can
   run a workflow that uses them, so keep that repo's collaborators to yourself. Rotate (make a new key, ship
   its public key in a release signed with the old one, then replace the secrets) if `onkey-dev` is ever
   compromised. The Mac side keeps using the key file and the Keychain as before.

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

## Windows and Linux: one release for all three platforms

### Why it's built this way

- **Every file people get is built from the public source.** The Mac build is made in the public copy; the
  Windows and Linux builds are made by CI checking out the *public* repo at the release tag (not the private
  branch that holds the workflow), and the first step proves it: HEAD is the commit `release.sh` tagged (passed
  in by hand, so a tag moved later is caught), the tag is annotated and points at it, and its version is
  `tauri.conf.json`'s (`scripts/ci-release-source.sh`). The public commit's `Exported-From` trailer names the
  private commit it came from, for the record.
- **One updater key, three platforms.** The same key signs all three updates: the Mac's from the key file on the
  Mac, the others from the key held as CI secrets. `tauri.conf.json`'s public key, address and app ID never
  change (a test pins them), so every installed copy, on any platform, trusts exactly one key. Each CI build
  checks its file's signature against that public key before anything else (`scripts/ci-verify-sig.sh`), and
  `release-publish.sh` checks all three again on the downloaded files, on the Mac, before publishing: a file OnKey
  would refuse never goes out. A test build with a throwaway key (`updater: throwaway`) must *fail* that check,
  which proves a throwaway build can't pass as a release.
- **Nothing is public until everything is proven.** The Mac side rehearses, then makes the release as a **draft**
  (invisible to installed copies and to `releases/latest`) with the Mac files and no `latest.json`. CI builds,
  verifies, rehearses on a real Windows and a real Linux machine, and uploads to that draft only (it refuses a
  published release). `release-publish.sh` requires every file and each platform's rehearsal note (version, SHA-256
  of the file, tagged commit, official build, "ok", the run's link), verifies the signatures, writes one
  `latest.json` naming all three platforms, and only then publishes. An installed copy sees a release only when
  it is complete: `updater.rs` `supported()` is true on all three platforms, so a `latest.json` missing one would
  make that platform's check fail every day. (A preview build, versioned `X.Y.Z-preview.N` by the CI workflow's
  version input, never checks: there is no entry for it, and Settings tells testers to install the next preview
  over it. A release candidate, `X.Y.Z-rc.N`, checks like a release.)
- **Official builds only install.** `ONKEY_OFFICIAL=1` is set only in the release path (`release.sh` on the Mac;
  `updater: release` in CI, which refuses to be official without the public tag). Throwaway and development
  builds aren't official and only say when a new version is out.
- **A slot for Windows code signing (Authenticode), when there is a certificate:** `tauri build` signs the
  installer and `onkey.exe` itself when `bundle.windows.certificateThumbprint` (or a `signCommand`) is configured;
  it happens inside the same build step, before the updater signature is made over the finished installer, so
  nothing here changes: add the certificate's secrets and the config override to `scripts/ci-build.sh`'s Windows
  path. Until then the installer is unsigned: a manual download gets Windows SmartScreen's "Windows protected your
  PC" (the browser marks downloads as from the Internet, and SmartScreen checks marked files' reputation when
  they're opened). The updater's path is different: the installer is written by OnKey itself, which doesn't add
  that mark, so SmartScreen isn't expected to interrupt an update (the CI rehearsal installs exactly this way, on
  a stock Windows runner, and would hang if a dialog appeared); Microsoft Defender still scans the file, and
  Windows 11's Smart App Control, where it's on, blocks unsigned programs however they're started, as it would
  OnKey itself. Signing removes both, in time (reputation builds per certificate).

### The sequence

```sh
# 1. development checkout, on a clean master: version bump + CHANGELOG entry, committed
scripts/release.sh 0.3.0 --prepare
# 2. export that commit to the public copy, review, commit it there; push the private master to the development
#    repo too (CI runs the workflows from there: ONKEY_CI_REF, default master)
# 3. public copy (branch main), gh logged in: the Mac build, notarised, and the Mac rehearsal
scripts/release.sh 0.3.0 --dry-run --notarise
# 4. tag, push main + tag, draft release with the Mac files, start the Windows and Linux CI builds of the tag
scripts/release.sh 0.3.0 --no-build --stage
# 5. watch both runs (the command it prints); each builds, verifies, rehearses and uploads to the draft (~1 h)
# 6. check the draft and publish: latest.json for all three, SHA256SUMS, release published as latest
scripts/release-publish.sh 0.3.0 --dry-run      # reads, downloads, verifies, writes latest.json, publishes nothing
scripts/release-publish.sh 0.3.0
```

Then the usual checks (`releases/latest` shows the six files + `latest.json` + `SHA256SUMS`; `latest.json` names
three platforms), and the live update test from the previous version on each platform.

**If a CI build fails after `--stage`:** nothing is published, and the draft and tag can go: fix forward on a new
version if the source is wrong (`gh release delete v0.3.0 --repo dsmurrell/onkey --yes --cleanup-tag`, and
`git tag -d v0.3.0` in the public copy, then prepare 0.3.1), or if only the build or the runner failed, rerun the
workflow (`gh run rerun <id> --repo dsmurrell/onkey-dev`, or `gh workflow run … -f public_tag=v0.3.0 -f
public_commit=<sha> -f updater=release`): the same tagged commit, so the same source. `release-publish.sh` refuses
a draft with any file missing or a rehearsal note that isn't "ok".

**Plain `scripts/release.sh X.Y.Z` (no `--stage`)** still makes a Mac-only release, as 0.1.0 and 0.2.0 were: don't
use it once Windows and Linux copies are out there, since their daily check would then find no build for them.

**Testing the CI side without the key:** run `windows.yml` or `linux.yml` by hand on any branch with
`updater: throwaway`: the job makes a key, builds the app to trust it (not official), checks that its file does
*not* pass as the release key's, builds the next version and runs the rehearsal. Nothing is uploaded.

## A hotfix

1. Fix it on a branch as usual, tests passing, merged into `master` with a tagged merge commit
   (`Merge [fix-name]: what's fixed`): that line becomes the changelog entry.
2. `scripts/release.sh 0.3.1 --prepare` in the development checkout, then export and commit to the public copy;
   push the private master to the development repo.
3. In the public copy: `scripts/release.sh 0.3.1 --dry-run --notarise`: read the CHANGELOG entry and the commands
   it prints.
4. `scripts/release.sh 0.3.1 --no-build --stage`, then, when both CI runs are green, `scripts/release-publish.sh
   0.3.1` (see "Windows and Linux"). A hotfix is a release like any other: all three platforms, every time.

Installed copies pick it up within a day (on next launch for anyone who quits OnKey). To reach someone sooner:
Settings → Check now.

### If a release is bad

Updates only move forward: an OnKey that installed 0.1.1 won't go back to 0.1.0 on its own. So fix forward with
0.1.2. To stop a bad update spreading in the meantime, unpublish it (keeps the tag):

```sh
gh release edit v0.1.1 --repo dsmurrell/onkey --draft
```

`latest` then points at the previous release again, for new downloads and for update checks, on every platform (one
`latest.json` serves all three, so a release is withdrawn for all or none). A Windows copy that already ran the
bad installer, or a Linux copy whose AppImage was replaced, is on the bad version like a Mac is: only a forward
release moves it.

## The pieces

| | |
|---|---|
| `scripts/release.sh` | version bump (tauri.conf.json, Cargo.toml/.lock, ui and site package.json) and CHANGELOG.md (`--prepare`), build, updater bundle, rehearsal; `--stage`: tag, draft release with the Mac files, start the Windows and Linux CI builds (plain: a Mac-only release) |
| `scripts/make-dmg.sh` | release build, signed, notarised, stapled DMG; runs `check-release.sh` before signing and `check-dmg.sh` on the finished DMG |
| `scripts/check-release.sh <OnKey.app> [version]` | fails if the app has development-only code: the `devtest` hooks (simulated singer and player, event log, kept audio) or the live mirror; or the debugging entitlement; or the wrong version; or a learning model missing or not matching its SHA-256 in `models.rs` |
| `scripts/check-dmg.sh <OnKey.dmg or .app> [version]` | what people download: the DMG holds only the app and an Applications link; the app only its expected files; no data files built into the window's pages; no build-machine paths, keys or tokens; every web host in `scripts/release-hosts.txt` (what OnKey contacts, links it opens, library text); Developer ID, hardened runtime, exactly the expected entitlements, notarised and stapled. Runs `check-release.sh` too. Anyone can run it on a downloaded DMG from a checkout of the same version |
| `scripts/verify-update-sig.py <update file>` | checks an update's signature (Mac tarball, Windows installer or Linux AppImage) as OnKey will |
| `scripts/update-rehearsal.sh <OnKey.app>` | the app updates itself from a fake next version served on this Mac, refuses one signed with another key, and is an official build |
| `scripts/update-rehearsal-ci.sh`, `.ps1` | the same on the Linux and Windows CI runners, with the next version built there; the Windows one checks the installer relaunches OnKey |
| `scripts/release-publish.sh <X.Y.Z>` | the last gate of a staged release: every file and rehearsal note on the draft, all three signatures, `latest.json` for all three, publish |
| `scripts/ci-release-source.sh`, `ci-updater-key.sh`, `ci-build.sh`, `ci-verify-sig.sh`, `ci-upload-release.sh` | the CI steps, in order: the public tag is what's checked out; which key; the build (and the next version); the signature is the trusted key's; upload to the draft with the rehearsal note |
| `.github/workflows/windows.yml`, `linux.yml` (development repo) | the Windows and Linux builds, launch checks, and with `updater: release` the release path above |
| `src-tauri/src/updater.rs` | the in-app updater |
