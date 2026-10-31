// OnKey's audio capture: one app's output (e.g. Spotify) via a Core Audio process tap, plus a microphone, on a
// single clock (macOS 14.2+). Built as a static library and called from Rust through a small C interface.
//
// Every callback delivers interleaved float frames [appL, appR, mic]: the mic is added to the same private
// aggregate device as the tap, so the music and your voice are sample-aligned. With a mic the mic clocks the
// aggregate and the output device is left out: putting Bluetooth headphones in a device that also does input
// can degrade what you hear.
import AppKit
import AudioToolbox
import CoreAudio
import Foundation

public typealias OnKeyAudioCallback = @convention(c) (UnsafePointer<Float>, Int32, UnsafeMutableRawPointer?) -> Void

// MARK: property helpers

private func readProp<T>(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector, _ value: inout T,
                         _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> OSStatus {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    var size = UInt32(MemoryLayout<T>.size)
    return AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, &value)
}

private func readArray(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector,
                       _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> [AudioObjectID] {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(obj, &addr, 0, nil, &size) == noErr, size > 0 else { return [] }
    var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, &ids) == noErr else { return [] }
    return ids
}

private func readString(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector) -> String? {
    var s: CFString? = nil
    let st = withUnsafeMutablePointer(to: &s) { ptr -> OSStatus in
        var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: kAudioObjectPropertyScopeGlobal,
                                              mElement: kAudioObjectPropertyElementMain)
        var size = UInt32(MemoryLayout<CFString?>.size)
        return AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, ptr)
    }
    return st == noErr ? (s as String?) : nil
}

private func inputChannels(_ dev: AudioObjectID) -> Int {
    var addr = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreamConfiguration,
                                          mScope: kAudioObjectPropertyScopeInput, mElement: kAudioObjectPropertyElementMain)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(dev, &addr, 0, nil, &size) == noErr, size > 0 else { return 0 }
    let raw = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: 16)
    defer { raw.deallocate() }
    guard AudioObjectGetPropertyData(dev, &addr, 0, nil, &size, raw) == noErr else { return 0 }
    let abl = UnsafeMutableAudioBufferListPointer(raw.assumingMemoryBound(to: AudioBufferList.self))
    return abl.reduce(0) { $0 + Int($1.mNumberChannels) }
}

// MARK: which microphone

private func transport(_ dev: AudioObjectID) -> UInt32 {
    var t: UInt32 = 0
    _ = readProp(dev, kAudioDevicePropertyTransportType, &t)
    return t
}

/// Bluetooth headphones' mic (AirPods): using it switches them to a call profile, and the music in them to call
/// quality.
private func isBluetooth(_ dev: AudioObjectID) -> Bool {
    let t = transport(dev)
    return t == kAudioDeviceTransportTypeBluetooth || t == kAudioDeviceTransportTypeBluetoothLE
}

/// A microphone: an input device, with Core Audio's id and uid for it and its name.
struct MicDevice {
    let id: AudioObjectID
    let uid: String
    let name: String
}

/// Every input device but aggregates (OnKey's own capture device among them).
func inputDevices() -> [MicDevice] {
    readArray(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDevices).compactMap { dev in
        guard inputChannels(dev) > 0, transport(dev) != kAudioDeviceTransportTypeAggregate,
              let uid = readString(dev, kAudioDevicePropertyDeviceUID) else { return nil }
        return MicDevice(id: dev, uid: uid, name: readString(dev, kAudioObjectPropertyName) ?? uid)
    }
}

private var preferredMic = ""            // a device uid chosen in Settings; "" automatic
private let micLock = NSLock()

/// The microphone OnKey listens to, the take engine and setup's mic check alike: the one chosen in Settings while
/// it's connected; else the Mac's default input (System Settings → Sound), unless that's Bluetooth headphones'
/// while the Mac has another (built-in first, then USB), so the music in them stays at full quality. nil: no
/// input device at all.
func chooseMic() -> MicDevice? {
    let devices = inputDevices()
    micLock.lock(); let pref = preferredMic; micLock.unlock()
    if !pref.isEmpty, let d = devices.first(where: { $0.uid == pref }) { return d }
    var def = AudioObjectID(kAudioObjectUnknown)
    _ = readProp(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultInputDevice, &def)
    let dflt = devices.first(where: { $0.id == def })
    if let d = dflt, !isBluetooth(d.id) { return d }
    return devices.first(where: { transport($0.id) == kAudioDeviceTransportTypeBuiltIn })
        ?? devices.first(where: { transport($0.id) == kAudioDeviceTransportTypeUSB })
        ?? dflt ?? devices.first
}

