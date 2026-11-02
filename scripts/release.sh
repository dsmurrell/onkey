#!/bin/bash
# Make an OnKey release: bump the version, write its CHANGELOG entry, build the notarised OnKey.dmg, the updater
# bundle (OnKey.app.tar.gz, signed with the updater key) and latest.json, check the app has no development-only
# code, then publish it as a GitHub release (vX.Y.Z), where every OnKey out there finds it within a day
# (src-tauri/src/updater.rs). The learning models ship inside the app, so in the DMG and in every update
# (check-release.sh checks each one's SHA-256). The steps for launch day and for a hotfix: docs/RELEASING.md.
#
# Releases are built and published from the PUBLIC copy (../public, branch main), so what ships is exactly the
# published code: --prepare in the private repo first (version + CHANGELOG, committed), then the public export,
# then this script in the public copy, which finds the version already prepared and goes straight to building.
# Run without --prepare in a repo whose version isn't prepared yet, it bumps and commits there itself.
#
#   scripts/release.sh <X.Y.Z> --prepare    only the version bump and CHANGELOG entry, committed as
#                                           "[release] vX.Y.Z": no build, no tag, no network, no keys needed
#   scripts/release.sh <X.Y.Z> --dry-run    everything but commit, tag, push and publish, and no network at all:
#                                           builds the app (signed ad hoc, not notarised: no Apple call), makes and
#                                           signs the updater bundle and latest.json, prints the git, gh and
#                                           network checks it would run, and puts the version files back
#   scripts/release.sh <X.Y.Z>              the release: commits "[release] vX.Y.Z", tags it, builds, pushes and
#                                           runs `gh release create` (the Mac alone: latest.json with darwin-aarch64
#                                           only, as 0.1.0 and 0.2.0 shipped)
#   scripts/release.sh <X.Y.Z> --stage      a release for all three platforms (docs/RELEASING.md "Windows and
#                                           Linux"): as above up to the rehearsal, then tags, pushes, makes the
#                                           release as a DRAFT with the Mac files (no latest.json yet) and starts the
#                                           Windows and Linux CI builds of this tag on the development repo, which
#                                           sign their updater files, rehearse the update and upload to the draft;
#                                           scripts/release-publish.sh then checks everything, writes latest.json
#                                           for all three and publishes
# Options:
#   --notarise        in a dry run, sign with the Developer ID and notarise as a release does (calls Apple), so
#                     the release can then publish exactly these files with --no-build
#   --no-build        reuse the app make-dmg.sh last built (to rehearse the packaging, or after a dropped
#                     notarisation); the version check is skipped in a dry run
# Environment: make-dmg.sh's (ONKEY_SIGN_IDENTITY, ONKEY_NOTARY_PROFILE, ONKEY_ADHOC=1, ...), and
#   ONKEY_UPDATER_KEY            the updater's private key (default ~/.tauri/onkey-updater.key)
#   ONKEY_UPDATER_KEY_PASSWORD   its password (default: from the login Keychain, service "onkey-updater-key")
#   ONKEY_REPO                   owner/repo (default dsmurrell/onkey)
#   ONKEY_DEV_REPO               the development repo whose CI builds Windows and Linux (default dsmurrell/onkey-dev)
#   ONKEY_CI_REF                 its branch holding the workflows to run, for --stage (default master; push it first)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD

V=""; DRY=0; BUILD=1; NOTARISE=0; PREP=0; STAGE=0
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    --no-build) BUILD=0 ;;
    --notarise) NOTARISE=1 ;;
    --prepare) PREP=1 ;;
    --stage) STAGE=1 ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    -*) echo "unknown option $a" >&2; exit 2 ;;
    *) V=$a ;;
  esac
