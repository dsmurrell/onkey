#!/bin/bash
# The update rehearsal (scripts/release.sh runs it before publishing): the exact app about to ship must update itself.
# A copy of it is pointed at a fake next version served on this Mac (http://127.0.0.1, which the app accepts only
# for this: updater.rs rehearsal_url) and must download it, check its signature against the key in tauri.conf.json
# and install it over itself. A second copy is offered one signed with a different key, and must refuse it. The app
# must also be an official build (updater.rs OFFICIAL: ONKEY_OFFICIAL=1 when built), the only kind that installs
# updates. The fake next version is this app with its version raised in Info.plist, packed and signed as a release's.
#   scripts/update-rehearsal.sh <OnKey.app>
# Environment: ONKEY_UPDATER_KEY (default ~/.tauri/onkey-updater.key), ONKEY_UPDATER_KEY_PASSWORD (default: the
# login Keychain, service "onkey-updater-key"). Exit 0: it updates itself and refuses a wrongly signed update.
set -euo pipefail
cd "$(dirname "$0")/.."
APP=${1:?usage: scripts/update-rehearsal.sh <OnKey.app>}
KEY=${ONKEY_UPDATER_KEY:-$HOME/.tauri/onkey-updater.key}
PW=${ONKEY_UPDATER_KEY_PASSWORD:-$(security find-generic-password -a onkey -s onkey-updater-key -w 2>/dev/null || true)}
die() { echo "update-rehearsal: $*" >&2; exit 1; }
[ -f "$KEY" ] && [ -n "$PW" ] || die "no updater key or password"
appv() { plutil -extract CFBundleShortVersionString raw "$1/Contents/Info.plist"; }
REH="$(cd "$(mktemp -d)" && pwd -P)"     # its real path: /var is a symlink, and Tauri won't run from behind one
cleanup_rehearsal() { [ -n "${REH_PID:-}" ] && { kill "$REH_PID" 2>/dev/null; wait "$REH_PID" 2>/dev/null; }; rm -rf "$REH"; }
trap cleanup_rehearsal EXIT
APPV=$(appv "$APP")
IFS=. read -r MA MI PA <<< "$APPV"
NEXT="$MA.$MI.$((PA + 1))"
mkdir -p "$REH/next" "$REH/serve" "$REH/good" "$REH/bad"
cp -R "$APP" "$REH/next/OnKey.app"
plutil -replace CFBundleShortVersionString -string "$NEXT" "$REH/next/OnKey.app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$NEXT" "$REH/next/OnKey.app/Contents/Info.plist"
COPYFILE_DISABLE=1 tar -czf "$REH/serve/next.app.tar.gz" -C "$REH/next" OnKey.app
cp "$REH/serve/next.app.tar.gz" "$REH/serve/next-badkey.app.tar.gz"
npx tauri signer sign -f "$KEY" -p "$PW" "$REH/serve/next.app.tar.gz" > /dev/null
npx tauri signer generate --ci -p "" -w "$REH/bad.key" > /dev/null 2>&1
npx tauri signer sign -f "$REH/bad.key" -p "" "$REH/serve/next-badkey.app.tar.gz" > /dev/null
PORT=$((47400 + RANDOM % 500))
for f in next next-badkey; do
  python3 - "$REH/serve" "$f" "$NEXT" "$PORT" <<'EOF2'
import json, sys
d, f, v, port = sys.argv[1:]
sig = open(f"{d}/{f}.app.tar.gz.sig").read().strip()
json.dump({"version": v, "notes": "rehearsal", "pub_date": "2026-01-01T00:00:00Z",
           "platforms": {"darwin-aarch64": {"signature": sig, "url": f"http://127.0.0.1:{port}/{f}.app.tar.gz"}}},
          open(f"{d}/{f}.json", "w"))
EOF2
done
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$REH/serve" > /dev/null 2>&1 &
REH_PID=$!
for _ in $(seq 1 50); do curl -sf "http://127.0.0.1:$PORT/next.json" > /dev/null && break; sleep 0.1; done
rehearse() {   # rehearse <copy> <manifest>: the copy's own updater, at most two minutes
  cp -R "$APP" "$1/OnKey.app"
  ONKEY_UPDATE_REHEARSAL="http://127.0.0.1:$PORT/$2.json" perl -e 'alarm 120; exec @ARGV' "$1/OnKey.app/Contents/MacOS/onkey" 2>/dev/null | grep "^update-rehearsal:" || true
}
OUT=$(rehearse "$REH/good" next)
echo "  $OUT"
[[ "$OUT" == "update-rehearsal: ok, installed $NEXT (official true)" ]] || die "the update rehearsal failed: this app couldn't update itself (or isn't an official build)"
[ "$(appv "$REH/good/OnKey.app")" = "$NEXT" ] || die "the update rehearsal said ok, but the app on disk isn't $NEXT"
OUT=$(rehearse "$REH/bad" next-badkey)
echo "  $OUT (an update signed with another key)"
[[ "$OUT" == "update-rehearsal: failed:"* ]] || die "the update rehearsal installed an update signed with a key OnKey shouldn't trust"
[ "$(appv "$REH/bad/OnKey.app")" = "$APPV" ] || die "an update signed with another key changed the app"
echo "  ok: $APPV updates itself to a signed $NEXT, and refuses one signed with another key"
kill "$REH_PID" 2>/dev/null; wait "$REH_PID" 2>/dev/null || true; REH_PID=
trap - EXIT
rm -rf "$REH"

