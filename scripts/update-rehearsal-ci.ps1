# The update rehearsal for Windows (.github/workflows/windows.yml runs it on the CI runner that built the installer,
# before anything is uploaded to the release): the exact installer about to ship must update itself. It is the
# Windows counterpart of scripts/update-rehearsal.sh (the Mac's), with the same contract, done the Windows way:
#   - the installer is installed as a person would (silently, for the current user: %LOCALAPPDATA%\OnKey);
#   - the installed OnKey is pointed at a fake next version served on this machine (http://127.0.0.1, which the app
#     accepts only for this: updater.rs rehearsal_url), signed with a key it doesn't trust first: it must refuse it
#     (log line "update-rehearsal: failed: download or signature"), and the installed version must be unchanged;
#   - then the same next version signed with the key it trusts: it must download it, check the signature, start the
#     installer passively (/P /UPDATE /R) and exit; the installer must then install the next version over it (the
#     onkey.exe on disk says the next version) and relaunch OnKey, whose window must load;
#   - the app must say whether it's an official build (updater.rs OFFICIAL), and that must be what was expected
#     (true for a release; a test build with a throwaway key is not official).
# The fake next version is a second build of the same source with its version raised (the workflow makes it with
# `tauri build --config {"version": …}`), signed as the release's files are, so its .sig is next to it.
#   pwsh scripts/update-rehearsal-ci.ps1 -Setup OnKey_X.Y.Z_x64-setup.exe -Next OnKey_X.Y.Z+1_x64-setup.exe -Out <dir> [-Official true|false]
# Exit 0: it refuses a wrongly signed update and updates itself with a rightly signed one. What it saw goes to -Out.
param(
  [Parameter(Mandatory)][string]$Setup,
  [Parameter(Mandatory)][string]$Next,
  [Parameter(Mandatory)][string]$Out,
  [string]$Official = "true"
)
$ErrorActionPreference = "Stop"
function Fail($msg) { Write-Host "::error::update-rehearsal: $msg"; exit 1 }
function Ver($path) { if ($path -match '^[^_]+_([^_]+)_') { $Matches[1] } else { Fail "can't read the version from the name $path" } }
function Installed() {
  $dir = Join-Path $env:LOCALAPPDATA 'OnKey'
  Get-ChildItem $dir -Filter *.exe -ErrorAction SilentlyContinue | Where-Object { $_.Name -notmatch 'uninstall' } | Select-Object -First 1
}
function InstalledVersion() { $exe = Installed; if ($exe) { $exe.VersionInfo.ProductVersion } else { "" } }
function Lines($log) {
  if (Test-Path $log) { Select-String -Path $log -Pattern 'update-rehearsal:' | ForEach-Object { $_.Line -replace '^.*(update-rehearsal:)', '$1' } | Sort-Object -Unique } else { @() }
}
function EndOnKey() { Get-Process onkey -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue; Start-Sleep 1 }

$Setup = (Resolve-Path $Setup).Path; $Next = (Resolve-Path $Next).Path
New-Item -ItemType Directory -Force $Out | Out-Null; $Out = (Resolve-Path $Out).Path
if (-not (Test-Path "$Next.sig")) { Fail "no signature beside the next version: $Next.sig" }
$appV = Ver (Split-Path -Leaf $Setup); $nextV = Ver (Split-Path -Leaf $Next)
if ($appV -eq $nextV) { Fail "the next version has the same version as the app ($appV)" }
Write-Host "update-rehearsal: $appV -> $nextV (official $Official expected)"
$log = Join-Path $env:TEMP 'onkey.log'

# install as a person would (the same as the launch check), from a clean slate
EndOnKey
Start-Process $Setup -ArgumentList '/S' -Wait
$exe = Installed
if (-not $exe) { Fail "the installer didn't install OnKey in $env:LOCALAPPDATA\OnKey" }
$v = InstalledVersion
if ($v -ne $appV) { Fail "installed onkey.exe says version '$v', expected $appV" }
Write-Host "  installed $($exe.FullName) ($v)"