done
[[ "$V" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: scripts/release.sh <X.Y.Z> [--prepare | --dry-run [--notarise] | --no-build] [--stage]" >&2; exit 2; }
[ $PREP = 1 ] && [ $DRY = 1 ] && { echo "--prepare commits only locally: no --dry-run needed" >&2; exit 2; }
[ $PREP = 1 ] && [ $STAGE = 1 ] && { echo "--prepare builds nothing: --stage comes after the export" >&2; exit 2; }
TAG="v$V"
REPO=${ONKEY_REPO:-dsmurrell/onkey}
DEV_REPO=${ONKEY_DEV_REPO:-dsmurrell/onkey-dev}
CI_REF=${ONKEY_CI_REF:-master}
KEY=${ONKEY_UPDATER_KEY:-$HOME/.tauri/onkey-updater.key}
export CARGO_TARGET_DIR="$ROOT/src-tauri/target-dist"     # make-dmg.sh's
# an official build: the only kind that installs updates (updater.rs OFFICIAL); the rehearsal below checks it is
export ONKEY_OFFICIAL=1
APP="$CARGO_TARGET_DIR/release/bundle/macos/OnKey.app"
DIST="$CARGO_TARGET_DIR/release-$TAG"
say() { printf '\n== %s\n' "$*"; }
die() { echo "release: $*" >&2; exit 1; }

# ---- before anything changes
say "OnKey $V$([ $DRY = 1 ] && echo " (dry run)")$([ $PREP = 1 ] && echo " (prepare)")$([ $STAGE = 1 ] && echo " (stage: a draft, Windows and Linux to follow)")"
if [ $PREP = 0 ]; then
  [ -f "$KEY" ] || die "no updater key at $KEY (docs/RELEASING.md: the key)"
  PW=${ONKEY_UPDATER_KEY_PASSWORD:-$(security find-generic-password -a onkey -s onkey-updater-key -w 2>/dev/null || true)}
  [ -n "$PW" ] || die "no password for the updater key (Keychain service onkey-updater-key, or ONKEY_UPDATER_KEY_PASSWORD)"
fi
CUR=$(python3 -c 'import json,sys; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')
# the last release: its tag (the public copy), else its "[release] vX.Y.Z" commit (the private repo, which isn't tagged)
LAST=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || git log -1 --format=%H --grep='^\[release\] v' 2>/dev/null || true)
echo "now $CUR; last release ${LAST:-(none)}"
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then die "$TAG is already tagged"; fi
# prepared: the version files already say $V and CHANGELOG.md has its entry (--prepare ran, then the export)
PREPARED=0
if [ $PREP = 0 ] && [ "$CUR" = "$V" ] && grep -q "^## $V " CHANGELOG.md 2>/dev/null; then
  PREPARED=1; echo "version $V already prepared (CHANGELOG.md has its entry): no bump"
else
  python3 - "$CUR" "$V" <<'EOF' || die "$V isn't newer than $CUR"
import sys
a, b = (tuple(map(int, x.split("."))) for x in sys.argv[1:])
sys.exit(0 if b > a or (b == a and b == (0, 1, 0)) else 1)
EOF
fi
BRANCH=$(git branch --show-current)
if [ $DRY = 0 ]; then
  [ -z "$(git status --porcelain)" ] || die "the working tree has changes: commit or stash them first"
  case "$BRANCH" in master|main) ;; *) die "release from master (private) or main (public), not '$BRANCH'" ;; esac
  [ $PREP = 1 ] || gh auth status >/dev/null 2>&1 || die "gh isn't logged in (gh auth login)"
fi
# a dry run makes no Apple call unless asked: signed ad hoc, not notarised
[ $DRY = 1 ] && [ $NOTARISE = 0 ] && export ONKEY_ADHOC=1

# ---- the version, everywhere it's written
ENTRY="$(mktemp)"
if [ $PREPARED = 1 ]; then
  # the prepared entry: from its "## $V" heading to the next one
  awk -v v="$V" '/^## /{ on = (index($0, "## " v " ") == 1) } on' CHANGELOG.md > "$ENTRY"
  NOTES="$(mktemp)"; tail -n +3 "$ENTRY" > "$NOTES"
  say "CHANGELOG.md (prepared)"; sed 's/^/  /' "$ENTRY"
else
VERSION_FILES=(src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock ui/package.json ui/package-lock.json site/package.json site/package-lock.json CHANGELOG.md)
if [ $DRY = 1 ]; then
  KEEP=$(mktemp -d)
  for f in "${VERSION_FILES[@]}"; do [ -f "$f" ] && mkdir -p "$KEEP/$(dirname "$f")" && cp "$f" "$KEEP/$f"; done
  [ -f CHANGELOG.md ] || touch "$KEEP/.no-changelog"
  restore() {
    for f in "${VERSION_FILES[@]}"; do [ -f "$KEEP/$f" ] && cp "$KEEP/$f" "$f"; done
    [ -f "$KEEP/.no-changelog" ] && rm -f CHANGELOG.md
    rm -rf "$KEEP"
    echo "(dry run: version files put back as they were)"
  }
  trap restore EXIT
