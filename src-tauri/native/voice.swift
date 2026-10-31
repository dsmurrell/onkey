// Your recording, played with the song ("Play with the song", src/voice.rs). Played here rather than by the
// window's Web Audio: WebKit holds its sound back ~0.1-0.15 s more than it reports, so the voice came out late. An
// AVAudioPlayerNode says exactly which sample it has rendered; that reaches the point where the music app's sound
// is heard (a process tap) a fixed HANDOFF later, and it can start at an exact moment, so the voice can be put on
// the song as a take's bars are.
// A peak limiter brings the quiet microphone up to a listening level (its pre-gain) without clipping.
import AVFoundation
import Foundation

private final class Voice {
    let engine = AVAudioEngine()
    let node = AVAudioPlayerNode()
    let limiter = AVAudioUnitEffect(audioComponentDescription: AudioComponentDescription(
        componentType: kAudioUnitType_Effect, componentSubType: kAudioUnitSubType_PeakLimiter,
        componentManufacturer: kAudioUnitManufacturer_Apple, componentFlags: 0, componentFlagsMask: 0))
    let file: AVAudioFile
    /// where the current play started in the recording (s); nil when stopped
    var from: Double? = nil
    init(_ f: AVAudioFile) {
        file = f
        engine.attach(node)
        engine.attach(limiter)
        engine.connect(node, to: limiter, format: f.processingFormat)
        engine.connect(limiter, to: engine.mainMixerNode, format: f.processingFormat)
    }
}

private var voice: Voice? = nil
private let voiceLock = NSLock()
private let FADE = 0.06
/// A start is set this far ahead (s): time to fade out what plays, then it begins exactly then.
private let LEAD = 0.1
/// What the node has rendered reaches the system's mix, where listening hears the song (a process tap), this much
/// later (s). Measured silently (a muted tap of this process, clocked by the built-in mic as listening is):
/// 0.061-0.064 s; the ear measured the same gap on the MacBook's speakers, AirPods and Sony headphones.
private let HANDOFF = 0.063

/// Open a take's recording. Its length (s), or -1 if it can't be opened.
@_cdecl("onkey_voice_open")
public func onkey_voice_open(_ path: UnsafePointer<CChar>) -> Double {
    voiceLock.lock(); defer { voiceLock.unlock() }
    voice?.engine.stop()
    voice = nil
    guard let f = try? AVAudioFile(forReading: URL(fileURLWithPath: String(cString: path))) else { return -1 }
    let v = Voice(f)
    v.engine.prepare()
    guard (try? v.engine.start()) != nil else { return -1 }
    voice = v
    return Double(f.length) / f.processingFormat.sampleRate
}

/// Louder or quieter (dB, -40 to 40): the limiter's pre-gain, so it never clips.
@_cdecl("onkey_voice_gain")
public func onkey_voice_gain(_ db: Double) {
    voiceLock.lock(); defer { voiceLock.unlock() }
    guard let v = voice else { return }
    AudioUnitSetParameter(v.limiter.audioUnit, kLimiterParam_PreGain, kAudioUnitScope_Global, 0, Float(max(-40, min(40, db))), 0)
}

/// Fade the node's volume to `to` over FADE (on the caller's thread: a few ms steps).
private func fade(_ n: AVAudioPlayerNode, to: Float) {
    let from = n.volume, steps = 6
    for i in 1...steps {
        n.volume = from + (to - from) * Float(i) / Float(steps)
        Thread.sleep(forTimeInterval: FADE / Double(steps))
    }
}

/// Play from `seconds` into the recording, as heard now (a move if it plays already: a short fade out, then in).
/// It begins exactly LEAD from now, that much further on, and HANDOFF further still (rendered that much before
/// it's heard).
@_cdecl("onkey_voice_play")
public func onkey_voice_play(_ seconds: Double) {
    voiceLock.lock(); defer { voiceLock.unlock() }
    guard let v = voice else { return }
    let at = mach_absolute_time() + AVAudioTime.hostTime(forSeconds: LEAD)
    if v.from != nil { fade(v.node, to: 0) }
    v.node.stop()
    let rate = v.file.processingFormat.sampleRate
    let start = AVAudioFramePosition(max(0, seconds + LEAD + HANDOFF) * rate)
    guard start < v.file.length else { v.from = nil; return }
    if !v.engine.isRunning { try? v.engine.start() }
    v.node.volume = 0
    v.node.scheduleSegment(v.file, startingFrame: start, frameCount: AVAudioFrameCount(v.file.length - start), at: nil)
    v.node.play(at: AVAudioTime(hostTime: at))
    v.from = Double(start) / rate
    // (faded in from when it begins)
    let now = mach_absolute_time()
    if at > now { Thread.sleep(forTimeInterval: AVAudioTime.seconds(forHostTime: at - now)) }
    fade(v.node, to: 1)
}

/// Stop (a short fade out).
@_cdecl("onkey_voice_stop")
public func onkey_voice_stop() {
    voiceLock.lock(); defer { voiceLock.unlock() }
    guard let v = voice, v.from != nil else { return }
    fade(v.node, to: 0)
    v.node.stop()
    v.from = nil
}

/// Let the recording go.
@_cdecl("onkey_voice_close")
public func onkey_voice_close() {
    voiceLock.lock(); defer { voiceLock.unlock() }
    voice?.node.stop()
    voice?.engine.stop()
    voice = nil
}

/// Where in the recording the sound heard now is (s: rendered, less HANDOFF), or -1 when it isn't playing (stopped,
/// not begun yet, or past its end).
@_cdecl("onkey_voice_position")
public func onkey_voice_position() -> Double {
    voiceLock.lock(); defer { voiceLock.unlock() }
    guard let v = voice, let from = v.from, v.node.isPlaying,
          let nt = v.node.lastRenderTime, let pt = v.node.playerTime(forNodeTime: nt), pt.sampleTime > 0 else { return -1 }
    let pos = from + Double(pt.sampleTime) / pt.sampleRate
    return pos < Double(v.file.length) / v.file.processingFormat.sampleRate ? pos - HANDOFF : -1
}
