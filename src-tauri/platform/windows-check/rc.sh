#!/bin/bash
# check-only stand-in for llvm-rc / rc.exe (Tauri embeds the Windows icon and manifest with one): answers the
# version probe and writes an empty output file. The resource is never linked, since this is only a check.
for a in "$@"; do [ "$a" = "/?" ] && { echo "OVERVIEW: Resource Converter"; echo "  /no-preprocess"; exit 0; }; done
out=; prev=
for a in "$@"; do [ "$prev" = "/fo" ] && out="$a"; prev="$a"; done
[ -n "$out" ] && : > "$out"
