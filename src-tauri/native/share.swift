// "Share this take": a vertical video (1080x1920, 30 fps, H.264 + AAC) of a stretch of a take: the song's title at
// the top, the middle left clear for Instagram's lyrics sticker, and the note lane at the bottom (the singer's notes,
// your pitch line coloured by how close it was), with the notes hit counting up; over the last moments an end card
// (the icon, what OnKey is, where to get it). Your voice only: the song is added in Instagram. Called from Rust (src/share.rs), which works out everything drawn, as JSON; this only draws and
// encodes it with macOS's own AVFoundation, on the caller's thread.
import AVFoundation
import AppKit
import CoreText
import Foundation

private let W = 1080, H = 1920, FPS: Int32 = 30
// the lane (as first prototyped in Python): where it sits, what it shows around the playhead
private let LX0: CGFloat = 40, LX1: CGFloat = 1040, LY0: CGFloat = 1210, LY1: CGFloat = 1650
private let BEFORE: CGFloat = 1.3, AFTER: CGFloat = 2.7

private struct Spec {
    let title: String, artist: String, footer: String
    let seconds: Double
    /// the end card's lines (what, for what, where) and how long it's up at the end (s)
    let end: [String], endSeconds: Double
    let lo: Double, hi: Double
    let notes: [(t0: Double, t1: Double, midi: Double, hit: Double)]
    let line: [(t: Double, midi: Double?, err: Double?)]
    let step: Double

    init?(_ json: String) {
        guard let d = json.data(using: .utf8), let o = try? JSONSerialization.jsonObject(with: d) as? [String: Any] else { return nil }
        title = o["title"] as? String ?? ""
        artist = o["artist"] as? String ?? ""
        footer = o["footer"] as? String ?? ""
        seconds = o["seconds"] as? Double ?? 0
        end = o["end"] as? [String] ?? []
        endSeconds = o["end_s"] as? Double ?? 0
        lo = o["lo"] as? Double ?? 48
        hi = o["hi"] as? Double ?? 72
        step = o["step"] as? Double ?? 0.01
        notes = (o["notes"] as? [[Double]] ?? []).compactMap { $0.count >= 4 ? ($0[0], $0[1], $0[2], $0[3]) : nil }
        line = (o["line"] as? [[Any]] ?? []).compactMap { a in
            guard let t = a.first as? Double else { return nil }
            return (t, a.count > 1 ? a[1] as? Double : nil, a.count > 2 ? a[2] as? Double : nil)
        }
        if seconds <= 0 { return nil }
    }
}

private func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: r / 255, green: g / 255, blue: b / 255, alpha: a)
}

/// Text centred at `y` (top of the line, top-left coordinates as the rest of the drawing).
private func text(_ ctx: CGContext, _ s: String, y: CGFloat, size: CGFloat, color: CGColor, weight: NSFont.Weight = .regular,
                  x: CGFloat? = nil) {
    let font = NSFont.systemFont(ofSize: size, weight: weight)
    let attr = NSAttributedString(string: s, attributes: [.font: font, .foregroundColor: NSColor(cgColor: color) ?? .white])
    let line = CTLineCreateWithAttributedString(attr)
    let width = CTLineGetTypographicBounds(line, nil, nil, nil)
    ctx.saveGState()
    // (the context is flipped to top-left coordinates: text is drawn the right way up by flipping it back)
    ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
    ctx.textPosition = CGPoint(x: x ?? (CGFloat(W) - CGFloat(width)) / 2, y: y + font.ascender)
    CTLineDraw(line, ctx)
    ctx.restoreGState()
}

/// Shortened with "…" to fit `maxWidth` at `size`.
private func fitted(_ s: String, size: CGFloat, weight: NSFont.Weight, maxWidth: CGFloat) -> String {
    let font = NSFont.systemFont(ofSize: size, weight: weight)
    func w(_ t: String) -> CGFloat { (t as NSString).size(withAttributes: [.font: font]).width }
    if w(s) <= maxWidth { return s }
    var t = s
    while !t.isEmpty && w(t + "…") > maxWidth { t.removeLast() }
    return t + "…"
}

