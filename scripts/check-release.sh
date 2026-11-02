#!/bin/bash
# Release hygiene: fails if a build meant for people has development-only parts in it. Release builds are made
# without the `devtest` feature, which compiles out the test hooks (src-tauri/src/devtest.rs: simulated singer
# and player, event log, kept audio) and the live mirror (mirror.rs); this checks the result, not the intention:
#   - none of their environment variables, log lines or mirror routes are in the app's binary, nor any experiment's
#     knob (an ONKEY_* variable read through crate::devtest::knob!, which release builds compile out)
#   - no symbols from those modules (when the binary isn't stripped)
#   - not signed with the debugging entitlement (get-task-allow), when signed
#   - the version in Info.plist, if one is given
#   - every learning model the app needs is inside it (Contents/Resources/models) with the SHA-256 that
#     src-tauri/src/models.rs expects: the app downloads none, so a build without them could never learn a song
# make-dmg.sh runs it on every build before signing; scripts/release.sh again on what it ships.
#   scripts/check-release.sh <OnKey.app> [expected version]
set -euo pipefail
APP=${1:?usage: scripts/check-release.sh <OnKey.app> [version]}
WANT_VERSION=${2:-}
BIN="$APP/Contents/MacOS/onkey"
[ -x "$BIN" ] || { echo "check-release: no app binary at $BIN" >&2; exit 1; }
fail=0
bad() { echo "check-release: $*" >&2; fail=1; }

# strings only the development-only code has (devtest.rs, mirror.rs, ear.rs and native/ear.swift)
FORBIDDEN=(
  ONKEY_SIM ONKEY_EVENTS_LOG ONKEY_MIRROR ONKEY_KEEP_AUDIO
  "simulated singer" "simulated player" "kept audio for lyric tests" "kept mic for echo tests"
  "__ONKEY_MIRROR__" "event: mirror" "/api/probe" "/dev/along" "/api/ear"
  "onkey_ear_start" "ear summary" "ear: listening to take"
)
STR=$(mktemp); trap 'rm -f "$STR"' EXIT
strings -a "$BIN" > "$STR"
for s in "${FORBIDDEN[@]}"; do
  if grep -qF -- "$s" "$STR"; then bad "development-only string in the release binary: \"$s\""; fi
done
# experiment knobs: environment variables the code reads only in development builds (crate::devtest::knob!), and any
# new one that skipped it. Only these may be in a release, each safe for anyone to set:
#   ONKEY_NO_SHAZAM           sound recognition's network use off (the lab's benchmarks run the app without Shazam)
#   ONKEY_UPDATE_REHEARSAL    the release's update rehearsal (updater.rs): the app updates from http://127.0.0.1 only,
#                             the signature still checked, then exits; whoever sets it controls the machine anyway
#   ONKEY_REPORT_TO           CI's launch checks: a problem report (logs and numbers, no lyrics or audio) saved to
#                             that path 30 s after launch, as "Report a problem" would
# (A name can run on into the next string in the binary, "ONKEY_NO_SHAZAMS...": allowed names match as prefixes.)
ALLOWED_VARS=(ONKEY_NO_SHAZAM ONKEY_UPDATE_REHEARSAL ONKEY_REPORT_TO)
while read -r v; do
  [ -n "$v" ] || continue
  ok=0; for a in "${ALLOWED_VARS[@]}"; do [[ "$v" == "$a"* ]] && ok=1; done
  [ $ok = 1 ] || bad "experiment knob in the release binary: $v (read it with crate::devtest::knob!)"
done < <(grep -oE '(ONKEY_[A-Z][A-Z0-9_]*|TRACKER_TRACE|SIM_ERR[A-Z_]*)' "$STR" | sort -u)
if nm "$BIN" 2>/dev/null | grep -E "onkey_lib(::|[0-9]+)(devtest|mirror|ear)" | grep -vq "stub"; then
  bad "development-only symbols (devtest/mirror/ear) in the release binary"
fi
if codesign -d "$APP" >/dev/null 2>&1; then
  if codesign -d --entitlements - "$APP" 2>/dev/null | grep -q "get-task-allow"; then
    bad "signed with the debugging entitlement (get-task-allow)"
  fi
fi
if [ -n "$WANT_VERSION" ]; then
  got=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP/Contents/Info.plist")
  [ "$got" = "$WANT_VERSION" ] || bad "Info.plist says version $got, expected $WANT_VERSION"
fi
# the learning models, as models.rs lists them: <file> <sha256> <bytes>
MODELS_RS="$(dirname "$0")/../src-tauri/src/models.rs"
want=$(perl -0ne 'while (/file: "([^"]+)",\s*sha256: "([0-9a-f]{64})", bytes: ([0-9_]+)/g) { my ($f, $h, $b) = ($1, $2, $3); $b =~ s/_//g; print "$f $h $b\n" }' "$MODELS_RS")
[ "$(grep -c . <<< "$want")" -ge 8 ] || bad "couldn't read the model list from $MODELS_RS"
nmodels=0
while read -r file sha bytes; do
  [ -n "$file" ] || continue
  f="$APP/Contents/Resources/models/$file"
  if [ ! -f "$f" ]; then bad "learning model missing from the app: models/$file"; continue; fi
  got=$(shasum -a 256 "$f" | cut -d' ' -f1)
  [ "$got" = "$sha" ] || bad "models/$file has SHA-256 $got, but models.rs expects $sha"
  nmodels=$((nmodels + 1))
done <<< "$want"
[ $fail = 0 ] || { echo "check-release: $APP is not fit to ship" >&2; exit 1; }
echo "check-release: ok ($APP: no development-only code, $nmodels learning models checked${WANT_VERSION:+, version $WANT_VERSION})"
