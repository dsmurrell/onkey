#!/bin/bash
# Publish a staged OnKey release (docs/RELEASING.md "Windows and Linux"): scripts/release.sh <V> --stage tagged the
# public commit, made the release as a DRAFT with the Mac files and started the Windows and Linux CI builds of that
# tag, which signed their updater files, rehearsed the update and uploaded them to the draft. This script is the last
# gate before anyone gets any of it:
#   - the release is still a draft, and holds every file: OnKey.dmg, the Mac update (.app.tar.gz + .sig), the Windows
#     installer (…_x64-setup.exe + .sig), the Linux AppImage (…_amd64.AppImage + .sig), and CI's note of each
#     rehearsal (update-rehearsal-<target>.txt: version, target, SHA-256 of the file, source commit, official, run);
#   - each note says the rehearsal passed, for this version, built from the tagged commit, as an official build, and
#     its SHA-256 is the file's;
#   - every one of the three updates is signed by the key every installed OnKey trusts (scripts/verify-update-sig.py
#     against src-tauri/tauri.conf.json), checked here on the downloaded files, not taken from CI's word;
#   - the Mac update on the release is the one this Mac built and rehearsed (its .sig matches the local one).
# Then it writes latest.json with all three platforms, SHA256SUMS over the four downloads, uploads them, drops the
# notes and publishes the release as the latest: within a day every OnKey on every platform finds it.
#   scripts/release-publish.sh <X.Y.Z> [--dry-run]     (--dry-run: everything but the uploads and the publish)
# Run in the public copy, where release.sh --stage ran (it reads src-tauri/target-dist/release-vX.Y.Z/). Environment:
# ONKEY_REPO (default dsmurrell/onkey).
set -euo pipefail
cd "$(dirname "$0")/.."
V=""; DRY=0
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    -*) echo "unknown option $a" >&2; exit 2 ;;
    *) V=$a ;;
  esac