/// A full frame's canvas, flipped to top-left coordinates, with the clip's gradient drawn.
private func canvas() -> CGContext? {
    guard let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0,
                              space: CGColorSpace(name: CGColorSpace.sRGB)!,
                              bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue) else { return nil }
    ctx.translateBy(x: 0, y: CGFloat(H)); ctx.scaleBy(x: 1, y: -1)
    let grad = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB)!,
                          colors: [rgb(22, 20, 38), rgb(48, 38, 112)] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: 0), end: CGPoint(x: CGFloat(W) * 0.4, y: CGFloat(H)), options: [.drawsAfterEndLocation])
    return ctx
}

/// The icon in `rect`, rounded by `corner` (top-left coordinates).
private func drawIcon(_ ctx: CGContext, _ icon: CGImage, in rect: CGRect, corner: CGFloat) {
    ctx.saveGState()
    ctx.addPath(CGPath(roundedRect: rect, cornerWidth: corner, cornerHeight: corner, transform: nil))
    ctx.clip()
    // (images draw upside down in the flipped context: flipped back around their own box)
    ctx.translateBy(x: 0, y: rect.minY * 2 + rect.height); ctx.scaleBy(x: 1, y: -1)
    ctx.draw(icon, in: rect)
    ctx.restoreGState()
}

/// The end card: the icon, what OnKey is ("Sing on key"), for what ("Free for Mac") and where (the site, in a pill
/// like the lane's panel), centred on the clip's gradient.
private func endCard(_ spec: Spec, icon: CGImage?) -> CGImage? {
    guard spec.end.count >= 3, spec.endSeconds > 0, let ctx = canvas() else { return nil }
    let iconSize: CGFloat = 280, iy: CGFloat = 600
    if let icon {
        // a soft glow behind it, in the notes' violet
        ctx.saveGState()
        ctx.setShadow(offset: .zero, blur: 120, color: rgb(124, 108, 246, 0.55))
        ctx.addPath(CGPath(roundedRect: CGRect(x: (CGFloat(W) - iconSize) / 2, y: iy, width: iconSize, height: iconSize),
                           cornerWidth: 64, cornerHeight: 64, transform: nil))
        ctx.setFillColor(rgb(48, 38, 112)); ctx.fillPath()
        ctx.restoreGState()
        drawIcon(ctx, icon, in: CGRect(x: (CGFloat(W) - iconSize) / 2, y: iy, width: iconSize, height: iconSize), corner: 64)
    }
    text(ctx, spec.end[0], y: iy + iconSize + 70, size: 100, color: rgb(255, 255, 255), weight: .bold)
    text(ctx, spec.end[1], y: iy + iconSize + 200, size: 52, color: rgb(190, 185, 230))
    let site = spec.end[2], siteSize: CGFloat = 48
    let sw = (site as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: siteSize, weight: .semibold)]).width
    let pill = CGRect(x: (CGFloat(W) - sw) / 2 - 44, y: iy + iconSize + 320, width: sw + 88, height: 100)
    ctx.addPath(CGPath(roundedRect: pill, cornerWidth: 50, cornerHeight: 50, transform: nil))
    ctx.setFillColor(rgb(18, 16, 34)); ctx.fillPath()
    text(ctx, site, y: pill.minY + (pill.height - siteSize * 1.2) / 2, size: siteSize, color: rgb(181, 168, 255), weight: .semibold)
    return ctx.makeImage()
}

