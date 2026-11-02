#!/bin/bash
# CI (.github/workflows/windows.yml and linux.yml): the updater file just built is signed by the key every installed
# OnKey trusts (src-tauri/tauri.conf.json's public key), checked as OnKey checks it (scripts/verify-update-sig.py):
#   release    must pass: a file OnKey would refuse must never be uploaded;
#   throwaway  must FAIL, since its key is not that one: the proof that a throwaway build could never pass as a release.
#   scripts/ci-verify-sig.sh <installer or AppImage>   (its .sig beside it; UPDATER_MODE: release|throwaway)
set -euo pipefail
F=${1:?usage: scripts/ci-verify-sig.sh <file>}
die() { echo "::error::updater signature: $*" >&2; exit 1; }
[ -f "$F" ] && [ -s "$F.sig" ] || die "no $F with a .sig beside it"
PY=$(command -v python3 || command -v python)
$PY -c 'import cryptography' 2>/dev/null || $PY -m pip install --quiet --disable-pip-version-check cryptography
rc=0; $PY scripts/verify-update-sig.py "$F" || rc=$?
case "${UPDATER_MODE:-}" in
  release)
    [ $rc = 0 ] || die "$(basename "$F") isn't signed by the key OnKey trusts (exit $rc): not fit to release"
    echo "ok: $(basename "$F") is signed by the key every installed OnKey trusts" ;;
  throwaway)
    [ $rc = 1 ] || die "a throwaway build's file must not pass as the release key's (exit $rc)"
    echo "ok: $(basename "$F") is signed by the throwaway key, which OnKey's releases don't trust (as it should be: this build can't be published)" ;;
  *) die "UPDATER_MODE isn't release or throwaway" ;;
esac
