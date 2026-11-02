#!/bin/bash
# The AppImage without the Wayland libraries linuxdeploy copies in from the build machine (Ubuntu 22.04):
# libwayland-client, -cursor, -egl and -server. Beside a newer system's own graphics stack (Mesa's EGL) they clash,
# and OnKey opens to a blank window ("EGL_BAD_PARAMETER", Fedora 45, issue #1, Nov 2026). Every desktop with
# Wayland has these libraries itself, so the system's are used. The same fix as Armbian's imager (its PR #170);
# Tauri can't pass linuxdeploy an exclusion itself.
#   scripts/appimage-unbundle-wayland.sh <file.AppImage>     rewritten in place (run by ci-build.sh before signing)
# Needs squashfs-tools (unsquashfs, mksquashfs). The AppImage's own runtime (the launcher before the file system) is
# kept byte for byte; the file system is repacked with the same compression and block size.
set -euo pipefail
die() { echo "::error::unbundle-wayland: $*" >&2; exit 1; }
f=${1:-}
[ -f "$f" ] || die "no AppImage at '$f'"
command -v unsquashfs >/dev/null && command -v mksquashfs >/dev/null || die "needs squashfs-tools (unsquashfs, mksquashfs)"
chmod +x "$f"
off=$("$f" --appimage-offset) || die "the AppImage didn't say where its file system starts (--appimage-offset)"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
head -c "$off" "$f" > "$work/runtime"
tail -c +$((off + 1)) "$f" > "$work/fs.sqsh"
info=$(unsquashfs -s "$work/fs.sqsh")
comp=$(awk '/^Compression /{print $2; exit}' <<<"$info")
bs=$(awk '/^Block size /{print $3; exit}' <<<"$info")
[ -n "$comp" ] && [ -n "$bs" ] || die "couldn't read the file system's compression and block size"
unsquashfs -q -no-progress -d "$work/root" "$work/fs.sqsh" >/dev/null
gone=$(find "$work/root" -name 'libwayland-*.so*' -print -delete)
# none there means linuxdeploy stopped bundling them: then this step has nothing to do, and should go
[ -n "$gone" ] || die "no libwayland-* in the AppImage (nothing to take out: has linuxdeploy changed?)"
sed "s#^$work/root/#  removed #" <<<"$gone"
mksquashfs "$work/root" "$work/new.sqsh" -comp "$comp" -b "$bs" -all-root -noappend -no-progress >/dev/null
cat "$work/runtime" "$work/new.sqsh" > "$f.new"
chmod +x "$f.new"
mv "$f.new" "$f"
# still a working AppImage: it finds its file system, and the libraries are gone from it
off2=$("$f" --appimage-offset) || die "the repacked AppImage doesn't start"
[ "$off2" = "$off" ] || die "the repacked AppImage's file system moved ($off -> $off2)"
left=$(tail -c +$((off + 1)) "$f" > "$work/check.sqsh" && unsquashfs -l "$work/check.sqsh" | grep -c 'libwayland-' || true)
[ "$left" = 0 ] || die "$left libwayland-* still in the repacked AppImage"
echo "unbundle-wayland: $(basename "$f") repacked ($comp, block $bs), $(wc -l <<<"$gone" | tr -d ' ') libraries out"