/// What stays the same in every frame: the background, title, the lane's panel and the footer.
private func background(_ spec: Spec, icon: CGImage?) -> CGImage? {
    guard let ctx = canvas() else { return nil }
    text(ctx, fitted(spec.title, size: 72, weight: .semibold, maxWidth: 960), y: 150, size: 72, color: rgb(255, 255, 255), weight: .semibold)
    text(ctx, fitted(spec.artist, size: 40, weight: .regular, maxWidth: 960), y: 245, size: 40, color: rgb(190, 185, 230))
    let panel = CGPath(roundedRect: CGRect(x: LX0 - 16, y: LY0 - 16, width: LX1 - LX0 + 32, height: LY1 - LY0 + 32),
                       cornerWidth: 36, cornerHeight: 36, transform: nil)
    ctx.addPath(panel); ctx.setFillColor(rgb(18, 16, 34)); ctx.fillPath()
    // footer: the icon (rounded) and the site
    let footerFont: CGFloat = 40
    let label = spec.footer
    let lw = (label as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: footerFont, weight: .medium)]).width
    let iconSize: CGFloat = 76, gap: CGFloat = 20
    let x0 = (CGFloat(W) - (iconSize + gap + lw)) / 2, iy: CGFloat = 1788
    if let icon {
        drawIcon(ctx, icon, in: CGRect(x: x0, y: iy, width: iconSize, height: iconSize), corner: 18)
    }
    text(ctx, label, y: iy + (iconSize - footerFont * 1.2) / 2, size: footerFont, color: rgb(225, 220, 250), weight: .medium,
         x: x0 + iconSize + gap)
    return ctx.makeImage()
}

private let GOOD = rgb(52, 210, 123), CLOSE = rgb(245, 184, 61), OFF = rgb(242, 98, 107), NONE = rgb(139, 148, 171)

/// One frame, `now` seconds into the clip, drawn into `ctx` (top-left coordinates).
private func drawFrame(_ ctx: CGContext, _ spec: Spec, bg: CGImage, end: CGImage?, now: Double) {
    ctx.saveGState()
    ctx.translateBy(x: 0, y: CGFloat(H)); ctx.scaleBy(x: 1, y: -1)
    ctx.translateBy(x: 0, y: CGFloat(H)); ctx.scaleBy(x: 1, y: -1)   // (the background image is drawn the right way up)
    ctx.draw(bg, in: CGRect(x: 0, y: 0, width: W, height: H))
    ctx.restoreGState()
    ctx.saveGState()
    ctx.translateBy(x: 0, y: CGFloat(H)); ctx.scaleBy(x: 1, y: -1)
    let pxs = (LX1 - LX0) / (BEFORE + AFTER)
    let head = LX0 + BEFORE * pxs
    let xOf = { (t: Double) -> CGFloat in head + CGFloat(t - now) * pxs }
    let yOf = { (m: Double) -> CGFloat in LY1 - 20 - CGFloat((m - spec.lo) / (spec.hi - spec.lo)) * (LY1 - LY0 - 40) }
    ctx.saveGState()
    ctx.clip(to: CGRect(x: LX0, y: LY0 - 16, width: LX1 - LX0, height: LY1 - LY0 + 32))
    // the singer's notes (a hit one brighter once it's passed)
    for n in spec.notes {
        let a = max(LX0, xOf(n.t0)), b = min(LX1, xOf(n.t1) - 3)
        if b - a < 6 { continue }
        let y = yOf(n.midi)
        let past = n.t1 <= now
        let col = past && n.hit >= 0.5 ? rgb(150, 132, 255) : rgb(98, 82, 224, past ? 0.6 : 0.92)
        let r = min(17, (b - a) / 2)
        ctx.addPath(CGPath(roundedRect: CGRect(x: a, y: y - 17, width: b - a, height: 34), cornerWidth: r, cornerHeight: r, transform: nil))
        ctx.setFillColor(col); ctx.fillPath()
    }
    // your pitch line, up to now, coloured by how close it was
    ctx.setLineWidth(11); ctx.setLineCap(.round)
    var prev: (t: Double, midi: Double?, err: Double?)? = nil
    for p in spec.line {
        if p.t > now { break }
        defer { prev = p }
        guard let q = prev, p.t >= now - Double(BEFORE) - 0.05, let m0 = q.midi, let m1 = p.midi, abs(m1 - m0) <= 1.5 else { continue }
        let e = p.err
        ctx.setStrokeColor(e == nil ? NONE : e! < 50 ? GOOD : e! < 100 ? CLOSE : OFF)
        ctx.move(to: CGPoint(x: xOf(q.t), y: yOf(m0))); ctx.addLine(to: CGPoint(x: xOf(p.t), y: yOf(m1))); ctx.strokePath()
    }
    ctx.restoreGState()
    // the playhead, and a dot where your voice is now
    ctx.setStrokeColor(rgb(255, 255, 255, 0.27)); ctx.setLineWidth(3)
    ctx.move(to: CGPoint(x: head, y: LY0)); ctx.addLine(to: CGPoint(x: head, y: LY1)); ctx.strokePath()
    if let cur = spec.line.last(where: { $0.t <= now }), now - cur.t < 0.05, let m = cur.midi {
        ctx.setFillColor(rgb(255, 255, 255)); ctx.fillEllipse(in: CGRect(x: head - 13, y: yOf(m) - 13, width: 26, height: 26))
    }
    // the notes hit so far in the clip
    let hit = spec.notes.filter { $0.t0 >= 0 && $0.t0 < spec.seconds && $0.t1 <= now && $0.hit >= 0.5 }.count
    text(ctx, "\(hit) note\(hit == 1 ? "" : "s") hit", y: 1690, size: 56, color: rgb(181, 168, 255), weight: .semibold)
    ctx.restoreGState()
    // the end card, faded in over a quarter of a second at the start of its time
    if let end {
        let into = now - (spec.seconds - spec.endSeconds)
        if into >= 0 {
            ctx.saveGState()
            ctx.setAlpha(CGFloat(min(1, into / 0.25)))
            ctx.draw(end, in: CGRect(x: 0, y: 0, width: W, height: H))
            ctx.restoreGState()
        }
    }
}

