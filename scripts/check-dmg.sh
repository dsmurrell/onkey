#!/bin/bash
# What's inside a finished OnKey download: run on the notarised OnKey.dmg (or a signed OnKey.app) before anyone gets
# it. make-dmg.sh runs it on every DMG it makes; anyone can run it on a DMG they downloaded, from a checkout of the
# same version. It checks:
#   - the disk image holds OnKey.app, an Applications link and its install window's layout, nothing else
#   - the app holds only the files a release should (binary, Info.plist, icon, notices, models.rs's models, the
#     demo song's files that git tracks in src-tauri/demo/, each language's permission prompts (InfoPlist.strings
#     for every src-tauri/messages/<lang>.json), signature, ticket, provisioning profile)
#   - scripts/check-release.sh: no development-only code, every model with its expected SHA-256 (and the version)
#   - no data files (recordings, event logs, lyrics) built into the window's pages
#   - no paths from the build machine, no private keys or tokens
#   - every web address inside is in scripts/release-hosts.txt (the hosts OnKey contacts, links it opens, and
#     library text), in the binary and in the window's pages (ui/out, when the check runs where the build did)
#   - signed with a Developer ID, hardened runtime, exactly the expected entitlements, notarised and stapled
#     (the app and the DMG), and macOS's Gatekeeper accepts it
#   scripts/check-dmg.sh <OnKey.dmg | OnKey.app> [expected version]
# Environment: ONKEY_ADHOC=1 for an ad hoc build (make-dmg.sh's): signature, team and notarisation not checked.
#   ONKEY_TEAM: the Developer ID team the release is signed by (default: the one make-dmg.sh signs with).
set -euo pipefail
TARGET=${1:?usage: scripts/check-dmg.sh <OnKey.dmg | OnKey.app> [version]}
WANT_VERSION=${2:-}
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEAM=${ONKEY_TEAM:-CCLWH7MCUY}
ADHOC=${ONKEY_ADHOC:-0}
BUNDLE_ID=audio.onkey.app
fail=0
bad() { echo "check-dmg: $*" >&2; fail=1; }
TMP=$(mktemp -d); MNT=""
cleanup() { [ -n "$MNT" ] && hdiutil detach -quiet "$MNT" 2>/dev/null; rm -rf "$TMP"; }
trap cleanup EXIT

# --- the disk image
if [[ "$TARGET" == *.dmg ]]; then
  [ -f "$TARGET" ] || { echo "check-dmg: no file at $TARGET" >&2; exit 1; }
  MNT="$TMP/mnt"; mkdir "$MNT"
  hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$MNT" "$TARGET" > /dev/null
  while IFS= read -r e; do
    case "$e" in
      OnKey.app|.fseventsd|.Trashes) ;;
      .DS_Store) ;;                          # the install window's layout (pack-dmg.sh)
      .background)
        [ "$(ls -A "$MNT/.background")" = background.tiff ] || bad "the DMG's .background holds more than background.tiff" ;;
      Applications) [ "$(readlink "$MNT/Applications")" = /Applications ] || bad "the DMG's Applications isn't a link to /Applications" ;;
      *) bad "unexpected item in the DMG: $e" ;;
    esac
  done < <(ls -A "$MNT")
  [ -L "$MNT/Applications" ] || bad "the DMG has no Applications link"
  [ -f "$MNT/.DS_Store" ] && [ -f "$MNT/.background/background.tiff" ] \
    || bad "the DMG has no install window layout (OnKey, an arrow, Applications): made without pack-dmg.sh?"
  APP="$MNT/OnKey.app"
  if [ "$ADHOC" != 1 ]; then
    codesign --verify --strict "$TARGET" 2>/dev/null || bad "the DMG isn't validly signed"
    # (read whole first: grep -q stopping early made codesign fail on a closed pipe, which pipefail counted)
    dmg_info=$(codesign -dv "$TARGET" 2>&1)
    grep -q "^TeamIdentifier=$TEAM$" <<< "$dmg_info" || bad "the DMG isn't signed by team $TEAM"
    xcrun stapler validate -q "$TARGET" 2>/dev/null || bad "the DMG has no stapled notarisation ticket"
    spctl -a -t open --context context:primary-signature "$TARGET" 2>/dev/null || bad "Gatekeeper rejects the DMG"
  fi
else
  APP="$TARGET"
fi
BIN="$APP/Contents/MacOS/onkey"
[ -x "$BIN" ] || { echo "check-dmg: no app binary at $BIN" >&2; exit 1; }

# --- the app's files: exactly these, plus models.rs's models
MODELS=$(perl -0ne 'while (/file: "([^"]+)",\s*sha256: "[0-9a-f]{64}"/g) { print "Contents/Resources/models/$1\n" }' "$ROOT/src-tauri/src/models.rs")
# the demo song (src-tauri/demo/, Resources/demo/): only files git tracks there, so a stand-in recording left in the
# folder (a placeholder used in development, never ours to give away) fails the check rather than shipping
if git -C "$ROOT" rev-parse --git-dir > /dev/null 2>&1; then
  DEMO=$(git -C "$ROOT" ls-files -- src-tauri/demo | sed 's|^src-tauri/demo/|Contents/Resources/demo/|')
