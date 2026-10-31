#!/bin/sh
# Fetches DirectML.dll for the Windows build into src-tauri/directml/ (not in git): the GPU for learning
# (src-tauri/src/gpu.rs). ONNX Runtime 1.22, which the app links, is built against DirectML 1.15.4; Windows' own
# DirectML.dll is older, so the installer puts this one beside onkey.exe (tauri.windows.conf.json), with its licence.
# From Microsoft's NuGet package, checked against the SHA-512 nuget.org lists for it. CI runs this (windows.yml).
set -e
VERSION=1.15.4
SHA512=fde767f56904abc90fd53f65d8729c918ab7f6e3c5e1ecdd479908fc02b4535cf2b0860f7ab2acb9b731d6cb809b72c3d5d4d02853fb8f5ea022a47bc44ef285
cd "$(dirname "$0")/../src-tauri"
[ -f directml/DirectML.dll ] && [ -f directml/LICENSE.txt ] && { echo "DirectML ready"; exit 0; }
PKG=microsoft.ai.directml.$VERSION.nupkg
curl -fL -o "$PKG" "https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/$VERSION/$PKG"
echo "$SHA512  $PKG" | sha512sum -c -
mkdir -p directml && unzip -j -o "$PKG" bin/x64-win/DirectML.dll LICENSE.txt -d directml && rm "$PKG"
echo "DirectML ready"
