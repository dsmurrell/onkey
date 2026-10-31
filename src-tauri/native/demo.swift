// The demo song's player: the one time OnKey plays music itself (src/demo.rs). An AVAudioPlayer in OnKey's own
// process, so the capture hears it exactly as it hears a music app: through a process tap of OnKey (its bundle id),
// sample-aligned with the microphone. Called from Rust through a small C interface; every call is quick.
import AVFoundation
import Foundation

private final class DemoPlayer {
    let player: AVAudioPlayer
    /// Asked to play (and not paused since): when the player has stopped by itself, it came to the end.
    var wanted = false
    init(_ p: AVAudioPlayer) { player = p }
}

private var demo: DemoPlayer? = nil
private let demoLock = NSLock()

/// Open the demo's audio file (any file Core Audio plays: AAC, MP3, WAV), ready to play from the start. Its length
/// in seconds, or -1 if it can't be opened.
@_cdecl("onkey_demo_open")
public func onkey_demo_open(_ path: UnsafePointer<CChar>) -> Double {
    demoLock.lock(); defer { demoLock.unlock() }
    demo?.player.stop()
    demo = nil
    let url = URL(fileURLWithPath: String(cString: path))
    guard let p = try? AVAudioPlayer(contentsOf: url) else { return -1 }
    p.prepareToPlay()
    demo = DemoPlayer(p)
    return p.duration
}

@_cdecl("onkey_demo_play")
public func onkey_demo_play() {
    demoLock.lock(); defer { demoLock.unlock() }
    guard let d = demo else { return }
    d.wanted = d.player.play()
}

@_cdecl("onkey_demo_pause")
public func onkey_demo_pause() {
    demoLock.lock(); defer { demoLock.unlock() }
    guard let d = demo else { return }
    d.player.pause()
    d.wanted = false
}

/// Go to `seconds` into the song (playing on if it was).
@_cdecl("onkey_demo_seek")
public func onkey_demo_seek(_ seconds: Double) {
    demoLock.lock(); defer { demoLock.unlock() }
    guard let d = demo else { return }
    d.player.currentTime = max(0, min(seconds, d.player.duration))
}

/// Stop and let the file go.
@_cdecl("onkey_demo_close")
public func onkey_demo_close() {
    demoLock.lock(); defer { demoLock.unlock() }
    demo?.player.stop()
    demo = nil
}

/// Where the player is (seconds) and whether it plays: 1 playing, 0 paused, 2 came to its end by itself (then the
/// position is the song's length), -1 nothing open.
@_cdecl("onkey_demo_state")
public func onkey_demo_state(_ position: UnsafeMutablePointer<Double>) -> Int32 {
    demoLock.lock(); defer { demoLock.unlock() }
    guard let d = demo else { position.pointee = 0; return -1 }
    if d.player.isPlaying {
        position.pointee = d.player.currentTime
        return 1
    }
    if d.wanted {
        // (AVAudioPlayer goes back to 0 when it finishes)
        position.pointee = d.player.duration
        return 2
    }
    position.pointee = d.player.currentTime
    return 0
}