fi
say "version $CUR -> $V"
python3 - "$V" <<'EOF'
import json, re, sys
v = sys.argv[1]
def sub(path, pattern, repl, count=1, flags=re.M):
    s = open(path).read()
    t, n = re.subn(pattern, repl, s, count=count, flags=flags)
    if n == 0:
        sys.exit(f"{path}: version not found")
    open(path, "w").write(t)
    print(f"  {path}")
sub("src-tauri/tauri.conf.json", r'^(  "version": )"[^"]*"', rf'\g<1>"{v}"')
sub("src-tauri/Cargo.toml", r'^(\[package\]\nname = "onkey"\nversion = )"[^"]*"', rf'\g<1>"{v}"')
sub("src-tauri/Cargo.lock", r'^(name = "onkey"\nversion = )"[^"]*"', rf'\g<1>"{v}"')
for d in ("ui", "site"):
    sub(f"{d}/package.json", r'^(  "version": )"[^"]*"', rf'\g<1>"{v}"')
    lock = f"{d}/package-lock.json"
    try:
        j = json.load(open(lock))
    except FileNotFoundError:
        continue
    j["version"] = v
    if "" in j.get("packages", {}):
        j["packages"][""]["version"] = v
    open(lock, "w").write(json.dumps(j, indent=2, ensure_ascii=False) + "\n")
    print(f"  {lock}")
EOF

# ---- the changelog: every merge into master since the last release, by its [tag]
say "CHANGELOG.md"
RANGE=${LAST:+$LAST..}HEAD
NOTES="$(mktemp)"
if [ -z "$LAST" ]; then
  echo "- First public release." > "$NOTES"      # (the development history before it is in git)
else
  git log --first-parent --merges --format='%s' "$RANGE" \
    | sed -E 's/^Merge (branch )?//; s/^\[([^]]+)\]:? */- **\1**: /; /^- /!s/^/- /' > "$NOTES"
fi
[ -s "$NOTES" ] || echo "- Fixes and improvements." > "$NOTES"
{ echo "## $V · $(date +%Y-%m-%d)"; echo; cat "$NOTES"; echo; } > "$ENTRY"
if [ -f CHANGELOG.md ]; then
  python3 - "$ENTRY" <<'EOF'
import sys
entry = open(sys.argv[1]).read()
s = open("CHANGELOG.md").read()
i = s.find("\n## ")
s = s + "\n" + entry if i < 0 else s[:i + 1] + entry + s[i + 1:]
open("CHANGELOG.md", "w").write(s)
EOF
else
  { echo "# Changelog"; echo; echo "What changed in each OnKey release. Made by scripts/release.sh from the tagged merges since the one before."; echo; cat "$ENTRY"; } > CHANGELOG.md
fi
sed 's/^/  /' "$ENTRY"

if [ $DRY = 0 ]; then
  git add "${VERSION_FILES[@]}"
  git commit -q -m "[release] $TAG" -m "$(cat "$NOTES")"
  echo "committed [release] $TAG"
fi
fi
if [ $PREP = 1 ]; then
  echo
  echo "prepared $TAG here. Next (docs/RELEASING.md): export this commit to the public copy, commit it there, and run"
  echo "  scripts/release.sh $V --dry-run, then scripts/release.sh $V, in the public copy"
  exit 0
fi

# ---- build, sign, notarise
if [ $BUILD = 1 ]; then
  rm -rf "$DIST"; mkdir -p "$DIST"
  say "build (make-dmg.sh: notarised unless ONKEY_ADHOC=1)"
  scripts/make-dmg.sh "$DIST/OnKey.dmg"