else
  DEMO=$(cd "$ROOT/src-tauri" && ls demo 2>/dev/null | sed 's|^|Contents/Resources/demo/|')
fi
# each language's permission prompts (build.rs writes them from src-tauri/messages/<lang>.json, under the Mac's
# name for the language: build.rs lproj_name, es-419 for our es and es for our es-ES)
lproj_name() { case "$1" in es) echo es-419;; es-ES) echo es;; *) echo "$1";; esac; }
LPROJ=$(cd "$ROOT/src-tauri/messages" && for f in [a-z]*.json; do echo "Contents/Resources/$(lproj_name "${f%.json}").lproj/InfoPlist.strings"; done)
EXPECTED="Contents/Info.plist
Contents/MacOS/onkey
Contents/Resources/icon.icns
Contents/Resources/THIRD_PARTY_NOTICES.md
$MODELS
$DEMO
$LPROJ"
SIGNED="Contents/_CodeSignature/CodeResources
Contents/CodeResources
Contents/embedded.provisionprofile"
while IFS= read -r f; do
  grep -qxF -- "$f" <<< "$EXPECTED" && continue
  grep -qxF -- "$f" <<< "$SIGNED" && continue
  bad "unexpected file in the app: $f"
done < <(cd "$APP" && find . \( -type f -o -type l \) | sed 's|^\./||' | sort)
while IFS= read -r f; do
  [ -e "$APP/$f" ] || bad "missing from the app: $f"
done <<< "$EXPECTED"
if [ "$ADHOC" != 1 ]; then
  while IFS= read -r f; do [ -e "$APP/$f" ] || bad "missing from the signed app: $f"; done <<< "$SIGNED"
fi
while IFS= read -r f; do
  [ -f "$APP/$f" ] && { plutil -lint -s "$APP/$f" > /dev/null || bad "$f doesn't read as strings"; }
done <<< "$LPROJ"
langs=$(cd "$ROOT/src-tauri/messages" && for f in [a-z]*.json; do lproj_name "${f%.json}"; done | LC_ALL=C sort | tr '\n' ' ')
got=$(/usr/bin/python3 -I -c 'import plistlib,sys; print(" ".join(sorted(plistlib.load(open(sys.argv[1],"rb")).get("CFBundleLocalizations", []))) + " ")' "$APP/Contents/Info.plist")
[ "$got" = "$langs" ] || bad "Info.plist's CFBundleLocalizations is '$got', expected '$langs'"

# --- development-only code, models, version
"$ROOT/scripts/check-release.sh" "$APP" $WANT_VERSION > /dev/null || bad "check-release.sh failed (above)"
got=$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$APP/Contents/Info.plist")
[ "$got" = "$BUNDLE_ID" ] || bad "bundle identifier is $got, expected $BUNDLE_ID"

# --- what's built into the binary
STR="$TMP/strings"; strings -a "$BIN" > "$STR"
# the window's pages are built in: their file names are kept as text. Only pages, scripts, styles, fonts and
# images belong there; a data file (an event log, recording or lyrics) means the build picked up a stray file
# (a name can run into the bytes around it in the binary, so match it anywhere in a line)
# Not pages: web addresses (//host/…) and the files OnKey keeps in its own data folder, named in its code
APP_DATA_FILES='^/takesmeta\.json$'
if grep -oE '/[A-Za-z0-9_./@-]+\.(ndjson|jsonl|json|lrc|wav|m4a|mp3|f32|csv|txt)([^A-Za-z0-9]|$)' "$STR" \
     | sed -E 's/[^A-Za-z0-9]$//' | grep -vE "^//|^/_next/|^/[a-z0-9-]+\.txt$|$APP_DATA_FILES" | sort -u > "$TMP/assets" && [ -s "$TMP/assets" ]; then
  while IFS= read -r a; do bad "data file built into the window's pages: $a"; done < "$TMP/assets"
fi
# the build machine's folders and account (make-dmg.sh maps them out of the binary). Third-party libraries built
# elsewhere keep theirs: ONNX Runtime's, built on GitHub's machines (user runner)
home_re=$(printf '%s' "$HOME" | sed 's/[.[\*^$/]/\\&/g')
if grep -E "$home_re/|/U[s]ers/[A-Za-z0-9._-]+/|/h[o]me/[A-Za-z0-9._-]+/|/p[r]ivate/tmp/|/v[a]r/folders/" "$STR" \
     | grep -vE '/U[s]ers/runner/' | head -3 > "$TMP/paths" && [ -s "$TMP/paths" ]; then
  while IFS= read -r p; do bad "build-machine path in the binary: ${p:0:120}"; done < "$TMP/paths"