/// The microphone to prefer (a device uid from `onkey_mic_list`), or "" for automatic.
@_cdecl("onkey_set_mic_preference")
public func onkey_set_mic_preference(_ uid: UnsafePointer<CChar>) {
    micLock.lock(); preferredMic = String(cString: uid); micLock.unlock()
}

private func copyOut(_ s: String, _ buf: UnsafeMutablePointer<CChar>, _ len: Int32) {
    guard len > 0 else { return }
    _ = s.withCString { strncpy(buf, $0, Int(len) - 1) }
    buf[Int(len) - 1] = 0
}

/// The microphone OnKey would listen to now (`chooseMic`): its uid and name into the buffers; 0 if there's none.
@_cdecl("onkey_mic_choice")
public func onkey_mic_choice(_ uid: UnsafeMutablePointer<CChar>, _ name: UnsafeMutablePointer<CChar>, _ len: Int32) -> Int32 {
    guard let m = chooseMic() else { copyOut("", uid, len); copyOut("", name, len); return 0 }
    copyOut(m.uid, uid, len)
    copyOut(m.name, name, len)
    return 1
}

/// The microphones connected, as a JSON array of {"uid", "name", "virtual"} (copied into `buf`), for Settings'
/// list. "virtual": a device another app installed (BlackHole, Krisp, Loopback, Zoom, Teams): listed apart, since
/// picked by mistake it gives silence or the wrong sound, but kept, as people route their mic through one on purpose.
@_cdecl("onkey_mic_list")
public func onkey_mic_list(_ buf: UnsafeMutablePointer<CChar>, _ len: Int32) {
    let list: [[String: Any]] = inputDevices().map {
        ["uid": $0.uid, "name": $0.name, "virtual": transport($0.id) == kAudioDeviceTransportTypeVirtual]
    }
    let json = (try? JSONSerialization.data(withJSONObject: list)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
    copyOut(json, buf, len)
}

// MARK: sleep and wake

private var wakes: Int32 = 0
private let wakeLock = NSLock()
private var wakeObserver: NSObjectProtocol? = nil

/// How many times the Mac has woken from sleep since this was first asked (it starts watching then): after a
/// wake a process tap can go silent, so the engine restarts its capture when this changes.
@_cdecl("onkey_wake_count")
public func onkey_wake_count() -> Int32 {
    wakeLock.lock(); defer { wakeLock.unlock() }
    if wakeObserver == nil {
        wakeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: nil) { _ in
            wakeLock.lock(); wakes &+= 1; wakeLock.unlock()
        }
    }
    return wakes
}

// MARK: whose sound

private typealias ResponsibleFn = @convention(c) (pid_t) -> pid_t

/// The process macOS holds responsible for `pid` (the app that launched a helper), through the private call
/// audio and security tools use (read-only). nil if it isn't there.
private let responsibleFor: ResponsibleFn? = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid")
    .map { unsafeBitCast($0, to: ResponsibleFn.self) }

private func path(of pid: pid_t) -> String {
    var buf = [CChar](repeating: 0, count: 4096)
    return proc_pidpath(pid, &buf, UInt32(buf.count)) > 0 ? String(cString: buf) : ""
}

/// Whether a Core Audio process object counts as the app `prefix` names. WebKit plays the sound of every app
/// built on it (Safari, Mail, OnKey's own window) from a GPU process with the same bundle id, so there the app
/// responsible for that process decides: Safari counts; OnKey itself and other apps don't. If macOS won't say
/// whose it is, it counts (and OnKey stops listening to WebKit while it plays a take back, see listen.rs).
private func belongs(_ p: AudioObjectID, _ prefix: String) -> Bool {
    guard (readString(p, kAudioProcessPropertyBundleID) ?? "").hasPrefix(prefix) else { return false }
    guard prefix.hasPrefix("com.apple.WebKit") else { return true }
    var pid: pid_t = 0
    guard readProp(p, kAudioProcessPropertyPID, &pid) == noErr, pid > 0, let responsibleFor else { return true }
    let host = responsibleFor(pid)
    if host == getpid() { return false }
    if host <= 0 || host == pid { return true }
    let app = path(of: host)
    return app.isEmpty || app.contains("/Safari.app/") || app.contains("/Safari Technology Preview.app/")
}

