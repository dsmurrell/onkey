// Development builds only (built by build.rs with the `devtest` feature; never in a release): the ear's taps
// (src/ear.rs). Two Core Audio process taps in one private aggregate device, so both arrive on one sample clock:
//   music  the music app's processes (bundle id prefix, as capture.swift follows it)
//   voice  OnKey's sound: its own process (the voice in "Play with the song", native/voice.swift) and the processes
//          macOS holds OnKey responsible for (its window's, from WebKit's GPU process)
// Each frame goes to the callback as [music, voice], each mixed to mono. Unmuted: what you hear is unchanged.
import AudioToolbox
import CoreAudio
import Foundation

private func earProp<T>(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector, _ value: inout T,
                        _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> OSStatus {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    var size = UInt32(MemoryLayout<T>.size)
    return AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, &value)
}

private func earArray(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector,
                      _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> [AudioObjectID] {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(obj, &addr, 0, nil, &size) == noErr, size > 0 else { return [] }
    var out = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, &out) == noErr else { return [] }
    return out
}

private func earString(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector) -> String? {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    var value: Unmanaged<CFString>? = nil
    guard AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, &value) == noErr, let v = value else { return nil }
    return v.takeRetainedValue() as String
}

private typealias EarResponsibleFn = @convention(c) (pid_t) -> pid_t
private let earResponsible: EarResponsibleFn? = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid")
    .map { unsafeBitCast($0, to: EarResponsibleFn.self) }

private func earPid(_ p: AudioObjectID) -> pid_t {
    var pid: pid_t = 0
    return earProp(p, kAudioProcessPropertyPID, &pid) == noErr ? pid : 0
}

private func allProcs() -> [AudioObjectID] {
    earArray(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyProcessObjectList)
}

/// OnKey's sound: its own process (where "Play with the song" plays your voice, native/voice.swift) and the ones
/// macOS holds it responsible for (its window's, from WebKit's GPU process).
private func voiceProcs() -> [AudioObjectID] {
    let me = getpid()
    return allProcs().filter { p in
        let pid = earPid(p)
        return pid > 0 && (pid == me || earResponsible?(pid) == me)
    }
}

/// The music app's: its bundle id starts with `prefix`, and it isn't OnKey's window (WebKit's GPU process has the
/// same bundle id for Safari's sound and OnKey's).
private func musicProcs(_ prefix: String) -> [AudioObjectID] {
    let mine = Set(voiceProcs())
    return allProcs().filter { (earString($0, kAudioProcessPropertyBundleID) ?? "").hasPrefix(prefix) && !mine.contains($0) }
}

private func deviceLatencyMs(_ dev: AudioObjectID) -> Int {
    var rate = Float64(0)
    _ = earProp(dev, kAudioDevicePropertyNominalSampleRate, &rate)
    let o = kAudioObjectPropertyScopeOutput
    var lat: UInt32 = 0, safety: UInt32 = 0, buf: UInt32 = 0, slat: UInt32 = 0
    _ = earProp(dev, kAudioDevicePropertyLatency, &lat, o)
    _ = earProp(dev, kAudioDevicePropertySafetyOffset, &safety, o)
    _ = earProp(dev, kAudioDevicePropertyBufferFrameSize, &buf, o)
    if let s = earArray(dev, kAudioDevicePropertyStreams, o).first { _ = earProp(s, kAudioStreamPropertyLatency, &slat) }
    guard rate > 0 else { return 0 }
    return Int((Double(lat + safety + buf + slat) / rate * 1000).rounded())
}

private func describe(_ procs: [AudioObjectID]) -> [[String: Any]] {
    procs.map { p in
        var running: UInt32 = 0
        _ = earProp(p, kAudioProcessPropertyIsRunningOutput, &running)
        let devs = earArray(p, kAudioProcessPropertyDevices, kAudioObjectPropertyScopeOutput).map { d -> [String: Any] in
            ["name": earString(d, kAudioObjectPropertyName) ?? "?", "latency_ms": deviceLatencyMs(d)]
        }
        let pid = earPid(p)
        return ["bundle": earString(p, kAudioProcessPropertyBundleID) ?? "", "pid": Int(pid),
                "responsible": Int(earResponsible?(pid) ?? -1), "sounding": running != 0, "devices": devs]
    }
}

