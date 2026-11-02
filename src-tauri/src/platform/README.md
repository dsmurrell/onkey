# platform/

Everything OnKey asks of the OS, one folder per OS. Each folder has the same modules with the same functions;
`mod.rs` re-exports the current OS's (`pub use os::{…}`), so shared code calls `platform::shell::open(…)` with no
`#[cfg]`, and a missing function is a compile error rather than a silent fallback.

| module | what | macOS (`mac/`) | Windows (`windows/`) | Linux (`linux/`) |
|---|---|---|---|---|
| `spotify`, `music`, `youtube` | what the app plays, control, seek, play a track | AppleScript (`applescript.rs`), JavaScript in the browser's tab | one system media session reader (`media.rs`) for all three (`browsers.rs` for YouTube) | one MPRIS reader on the session D-Bus (`mpris.rs`; `browsers.rs` for YouTube); no Apple Music |
| `shell` | open, reveal, clipboard, home, privacy settings | `open`, `open -R`, `pbcopy` | ShellExecute, Explorer, the clipboard API | `xdg-open`, the file manager's D-Bus, GTK's clipboard, the sound settings |
| `sys` | memory, CPU, machine, region, preferred languages, power, priority, user names, app running, fonts | sysctl, rusage, NSLocale, IOKit, QoS | registry, process APIs, Toolhelp (`procs.rs`), GetUserDefaultGeoName, GetUserPreferredUILanguages | /proc, /sys, /etc/os-release, LANG, LANGUAGE, nice (`procs.rs`) |
| `app` | menu, window buttons, Dock, OS-only events, report location | AppKit through Tauri | Tauri's decorations; no menu | as Windows |
| `diag` | report and log extras | nothing extra | crash logging, WebView2, mic switches, summary.txt | panic logging, WebKitGTK, the sound server, MPRIS players, summary.txt |
| `native` | the Swift library's C interface | `extern "C"` (native/*.swift) | the same functions in Rust (WASAPI, Media Foundation…) | the same functions in Rust (the sound server through the PulseAudio protocol, Symphonia) |
| `signature` | song signatures and their catalog | ShazamKit | landmark.rs | landmark.rs |
| `gpu` | where a model runs, CPU fallback | Core ML (`pitch::load_coreml`) | DirectML | the CPU |

Shared code keeps what's the same job everywhere: ids, covers, title cleaning, the stale-id check, song rules,
settings (spotify.rs, music.rs, youtube.rs, source.rs…). A function is in the platform only where the mechanism
differs, and each OS's version does only its own thing (no branches for the other OS inside).

Left in shared code on purpose (same job, small OS difference): store.rs's C time functions (Windows spells
`localtime_r` differently), report.rs's "Mac"/"PC" wording and its Windows profile-path redaction, listen.rs's app
list (Windows adds Opera and Vivaldi), takeaudio.rs's tests of each OS's AAC encoder (skipped on Linux, which has
none yet), and engine.rs's Windows-only "app vs sound" measurement (digest.rs, compiled on Windows and Linux).

## Adding an OS

Add a folder with every module `mod.rs` re-exports, then the two `#[cfg(target_os = …)]` lines in `mod.rs`
(`pub mod <os>;`, `use self::<os> as os;`). The compiler lists what's missing. `linux/` is the latest example: what
the OS can't do yet is a stand-in that fails the way the Swift function does (`native.rs`), so the engine carries on
without that part.

## Linux status (first version, 30 Oct 2026; review fixes the same night)

Built and tested by CI only (`.github/workflows/linux.yml`, Ubuntu 22.04): unit tests on PulseAudio, the Linux
audio tests again on PipeWire 0.3.48 (pipewire-pulse), clippy, the AppImage, and a launch check that opens the
AppImage on a virtual display with a session D-Bus and PulseAudio, checks the window loads and it stays up, then
makes a report. In CI, on both sound servers, OnKey captures only its own stream (per-app) and hears its tone, and on
PipeWire the demo and voice playback keep time with the wall clock. Nothing here has been run on a real Linux
desktop with a real music app yet.