else
  say "no build: packaging the app built $(stat -f %Sm "$APP" 2>/dev/null || echo never)"
  [ -d "$APP" ] || die "no app at $APP: build first (drop --no-build)"
  mkdir -p "$DIST"
  # the DMG a dry run of this version made (or make-dmg.sh's default output), with the app it was made from
  if [ -f "$DIST/OnKey.dmg" ]; then echo "reusing $DIST/OnKey.dmg ($(stat -f %Sm "$DIST/OnKey.dmg"))"
  elif [ -f "$HOME/Desktop/OnKey.dmg" ]; then cp "$HOME/Desktop/OnKey.dmg" "$DIST/OnKey.dmg"; echo "reusing ~/Desktop/OnKey.dmg"
  elif [ $DRY = 1 ]; then echo "(no DMG to reuse: a release needs one)"
  else die "no DMG to reuse in $DIST or ~/Desktop"; fi
  [ -n "$(find "$DIST/OnKey.dmg" -newer "$APP/Contents/Info.plist" 2>/dev/null)" ] || [ ! -f "$DIST/OnKey.dmg" ] \
    || die "the DMG is older than the app: they're from different builds"
fi
APPV=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP/Contents/Info.plist")
if [ $BUILD = 1 ] || [ $DRY = 0 ]; then
  scripts/check-release.sh "$APP" "$V"
else
  scripts/check-release.sh "$APP"
  [ "$APPV" = "$V" ] || echo "WARNING: the reused app is version $APPV, not $V (fine for a rehearsal, not for a release)"
fi
if [ $DRY = 0 ]; then
  # (grep reads it all, not -q: -q stops at the first match, codesign is cut off writing the rest, and pipefail
  # took that for a failure, now and then)
  codesign -dv --verbose=2 "$APP" 2>&1 | grep "^Authority=Developer ID Application" > /dev/null || die "the app isn't signed with a Developer ID (an ad hoc dry-run build?)"
  xcrun stapler validate -q "$APP" || die "the app has no notarisation ticket stapled"
  [ -f "$DIST/OnKey.dmg" ] && { xcrun stapler validate -q "$DIST/OnKey.dmg" || die "the DMG has no notarisation ticket stapled"; }
  # what people download: only the expected files, no data files, paths or keys, listed hosts, entitlements
  scripts/check-dmg.sh "$DIST/OnKey.dmg" "$V" || die "check-dmg.sh: the DMG isn't fit to release"
fi

# ---- the updater bundle and latest.json
say "updater bundle"
ARCHS=$(lipo -archs "$APP/Contents/MacOS/onkey")
TARBALL="OnKey_${V}_$(tr ' ' '-' <<< "$ARCHS").app.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$DIST/$TARBALL" -C "$(dirname "$APP")" OnKey.app
npx tauri signer sign -f "$KEY" -p "$PW" "$DIST/$TARBALL" > /dev/null
[ -s "$DIST/$TARBALL.sig" ] || die "the updater bundle wasn't signed"
# checked as OnKey will check it, against the public key in tauri.conf.json (a wrong key: every install refuses it)
rc=0; python3 scripts/verify-update-sig.py "$DIST/$TARBALL" || rc=$?
[ $rc = 1 ] && die "the updater bundle's signature doesn't match the public key OnKey trusts"
echo "  $TARBALL ($(du -h "$DIST/$TARBALL" | cut -f1)) + .sig ($ARCHS)"
cp "$ENTRY" "$DIST/notes.md"
python3 - "$V" "$DIST" "$TARBALL" "$REPO" "$TAG" "$ARCHS" <<'EOF'
import datetime, json, sys
v, dist, tarball, repo, tag, archs = sys.argv[1:]
notes = open(f"{dist}/notes.md").read().split("\n", 2)[2].strip()
sig = open(f"{dist}/{tarball}.sig").read().strip()
url = f"https://github.com/{repo}/releases/download/{tag}/{tarball}"
targets = {"arm64": "darwin-aarch64", "x86_64": "darwin-x86_64"}
platforms = {targets[a]: {"signature": sig, "url": url} for a in archs.split() if a in targets}
latest = {"version": v, "notes": notes,
          "pub_date": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
          "platforms": platforms}
json.dump(latest, open(f"{dist}/latest.json", "w"), indent=2, ensure_ascii=False)
print("  latest.json:", ", ".join(platforms), "->", url)
EOF
(cd "$DIST" && shasum -a 256 OnKey.dmg "$TARBALL" 2>/dev/null > SHA256SUMS || true)