private final class Ear {
    let prefix: String
    var music: [AudioObjectID] = []
    var voice: [AudioObjectID] = []
    var musicTap = AudioObjectID(kAudioObjectUnknown)
    var voiceTap = AudioObjectID(kAudioObjectUnknown)
    let musicUUID = UUID()
    let voiceUUID = UUID()
    var aggID = AudioObjectID(kAudioObjectUnknown)
    var procID: AudioDeviceIOProcID? = nil
    var rescan: DispatchSourceTimer? = nil
    var scratch = [Float](repeating: 0, count: 2 * 8192)
    var rate: Double = 0

    init(prefix: String) { self.prefix = prefix }

    private func tap(_ procs: [AudioObjectID], _ uuid: UUID) -> AudioObjectID? {
        let d = CATapDescription(stereoMixdownOfProcesses: procs)
        d.uuid = uuid
        d.isPrivate = true
        d.muteBehavior = .unmuted
        var id = AudioObjectID(kAudioObjectUnknown)
        return AudioHardwareCreateProcessTap(d, &id) == noErr ? id : nil
    }

    /// 0 ok, 1 no music app process, 5 no OnKey window process, 2 tap refused, 3 aggregate failed, 4 start failed
    func start(cb: @escaping OnKeyAudioCallback, ctx: UnsafeMutableRawPointer?) -> Int32 {
        music = musicProcs(prefix)
        voice = voiceProcs()
        if music.isEmpty { return 1 }
        if voice.isEmpty { return 5 }
        guard let m = tap(music, musicUUID) else { return 2 }
        musicTap = m
        guard let v = tap(voice, voiceUUID) else { stop(); return 2 }
        voiceTap = v

        // clocked as OnKey's own capture is (capture.swift): by the mic it uses, else the output device (Bluetooth
        // headphones' output in a device that also does input can degrade what you hear). The mic's own input
        // streams come first and are ignored
        var outDev = AudioObjectID(kAudioObjectUnknown)
        _ = earProp(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultSystemOutputDevice, &outDev)
        let clockUID = chooseMic()?.uid ?? earString(outDev, kAudioDevicePropertyDeviceUID) ?? ""
        let agg: [String: Any] = [
            kAudioAggregateDeviceNameKey: "OnKey ear",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: clockUID,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: clockUID]],
            // (taps' streams come after the sub-device's, in this order: music, then voice)
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: musicUUID.uuidString],
                                              [kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: voiceUUID.uuidString]],
        ]
        if AudioHardwareCreateAggregateDevice(agg as CFDictionary, &aggID) != noErr { stop(); return 3 }
        let st = AudioDeviceCreateIOProcIDWithBlock(&procID, aggID, nil) { [unowned self] _, inData, _, _, _ in
            let abl = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: inData))
            guard abl.count >= 2 else { return }
            let a = abl[abl.count - 2], b = abl[abl.count - 1]
            guard let ap = a.mData, let bp = b.mData else { return }
            let ach = max(1, Int(a.mNumberChannels)), bch = max(1, Int(b.mNumberChannels))
            let frames = min(Int(a.mDataByteSize) / (4 * ach), Int(b.mDataByteSize) / (4 * bch), 8192)
            if frames <= 0 { return }
            let x = ap.assumingMemoryBound(to: Float.self), y = bp.assumingMemoryBound(to: Float.self)
            self.scratch.withUnsafeMutableBufferPointer { o in
                for i in 0..<frames {
                    o[2 * i] = ach > 1 ? 0.5 * (x[i * ach] + x[i * ach + 1]) : x[i * ach]
                    o[2 * i + 1] = bch > 1 ? 0.5 * (y[i * bch] + y[i * bch + 1]) : y[i * bch]
                }
                cb(o.baseAddress!, Int32(frames), ctx)
            }
        }
        if st != noErr { stop(); return 4 }
        if AudioDeviceStart(aggID, procID) != noErr { stop(); return 4 }
        var r = Float64(0)
        _ = earProp(aggID, kAudioDevicePropertyNominalSampleRate, &r)
        rate = r
        // Core Audio can replace a process's objects (and WebKit's GPU process can restart): re-point the taps
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue.global(qos: .utility))
        timer.schedule(deadline: .now() + 2, repeating: 2)
        timer.setEventHandler { [weak self] in self?.repoint() }
        timer.resume()
        rescan = timer
        return 0
    }

    private func retarget(_ tapID: AudioObjectID, _ procs: [AudioObjectID], _ uuid: UUID) {
        let d = CATapDescription(stereoMixdownOfProcesses: procs)
        d.uuid = uuid
        d.isPrivate = true
        d.muteBehavior = .unmuted
        var addr = AudioObjectPropertyAddress(mSelector: kAudioTapPropertyDescription, mScope: kAudioObjectPropertyScopeGlobal,
                                              mElement: kAudioObjectPropertyElementMain)
        var ref: CATapDescription = d
        _ = withUnsafeMutablePointer(to: &ref) {
            AudioObjectSetPropertyData(tapID, &addr, 0, nil, UInt32(MemoryLayout<CATapDescription>.size), $0)
        }
    }

    func repoint() {
        let m = musicProcs(prefix), v = voiceProcs()
        if !m.isEmpty && Set(m) != Set(music) { retarget(musicTap, m, musicUUID); music = m }
        if !v.isEmpty && Set(v) != Set(voice) { retarget(voiceTap, v, voiceUUID); voice = v }
    }

    func stop() {
        rescan?.cancel()
        rescan = nil
        if aggID != kAudioObjectUnknown {
            if let procID { AudioDeviceStop(aggID, procID); AudioDeviceDestroyIOProcID(aggID, procID) }
            AudioHardwareDestroyAggregateDevice(aggID)
        }
        if musicTap != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(musicTap) }
        if voiceTap != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(voiceTap) }
        procID = nil
        aggID = AudioObjectID(kAudioObjectUnknown)
        musicTap = AudioObjectID(kAudioObjectUnknown)
        voiceTap = AudioObjectID(kAudioObjectUnknown)
    }
}