What's real:
- Following songs: Spotify through its MPRIS player (Spotify's own ids, ads and episodes as `spotify:ad:` /
  `spotify:episode:` so the engine skips them as on the Mac, covers, play, pause, next, seek, a track opened by its
  `spotify:track:` link through OpenUri). Its position is run on from when Spotify's Position last changed (as for
  the browsers), and the log says, 10 s into each song, whether Spotify's own Position moved. YouTube and YouTube
  Music in Chrome, Chromium, Edge, Brave, Vivaldi, Opera and Firefox through the browser's MPRIS player (the song
  named from title and artist, as on Windows; KDE's plasma-browser-integration and playerctld copies left out;
  Restart in Firefox, which takes no position, is Previous). Apple Music: none (there's no app). Listening mode uses
  Windows' app list (Opera and Vivaldi in, no Safari or Arc to ask about).
- Audio: the sound server spoken to in the PulseAudio protocol (the `pulseaudio` crate, plain Rust: nothing to
  link, nothing for the AppImage to carry). PipeWire answers it through pipewire-pulse (the default on Ubuntu 22.10+,
  Fedora 34+, Debian 12 with GNOME…); PulseAudio itself does too. Capture is per-app: a record stream on the
  output's monitor tied to the music app's own playback stream (`direct_on_input`, what pavucontrol's per-app meters
  use), following the app to its next stream when one ends; if the server refuses that, the whole default output
  (its monitor) is heard instead, and the log says so. `ONKEY_LINUX_CAPTURE=system` forces the whole output. The
  mic is the chosen input (else the default), listed and metered for setup's mic check. The engine's device
  questions share one kept-open connection, answers kept for a second.
- OnKey's own sound (the demo song, a take played with the song): a playback stream at the output's own volume, its
  clock read from the server's latency reports; Windows' demo.rs and voice.rs unchanged apart from the decoder.
- Decoding takes, the demo song and anything else in Rust (Symphonia), with an .m4a's edit list followed (mp4.rs).
- Facts for the log and reports: distribution, kernel, session, desktop, GDK's real display (Wayland or X11) and
  GDK_BACKEND, the WebKit switches, WebKitGTK, the sound server and its devices, the MPRIS players and how each is
  installed (native, snap, flatpak), and per song how far the app's position was from the sound ("app vs sound", as
  on Windows: the number `HANDOFF` and MPRIS's accuracy need). Panics are logged. Reports (Settings → Report a problem) are saved to the
  Desktop (else the home folder) with a summary.txt, and shown in the file manager.
- Song signatures: landmark.rs, as Windows. The models run on the CPU.
- The window says "Linux", "this PC", the sound settings and the file manager where Windows' says Windows, and where
  a report was really saved.
- The clipboard ("Challenge a friend", the share clock) through GTK, the window's own toolkit: nothing to install.
- Links open with `xdg-open`, never holding the window more than a second; programs OnKey starts (the sound
  settings, a browser) get the environment without the AppImage's own GTK, GIO and library paths.
- Mini mode on Wayland: no app may place its window or keep it on top there, so the bar stays put (drag it) and the
  log says so once; on X11 it floats as on Windows. In practice the AppImage's GTK hook sets `GDK_BACKEND=x11`
  (CI's log: "display GdkX11Display (GDK_BACKEND x11…)"), so on a Wayland desktop OnKey runs under XWayland: mini mode
  floats, but the window can look soft at fractional scaling. Left as it is for now; `GDK_BACKEND=wayland
  ./OnKey_*.AppImage` runs it natively.

Stand-ins (fail the way the Swift functions do; the engine carries on without them):
- AAC encoding: takes' recordings stay WAV (about 5.8 MB a minute instead of 0.5 MB). The usual encoder, FDK AAC,
  has a licence to decide on first; an Opus or FLAC file would need the players and takeaudio.rs to take it.
- The share video (no video encoder yet), Shazam (Apple's: songs not learned are named by the music app), the
  development ear, wakes from sleep (logind's PrepareForSleep would do it), echo cancelling (off).
- The GPU: the models run on the CPU. ONNX Runtime could use CUDA or TensorRT (NVIDIA), ROCm or MIGraphX (AMD),
  OpenVINO (Intel) or WebGPU; each needs its own ONNX Runtime build and libraries shipped with the app, so none is
  built yet.
- Updates: latest.json has no `linux-x86_64` entry yet, so a Linux copy doesn't check at all (updater.rs
  `supported` is macOS only, as for Windows: else every check would fail). To ship Linux updates: build with
  `createUpdaterArtifacts` on, sign the AppImage, add `"linux-x86_64": {"signature": "<the .AppImage.sig>", "url":
  "<the .AppImage on the release>"}` to latest.json's `platforms` (the updater then replaces the AppImage file in
  place), and let `supported` say yes on Linux. Release scripts unchanged.

Needs a real Linux machine to check:
- Per-app capture of a real music app (CI checks it with OnKey's own stream on PulseAudio 15 and PipeWire 0.3.48),
  and how Chrome's and Firefox's streams come and go between videos; the gap between the music and the mic (the
  engine measures it); a real microphone (CI has none, so the mic channel is silence there).
- Spotify's MPRIS position (some older clients always said 0; the log says what it gave, once per song) and whether
  OpenUri plays a track or only shows it.
- The voice playback clock against the music (`HANDOFF` in voice.rs is 0 until measured).
- A blank window: WebKitGTK has two switches for it. OnKey sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` itself when
  NVIDIA's driver is loaded and `WEBKIT_DISABLE_COMPOSITING_MODE=1` when there's no GPU device (/dev/dri: most VMs),
  unless already set (diag.rs). On Ubuntu 24.04, AppArmor's limit on unprivileged user namespaces can stop WebKit's
  sandbox ("bwrap: setting up uid map: Permission denied" in the terminal).
- Spotify's web player in a browser is followed as "YouTube in your browser" (named from its title and artist, no
  Spotify id); the desktop app is the one that's Spotify.

### For testers

Runs on 64-bit Intel/AMD Linux with glibc 2.35 or newer (built on Ubuntu 22.04): Ubuntu 22.04+, Debian 12+, Fedora
36+, Linux Mint 21+, Pop!_OS 22.04+, recent Arch. Needs a desktop with PipeWire (with pipewire-pulse) or PulseAudio,
which nearly every desktop has.

1. Download `OnKey_<version>_amd64.AppImage` (CI artifact `onkey-linux-<commit>`, a zip holding it), then
   `chmod +x OnKey_*.AppImage` and run it (double-click, or `./OnKey_*.AppImage` in a terminal to see its output).
   If it says FUSE is missing: install `libfuse2` (Ubuntu 22.04: `sudo apt install libfuse2`; 24.04:
   `sudo apt install libfuse2t64`), or run it with `--appimage-extract-and-run`.
2. Play a song in the Spotify desktop app, or a music video in YouTube / YouTube Music in Chrome or Firefox (turn
   on "YouTube in your browser" in OnKey's Settings for that), and sing along with a microphone.
3. The first time, run it from a terminal (`./OnKey_*.AppImage`) and keep what it prints: a crash before OnKey's
   own log starts (GTK, FUSE, WebKit's sandbox) only shows there.
4. If the window is blank: try `WEBKIT_DISABLE_DMABUF_RENDERER=1 ./OnKey_*.AppImage`, then
   `WEBKIT_DISABLE_COMPOSITING_MODE=1 ./OnKey_*.AppImage`. On Ubuntu 24.04, if the terminal says "bwrap: setting up
   uid map: Permission denied", `sudo sysctl kernel.apparmor_restrict_unprivileged_userns=0` lets it run (until the
   next restart); tell us either way.
5. If OnKey doesn't hear the music: try `ONKEY_LINUX_CAPTURE=system ./OnKey_*.AppImage` (it then hears everything the
   computer plays, not only the music app) and tell us whether that helped.
6. Use the Spotify desktop app, not Spotify in a browser: Spotify's web player is followed as "YouTube in your
   browser", without Spotify's own song ids.

When something goes wrong: Settings → Report a problem saves `OnKey-report-<time>.zip` to the Desktop (or the home
folder when there's no Desktop folder; the dialog says which) and shows it; send that, with what the terminal
printed, and say: your distribution, whether you're on Wayland or X11 (Settings → About on GNOME), and how Spotify
and your browser are installed (from the distribution, Snap or Flatpak). The live log is `/tmp/onkey.log`; each
day's log is kept in `~/.local/share/audio.onkey.app/records/logs/` (OnKey's data folder: takes, songs learned,
settings).