/// Renders the frames to a silent H.264 .mov at `url`.
private func writeVideo(_ spec: Spec, icon: CGImage?, to url: URL) -> Int32 {
    try? FileManager.default.removeItem(at: url)
    let end = endCard(spec, icon: icon)
    guard let bg = background(spec, icon: icon), let writer = try? AVAssetWriter(outputURL: url, fileType: .mov) else { return -2 }
    let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
        AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: W, AVVideoHeightKey: H,
        AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: 6_000_000, AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel],
    ])
    input.expectsMediaDataInRealTime = false
    let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA, kCVPixelBufferWidthKey as String: W,
        kCVPixelBufferHeightKey as String: H, kCVPixelBufferCGImageCompatibilityKey as String: true,
        kCVPixelBufferCGBitmapContextCompatibilityKey as String: true,
    ])
    guard writer.canAdd(input) else { return -3 }
    writer.add(input)
    guard writer.startWriting() else { return -4 }
    writer.startSession(atSourceTime: .zero)
    let frames = Int(spec.seconds * Double(FPS))
    for k in 0..<frames {
        while !input.isReadyForMoreMediaData { usleep(1000) }
        guard let pool = adaptor.pixelBufferPool else { return -5 }
        var pb: CVPixelBuffer?
        CVPixelBufferPoolCreatePixelBuffer(nil, pool, &pb)
        guard let buf = pb else { return -6 }
        CVPixelBufferLockBaseAddress(buf, [])
        if let ctx = CGContext(data: CVPixelBufferGetBaseAddress(buf), width: W, height: H, bitsPerComponent: 8,
                               bytesPerRow: CVPixelBufferGetBytesPerRow(buf), space: CGColorSpace(name: CGColorSpace.sRGB)!,
                               bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue) {
            drawFrame(ctx, spec, bg: bg, end: end, now: Double(k) / Double(FPS))
        }
        CVPixelBufferUnlockBaseAddress(buf, [])
        if !adaptor.append(buf, withPresentationTime: CMTime(value: CMTimeValue(k), timescale: FPS)) { return -7 }
    }
    input.markAsFinished()
    let done = DispatchSemaphore(value: 0)
    writer.finishWriting { done.signal() }
    done.wait()
    return writer.status == .completed ? 0 : -8
}

