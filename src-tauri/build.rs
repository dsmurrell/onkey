// Builds the Swift library (native/capture.swift for audio capture, native/shazam.swift for recognising songs,
// native/audiofile.swift for the takes' AAC recordings, native/share.swift for a take's share video, native/demo.swift for the demo song's player, native/voice.swift for a take played with the song) into a static library and links it, then runs
// Tauri's own build step.
use std::{env, path::PathBuf, process::Command};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut srcs = vec!["native/capture.swift", "native/permissions.swift", "native/shazam.swift", "native/audiofile.swift", "native/share.swift", "native/demo.swift", "native/voice.swift"];
    // development builds only: the ear's taps (src/ear.rs), never in a release
    println!("cargo:rerun-if-changed=native/ear.swift");
    if env::var_os("CARGO_FEATURE_DEVTEST").is_some() {
        srcs.push("native/ear.swift");
    }
    for src in &srcs {
        println!("cargo:rerun-if-changed={src}");
    }

    let lib = out.join("libonkeycapture.a");
    let status = Command::new("xcrun")
        .args(["swiftc", "-O", "-parse-as-library", "-emit-library", "-static", "-module-name", "OnKeyCapture",
               "-target", "arm64-apple-macos14.2", "-o"])
        .arg(&lib)
        .args(&srcs)
        .status()
        .expect("couldn't run swiftc (are Xcode's command line tools installed?)");
    assert!(status.success(), "swiftc failed to build {srcs:?}");

    // where the Swift runtime and its compatibility shims live
    let toolchain = Command::new("xcrun").args(["--find", "swiftc"]).output().expect("xcrun --find swiftc");
    let swiftc = PathBuf::from(String::from_utf8_lossy(&toolchain.stdout).trim());
    let swift_lib = swiftc.parent().unwrap().parent().unwrap().join("lib/swift/macosx");
    let sdk = Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]).output().expect("sdk path");
    let sdk = String::from_utf8_lossy(&sdk.stdout).trim().to_string();

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=onkeycapture");
    println!("cargo:rustc-link-search=native={}", swift_lib.display());
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    for fw in ["CoreAudio", "AudioToolbox", "AVFoundation", "Foundation", "ShazamKit", "AppKit", "CoreServices", "IOKit", "CoreMedia", "CoreVideo", "CoreText", "CoreGraphics", "ImageIO"] {
        println!("cargo:rustc-link-lib=framework={fw}");
    }

    // keeping test audio (devtest's keep_audio) and the tools that use it (dev/devhooks.rs): only in the private
    // development checkout, which has dev/ (it isn't published), and only in development builds
    println!("cargo:rerun-if-changed=dev/devkeep.rs");
    println!("cargo:rerun-if-changed=dev/devhooks.rs");
    println!("cargo:rustc-check-cfg=cfg(keepaudio)");
    if env::var_os("CARGO_FEATURE_DEVTEST").is_some() && std::path::Path::new("dev/devkeep.rs").exists() {
        println!("cargo:rustc-cfg=keepaudio");
    }

    build_identity();
    tauri_build::build()
}

/// Which commit this build is (ONKEY_GIT_SHA, "-dirty" with uncommitted changes when the build script ran) and when
/// that commit was made (ONKEY_COMMIT_TIME, Unix seconds): written at the top of every log session and in every
/// take's record, so a report says which build it came from. Worked out again when the checkout moves to another
/// commit.
fn build_identity() {
    let git = |args: &[&str]| Command::new("git").args(args).output().ok().filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    // a public export says which commit of the private repo it is (.source-commit: "<sha> <unix time>", written by
    // the export and not committed there): what a downloaded build reports, since the public copy's own git has no
    // commit yet, or an older one, while a new export waits to be committed
    println!("cargo:rerun-if-changed=.source-commit");
    let exported = std::fs::read_to_string(".source-commit").ok()
        .and_then(|s| s.split_once(' ').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())));
    let (sha, time) = match exported {
        Some((sha, time)) => (sha, time),
        None => {
            let sha = git(&["rev-parse", "--short=10", "HEAD"]).unwrap_or_else(|| "unknown".into());
            let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
            (format!("{sha}{}", if dirty { "-dirty" } else { "" }), git(&["log", "-1", "--format=%ct"]).unwrap_or_else(|| "0".into()))
        }
    };
    println!("cargo:rustc-env=ONKEY_GIT_SHA={sha}");
    println!("cargo:rustc-env=ONKEY_COMMIT_TIME={time}");
    // (only files that are there: a missing one would run this script, and swiftc, on every build)
    let head = git(&["rev-parse", "--git-path", "HEAD"]);
    let branch = git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git(&["rev-parse", "--git-path", &r]));
    for path in [head, branch].into_iter().flatten().filter(|p| std::path::Path::new(p).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
}
