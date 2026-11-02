#!/bin/bash
# Which updater key a CI build signs its updater files with (.github/workflows/windows.yml and linux.yml, "The
# updater key"), set for the steps after it through $GITHUB_ENV (docs/RELEASING.md "Windows and Linux"):
#   release    the real key, from the repo's secrets TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD
#              (the build step reads them straight from `secrets`; this step only checks they're there). An official
#              build (ONKEY_OFFICIAL=1: the only kind that installs updates), only ever from the public tag.
#   throwaway  a key made here and gone with the job: the app is built to trust its public key (PUBKEY_OVERRIDE, a
#              --config override of plugins.updater.pubkey), so the rehearsal can run without the real key. Not an
#              official build, and never uploaded anywhere: ci-verify-sig.sh proves its files don't pass as a release.
# Nothing here prints a key. Environment: UPDATER, HAVE_KEY ("true" when both secrets are set), PUBLIC_TAG.
set -euo pipefail
die() { echo "::error::updater key: $*" >&2; exit 1; }
ENV_OUT=${GITHUB_ENV:-/dev/stdout}
case "${UPDATER:-none}" in
  release)
    [ -n "${PUBLIC_TAG:-}" ] || die "a release build is made from the public tag only (public_tag is empty)"
    [ "${HAVE_KEY:-}" = true ] || die "the secrets TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD aren't both set on this repo (docs/RELEASING.md: One-time setup)"
    {
      echo "UPDATER_MODE=release"
      echo "UPDATER_OFFICIAL=true"
      echo "ONKEY_OFFICIAL=1"
    } >> "$ENV_OUT"
    echo "release: signing with the updater key from the secrets; an official build"
    ;;
  throwaway)
    KEYDIR=$(mktemp -d "${RUNNER_TEMP:-/tmp}/throwaway-key.XXXXXX")
    npx tauri signer generate --ci -p "" -w "$KEYDIR/throwaway.key" > /dev/null 2>&1
    [ -s "$KEYDIR/throwaway.key" ] && [ -s "$KEYDIR/throwaway.key.pub" ] || die "couldn't make a throwaway key"
    {
      echo "UPDATER_MODE=throwaway"
      echo "UPDATER_OFFICIAL=false"
      echo "THROWAWAY_KEY=$KEYDIR/throwaway.key"
      echo "PUBKEY_OVERRIDE=$(tr -d '\r\n' < "$KEYDIR/throwaway.key.pub")"
    } >> "$ENV_OUT"
    echo "throwaway: a key made for this job; the app is built to trust it, not official, uploaded nowhere"
    ;;
  none|"") echo "no updater files" ;;
  *) die "unknown updater mode '$UPDATER'" ;;
esac