private var ear: Ear? = nil
private var earPrefix = ""
private let earLock = NSLock()

/// Start the ear's taps (music: processes whose bundle id starts with `music`). 0 on success (rate in `outRate`),
/// else Ear.start's code.
@_cdecl("onkey_ear_start")
public func onkey_ear_start(_ music: UnsafePointer<CChar>, _ cb: @escaping OnKeyAudioCallback, _ ctx: UnsafeMutableRawPointer?,
                            _ outRate: UnsafeMutablePointer<Double>) -> Int32 {
    earLock.lock(); defer { earLock.unlock() }
    ear?.stop()
    ear = nil
    earPrefix = String(cString: music)
    let e = Ear(prefix: earPrefix)
    let code = e.start(cb: cb, ctx: ctx)
    if code == 0 {
        ear = e
        outRate.pointee = e.rate
    }
    return code
}

@_cdecl("onkey_ear_stop")
public func onkey_ear_stop() {
    earLock.lock(); defer { earLock.unlock() }
    ear?.stop()
    ear = nil
}

/// Which processes the ear taps (or would), whether each is sending sound, and the output devices each plays to
/// with their latency (ms): JSON {"music": [...], "voice": [...]}, copied into `buf`.
@_cdecl("onkey_ear_outputs")
public func onkey_ear_outputs(_ buf: UnsafeMutablePointer<CChar>, _ len: Int32) {
    earLock.lock()
    let prefix = earPrefix
    earLock.unlock()
    let v: [String: Any] = ["music": describe(musicProcs(prefix)), "voice": describe(voiceProcs())]
    let s = (try? JSONSerialization.data(withJSONObject: v)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    let bytes = Array(s.utf8.prefix(Int(len) - 1))
    for (i, c) in bytes.enumerated() { buf[i] = CChar(bitPattern: c) }
    buf[bytes.count] = 0
}
