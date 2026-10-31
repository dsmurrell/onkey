#!/bin/bash
# The Linux launch check (.github/workflows/linux.yml), run inside a session D-Bus: opens the AppImage as a tester
# would, on a virtual X display with a PulseAudio null output, and checks that its window loads and it stays up;
# then a second launch makes a report as "Report a problem" does (ONKEY_REPORT_TO, lib.rs) and checks what it says.
#   linux-launch-check.sh <path to the .AppImage> <folder for what it finds>
set -u
app=$1
out=$2
mkdir -p "$out"
# (CI has no FUSE mount for AppImages: they unpack themselves into /tmp and run from there)
export APPIMAGE_EXTRACT_AND_RUN=1
pulseaudio --start --exit-idle-time=-1 2>/dev/null || echo "launch check: PulseAudio didn't start (OnKey should say so and carry on)"
pactl info > "$out/pactl-info.txt" 2>&1 || true
log=/tmp/onkey.log

# open it, in its own process group so all of it can be ended at once
launch() {
  setsid xvfb-run -a -s "-screen 0 1600x1000x24" "$app" > "$out/$1.txt" 2>&1 &
  echo $!
}

# end it the hard way (as a crash or a forced quit would): the next launch should notice
end() {
  kill -KILL -- "-$1" 2>/dev/null || kill -KILL "$1" 2>/dev/null
  sleep 1
}

rm -f "$log"
pid=$(launch first-launch-output)
loaded=false
for _ in $(seq 1 60); do
  if grep -q "ui: window loaded" "$log" 2>/dev/null; then loaded=true; break; fi
  kill -0 "$pid" 2>/dev/null || break
  sleep 2
done
sleep 20   # and it's still up a little later
alive=false
kill -0 "$pid" 2>/dev/null && alive=true
cp "$log" "$out/onkey.log" 2>/dev/null || echo "no onkey.log"
cat "$log" 2>/dev/null
end "$pid"
fail=0
$alive || { echo "::error::OnKey quit by itself"; cat "$out/first-launch-output.txt"; fail=1; }
$loaded || { echo "::error::OnKey's window never loaded (no 'ui: window loaded' in onkey.log)"; cat "$out/first-launch-output.txt"; fail=1; }
for want in "linux: " "audio server: " "MPRIS: "; do
  grep -q "$want" "$log" 2>/dev/null || { echo "::error::onkey.log doesn't say '$want'"; fail=1; }
done
[ $fail = 0 ] || exit 1

# a tester's report, from a second launch: the first was ended without a quit, so its summary should say so
report="$out/report.zip"
export ONKEY_REPORT_TO="$report"
pid=$(launch second-launch-output)
for _ in $(seq 1 60); do
  [ -f "$report" ] && break
  kill -0 "$pid" 2>/dev/null || break
  sleep 2
done
sleep 2
end "$pid"
unset ONKEY_REPORT_TO
cp "$log" "$out/onkey-second-launch.log" 2>/dev/null
[ -f "$report" ] || { tail -40 "$log"; echo "::error::no report made (ONKEY_REPORT_TO)"; exit 1; }
mkdir -p "$out/report" && (cd "$out/report" && unzip -o -q "$report")
summary="$out/report/summary.txt"
[ -f "$summary" ] || { ls -R "$out/report"; echo "::error::the report has no summary.txt"; exit 1; }
cat "$summary"
for want in "didn't close normally" "audio server:" "linux:"; do
  grep -qF "$want" "$summary" || { echo "::error::summary.txt doesn't say '$want'"; exit 1; }
done
# nothing of the runner's user name or home folder in it
if grep -rlF "$USER" "$out/report"; then echo "::error::the report names the user ($USER)"; exit 1; fi
echo "launch check: passed"
