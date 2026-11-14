// What first-run setup needs to know and ask for: the microphone and system audio recording (the Core Audio
// tap that hears the music app), plus a simple mic level meter for "say something". Kept apart from
// capture.swift so the two can change independently. Called from Rust through a small C interface.
import AVFoundation
import AudioToolbox
import CoreAudio
import AppKit
import CoreServices
import Foundation

// MARK: microphone

/// 0 not asked yet, 1 restricted (by a profile), 2 denied, 3 allowed.
@_cdecl("onkey_mic_status")
public func onkey_mic_status() -> Int32 {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .notDetermined: return 0
    case .restricted: return 1
    case .denied: return 2
    case .authorized: return 3
    @unknown default: return 0
    }
}

/// Ask for the microphone (shows macOS's prompt the first time; does nothing once answered).
@_cdecl("onkey_mic_request")
public func onkey_mic_request() {
    AVCaptureDevice.requestAccess(for: .audio) { _ in }
}

// MARK: system audio recording

private typealias PreflightFn = @convention(c) (CFString, CFDictionary?) -> Int

/// 0 not asked yet, 2 denied, 3 allowed, -1 unknown. macOS has no public call to read this without asking, so
/// this reads it the way audio tools do (TCC's preflight, read-only and never prompts); if that isn't there,
/// "unknown", and setup offers to check by asking.
@_cdecl("onkey_audio_capture_status")
public func onkey_audio_capture_status() -> Int32 {
    guard let h = dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW),
          let sym = dlsym(h, "TCCAccessPreflight") else { return -1 }
    let preflight = unsafeBitCast(sym, to: PreflightFn.self)
    switch preflight("kTCCServiceAudioCapture" as CFString, nil) {
    case 0: return 3
    case 1: return 2
    case 2: return 0
    default: return -1
    }
}

/// Ask for system audio recording by briefly making a tap of the whole system's output (macOS prompts the first
/// time), then removing it. 0 the tap was allowed, 1 refused. Nothing is recorded.
@_cdecl("onkey_audio_capture_request")
public func onkey_audio_capture_request() -> Int32 {
    let desc = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
    desc.isPrivate = true
    var tap = AudioObjectID(kAudioObjectUnknown)
    let st = AudioHardwareCreateProcessTap(desc, &tap)
    if st == noErr && tap != kAudioObjectUnknown {
        AudioHardwareDestroyProcessTap(tap)
        return 0
    }
    return 1
}

/// 1 if this macOS can tap another app's audio (14.2 or later).
@_cdecl("onkey_macos_supported")
public func onkey_macos_supported() -> Int32 {
    ProcessInfo.processInfo.isOperatingSystemAtLeast(OperatingSystemVersion(majorVersion: 14, minorVersion: 2, patchVersion: 0)) ? 1 : 0
}

// MARK: mic level meter

/// The level of the microphone the take engine uses (`chooseMic` in capture.swift: so "Heard you" is said about
/// the mic your takes will hear), read straight from Core Audio (an IO proc on the device, as the take engine
/// reads its mic through its aggregate device). AVAudioEngine's input node was used before; in the app it could
/// sit silent next to the take engine's own capture, leaving setup's meter empty.
private final class Meter {
    var device = AudioObjectID(kAudioObjectUnknown)
    var proc: AudioDeviceIOProcID? = nil
    var level: Float = -120          // dBFS of the latest block
    var blocks: Int = 0              // blocks heard, to tell "running but silent" from "no audio arriving"
    // The mic's latest sound, for the tuner (tuner.rs reads it with onkey_mic_samples): one channel, the loudest
    // input that doesn't carry the music (Capture.knownCopies), in a ring written here on Core Audio's thread and
    // read by one reader. Fixed storage; `wrote` and `read` only ever grow.
    static let ringN = 1 << 17
    let ring = UnsafeMutablePointer<Float>.allocate(capacity: Meter.ringN)
    var wrote = 0, read = 0
    var rate: Double = 0
    var copies: UInt32 = 0
    init() { ring.initialize(repeating: 0, count: Meter.ringN) }
    deinit { ring.deallocate() }

    /// 0 running, 1 no input device, 2 couldn't make the IO proc, 3 couldn't start it
    func start() -> Int32 {
        guard let mic = chooseMic() else { return 1 }
        device = mic.id
        rate = deviceRate(device)
        copies = knownMusicInputs(mic.uid)
        let st = AudioDeviceCreateIOProcIDWithBlock(&proc, device, nil) { [weak self] _, inData, _, _, _ in
            guard let self else { return }
            let abl = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: inData))
            var sum: Float = 0
            var n = 0
            for buf in abl {
                guard let p = buf.mData?.assumingMemoryBound(to: Float.self) else { continue }
                let count = Int(buf.mDataByteSize) / MemoryLayout<Float>.size
                for i in 0..<count { sum += p[i] * p[i] }
                n += count
            }
            let rms = n > 0 ? (sum / Float(n)).squareRoot() : 0
            self.level = 20 * log10(max(rms, 1e-6))
            self.blocks += 1
            // the loudest input that isn't a copy of the music, into the ring
            var best: (p: UnsafeMutablePointer<Float>, stride: Int, at: Int, frames: Int)? = nil
            var bestE: Float = -1, c = 0
            for buf in abl {
                let ch = max(1, Int(buf.mNumberChannels))
                if let p = buf.mData?.assumingMemoryBound(to: Float.self) {
                    let frames = Int(buf.mDataByteSize) / (MemoryLayout<Float>.size * ch)
                    for k in 0..<ch where c + k >= 32 || (self.copies >> UInt32(c + k)) & 1 == 0 {
                        var e: Float = 0, i = 0
                        while i < frames { let v = p[i * ch + k]; e += v * v; i += 4 }
                        if e > bestE { bestE = e; best = (p, ch, k, frames) }
                    }
                }
                c += ch
            }
            if let b = best {
                var w = self.wrote
                for i in 0..<b.frames {
                    self.ring[w & (Meter.ringN - 1)] = b.p[i * b.stride + b.at]
                    w += 1
                }
                self.wrote = w
            }
        }
        guard st == noErr, proc != nil else { return 2 }
        if AudioDeviceStart(device, proc) != noErr {
            AudioDeviceDestroyIOProcID(device, proc!)
            proc = nil
            return 3
        }
        return 0
    }
    func stop() {
        if let p = proc {
            AudioDeviceStop(device, p)
            AudioDeviceDestroyIOProcID(device, p)
            proc = nil
        }
    }
}

