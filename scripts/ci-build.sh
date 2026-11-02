#!/bin/bash
# The CI build of OnKey's installer or AppImage (.github/workflows/windows.yml and linux.yml), into dist/:
#   - `tauri build` (release, no devtest); with a version asked for (a preview), the app says that version;
#   - with an updater key (UPDATER_MODE, set by scripts/ci-updater-key.sh): the updater files too
#     (createUpdaterArtifacts: the installer or AppImage is signed, a .sig beside it), then a second build of the
#     same source with the version raised by one patch, into dist/next/, as the fake next version for the update
#     rehearsal (scripts/update-rehearsal-ci.sh, .ps1). A throwaway run also builds the app to trust its throwaway key
#     (PUBKEY_OVERRIDE) rather than the one in tauri.conf.json, which only the release's key matches.
# Environment: VERSION (optional), UPDATER_MODE (optional: throwaway|release), PUBKEY_OVERRIDE (throwaway only),
# BUNDLE_GLOB (where tauri build puts the file: src-tauri/target/release/bundle/nsis/*-setup.exe or
# .../appimage/*.AppImage), BUNDLES (optional: tauri build --bundles), and the signing key's TAURI_SIGNING_* variables.
set -euo pipefail
die() { echo "::error::build: $*" >&2; exit 1; }
[ -n "${BUNDLE_GLOB:-}" ] || die "BUNDLE_GLOB not set"
PY=$(command -v python3 || command -v python)
# the version the app says: tauri.conf.json's, or the one asked for
CONF_V=$($PY -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')
V=${VERSION:-$CONF_V}
[[ "$V" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "not a version: $V"
[ "${UPDATER_MODE:-}" != release ] || [ -z "${VERSION:-}" ] || die "a release is the source's version, not one asked for"

# the config overrides, one JSON object: no frontend build (the UI is built already), the version, and for updater
# files createUpdaterArtifacts (+ the throwaway key's public key)
config() {   # config <version>
  $PY - "$1" "${UPDATER_MODE:-}" "${PUBKEY_OVERRIDE:-}" <<'EOF'
import json, sys
v, mode, pub = sys.argv[1:]
cfg = {"version": v, "build": {"beforeBuildCommand": ""}}
if mode:
    cfg["bundle"] = {"createUpdaterArtifacts": True}
if mode == "throwaway":
    cfg["plugins"] = {"updater": {"pubkey": pub}}
print(json.dumps(cfg))
EOF
}
build() {   # build <version> <out dir>: tauri build, its file (+ .sig) moved to <out dir>
  local v=$1 out=$2 f
  rm -rf "src-tauri/target/release/bundle/nsis" "src-tauri/target/release/bundle/appimage"
  # shellcheck disable=SC2086
  npx tauri build --ci ${BUNDLES:+--bundles $BUNDLES} --config "$(config "$v")"
  # the AppImage: the build machine's Wayland libraries out (a blank window on newer systems), then signed again,
  # as the signature tauri build made was of the file before
  for f in $BUNDLE_GLOB; do
    case "$f" in *.AppImage) ;; *) continue ;; esac
    bash scripts/appimage-unbundle-wayland.sh "$f"
    if [ -n "${UPDATER_MODE:-}" ]; then
      rm -f "$f.sig"
      # the key and its password from the environment, as tauri build had them (a throwaway key's is empty: said
      # so, rather than asked for); never waiting on a prompt
      if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]; then
        npx tauri signer sign -p "" "$f" </dev/null
      else
        npx tauri signer sign "$f" </dev/null
      fi || die "couldn't sign $(basename "$f") again after taking the Wayland libraries out"
    fi
  done
  mkdir -p "$out"
  local n=0
  for f in $BUNDLE_GLOB; do
    [ -f "$f" ] || continue
    case "$f" in *.sig) continue ;; esac
    n=$((n + 1))
    [[ "$(basename "$f")" == *"_${v}_"* ]] || die "the built file $(basename "$f") doesn't say version $v"
    mv "$f" "$out/"
    if [ -n "${UPDATER_MODE:-}" ]; then
      [ -s "$f.sig" ] || die "no updater signature beside $(basename "$f") (createUpdaterArtifacts with the key should have made one)"
      mv "$f.sig" "$out/"
    fi
  done
  [ $n = 1 ] || die "expected one built file at $BUNDLE_GLOB, found $n"
}

if [ -n "${UPDATER_MODE:-}" ]; then
  [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] || die "no signing key in TAURI_SIGNING_PRIVATE_KEY"
  [ "$UPDATER_MODE" != throwaway ] || [ -n "${PUBKEY_OVERRIDE:-}" ] || die "a throwaway build needs its public key (PUBKEY_OVERRIDE)"
fi
echo "== build $V${UPDATER_MODE:+ (updater files: $UPDATER_MODE${ONKEY_OFFICIAL:+, official})}"
build "$V" dist
if [ -n "${UPDATER_MODE:-}" ]; then
  # the next version: one patch up (a prerelease label dropped, so it's newer by semver), the same source otherwise
  NEXT=$($PY -c 'import sys; v = sys.argv[1].split("-")[0].split("."); v[2] = str(int(v[2]) + 1); print(".".join(v))' "$V")
  echo "== build $NEXT (the rehearsal's next version)"
  build "$NEXT" dist/next
fi
echo "== built:"; ls -la dist; [ -d dist/next ] && ls -la dist/next || true   # (dist/next: only with updater files; a preview has none)
