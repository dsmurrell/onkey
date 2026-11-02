#!/bin/bash
# A release build in CI (.github/workflows/windows.yml and linux.yml, run by hand with `public_tag`) checks out the
# PUBLIC repo at the release tag, so what it builds is exactly the published source. This step proves that is what
# was checked out, before anything is built (docs/RELEASING.md "Windows and Linux"):
#   - HEAD is the commit scripts/release.sh --stage tagged ($WANT, passed in by hand: a tag moved later is caught);
#   - the tag is an annotated one pointing at HEAD, and its version is the one in src-tauri/tauri.conf.json;
#   - no other version was asked for (the tag's is the version);
#   - the export commit names the private commit it came from (Exported-From), for the record.
# Environment: TAG (v0.3.0), WANT (full commit sha), UPDATER (none|throwaway|release), VERSION (must be empty).
set -euo pipefail
die() { echo "::error::release source: $*" >&2; exit 1; }
[ -n "${TAG:-}" ] || die "no public_tag"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "public_tag '$TAG' isn't a release tag (vX.Y.Z)"
[ -z "${VERSION:-}" ] || die "a release says the tag's version: leave 'version' empty"
HEAD=$(git rev-parse HEAD)
echo "checked out $HEAD ($(git log -1 --format='%s' HEAD))"
[ -n "${WANT:-}" ] || die "no public_commit: pass the commit the tag must point at (scripts/release.sh --stage prints it)"
[ "$HEAD" = "$WANT" ] || die "HEAD $HEAD isn't the commit passed in ($WANT)"
git fetch -q --depth=1 origin "refs/tags/$TAG:refs/tags/$TAG" 2>/dev/null || true
[ "$(git cat-file -t "refs/tags/$TAG" 2>/dev/null)" = tag ] || die "$TAG isn't an annotated tag on the public repo (release.sh makes one)"
[ "$(git rev-parse "$TAG^{commit}")" = "$HEAD" ] || die "$TAG points at $(git rev-parse "$TAG^{commit}"), not HEAD"
CONF_V=$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])' 2>/dev/null \
  || python -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')
[ "v$CONF_V" = "$TAG" ] || die "tauri.conf.json says $CONF_V, the tag says ${TAG#v}"
FROM=$(git log -1 --format=%B HEAD | sed -n 's/^Exported-From: *//p' | head -1)
# what the build says it was built from (build.rs: src-tauri/.source-commit, "<sha> <unix time>", which the export
# writes but git ignores in the public copy): the private commit, as the Mac's build of this release says, so a report
# from any platform names the same commit (with this public commit's time: the private one's isn't known here)
if [[ "$FROM" =~ ^[0-9a-f]{40}$ ]]; then
  echo "${FROM:0:10} $(git log -1 --format=%ct HEAD)" > src-tauri/.source-commit
fi
echo "release source ok: $TAG = $HEAD (version $CONF_V), exported from private commit ${FROM:-(not recorded)}, updater: ${UPDATER:-none}"
{
  echo "### Release source"
  echo "- public tag \`$TAG\` at \`$HEAD\` (version $CONF_V)"
  echo "- exported from private commit \`${FROM:-?}\`"
} >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
