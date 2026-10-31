// Take recordings as AAC (.m4a), with macOS's own encoder (Audio Toolbox's ExtAudioFile): mono, about 64 kbps,
// ~0.5 MB a minute instead of 16-bit WAV's 5.8 (a share clip's voice higher, as it's encoded a second time). Called from Rust (src/takeaudio.rs) through a small C interface.
// Everything here runs on the caller's thread (the encoder makes none of its own), so a background-QoS caller
// keeps it at background priority.
import AudioToolbox
import Foundation

private let takeBitRate: UInt32 = 64_000
private let chunkFrames = 16_384

private func url(_ p: UnsafePointer<CChar>) -> CFURL {
    URL(fileURLWithPath: String(cString: p)) as CFURL
}

/// Float32 interleaved PCM, `channels` channels at `rate`.
private func floatFormat(_ rate: Double, _ channels: UInt32) -> AudioStreamBasicDescription {
    AudioStreamBasicDescription(mSampleRate: rate, mFormatID: kAudioFormatLinearPCM,
                                mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
                                mBytesPerPacket: 4 * channels, mFramesPerPacket: 1, mBytesPerFrame: 4 * channels,
                                mChannelsPerFrame: channels, mBitsPerChannel: 32, mReserved: 0)
}

/// AAC-LC takes 8-48 kHz: the capture's rate when it's a standard one up to 48 kHz, else resampled to 48 kHz
/// (or the next standard rate up from a lower one).
private func aacRate(_ rate: Double) -> Double {
    let standard: [Double] = [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000]
    if standard.contains(rate) { return rate }
    return standard.first(where: { $0 >= rate }) ?? 48000
}

/// A new mono AAC .m4a at `path` (replacing any file there), taking float mono at `rate` from the caller.
/// `target`: the bit rate to aim for (bits/s).
private func createAAC(_ path: UnsafePointer<CChar>, _ rate: Double, _ target: UInt32) -> (ExtAudioFileRef?, OSStatus) {
    var fmt = AudioStreamBasicDescription(mSampleRate: aacRate(rate), mFormatID: kAudioFormatMPEG4AAC, mFormatFlags: 0,
                                          mBytesPerPacket: 0, mFramesPerPacket: 1024, mBytesPerFrame: 0,
                                          mChannelsPerFrame: 1, mBitsPerChannel: 0, mReserved: 0)
    var file: ExtAudioFileRef?
    var st = ExtAudioFileCreateWithURL(url(path), kAudioFileM4AType, &fmt, nil, AudioFileFlags.eraseFile.rawValue, &file)
    guard st == noErr, let f = file else { return (nil, st) }
    var client = floatFormat(rate, 1)
    st = ExtAudioFileSetProperty(f, kExtAudioFileProperty_ClientDataFormat, UInt32(MemoryLayout.size(ofValue: client)), &client)
    guard st == noErr else { ExtAudioFileDispose(f); return (nil, st) }
    // the bit rate: set on the file's converter, then the converter told to take it up (a rate the codec
    // doesn't offer at this sample rate is left at its default rather than failing the take)
    var conv: AudioConverterRef?
    var size = UInt32(MemoryLayout<AudioConverterRef?>.size)
    if ExtAudioFileGetProperty(f, kExtAudioFileProperty_AudioConverter, &size, &conv) == noErr, let c = conv {
        var br = bitRate(c, target)
        if AudioConverterSetProperty(c, kAudioConverterEncodeBitRate, UInt32(MemoryLayout<UInt32>.size), &br) == noErr {
            var config: UnsafeRawPointer? = nil      // (a null CFPropertyListRef: "rebuild from its properties")
            _ = ExtAudioFileSetProperty(f, kExtAudioFileProperty_ConverterConfig,
                                        UInt32(MemoryLayout<UnsafeRawPointer?>.size), &config)
        }
    }
    return (f, noErr)
}

/// `target`, or the nearest the encoder offers at this sample rate (lower rates allow less: 16 kHz mono
/// tops out under 64 kbps).
private func bitRate(_ c: AudioConverterRef, _ target: UInt32) -> UInt32 {
    var size: UInt32 = 0
    guard AudioConverterGetPropertyInfo(c, kAudioConverterApplicableEncodeBitRates, &size, nil) == noErr, size > 0 else {
        return target
    }
    var ranges = [AudioValueRange](repeating: AudioValueRange(), count: Int(size) / MemoryLayout<AudioValueRange>.size)
    guard AudioConverterGetProperty(c, kAudioConverterApplicableEncodeBitRates, &size, &ranges) == noErr else { return target }
    let rates = ranges.flatMap { [$0.mMinimum, $0.mMaximum] }.filter { $0 > 0 }
    guard let best = rates.min(by: { abs($0 - Double(target)) < abs($1 - Double(target)) }) else { return target }
    return UInt32(best)
}

private func writeFloats(_ f: ExtAudioFileRef, _ p: UnsafeMutablePointer<Float>, _ frames: Int) -> OSStatus {
    var list = AudioBufferList(mNumberBuffers: 1, mBuffers: AudioBuffer(mNumberChannels: 1, mDataByteSize: UInt32(frames * 4),
                                                                         mData: UnsafeMutableRawPointer(p)))
    return ExtAudioFileWrite(f, UInt32(frames), &list)
}

