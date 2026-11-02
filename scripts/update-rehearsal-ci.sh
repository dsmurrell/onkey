#!/bin/bash
# The update rehearsal for Linux (.github/workflows/linux.yml runs it on the CI runner that built the AppImage, inside
# a session D-Bus, before anything is uploaded to the release): the exact AppImage about to ship must update itself.
# It is the Linux counterpart of scripts/update-rehearsal.sh (the Mac's), with the same contract:
#   - a copy of the AppImage is pointed at a fake next version served on this machine (http://127.0.0.1, which the app
#     accepts only for this: updater.rs rehearsal_url) and must download it, check its signature against the key
#     built into it and replace its own file with it (an AppImage updates by rewriting its file in place);
#   - the replaced file must be the served one, byte for byte, still executable, and must open (its window loads);
#   - a second copy is offered the same next version signed with a different key, and must refuse it, unchanged;
#   - the app must say whether it's an official build (updater.rs OFFICIAL), and that must be what was expected
#     (true for a release; a test build with a throwaway key is not official).
# The fake next version is a second build of the same source with its version raised (the workflow makes it with
# `tauri build --config {"version": …}`), signed as the release's files are, so its .sig is next to it.
#   scripts/update-rehearsal-ci.sh <OnKey_X.Y.Z_amd64.AppImage> <OnKey_X.Y.Z+1_amd64.AppImage> <out dir> [official]
#     official: "true" (default) or "false": what the app must report
# Exit 0: it updates itself and refuses a wrongly signed update. What it saw goes to <out dir>.
set -euo pipefail
cd "$(dirname "$0")/.."
APP=${1:?usage: scripts/update-rehearsal-ci.sh <app.AppImage> <next.AppImage> <out dir> [official]}
NEXT=${2:?the AppImage of the next version}
OUT=${3:?a folder for what it finds}
OFFICIAL=${4:-true}
die() { echo "::error::update-rehearsal: $*" >&2; exit 1; }
[ -f "$APP" ] || die "no AppImage at $APP"
[ -f "$NEXT" ] && [ -s "$NEXT.sig" ] || die "no signed next version at $NEXT (+ .sig)"
mkdir -p "$OUT"
APP=$(readlink -f "$APP"); NEXT=$(readlink -f "$NEXT"); OUT=$(readlink -f "$OUT")
# the versions, from the file names (OnKey_0.2.0_amd64.AppImage), as latest.json must say them
ver() { basename "$1" | sed -E 's/^[^_]+_([^_]+)_.*$/\1/'; }
APPV=$(ver "$APP"); NEXTV=$(ver "$NEXT")
[ "$APPV" != "$NEXTV" ] || die "the next version has the same version as the app ($APPV)"
echo "update-rehearsal: $APPV -> $NEXTV (official $OFFICIAL expected)"

REH=$(mktemp -d /tmp/onkey-rehearsal.XXXXXX)      # /tmp: the updater keeps the old file beside its temp folder, on the same disk
REH_PID=
cleanup_rehearsal() {
  [ -n "$REH_PID" ] && { kill "$REH_PID" 2>/dev/null; wait "$REH_PID" 2>/dev/null; } || true
  pkill -KILL -f "$REH/" 2>/dev/null || true
  rm -rf "$REH"
}
trap cleanup_rehearsal EXIT
mkdir -p "$REH/serve" "$REH/good" "$REH/bad"
cp "$NEXT" "$REH/serve/next.AppImage"; cp "$NEXT.sig" "$REH/serve/next.AppImage.sig"
cp "$NEXT" "$REH/serve/next-badkey.AppImage"
# a key made here and thrown away: the app must refuse what it signs
npx tauri signer generate --ci -p "" -w "$REH/bad.key" > /dev/null 2>&1
npx tauri signer sign -f "$REH/bad.key" -p "" "$REH/serve/next-badkey.AppImage" > /dev/null
[ -s "$REH/serve/next-badkey.AppImage.sig" ] || die "couldn't sign with the throwaway key"
PORT=$((47400 + RANDOM % 500))
for f in next next-badkey; do
  python3 - "$REH/serve" "$f" "$NEXTV" "$PORT" <<'EOF'