/// 1 if WebKit's shared GPU process can be told apart by the app it plays for (see `belongs`).
@_cdecl("onkey_webkit_separable")
public func onkey_webkit_separable() -> Int32 {
    responsibleFor == nil ? 0 : 1
}

// MARK: capture

private final class Capture {
    let bundlePrefix: String
    var procs: [AudioObjectID] = []
    var tapID = AudioObjectID(kAudioObjectUnknown)
    var aggID = AudioObjectID(kAudioObjectUnknown)
    var procID: AudioDeviceIOProcID? = nil
    var tapUUID = UUID()
    var rescan: DispatchSourceTimer? = nil
    var scratch = [Float](repeating: 0, count: 3 * 8192)
    // An interface with several inputs (a Focusrite 2i2's mic on input 2): the mic channel is the one your voice is
    // on, not always the first. Each input channel's level (smoothed over ~0.2 s), and the channel in use: another
    // takes over once it's clearly louder (4x the energy), so it doesn't flick between them. Fixed storage: this is
    // read and written on Core Audio's real-time thread only.
    static let maxMicChannels = 64
    let micLevel = UnsafeMutablePointer<Float>.allocate(capacity: Capture.maxMicChannels)
    var micChannel = 0
    var micChannels = 0
    var rate: Double = 0
    var micName = ""
    var micUID = ""

    init(bundlePrefix: String) {
        self.bundlePrefix = bundlePrefix
        micLevel.initialize(repeating: 0, count: Capture.maxMicChannels)
    }
    deinit { micLevel.deallocate() }

    /// Updates each input channel's level from this block and picks the loudest (see `micLevel`). The mic's
    /// buffers are all but the last (the tap's); each may hold several interleaved channels.
    func pickMicChannel(_ abl: UnsafeMutableAudioBufferListPointer, frames: Int) {
        var c = 0
        for b in 0..<(abl.count - 1) {
            let buf = abl[b], ch = max(1, Int(buf.mNumberChannels))
            if let d = buf.mData {
                let p = d.assumingMemoryBound(to: Float.self), nf = min(Int(buf.mDataByteSize) / (4 * ch), frames)
                for k in 0..<ch where c + k < Capture.maxMicChannels {
                    var e: Float = 0, i = 0, n = 0
                    while i < nf { let v = p[i * ch + k]; e += v * v; i += 4; n += 1 }
                    micLevel[c + k] = 0.95 * micLevel[c + k] + 0.05 * (e / Float(max(1, n)))
                }
            }
            c += ch
        }
        micChannels = min(c, Capture.maxMicChannels)
        var best = micChannel < micChannels ? micChannel : 0
        for k in 0..<micChannels where micLevel[k] > 4 * micLevel[best] && micLevel[k] > 1e-8 { best = k }
        micChannel = best
    }

    func findProcs() -> [AudioObjectID] {
        readArray(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyProcessObjectList)
            .filter { belongs($0, bundlePrefix) }
    }