/// 16-bit mono samples at `rate` to a mono AAC .m4a at `path`, at about `bitRate` bits/s. 0, or the Core Audio error.
@_cdecl("onkey_aac_write")
public func onkey_aac_write(_ samples: UnsafePointer<Int16>?, _ frames: Int64, _ rate: Double, _ bitRate: UInt32,
                            _ path: UnsafePointer<CChar>) -> Int32 {
    guard rate > 0 else { return -50 }
    let (file, st) = createAAC(path, rate, bitRate)
    guard let f = file else { return st }
    var buf = [Float](repeating: 0, count: chunkFrames)
    var done = 0
    var err: OSStatus = noErr
    let total = Int(max(frames, 0))
    while done < total && err == noErr {
        let n = min(chunkFrames, total - done)
        for i in 0..<n { buf[i] = Float(samples![done + i]) / 32768 }
        err = buf.withUnsafeMutableBufferPointer { writeFloats(f, $0.baseAddress!, n) }
        done += n
    }
    let closed = ExtAudioFileDispose(f)
    return err != noErr ? err : closed
}

/// Any audio file Core Audio reads (an older take's mic.wav) to a mono AAC .m4a at `dst`, at the source's rate
/// (resampled when AAC doesn't take it; several channels are mixed to one). 0, or the Core Audio error.
@_cdecl("onkey_aac_convert")
public func onkey_aac_convert(_ src: UnsafePointer<CChar>, _ dst: UnsafePointer<CChar>) -> Int32 {
    var input: ExtAudioFileRef?
    var st = ExtAudioFileOpenURL(url(src), &input)
    guard st == noErr, let i = input else { return st }
    defer { ExtAudioFileDispose(i) }
    var fmt = AudioStreamBasicDescription()
    var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
    st = ExtAudioFileGetProperty(i, kExtAudioFileProperty_FileDataFormat, &size, &fmt)
    guard st == noErr, fmt.mSampleRate > 0, fmt.mChannelsPerFrame > 0 else { return st != noErr ? st : -50 }
    let ch = Int(fmt.mChannelsPerFrame)
    var client = floatFormat(fmt.mSampleRate, UInt32(ch))
    st = ExtAudioFileSetProperty(i, kExtAudioFileProperty_ClientDataFormat, UInt32(MemoryLayout.size(ofValue: client)), &client)
    guard st == noErr else { return st }
    let (file, cst) = createAAC(dst, fmt.mSampleRate, takeBitRate)
    guard let o = file else { return cst }
    var inBuf = [Float](repeating: 0, count: chunkFrames * ch)
    var mono = [Float](repeating: 0, count: chunkFrames)
    var err: OSStatus = noErr
    while err == noErr {
        var n = UInt32(chunkFrames)
        err = inBuf.withUnsafeMutableBufferPointer { b -> OSStatus in
            var list = AudioBufferList(mNumberBuffers: 1, mBuffers: AudioBuffer(mNumberChannels: UInt32(ch),
                                       mDataByteSize: UInt32(chunkFrames * ch * 4), mData: UnsafeMutableRawPointer(b.baseAddress!)))
            return ExtAudioFileRead(i, &n, &list)
        }
        if err != noErr || n == 0 { break }
        for k in 0..<Int(n) {
            var s: Float = 0
            for c in 0..<ch { s += inBuf[k * ch + c] }
            mono[k] = s / Float(ch)
        }
        err = mono.withUnsafeMutableBufferPointer { writeFloats(o, $0.baseAddress!, Int(n)) }
    }
    let closed = ExtAudioFileDispose(o)
    return err != noErr ? err : closed
}

/// Decodes the whole file at `path` (proving it decodes): its length in frames, sample rate and channels, and
/// with `out` (room for `cap` frames) the audio itself, mixed to mono. 0, or the Core Audio error.
@_cdecl("onkey_audio_decode")
public func onkey_audio_decode(_ path: UnsafePointer<CChar>, _ out: UnsafeMutablePointer<Float>?, _ cap: Int64,
                               _ frames: UnsafeMutablePointer<Int64>, _ rate: UnsafeMutablePointer<Double>,
                               _ channels: UnsafeMutablePointer<Int32>) -> Int32 {
    frames.pointee = 0
    var input: ExtAudioFileRef?
    var st = ExtAudioFileOpenURL(url(path), &input)
    guard st == noErr, let i = input else { return st }
    defer { ExtAudioFileDispose(i) }
    var fmt = AudioStreamBasicDescription()
    var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
    st = ExtAudioFileGetProperty(i, kExtAudioFileProperty_FileDataFormat, &size, &fmt)
    guard st == noErr, fmt.mSampleRate > 0, fmt.mChannelsPerFrame > 0 else { return st != noErr ? st : -50 }
    rate.pointee = fmt.mSampleRate
    channels.pointee = Int32(fmt.mChannelsPerFrame)
    let ch = Int(fmt.mChannelsPerFrame)
    var client = floatFormat(fmt.mSampleRate, UInt32(ch))
    st = ExtAudioFileSetProperty(i, kExtAudioFileProperty_ClientDataFormat, UInt32(MemoryLayout.size(ofValue: client)), &client)
    guard st == noErr else { return st }
    var buf = [Float](repeating: 0, count: chunkFrames * ch)
    var total: Int64 = 0
    while true {
        var n = UInt32(chunkFrames)
        st = buf.withUnsafeMutableBufferPointer { b -> OSStatus in
            var list = AudioBufferList(mNumberBuffers: 1, mBuffers: AudioBuffer(mNumberChannels: UInt32(ch),
                                       mDataByteSize: UInt32(chunkFrames * ch * 4), mData: UnsafeMutableRawPointer(b.baseAddress!)))
            return ExtAudioFileRead(i, &n, &list)
        }
        if st != noErr { return st }
        if n == 0 { break }
        if let o = out {
            for k in 0..<Int(n) where total + Int64(k) < cap {
                var s: Float = 0
                for c in 0..<ch { s += buf[k * ch + c] }
                o[Int(total) + k] = s / Float(ch)
            }
        }
        total += Int64(n)
    }
    frames.pointee = total
    return noErr
}
