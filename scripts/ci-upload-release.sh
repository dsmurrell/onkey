#!/bin/bash
# CI, a release (.github/workflows/windows.yml and linux.yml with `updater: release`): the installer or AppImage and
# its .sig go to the DRAFT release scripts/release.sh --stage made on the public repo, with a note of this build and
# its rehearsal (update-rehearsal-<target>.txt) that scripts/release-publish.sh reads before it writes latest.json
# and publishes. Only a draft is written to: a published release is what people already get, and never changed here.
#   scripts/ci-upload-release.sh <installer or AppImage>
# Environment: GH_TOKEN (a token that may write releases on the public repo: ONKEY_PUBLIC_RELEASE_TOKEN), TAG,
# TARGET (windows-x86_64 | linux-x86_64), ONKEY_REPO (default dsmurrell/onkey), and GitHub's GITHUB_* for the note.
set -euo pipefail
F=${1:?usage: scripts/ci-upload-release.sh <file>}
REPO=${ONKEY_REPO:-dsmurrell/onkey}
die() { echo "::error::upload: $*" >&2; exit 1; }
[ -n "${GH_TOKEN:-}" ] || die "no token (the secret ONKEY_PUBLIC_RELEASE_TOKEN: docs/RELEASING.md, One-time setup)"
[ -n "${TAG:-}" ] && [ -n "${TARGET:-}" ] || die "TAG and TARGET must be set"
[ -f "$F" ] && [ -s "$F.sig" ] || die "no $F with a .sig beside it"
[ "${UPDATER_MODE:-}" = release ] || die "only a release build is uploaded (UPDATER_MODE is '${UPDATER_MODE:-}')"
V=${TAG#v}
[[ "$(basename "$F")" == *"_${V}_"* ]] || die "$(basename "$F") isn't version $V"
DRAFT=$(gh release view "$TAG" --repo "$REPO" --json isDraft --jq .isDraft 2>/dev/null) || die "no release $TAG on $REPO (scripts/release.sh --stage makes the draft first)"
[ "$DRAFT" = true ] || die "release $TAG on $REPO is already published: nothing is added to a published release"
NOTE="update-rehearsal-$TARGET.txt"
{
  echo "version=$V"
  echo "target=$TARGET"
  echo "file=$(basename "$F")"
  echo "sha256=$(sha256sum "$F" | cut -d' ' -f1)"
  echo "source=$(git rev-parse HEAD)"
  echo "official=${UPDATER_OFFICIAL:-}"
  echo "rehearsal=ok"
  echo "run=${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-?}/actions/runs/${GITHUB_RUN_ID:-?}"
  echo "built=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$NOTE"
cat "$NOTE"
gh release upload "$TAG" --repo "$REPO" --clobber "$F" "$F.sig" "$NOTE"
echo "uploaded $(basename "$F"), its .sig and $NOTE to the draft release $TAG on $REPO"
