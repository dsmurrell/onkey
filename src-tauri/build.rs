// Builds the Swift library (native/capture.swift for audio capture, native/shazam.swift for recognising songs,
// native/audiofile.swift for the takes' AAC recordings, native/share.swift for a take's share video, native/demo.swift for the demo song's player, native/voice.swift for a take played with the song) into a static library and links it, then runs
// Tauri's own build step.
use std::{env, path::PathBuf, process::Command};

fn main() {
    // the Swift glue is macOS-only: a Windows build (src/platform/windows) has no Swift to build or link
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_swift();
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
    build_messages();
    tauri_build::build()
}

/// The Swift library (native/*.swift) built with swiftc and linked, with the Swift runtime and the frameworks it uses.
fn build_swift() {
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
}

/// The Mac's name for a language's folder of strings, <name>.lproj: Apple's own where it differs from our code.
/// Our `es` is Latin American Spanish, which Apple calls es-419; our `es-ES` is Spain's, Apple's plain es. (Our
/// `pt` is Brazil's, as Apple's plain pt is; zh-Hans and zh-Hant are Apple's names too.) Info.plist's
/// CFBundleLocalizations lists these same names; src/messages.rs tests that both match.
pub fn lproj_name(lang: &str) -> &str {
    match lang {
        "es" => "es-419",
        "es-ES" => "es",
        other => other,
    }
}

/// The native strings (messages/<lang>.json: the menu, the share video's end card, macOS's permission prompts):
/// every language file is embedded in the binary (src/messages.rs reads `$OUT_DIR/messages.rs`, with `LPROJ`,
/// each language's lproj name), and each one's `infoPlist` section is written as
/// gen/lproj/<lproj_name>.lproj/InfoPlist.strings, which the Mac bundle copies into Contents/Resources
/// (tauri.conf.json `bundle.macOS.files`), so macOS asks for the microphone and the rest in the Mac's language.
/// A key a language lacks is English's. A file that isn't valid JSON stops the build.
fn build_messages() {
    println!("cargo:rerun-if-changed=messages");
    let mut langs: Vec<(String, PathBuf)> = std::fs::read_dir("messages").expect("src-tauri/messages")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))   // not the translate tool's .sources.json
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), std::fs::canonicalize(&p).unwrap()))
        .collect();
    langs.sort();
    let read = |p: &PathBuf| -> serde_json::Value {
        println!("cargo:rerun-if-changed={}", p.display());
        let text = std::fs::read_to_string(p).unwrap();
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} isn't valid JSON: {e}", p.display()))
    };
    let en = read(&langs.iter().find(|(l, _)| l == "en").expect("messages/en.json").1);
    let mut code = String::from("/// (generated by build.rs from src-tauri/messages/*.json)\npub static CATALOGS: &[(&str, &str)] = &[\n");
    let mut names = String::from("/// Each language's folder of strings in the Mac bundle, <name>.lproj (build.rs lproj_name).\npub static LPROJ: &[(&str, &str)] = &[\n");
    let lproj = PathBuf::from("gen/lproj");
    let _ = std::fs::remove_dir_all(&lproj);
    for (lang, path) in &langs {
        assert!(lang.len() <= 35 && lang.split('-').next().is_some_and(|b| (2..=3).contains(&b.len()))
                && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "messages/{lang}.json: the file's name must be a language code (\"es\", \"zh-Hans\", \"pt-BR\")");
        let v = read(path);
        code += &format!("    ({lang:?}, include_str!({:?})),\n", path.display().to_string());
        names += &format!("    ({lang:?}, {:?}),\n", lproj_name(lang));
        // InfoPlist.strings: "key" = "text"; (UTF-8 is accepted by macOS), English's for a key this one lacks
        let mut strings = String::from("/* Generated by build.rs from src-tauri/messages/");
        strings += &format!("{lang}.json: edit that file, not this one. */\n");
        for (key, english) in en["infoPlist"].as_object().expect("en.json infoPlist") {
            let text = v["infoPlist"][key].as_str().or(english.as_str()).unwrap_or_default();
            let esc = text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
            strings += &format!("\"{key}\" = \"{esc}\";\n");
        }
        let dir = lproj.join(format!("{}.lproj", lproj_name(lang)));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("InfoPlist.strings"), strings).unwrap();
    }
    code += "];\n";
    code += &names;
    code += "];\n";
    std::fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("messages.rs"), code).unwrap();
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
    // an official build (scripts/release.sh): updater.rs OFFICIAL
    println!("cargo:rerun-if-env-changed=ONKEY_OFFICIAL");
    println!("cargo:rustc-env=ONKEY_COMMIT_TIME={time}");
    // (only files that are there: a missing one would run this script, and swiftc, on every build)
    let head = git(&["rev-parse", "--git-path", "HEAD"]);
    let branch = git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git(&["rev-parse", "--git-path", &r]));
    for path in [head, branch].into_iter().flatten().filter(|p| std::path::Path::new(p).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
}