/// The silent video and the voice (an .m4a) into one .mp4 at `out`, without re-encoding either.
private func mux(video: URL, audio: URL, out: URL, seconds: Double) -> Int32 {
    let comp = AVMutableComposition()
    let v = AVURLAsset(url: video), a = AVURLAsset(url: audio)
    let range = CMTimeRange(start: .zero, duration: CMTime(seconds: seconds, preferredTimescale: 600))
    let sem = DispatchSemaphore(value: 0)
    var vt: AVAssetTrack?, at: AVAssetTrack?
    Task {
        vt = try? await v.loadTracks(withMediaType: .video).first
        at = try? await a.loadTracks(withMediaType: .audio).first
        sem.signal()
    }
    sem.wait()
    guard let vt, let cv = comp.addMutableTrack(withMediaType: .video, preferredTrackID: kCMPersistentTrackID_Invalid),
          (try? cv.insertTimeRange(range, of: vt, at: .zero)) != nil else { return -10 }
    if let at, let ca = comp.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid) {
        let aRange = CMTimeRange(start: .zero, duration: min(range.duration, at.timeRange.duration))
        _ = try? ca.insertTimeRange(aRange, of: at, at: .zero)
    }
    try? FileManager.default.removeItem(at: out)
    guard let ex = AVAssetExportSession(asset: comp, presetName: AVAssetExportPresetPassthrough) else { return -11 }
    ex.outputURL = out
    ex.outputFileType = .mp4
    ex.exportAsynchronously { sem.signal() }
    sem.wait()
    return ex.status == .completed ? 0 : -12
}

/// Renders the clip described by `json` (src/share.rs) with the voice in `audio` (an .m4a, already cut and raised)
/// to `out` (.mp4). `icon`: PNG bytes for the footer. 0, or a negative step that failed.
@_cdecl("onkey_share_render")
public func onkey_share_render(_ json: UnsafePointer<CChar>, _ audio: UnsafePointer<CChar>, _ out: UnsafePointer<CChar>,
                               _ icon: UnsafePointer<UInt8>?, _ iconLen: Int64) -> Int32 {
    guard let spec = Spec(String(cString: json)) else { return -1 }
    var img: CGImage? = nil
    if let icon, iconLen > 0, let src = CGImageSourceCreateWithData(Data(bytes: icon, count: Int(iconLen)) as CFData, nil) {
        img = CGImageSourceCreateImageAtIndex(src, 0, nil)
    }
    let outURL = URL(fileURLWithPath: String(cString: out))
    let tmp = outURL.deletingLastPathComponent().appendingPathComponent(".\(UUID().uuidString).mov")
    defer { try? FileManager.default.removeItem(at: tmp) }
    let st = writeVideo(spec, icon: img, to: tmp)
    if st != 0 { return st }
    return mux(video: tmp, audio: URL(fileURLWithPath: String(cString: audio)), out: outURL, seconds: spec.seconds)
}

/// The tracks in a movie file: video count * 10 + audio count (for tests). -1 when it doesn't open.
@_cdecl("onkey_media_tracks")
public func onkey_media_tracks(_ path: UnsafePointer<CChar>) -> Int32 {
    let asset = AVURLAsset(url: URL(fileURLWithPath: String(cString: path)))
    let sem = DispatchSemaphore(value: 0)
    var n: Int32 = -1
    Task {
        if let v = try? await asset.loadTracks(withMediaType: .video), let a = try? await asset.loadTracks(withMediaType: .audio) {
            n = Int32(v.count * 10 + a.count)
        }
        sem.signal()
    }
    sem.wait()
    return n
}