fi
if [ -n "${USER:-}" ] && [ ${#USER} -ge 4 ] && grep -qiw -- "$USER" "$STR"; then
  bad "the build account's name ($USER) is in the binary"
fi
# (written with [x] classes so this script doesn't itself look like a key to a scanner)
if grep -E 'BEGIN [A-Z ]*PRIVAT[E] KEY|untruste[d] comment: [a-z]+ (encrypted )?secre[t] key|g[h][pousr]_[A-Za-z0-9]{30,}|githu[b]_pat_|s[k]-ant-|A[K]IA[0-9A-Z]{16}|A[I]za[0-9A-Za-z_-]{35}' "$STR" \
     | head -3 > "$TMP/keys" && [ -s "$TMP/keys" ]; then
  bad "something like a private key or token is in the binary"
fi

# --- web addresses: every host must be in release-hosts.txt. Real hosts only: lower case, a real top-level
# domain (a compression library's word list reads like "http://www.style" but isn't an address)
HOSTS="$ROOT/scripts/release-hosts.txt"
TLDS="com|net|org|io|rs|dev|app|ai|co|uk|de|eu|fr|me|info|edu|gov|us|ca|au|jp|tv|fm|ly|gl|gg|sh|cc|xyz|so|page|site|cloud|ms|be|nl|ch|se"
hosts_in() { grep -rhoE 'https?://[a-z0-9.-]+' "$@" 2>/dev/null | sed -E 's|^https?://||' | grep -E "\.($TLDS)$" | sort -u; }
listed() {
  local h=$1 pat
  while read -r pat _; do
    case "$pat" in ''|\#*) continue ;; esac
    [ "$h" = "$pat" ] && return 0
    [[ "$pat" == \*.* && "$h" == *"${pat#\*}" ]] && return 0
  done < "$HOSTS"
  return 1
}
nhosts=0
check_hosts() {   # check_hosts <where> <files…>
  local where=$1; shift
  while IFS= read -r h; do
    [ -n "$h" ] || continue
    nhosts=$((nhosts + 1))
    listed "$h" || bad "web address not in release-hosts.txt, in the $where: $h"
  done < <(hosts_in "$@")
}
check_hosts binary "$STR"
# the window's pages are stored compressed in the binary, so read them where they were built (the same checkout)
UI="$ROOT/ui/out"
if [ -d "$UI" ]; then
  check_hosts "window's pages (ui/out)" "$UI"
  ui_note=", window's pages from ui/out"
else
  ui_note=" (no ui/out here: window's pages not read)"
fi

# --- signature, entitlements, notarisation
if [ "$ADHOC" != 1 ]; then
  codesign --verify --deep --strict "$APP" 2>/dev/null || bad "the app's signature doesn't verify"
  info=$(codesign -dv "$APP" 2>&1)
  grep -q "^TeamIdentifier=$TEAM$" <<< "$info" || bad "the app isn't signed by team $TEAM"
  grep -q "^Identifier=$BUNDLE_ID$" <<< "$info" || bad "the app's signing identifier isn't $BUNDLE_ID"
  grep -qE "flags=0x[0-9a-f]*\(.*runtime" <<< "$info" || bad "the app isn't signed with the hardened runtime"
  codesign -d --entitlements - --xml "$APP" 2>/dev/null > "$TMP/ent.plist" || true
  /usr/bin/python3 -I - "$TMP/ent.plist" "$ROOT/src-tauri/Entitlements.plist" "$TEAM" "$BUNDLE_ID" <<'PY' || fail=1
import plistlib, sys
got_path, want_path, team, bundle = sys.argv[1:]
try:
    got = plistlib.load(open(got_path, "rb"))
except Exception:
    got = {}
want = dict(plistlib.load(open(want_path, "rb")))
want["com.apple.application-identifier"] = f"{team}.{bundle}"
want["com.apple.developer.team-identifier"] = team
for k in sorted(set(got) | set(want)):
    if k not in want:
        print(f"check-dmg: unexpected entitlement: {k} = {got[k]!r}", file=sys.stderr); bad = True
    elif k not in got:
        print(f"check-dmg: missing entitlement: {k}", file=sys.stderr); bad = True
    elif got[k] != want[k]:
        print(f"check-dmg: entitlement {k} is {got[k]!r}, expected {want[k]!r}", file=sys.stderr); bad = True
sys.exit(1 if "bad" in dir() else 0)
PY
  xcrun stapler validate -q "$APP" 2>/dev/null || bad "the app has no stapled notarisation ticket"
  spctl -a -t exec "$APP" 2>/dev/null || bad "Gatekeeper rejects the app"
fi

[ $fail = 0 ] || { echo "check-dmg: $TARGET is not fit to give to anyone" >&2; exit 1; }
echo "check-dmg: ok ($TARGET: only the expected files, no data files, paths or keys, $nhosts web hosts all listed${ui_note}$([ "$ADHOC" = 1 ] && echo ', ad hoc: signature not checked' || echo ', signed, notarised and stapled')${WANT_VERSION:+, version $WANT_VERSION})"
