#!/bin/bash
# OnKey to give to someone: release build (no developer test features), signed with a Developer ID (hardened
# runtime, the app's entitlements, its provisioning profile embedded so ShazamKit works), notarised by Apple and
# stapled, then packed with an Applications link into a signed, notarised, stapled OnKey.dmg. It opens on any Mac
# with no security warning.
#   scripts/make-dmg.sh [out.dmg]      (default ~/Desktop/OnKey.dmg)
# Settings (environment), with Daniel's defaults:
#   ONKEY_SIGN_IDENTITY   the Developer ID Application certificate in the keychain
#   ONKEY_PROFILE         the Developer ID provisioning profile for audio.onkey.app
#   ONKEY_NOTARY_PROFILE  the notarytool keychain profile (xcrun notarytool store-credentials …)
#   ONKEY_ADHOC=1         no Developer ID: sign ad hoc and skip notarisation (macOS then warns on other Macs)
# Every build is checked before it's signed (scripts/check-release.sh): no development-only code, and the
# learning models it ships inside the app (~380 MB) all there and matching their SHA-256s.
# The finished DMG is checked too (scripts/check-dmg.sh): only the expected files, no stray data files, build
# paths or keys, every web host listed in scripts/release-hosts.txt, the expected entitlements, notarised.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=${1:-$HOME/Desktop/OnKey.dmg}
IDENTITY=${ONKEY_SIGN_IDENTITY:-"Developer ID Application: Daniel Murrell (CCLWH7MCUY)"}
PROFILE=${ONKEY_PROFILE:-"$HOME/Documents/Apple Developer/OnKey_Developer_ID.provisionprofile"}
NOTARY=${ONKEY_NOTARY_PROFILE:-onkey-notary}
BUNDLE_ID=audio.onkey.app
export CARGO_TARGET_DIR="$PWD/src-tauri/target-dist"

# submit to Apple's notary service and wait; on anything but Accepted, show Apple's log and stop
notarise() {
  local out id try
  out=$(xcrun notarytool submit "$1" --keychain-profile "$NOTARY" --wait 2>&1) || true
  echo "$out"
  id=$(sed -nE 's/^ *id: ([0-9a-f-]{36}).*/\1/p' <<< "$out" | head -1)
  [ -n "$id" ] || { echo "notarisation of $1 wasn't submitted" >&2; exit 1; }
  # the wait can drop on a slow connection while Apple carries on: keep waiting on the submission itself
  for try in 1 2 3 4 5; do
    grep -q "status: \(Accepted\|Invalid\|Rejected\)" <<< "$out" && break
    sleep 20
    out=$(xcrun notarytool wait "$id" --keychain-profile "$NOTARY" --timeout 30m 2>&1) || true
    echo "$out"
  done
  if ! grep -q "status: Accepted" <<< "$out"; then
    xcrun notarytool log "$id" --keychain-profile "$NOTARY" || true
    echo "notarisation of $1 failed (submission $id)" >&2; exit 1
  fi
}

# ONKEY_SKIP_BUILD=1: sign and package the app already built (e.g. after a dropped notarisation)
# Tauri's own clean-up step runs `xattr -crs`, which fails if a Python xattr shadows macOS's: make sure it's /usr/bin's
APP="$CARGO_TARGET_DIR/release/bundle/macos/OnKey.app"
BIN="$APP/Contents/MacOS/onkey"
if [ "${ONKEY_SKIP_BUILD:-0}" = 1 ]; then
  [ -x "$BIN" ] || { echo "no app was built" >&2; exit 1; }
  echo "ONKEY_SKIP_BUILD=1: packaging the app built $(stat -f %Sm "$APP")"