private var meter: Meter? = nil
private let meterLock = NSLock()

/// Start (1) or stop (0) listening to the mic for the level meter. Returns 0 if it's running afterwards (or
/// stopped, when asked to stop), else why not: 1 no input device, 2/3 Core Audio refused.
@_cdecl("onkey_mic_meter")
public func onkey_mic_meter(_ on: Int32) -> Int32 {
    meterLock.lock(); defer { meterLock.unlock() }
    if on == 0 {
        meter?.stop()
        meter = nil
        return 0
    }
    if meter != nil { return 0 }
    let m = Meter()
    let r = m.start()
    if r == 0 { meter = m }
    return r
}

/// The mic's latest level in dBFS (-120 when the meter isn't running).
@_cdecl("onkey_mic_level")
public func onkey_mic_level() -> Float {
    meterLock.lock(); defer { meterLock.unlock() }
    return meter?.level ?? -120
}

/// The mic's sound since the last call, for the tuner: up to `cap` samples of one channel into `out`, at `rate` Hz.
/// Returns how many (0 when the meter isn't running or nothing new came). Sound older than the ring holds (a second
/// or more) is skipped.
@_cdecl("onkey_mic_samples")
public func onkey_mic_samples(_ out: UnsafeMutablePointer<Float>, _ cap: Int32, _ rate: UnsafeMutablePointer<Double>) -> Int32 {
    meterLock.lock(); defer { meterLock.unlock() }
    guard let m = meter, cap > 0 else { return 0 }
    rate.pointee = m.rate
    let w = m.wrote
    var r = max(m.read, w - Meter.ringN / 2)
    var n = 0
    while r < w && n < Int(cap) {
        out[n] = m.ring[r & (Meter.ringN - 1)]
        r += 1; n += 1
    }
    m.read = r
    return Int32(n)
}

/// How many audio blocks the meter has received since it started (0: nothing is arriving from the mic).
@_cdecl("onkey_mic_blocks")
public func onkey_mic_blocks() -> Int32 {
    meterLock.lock(); defer { meterLock.unlock() }
    return Int32(min(meter?.blocks ?? 0, Int(Int32.max)))
}

// MARK: control of the music apps (Automation)

/// Whether OnKey may send an app Apple events (macOS's Automation permission), by its bundle id, without ever
/// asking: 3 allowed, 2 denied, 0 not asked yet, 4 the app isn't running, 5 not installed, -1 unknown. It never
/// prompts (asking is done by sending the app a real Apple event: see permissions.rs `ask_automation`), and it
/// only asks macOS about a running app, as AEDeterminePermissionToAutomateTarget requires ("The target
/// AEAddressDesc must refer to an already running application"): about a closed one it was seen to wait
/// indefinitely (a 9-minute hang at launch), and asked with askUserIfNeeded it most likely waited until the app was next opened and
/// only then show macOS's question (setup's "Asking…" forever; the prompt came later, after "Listen only").
/// It can still take long: call it off the main thread (permissions.rs waits for it 1.5 s at most).
@_cdecl("onkey_automation")
public func onkey_automation(_ bundle: UnsafePointer<CChar>) -> Int32 {
    let id = String(cString: bundle)
    let present = onkey_app_running(bundle)
    if present != 1 { return present }
    var target = AEAddressDesc()
    let bytes = Array(id.utf8)
    let made = bytes.withUnsafeBytes { AECreateDesc(DescType(typeApplicationBundleID), $0.baseAddress, $0.count, &target) }
    if made != noErr { return -1 }
    defer { AEDisposeDesc(&target) }
    switch AEDeterminePermissionToAutomateTarget(&target, AEEventClass(typeWildCard), AEEventID(typeWildCard), false) {
    case noErr: return 3
    case OSStatus(errAEEventNotPermitted): return 2
    case OSStatus(errAEEventWouldRequireUserConsent): return 0
    case OSStatus(procNotFound): return 4
    default: return -1
    }
}

/// 1 the app (by bundle id) is running, 4 it isn't, 5 it isn't installed. Sends the app nothing.
@_cdecl("onkey_app_running")
public func onkey_app_running(_ bundle: UnsafePointer<CChar>) -> Int32 {
    let id = String(cString: bundle)
    if NSWorkspace.shared.urlForApplication(withBundleIdentifier: id) == nil { return 5 }
    return NSRunningApplication.runningApplications(withBundleIdentifier: id).isEmpty ? 4 : 1
}