    /// 0 ok, 1 the app has no audio yet, 2 tap refused, 3 aggregate failed, 4 audio start failed
    func start(cb: @escaping OnKeyAudioCallback, ctx: UnsafeMutableRawPointer?) -> Int32 {
        procs = findProcs()
        if procs.isEmpty { return 1 }
        let desc = CATapDescription(stereoMixdownOfProcesses: procs)
        desc.uuid = tapUUID
        desc.isPrivate = true
        desc.muteBehavior = .unmuted
        if AudioHardwareCreateProcessTap(desc, &tapID) != noErr { return 2 }

        var outDev = AudioObjectID(kAudioObjectUnknown)
        _ = readProp(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultSystemOutputDevice, &outDev)
        let outUID = readString(outDev, kAudioDevicePropertyDeviceUID) ?? ""
        // the same microphone setup's mic check listens to (none: the takes are learned but can't be sung)
        let mic = chooseMic()
        let micUID = mic?.uid
        micName = mic?.name ?? ""
        self.micUID = micUID ?? ""
        let clockUID = micUID ?? outUID
        let agg: [String: Any] = [
            kAudioAggregateDeviceNameKey: "OnKey",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: clockUID,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: clockUID]],
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapDriftCompensationKey: true,
                                               kAudioSubTapUIDKey: tapUUID.uuidString]],
        ]
        if AudioHardwareCreateAggregateDevice(agg as CFDictionary, &aggID) != noErr { stop(); return 3 }

        let withMic = micUID != nil
        let st = AudioDeviceCreateIOProcIDWithBlock(&procID, aggID, nil) { [unowned self] _, inData, _, _, _ in
            let abl = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: inData))
            // taps come after the sub-device's streams, so the tap is the last buffer
            guard let tap = abl.last, let tp = tap.mData, tap.mDataByteSize > 0 else { return }
            let tch = max(1, Int(tap.mNumberChannels))
            let frames = min(Int(tap.mDataByteSize) / (4 * tch), 8192)
            let t = tp.assumingMemoryBound(to: Float.self)
            var m: UnsafeMutablePointer<Float>? = nil
            var mch = 1, micFrames = 0, mk = 0
            if withMic && abl.count >= 2 {
                self.pickMicChannel(abl, frames: frames)
                // the buffer holding the channel in use, and its place among that buffer's channels
                var c = 0
                for b in 0..<(abl.count - 1) {
                    let ch = max(1, Int(abl[b].mNumberChannels))
                    if self.micChannel < c + ch, let md = abl[b].mData {
                        m = md.assumingMemoryBound(to: Float.self)
                        mch = ch
                        mk = self.micChannel - c
                        micFrames = Int(abl[b].mDataByteSize) / (4 * ch)
                        break
                    }
                    c += ch
                }
            }
            self.scratch.withUnsafeMutableBufferPointer { o in
                for i in 0..<frames {
                    o[3 * i] = t[i * tch]
                    o[3 * i + 1] = tch > 1 ? t[i * tch + 1] : t[i * tch]
                    o[3 * i + 2] = (m != nil && i < micFrames) ? m![i * mch + mk] : 0
                }
                cb(o.baseAddress!, Int32(frames), ctx)
            }
        }
        if st != noErr { stop(); return 4 }
        if AudioDeviceStart(aggID, procID) != noErr { stop(); return 4 }
        var r = Float64(0)
        _ = readProp(aggID, kAudioDevicePropertyNominalSampleRate, &r)
        rate = r

        // Core Audio can replace an app's process objects; a tap pointed at the old ones goes silent, so re-point it.
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue.global(qos: .utility))
        timer.schedule(deadline: .now() + 2, repeating: 2)
        timer.setEventHandler { [weak self] in self?.repoint() }
        timer.resume()
        rescan = timer
        return 0
    }

    func repoint() {
        let now = findProcs()
        guard !now.isEmpty, Set(now) != Set(procs) else { return }
        let d = CATapDescription(stereoMixdownOfProcesses: now)
        d.uuid = tapUUID
        d.isPrivate = true
        d.muteBehavior = .unmuted
        var addr = AudioObjectPropertyAddress(mSelector: kAudioTapPropertyDescription, mScope: kAudioObjectPropertyScopeGlobal,
                                              mElement: kAudioObjectPropertyElementMain)
        var ref: CATapDescription = d
        _ = withUnsafeMutablePointer(to: &ref) {
            AudioObjectSetPropertyData(tapID, &addr, 0, nil, UInt32(MemoryLayout<CATapDescription>.size), $0)
        }
        procs = now
    }

    func stop() {
        rescan?.cancel()
        rescan = nil
        if aggID != kAudioObjectUnknown {
            if let procID { AudioDeviceStop(aggID, procID); AudioDeviceDestroyIOProcID(aggID, procID) }
            AudioHardwareDestroyAggregateDevice(aggID)
        }
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
        procID = nil
        aggID = AudioObjectID(kAudioObjectUnknown)
        tapID = AudioObjectID(kAudioObjectUnknown)
    }
}

private var current: Capture? = nil
private let lock = NSLock()

// MARK: C interface