done
[[ "$V" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: scripts/release-publish.sh <X.Y.Z> [--dry-run]" >&2; exit 2; }
TAG="v$V"
REPO=${ONKEY_REPO:-dsmurrell/onkey}
DIST="src-tauri/target-dist/release-$TAG"
say() { printf '\n== %s\n' "$*"; }
die() { echo "release-publish: $*" >&2; exit 1; }
say "OnKey $V: publish$([ $DRY = 1 ] && echo " (dry run)")"
gh auth status >/dev/null 2>&1 || die "gh isn't logged in (gh auth login)"
python3 -c 'import cryptography' 2>/dev/null || die "python's cryptography module is needed to check the signatures here (python3 -m pip install cryptography)"
[ -d "$DIST" ] || die "no $DIST: run scripts/release.sh $V --stage first (in this copy)"
[ -f "$DIST/notes.md" ] || die "no $DIST/notes.md (release.sh writes it)"
MAC_TARBALL=$(cd "$DIST" && ls OnKey_"$V"_*.app.tar.gz 2>/dev/null | head -1)
[ -n "$MAC_TARBALL" ] && [ -s "$DIST/$MAC_TARBALL.sig" ] || die "no Mac update OnKey_${V}_*.app.tar.gz (+ .sig) in $DIST"
[ -f "$DIST/OnKey.dmg" ] || die "no $DIST/OnKey.dmg"
COMMIT=$(git rev-parse -q --verify "refs/tags/$TAG^{commit}" 2>/dev/null) || die "no tag $TAG here: release.sh --stage makes it"
WIN="OnKey_${V}_x64-setup.exe"; LINUX="OnKey_${V}_amd64.AppImage"
NOTES=(update-rehearsal-windows-x86_64.txt update-rehearsal-linux-x86_64.txt)

# ---- the draft and its files
say "the draft release"
INFO=$(gh release view "$TAG" --repo "$REPO" --json isDraft,url,assets) || die "no release $TAG on $REPO"
[ "$(jq -r .isDraft <<< "$INFO")" = true ] || die "$TAG on $REPO is already published"
echo "$(jq -r .url <<< "$INFO") (draft)"
HAVE=$(jq -r '.assets[].name' <<< "$INFO")
missing=0
for f in OnKey.dmg "$MAC_TARBALL" "$MAC_TARBALL.sig" "$WIN" "$WIN.sig" "$LINUX" "$LINUX.sig" "${NOTES[@]}"; do
  if grep -qxF -- "$f" <<< "$HAVE"; then echo "  $f"; else echo "  MISSING: $f"; missing=1; fi
done
[ $missing = 0 ] || die "the draft is missing files: the Windows or Linux CI build hasn't finished, or failed (gh run list --repo dsmurrell/onkey-dev)"
extra=$(grep -vxF -e OnKey.dmg -e "$MAC_TARBALL" -e "$MAC_TARBALL.sig" -e "$WIN" -e "$WIN.sig" -e "$LINUX" -e "$LINUX.sig" -e "${NOTES[0]}" -e "${NOTES[1]}" -e latest.json -e SHA256SUMS <<< "$HAVE" || true)
[ -z "$extra" ] || die "unexpected files on the draft: $(tr '\n' ' ' <<< "$extra")"

# ---- download what CI made, and check it
say "download"
CI="$DIST/ci"; mkdir -p "$CI"
gh release download "$TAG" --repo "$REPO" --dir "$CI" --clobber -p "$WIN" -p "$WIN.sig" -p "$LINUX" -p "$LINUX.sig" -p "$MAC_TARBALL.sig" -p "$MAC_TARBALL" -p OnKey.dmg -p 'update-rehearsal-*.txt'
for f in "$WIN" "$WIN.sig" "$LINUX" "$LINUX.sig" "$MAC_TARBALL.sig" "$MAC_TARBALL" OnKey.dmg "${NOTES[@]}"; do [ -s "$CI/$f" ] || die "didn't download $f"; done
ls -lh "$CI" | tail -n +2 | sed 's/^/  /'
# the Mac update on the release is the one this Mac built and rehearsed
cmp -s "$CI/$MAC_TARBALL.sig" "$DIST/$MAC_TARBALL.sig" || die "the Mac update's signature on the release isn't this Mac's build's"
# and the files themselves, byte for byte: a damaged upload would leave every Mac failing its update check
cmp -s "$CI/$MAC_TARBALL" "$DIST/$MAC_TARBALL" || die "the Mac update on the release isn't the file this Mac built and rehearsed"
cmp -s "$CI/OnKey.dmg" "$DIST/OnKey.dmg" || die "OnKey.dmg on the release isn't the one this Mac built"

say "CI's rehearsal notes"
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
note() { sed -n "s/^$2=//p" "$1" | head -1; }
check_note() {   # check_note <note file> <target> <file>
  local n="$CI/$1" target=$2 file=$3 k
  for k in version target file sha256 source official rehearsal run; do [ -n "$(note "$n" $k)" ] || die "$1: no '$k' line"; done
  [ "$(note "$n" version)" = "$V" ] || die "$1 is for version $(note "$n" version), not $V"
  [ "$(note "$n" target)" = "$target" ] || die "$1 says target $(note "$n" target)"
  [ "$(note "$n" file)" = "$file" ] || die "$1 is about $(note "$n" file), not $file"
  [ "$(note "$n" rehearsal)" = ok ] || die "$1: the rehearsal didn't pass ($(note "$n" rehearsal))"
  [ "$(note "$n" official)" = true ] || die "$1: not an official build (official=$(note "$n" official)): it would never install updates"
  [ "$(note "$n" source)" = "$COMMIT" ] || die "$1: built from $(note "$n" source), not the tagged commit $COMMIT"
  [ "$(note "$n" sha256)" = "$(sha "$CI/$file")" ] || die "$1: the SHA-256 isn't the downloaded $file's"
  echo "  $target: $file, rehearsal ok, official, from $COMMIT, $(note "$n" run)"
}
check_note "${NOTES[0]}" windows-x86_64 "$WIN"
check_note "${NOTES[1]}" linux-x86_64 "$LINUX"

say "signatures (as every installed OnKey checks them, against the key in tauri.conf.json)"
for f in "$DIST/$MAC_TARBALL" "$CI/$WIN" "$CI/$LINUX"; do
  rc=0; python3 scripts/verify-update-sig.py "$f" || rc=$?
  [ $rc = 0 ] || die "$(basename "$f") isn't signed by the key OnKey trusts (exit $rc): not published"
done

# ---- latest.json (all three) and SHA256SUMS
say "latest.json"
cp "$CI/$WIN" "$CI/$LINUX" "$DIST/" 2>/dev/null || true
python3 - "$V" "$DIST" "$MAC_TARBALL" "$WIN" "$LINUX" "$REPO" "$TAG" <<'EOF'
import datetime, json, re, sys
v, dist, mac, win, linux, repo, tag = sys.argv[1:]
notes = open(f"{dist}/notes.md").read().split("\n", 2)[2].strip()
def sig(name):
    s = open(f"{dist}/{name}.sig").read().strip()
    assert re.fullmatch(r"[A-Za-z0-9+/=]+", s) and len(s) > 100, f"{name}.sig doesn't look like a Tauri signature"
    return s
def url(name):
    return f"https://github.com/{repo}/releases/download/{tag}/{name}"
mac_target = {"arm64": "darwin-aarch64", "aarch64": "darwin-aarch64", "x86_64": "darwin-x86_64"}[re.search(r"_([^_]+)\.app\.tar\.gz$", mac).group(1)]
latest = {"version": v, "notes": notes,
          "pub_date": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
          "platforms": {mac_target: {"signature": sig(mac), "url": url(mac)},
                        "windows-x86_64": {"signature": sig(win), "url": url(win)},
                        "linux-x86_64": {"signature": sig(linux), "url": url(linux)}}}
assert set(latest["platforms"]) == {"darwin-aarch64", "windows-x86_64", "linux-x86_64"}, latest["platforms"].keys()
json.dump(latest, open(f"{dist}/latest.json", "w"), indent=2, ensure_ascii=False)
for t, p in latest["platforms"].items():
    print(f"  {t}: {p['url']}")
EOF
(cd "$DIST" && shasum -a 256 OnKey.dmg "$MAC_TARBALL" "$WIN" "$LINUX" > SHA256SUMS && sed 's/^/  /' SHA256SUMS)

# ---- publish
say "publish"
DROP=(gh release delete-asset "$TAG" --repo "$REPO" --yes)
UPLOAD=(gh release upload "$TAG" --repo "$REPO" --clobber "$DIST/latest.json" "$DIST/SHA256SUMS")
PUBLISH=(gh release edit "$TAG" --repo "$REPO" --draft=false --latest)
q() { printf '%q ' "$@"; }
if [ $DRY = 1 ]; then
  echo "dry run: would run"
  for n in "${NOTES[@]}"; do echo "  $(q "${DROP[@]}") $n"; done
  echo "  $(q "${UPLOAD[@]}")"
  echo "  $(q "${PUBLISH[@]}")"
  echo "latest.json and SHA256SUMS written to $DIST"
  exit 0
fi
for n in "${NOTES[@]}"; do "${DROP[@]}" "$n"; done
"${UPLOAD[@]}"
"${PUBLISH[@]}"
echo "published $TAG: https://github.com/$REPO/releases/tag/$TAG"
# what every OnKey, on every platform, will read within a day
sleep 5
curl -fsSL "https://github.com/$REPO/releases/latest/download/latest.json" | python3 -c '
import json, sys
j = json.load(sys.stdin)
print("latest.json now says", j["version"], "for", ", ".join(sorted(j["platforms"])))'