# ---- the update rehearsal: the exact app about to ship must update itself, or nothing is published
say "update rehearsal"
ONKEY_UPDATER_KEY="$KEY" ONKEY_UPDATER_KEY_PASSWORD="$PW" scripts/update-rehearsal.sh "$APP" || die "the update rehearsal failed (above): not published"

# ---- publish (or, with --stage, a draft: Windows and Linux are built from this tag by CI, then release-publish.sh)
ASSETS=()
if [ $STAGE = 1 ]; then
  # the Mac files only: latest.json and SHA256SUMS come with release-publish.sh, once all three platforms are there
  for f in OnKey.dmg "$TARBALL" "$TARBALL.sig"; do [ -f "$DIST/$f" ] && ASSETS+=("$DIST/$f"); done
  rm -f "$DIST/latest.json" "$DIST/SHA256SUMS"     # (Mac-only ones: release-publish.sh writes the real ones)
else
  for f in OnKey.dmg "$TARBALL" "$TARBALL.sig" latest.json SHA256SUMS; do [ -f "$DIST/$f" ] && ASSETS+=("$DIST/$f"); done
fi
PUSH=(git push origin "$BRANCH" "$TAG")
if [ $STAGE = 1 ]; then
  CREATE=(gh release create "$TAG" --repo "$REPO" --verify-tag --draft --title "OnKey $V" --notes-file "$DIST/notes.md" "${ASSETS[@]}")
else
  CREATE=(gh release create "$TAG" --repo "$REPO" --verify-tag --latest --title "OnKey $V" --notes-file "$DIST/notes.md" "${ASSETS[@]}")
fi
HEADSHA=$(git rev-parse HEAD)
CI_WIN=(gh workflow run windows.yml --repo "$DEV_REPO" --ref "$CI_REF" -f "public_tag=$TAG" -f "public_commit=$HEADSHA" -f updater=release)
CI_LINUX=(gh workflow run linux.yml --repo "$DEV_REPO" --ref "$CI_REF" -f "public_tag=$TAG" -f "public_commit=$HEADSHA" -f updater=release)
q() { printf '%q ' "$@"; }
say "$([ $STAGE = 1 ] && echo stage || echo publish)"
if [ $DRY = 1 ]; then
  echo "dry run: would run"
  [ $PREPARED = 1 ] || echo "  git commit -m \"[release] $TAG\"   (the version files and CHANGELOG.md above)"
  echo "  git tag -a $TAG -m \"OnKey $V\""
  echo "  $(q "${PUSH[@]}")"
  echo "  $(q "${CREATE[@]}")"
  if [ $STAGE = 1 ]; then
    echo "  $(q "${CI_WIN[@]}")"
    echo "  $(q "${CI_LINUX[@]}")"
    echo "  (then, when both CI runs are green: scripts/release-publish.sh $V)"
  fi
  echo "artifacts: $DIST"; ls -lh "$DIST" | tail -n +2 | sed 's/^/  /'
  exit 0
fi
git tag -a "$TAG" -m "OnKey $V"
"${PUSH[@]}"
"${CREATE[@]}"
if [ $STAGE = 1 ]; then
  echo "draft release $TAG made: https://github.com/$REPO/releases/tag/$TAG (not published: no latest.json yet)"
  # the Windows and Linux builds of exactly this commit, on the development repo's CI (its workflows at $CI_REF)
  "${CI_WIN[@]}"
  "${CI_LINUX[@]}"
  echo
  echo "staged $TAG at $HEADSHA. CI is building Windows and Linux from the public tag:"
  echo "  gh run list --repo $DEV_REPO --workflow windows.yml -L 1; gh run list --repo $DEV_REPO --workflow linux.yml -L 1"
  echo "  (gh run watch <id> --repo $DEV_REPO)"
  echo "When both are green (each rehearsed its update and uploaded to the draft): scripts/release-publish.sh $V"
  exit 0
fi
echo "published $TAG: https://github.com/$REPO/releases/tag/$TAG"
# what every OnKey will read within a day
sleep 5
curl -fsSL "https://github.com/$REPO/releases/latest/download/latest.json" | python3 -c 'import json,sys; print("latest.json now says", json.load(sys.stdin)["version"])'
