#!/bin/bash
# check-only stand-in for lib.exe: turns its arguments into a plain ar call (the archive is never linked)
out=; files=()
for a in "$@"; do case "$a" in -out:*) out="${a#-out:}";; -*) ;; *) files+=("$a");; esac; done
exec /usr/bin/ar crs "$out" "${files[@]}"