/// Start capturing the app whose bundle id starts with `bundle`, plus the microphone `chooseMic` picks (none if
/// there's no input device). Returns 0 on success (sample rate in `outRate`), else an error code (see Capture.start).
@_cdecl("onkey_capture_start")
public func onkey_capture_start(_ bundle: UnsafePointer<CChar>,
                                _ cb: @escaping OnKeyAudioCallback, _ ctx: UnsafeMutableRawPointer?,
                                _ outRate: UnsafeMutablePointer<Double>) -> Int32 {
    lock.lock(); defer { lock.unlock() }
    current?.stop()
    let c = Capture(bundlePrefix: String(cString: bundle))
    let code = c.start(cb: cb, ctx: ctx)
    if code == 0 {
        current = c
        outRate.pointee = c.rate
    }
    return code
}

/// The mic's input channel in use (from 0) and how many it has (0 when not capturing), for the log.
@_cdecl("onkey_mic_channel")
public func onkey_mic_channel(_ count: UnsafeMutablePointer<Int32>) -> Int32 {
    count.pointee = Int32(current?.micChannels ?? 0)
    return Int32(current?.micChannel ?? 0)
}

@_cdecl("onkey_capture_stop")
public func onkey_capture_stop() {
    lock.lock(); defer { lock.unlock() }
    current?.stop()
    current = nil
}

/// The uid and name of the mic in use (UTF-8, copied into the buffers; "" when the capture has none).
@_cdecl("onkey_capture_mic")
public func onkey_capture_mic(_ uid: UnsafeMutablePointer<CChar>, _ name: UnsafeMutablePointer<CChar>, _ len: Int32) {
    lock.lock(); defer { lock.unlock() }
    copyOut(current?.micUID ?? "", uid, len)
    copyOut(current?.micName ?? "", name, len)
}

/// The default output device's uid (UTF-8, copied into the buffer; "" if unknown): what's learned about an output
/// (how far a music app's position runs ahead of what's heard on it) is kept per device.
@_cdecl("onkey_output_uid")
public func onkey_output_uid(_ uid: UnsafeMutablePointer<CChar>, _ len: Int32) {
    var out = AudioObjectID(0)
    _ = readProp(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice, &out)
    copyOut(readString(out, kAudioDevicePropertyDeviceUID) ?? "", uid, len)
}

/// The default output device's name (UTF-8, copied into the buffer; "" if unknown), for Settings' timing check.
@_cdecl("onkey_output_name")
public func onkey_output_name(_ name: UnsafeMutablePointer<CChar>, _ len: Int32) {
    var out = AudioObjectID(0)
    _ = readProp(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice, &out)
    copyOut(readString(out, kAudioObjectPropertyName) ?? "", name, len)
}

/// How long after an app plays audio you hear it on the default output (device + safety + buffer + stream), in ms.
@_cdecl("onkey_output_latency_ms")
public func onkey_output_latency_ms() -> Int32 {
    var out = AudioObjectID(0)
    _ = readProp(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice, &out)
    var rate = Float64(0)
    _ = readProp(out, kAudioDevicePropertyNominalSampleRate, &rate)
    let o = kAudioObjectPropertyScopeOutput
    var lat: UInt32 = 0, safety: UInt32 = 0, buf: UInt32 = 0, slat: UInt32 = 0
    _ = readProp(out, kAudioDevicePropertyLatency, &lat, o)
    _ = readProp(out, kAudioDevicePropertySafetyOffset, &safety, o)
    _ = readProp(out, kAudioDevicePropertyBufferFrameSize, &buf, o)
    if let s = readArray(out, kAudioDevicePropertyStreams, o).first { _ = readProp(s, kAudioStreamPropertyLatency, &slat) }
    guard rate > 0 else { return 0 }
    return Int32((Double(lat + safety + buf + slat) / rate * 1000).rounded())
}

/// 1 if a process whose bundle id starts with `bundle` is sending audio out right now (no tap, so no permission
/// needed): lets OnKey look for YouTube only in a browser that is making sound.
@_cdecl("onkey_app_sounding")
public func onkey_app_sounding(_ bundle: UnsafePointer<CChar>) -> Int32 {
    let prefix = String(cString: bundle)
    for p in readArray(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyProcessObjectList)
        where belongs(p, prefix) {
        var running: UInt32 = 0
        if readProp(p, kAudioProcessPropertyIsRunningOutput, &running) == noErr && running != 0 { return 1 }
    }
    return 0
}