import json, sys
d, f, v, port = sys.argv[1:]
sig = open(f"{d}/{f}.AppImage.sig").read().strip()
json.dump({"version": v, "notes": "rehearsal", "pub_date": "2026-01-01T00:00:00Z",
           "platforms": {"linux-x86_64": {"signature": sig, "url": f"http://127.0.0.1:{port}/{f}.AppImage"}}},
          open(f"{d}/{f}.json", "w"))
EOF
done
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$REH/serve" > "$OUT/http.log" 2>&1 &
REH_PID=$!
for _ in $(seq 1 50); do curl -sf "http://127.0.0.1:$PORT/next.json" > /dev/null && break; sleep 0.1; done
curl -sf "http://127.0.0.1:$PORT/next.json" > /dev/null || die "the local server didn't start"

# (CI has no FUSE mount for AppImages: they unpack themselves into /tmp and run from there; the runtime still tells
# the app where its AppImage is, APPIMAGE, which is the file the updater replaces)
export APPIMAGE_EXTRACT_AND_RUN=1
LOG=/tmp/onkey.log
sha() { sha256sum "$1" | cut -d' ' -f1; }
rehearse() {   # rehearse <copy dir> <manifest>: the copy's own updater, at most three minutes; its lines
  cp "$APP" "$1/OnKey.AppImage"; chmod +x "$1/OnKey.AppImage"
  rm -f "$LOG"
  ONKEY_UPDATE_REHEARSAL="http://127.0.0.1:$PORT/$2.json" timeout 180 xvfb-run -a -s "-screen 0 1280x800x24" \
    "$1/OnKey.AppImage" > "$OUT/$2-output.txt" 2>&1 || true
  cp "$LOG" "$OUT/$2-onkey.log" 2>/dev/null || true
  grep -h "^update-rehearsal:" "$OUT/$2-output.txt" "$OUT/$2-onkey.log" 2>/dev/null | sed -E 's/^.*(update-rehearsal:)/\1/' | sort -u || true
}

# the good one: the copy updates to the next version, which is then the served file, and opens
OUT1=$(rehearse "$REH/good" next)
echo "  $OUT1"
[[ "$OUT1" == "update-rehearsal: ok, installed $NEXTV (official $OFFICIAL)" ]] \
  || die "this AppImage couldn't update itself (or isn't official $OFFICIAL): see $OUT/next-output.txt"
[ "$(sha "$REH/good/OnKey.AppImage")" = "$(sha "$NEXT")" ] || die "the rehearsal said ok, but the file on disk isn't the next version"
[ -x "$REH/good/OnKey.AppImage" ] || die "the updated AppImage lost its executable permission"
rm -f "$LOG"
setsid xvfb-run -a -s "-screen 0 1280x800x24" "$REH/good/OnKey.AppImage" > "$OUT/updated-launch-output.txt" 2>&1 &
UPID=$!; disown "$UPID" 2>/dev/null || true     # (ended the hard way below: no job notice for that)
loaded=false
for _ in $(seq 1 45); do
  grep -q "ui: window loaded" "$LOG" 2>/dev/null && { loaded=true; break; }
  kill -0 "$UPID" 2>/dev/null || break
  sleep 2
done
cp "$LOG" "$OUT/updated-launch-onkey.log" 2>/dev/null || true
kill -KILL -- "-$UPID" 2>/dev/null || kill -KILL "$UPID" 2>/dev/null || true
$loaded || die "the updated AppImage ($NEXTV) didn't open (no 'ui: window loaded'): see $OUT/updated-launch-output.txt"
echo "  the updated AppImage opens ($NEXTV)"

# the bad one: signed with a key the app doesn't trust, refused, nothing changed
OUT2=$(rehearse "$REH/bad" next-badkey)
echo "  $OUT2 (an update signed with another key)"
[[ "$OUT2" == "update-rehearsal: failed: download or signature:"* ]] \
  || die "the app didn't refuse an update signed with a key it shouldn't trust: see $OUT/next-badkey-output.txt"
[ "$(sha "$REH/bad/OnKey.AppImage")" = "$(sha "$APP")" ] || die "an update signed with another key changed the app"
echo "update-rehearsal: ok: $APPV updates itself to a signed $NEXTV (official $OFFICIAL), which opens, and refuses one signed with another key"