else
  # A failed build stops here. The last build's OnKey.app is removed first and the new one must be newer than
  # the build's start, so a stale (already notarised) binary can never be signed and shipped again.
  mkdir -p "$CARGO_TARGET_DIR"
  STAMP="$CARGO_TARGET_DIR/.build-started"
  rm -rf "$APP"; touch "$STAMP"
  # the key challenge links are signed with (src-tauri/src/challenge.rs), if this Mac has it: without it the build's
  # links are unsigned and singonkey.app shows their scores as not checked
  if [ -z "${ONKEY_CHALLENGE_KEY:-}" ] && [ -f "$HOME/.tauri/onkey-challenge.key" ]; then
    export ONKEY_CHALLENGE_KEY="$(cat "$HOME/.tauri/onkey-challenge.key")"
  fi
  [ -n "${ONKEY_CHALLENGE_KEY:-}" ] || echo "note: no challenge key (~/.tauri/onkey-challenge.key): this build's challenge links will be unsigned" >&2
  status=0
  # the build machine's folders (home, the checkout) out of the binary's panic and error paths
  export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$PWD=onkey --remap-path-prefix=$HOME=~"
  PATH="/usr/bin:$PATH" npx tauri build --bundles app || status=$?
  [ "$status" = 0 ] || { echo "tauri build failed (exit $status): stopping, nothing signed" >&2; exit 1; }
  [ -x "$BIN" ] || { echo "no app was built" >&2; exit 1; }
  [ -n "$(find "$APP" -maxdepth 0 -newer "$STAMP")" ] || { echo "$APP is older than this build: stale, stopping" >&2; exit 1; }
fi
echo "app binary sha256 $(shasum -a 256 "$BIN" | cut -c1-16)…, bundled $(stat -f %Sm "$APP")"
scripts/check-release.sh "$APP"
/usr/bin/xattr -cr "$APP"

if [ "${ONKEY_ADHOC:-0}" = 1 ]; then
  codesign --force --deep --sign - "$APP"
  codesign --verify --deep --strict "$APP"
else
  TEAM=$(sed -E 's/.*\(([A-Z0-9]{10})\)$/\1/' <<< "$IDENTITY")
  [ -f "$PROFILE" ] || { echo "no provisioning profile at $PROFILE (set ONKEY_PROFILE, or ONKEY_ADHOC=1)" >&2; exit 1; }
  # the profile must be for this app and team, or ShazamKit refuses the app at run time
  security cms -D -i "$PROFILE" > "$CARGO_TARGET_DIR/profile.plist"
  [ "$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$CARGO_TARGET_DIR/profile.plist")" = "$TEAM.$BUNDLE_ID" ] \
    || { echo "the provisioning profile isn't for $TEAM.$BUNDLE_ID" >&2; exit 1; }
  cp "$PROFILE" "$APP/Contents/embedded.provisionprofile"
  # the app's entitlements plus its identity (what the embedded profile allows), so app services such as ShazamKit
  # recognise it
  ENT="$CARGO_TARGET_DIR/entitlements.plist"
  cp src-tauri/Entitlements.plist "$ENT"
  /usr/libexec/PlistBuddy -c "Add :com.apple.application-identifier string $TEAM.$BUNDLE_ID" \
                          -c "Add :com.apple.developer.team-identifier string $TEAM" "$ENT"
  # inside out: any nested code first (none today: ONNX Runtime is linked in statically), then the app
  find "$APP/Contents" -type f -not -path "$APP/Contents/MacOS/onkey" -not -name "*.onnx" -print0 \
    | while IFS= read -r -d '' f; do
        if file -b "$f" | grep -q "Mach-O"; then
          codesign --force --timestamp --options runtime --sign "$IDENTITY" "$f"
        fi
      done
  codesign --force --timestamp --options runtime --entitlements "$ENT" --sign "$IDENTITY" "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"

  # notarise and staple the app itself (so a copy dragged out of the DMG carries its ticket)
  ZIP="$CARGO_TARGET_DIR/OnKey.zip"
  rm -f "$ZIP"; /usr/bin/ditto -c -k --keepParent "$APP" "$ZIP"
  notarise "$ZIP"
  xcrun stapler staple "$APP"
  rm -f "$ZIP"
fi

# the install window: OnKey, an arrow, Applications (scripts/pack-dmg.sh)
scripts/pack-dmg.sh "$APP" "$OUT"

if [ "${ONKEY_ADHOC:-0}" != 1 ]; then
  codesign --force --timestamp --sign "$IDENTITY" "$OUT"
  notarise "$OUT"
  xcrun stapler staple "$OUT"
  xcrun stapler validate "$OUT"
  spctl -a -vvv -t open --context context:primary-signature "$OUT"
fi
# what's inside the finished DMG (files, hosts, paths, entitlements, notarisation): see scripts/check-dmg.sh
scripts/check-dmg.sh "$OUT"
echo "made $OUT ($(du -h "$OUT" | cut -f1))"
