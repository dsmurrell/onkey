#!/bin/bash
# Type-checks the engine for Windows from a Mac: cargo check --target x86_64-pc-windows-msvc, nothing built to run.
# A Mac has no Windows C headers, linker or onnxruntime, so for checking only:
#   - ring's C (via ureq's rustls) compiles with Apple clang against the tiny stand-in headers in include/,
#     and is archived by libexe.sh (lib.exe's arguments turned into ar's); nothing is linked
#   - Tauri's Windows resource (icon, manifest) goes to rc.sh, which writes an empty file in place of llvm-rc's
#   - onnxruntime isn't downloaded (ORT_SKIP_DOWNLOAD) and its lib folder is an empty one
#   - DirectML.dll, which the installer bundles (scripts/fetch-directml.sh), is left out of the resources
# A real Windows build (MSVC + Windows SDK) needs none of this.
#   src-tauri/platform/windows-check/check.sh [extra cargo args]
set -e
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/../.."
rustup target list --installed | grep -q x86_64-pc-windows-msvc || { echo "rustup target add x86_64-pc-windows-msvc first" >&2; exit 1; }
ortlib="${TMPDIR:-/tmp}/onkey-windows-check-ortlib"; mkdir -p "$ortlib"
export ORT_SKIP_DOWNLOAD=1 ORT_LIB_LOCATION="$ortlib"
export TAURI_CONFIG='{"bundle":{"resources":{"directml/DirectML.dll":null,"directml/LICENSE.txt":null}}}'
export CFLAGS_x86_64_pc_windows_msvc="-I$here/include" AR_x86_64_pc_windows_msvc="$here/libexe.sh" RC_x86_64_pc_windows_msvc="$here/rc.sh"
exec cargo check --target x86_64-pc-windows-msvc "$@"