# the fake next versions, served on this machine: one signed with the key the app trusts (the build signed it), one
# with a key made here and thrown away
$reh = Join-Path $env:RUNNER_TEMP ("onkey-rehearsal-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
if (-not $env:RUNNER_TEMP) { $reh = Join-Path $env:TEMP (Split-Path -Leaf $reh) }
$serve = Join-Path $reh 'serve'
New-Item -ItemType Directory -Force $serve | Out-Null
Copy-Item $Next (Join-Path $serve 'next-setup.exe'); Copy-Item "$Next.sig" (Join-Path $serve 'next-setup.exe.sig')
Copy-Item $Next (Join-Path $serve 'next-badkey-setup.exe')
$badKey = Join-Path $reh 'bad.key'
& npx tauri signer generate --ci -p "" -w $badKey *> $null
& npx tauri signer sign -f $badKey -p "" (Join-Path $serve 'next-badkey-setup.exe') *> $null
if (-not (Test-Path (Join-Path $serve 'next-badkey-setup.exe.sig'))) { Fail "couldn't sign with the throwaway key" }
$port = Get-Random -Minimum 47400 -Maximum 47900
foreach ($f in 'next', 'next-badkey') {
  $sig = (Get-Content (Join-Path $serve "$f-setup.exe.sig") -Raw).Trim()
  $json = @{ version = $nextV; notes = 'rehearsal'; pub_date = '2026-01-01T00:00:00Z'
             platforms = @{ 'windows-x86_64' = @{ signature = $sig; url = "http://127.0.0.1:$port/$f-setup.exe" } } } | ConvertTo-Json -Depth 5
  [IO.File]::WriteAllText((Join-Path $serve "$f.json"), $json)
}
$server = Start-Process python -ArgumentList '-m', 'http.server', $port, '--bind', '127.0.0.1', '--directory', $serve -PassThru -WindowStyle Hidden `
  -RedirectStandardOutput (Join-Path $Out 'http.log') -RedirectStandardError (Join-Path $Out 'http-err.log')
$up = $false
foreach ($i in 1..50) { try { Invoke-WebRequest "http://127.0.0.1:$port/next.json" -UseBasicParsing | Out-Null; $up = $true; break } catch { Start-Sleep -Milliseconds 100 } }
if (-not $up) { Stop-Process -Id $server.Id -Force; Fail "the local server didn't start" }

try {
  # rehearse <manifest>: the installed OnKey's own updater, at most three minutes; its exit code and lines
  function Rehearse($manifest) {
    Remove-Item $log -ErrorAction SilentlyContinue
    $env:ONKEY_UPDATE_REHEARSAL = "http://127.0.0.1:$port/$manifest.json"
    $p = Start-Process (Installed).FullName -PassThru
    $exited = $p.WaitForExit(180000)
    Remove-Item Env:ONKEY_UPDATE_REHEARSAL
    if (-not $exited) { Stop-Process -Id $p.Id -Force; $code = 'timeout' } else { $code = $p.ExitCode }
    Copy-Item $log (Join-Path $Out "$manifest-onkey.log") -ErrorAction SilentlyContinue
    return @{ code = $code; lines = (Lines $log) }
  }

  # 1. signed with a key the app doesn't trust: refused, nothing changed
  $bad = Rehearse 'next-badkey'
  Write-Host "  $($bad.lines -join ' | ') (exit $($bad.code); an update signed with another key)"
  if (-not ($bad.lines | Where-Object { $_ -like "update-rehearsal: failed: download or signature:*" })) { Fail "the app didn't refuse an update signed with a key it shouldn't trust: see $Out\next-badkey-onkey.log" }
  if ($bad.code -ne 1) { Fail "the refused update should end with exit code 1, not $($bad.code)" }
  if ((InstalledVersion) -ne $appV) { Fail "an update signed with another key changed the installed app" }
  if (Get-Process onkey -ErrorAction SilentlyContinue) { Fail "OnKey is still running after refusing the update" }

  # 2. signed with the key it trusts: the installer is started and this OnKey exits (the install line comes before,
  #    since the app ends inside the install: updater.rs rehearse); then the installer puts the next version in place
  #    and relaunches OnKey
  $good = Rehearse 'next'
  Write-Host "  $($good.lines -join ' | ') (exit $($good.code))"
  if (-not ($good.lines -contains "update-rehearsal: installing $nextV (official $Official)")) { Fail "this OnKey couldn't update itself (or isn't official $Official): see $Out\next-onkey.log" }
  if ($good.code -ne 0) { Fail "handing over to the installer should end with exit code 0, not $($good.code)" }
  $deadline = (Get-Date).AddSeconds(180); $v = ""
  while ((Get-Date) -lt $deadline) { $v = InstalledVersion; if ($v -eq $nextV) { break }; Start-Sleep 2 }
  if ($v -ne $nextV) { Fail "the installer didn't install $nextV within 3 minutes (onkey.exe says '$v')" }
  Write-Host "  installed onkey.exe now says $v"
  # the relaunch (/R), once the installer has finished copying (the models take a while after onkey.exe): a new OnKey,
  # whose window loads: a 'ui: window loaded' line after the install line in the log (the log is appended to)
  $deadline = (Get-Date).AddSeconds(180); $relaunched = $false
  while ((Get-Date) -lt $deadline) {
    if (Test-Path $log) {
      $all = @(Get-Content $log)
      $at = [array]::LastIndexOf($all, ($all | Where-Object { $_ -like '*update-rehearsal: installing*' } | Select-Object -Last 1))
      if ($at -ge 0 -and ($all[($at + 1)..($all.Count)] | Where-Object { $_ -like '*ui: window loaded*' })) { $relaunched = $true; break }
    }
    Start-Sleep 2
  }
  Copy-Item $log (Join-Path $Out 'relaunched-onkey.log') -ErrorAction SilentlyContinue
  $exeNow = Installed
  $exeV = if ($exeNow) { $exeNow.VersionInfo.ProductVersion } else { '' }
  EndOnKey
  if (-not $relaunched) { Fail "the installer didn't relaunch OnKey $nextV (or its window never loaded): see $Out\relaunched-onkey.log" }
  Write-Host "  the installer relaunched OnKey, whose window loaded ($exeV)"
  Write-Host "update-rehearsal: ok: $appV refuses an update signed with another key, and updates itself to a signed $nextV (official $Official), which the installer relaunches"
} finally {
  Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
  Remove-Item $reh -Recurse -Force -ErrorAction SilentlyContinue
}
