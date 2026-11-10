// synth-tap: the sound of the soft synths (Analog Lab V, CS-80 V…), for the spyglass's waveform.
//
// A Core Audio process tap (macOS 14.2+) listens to just those apps' output, not YouTube or
// anything else that's playing, and nothing extra has to be installed. The bridge runs this
// and reads its stdout: chunks of [u32 sample rate][u32 count][count × f32 mono], little-endian.
// A chunk with rate 0 instead carries [count bytes]: the names of the apps tapped, ", " between.
// It looks for synths every second and re-taps when one opens or quits; it ends when the bridge
// does (stdout closes).

import CoreAudio
import Darwin
import Foundation

/// Apps whose path has one of these in it are synths.
let synthHints = ["/arturia/", "splice", "serum", "pianoteq", "garageband", "logic pro", "mainstage", "kontakt", "vital", "surge",
                  "fl studio", "ableton", "reaper", "bitwig"] // (the last ones are hosts, for plug-ins like Serum 2)

func prop<T>(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector, _ initial: T) -> T? {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var value = initial
    var size = UInt32(MemoryLayout<T>.size)
    return AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, &value) == noErr ? value : nil
}

func list(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector) -> [AudioObjectID] {
    var addr = AudioObjectPropertyAddress(mSelector: sel, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(obj, &addr, 0, nil, &size) == noErr, size > 0 else { return [] }
    var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, &ids) == noErr else { return [] }
    return ids
}

func path(of pid: pid_t) -> String {
    var buf = [CChar](repeating: 0, count: 4096)
    return proc_pidpath(pid, &buf, UInt32(buf.count)) > 0 ? String(cString: buf) : ""
}

/// "/Applications/Arturia/CS-80 V4.app/Contents/MacOS/CS-80 V4" → "CS-80 V4"
func appName(_ obj: AudioObjectID) -> String {
    guard let pid = prop(obj, kAudioProcessPropertyPID, pid_t(0)) else { return "" }
    let parts = path(of: pid).split(separator: "/")
    let app = parts.first { $0.hasSuffix(".app") } ?? parts.last ?? ""
    return app.hasSuffix(".app") ? String(app.dropLast(4)) : String(app)
}

/// Core Audio's objects for the synth apps that are running, sorted so a change is easy to spot.
func synthProcesses() -> [AudioObjectID] {
    list(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyProcessObjectList).filter { obj in
        guard let pid = prop(obj, kAudioProcessPropertyPID, pid_t(0)), pid > 0 else { return false }
        let p = path(of: pid).lowercased()
        return synthHints.contains { p.contains($0) }
    }.sorted()
}

func defaultOutputUID() -> String? {
    guard let dev = prop(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice, AudioObjectID(0)),
          let uid = prop(dev, kAudioDevicePropertyDeviceUID, "" as CFString) else { return nil }
    return uid as String
}

let out = FileHandle.standardOutput
let outLock = NSLock()
signal(SIGPIPE, SIG_IGN)

/// One chunk to the bridge (from the audio thread or the main loop, never interleaved).
func send(_ chunk: Data) {
    outLock.lock()
    defer { outLock.unlock() }
    do { try out.write(contentsOf: chunk) } catch { exit(0) } // the bridge quit
}

func sendNames(_ names: [String]) {
    let text = Data(names.joined(separator: ", ").utf8)
    var chunk = Data()
    withUnsafeBytes(of: UInt32(0)) { chunk.append(contentsOf: $0) }
    withUnsafeBytes(of: UInt32(text.count).littleEndian) { chunk.append(contentsOf: $0) }
    send(chunk + text)
}

final class Tap {
    var tap = AudioObjectID(kAudioObjectUnknown)
    var agg = AudioObjectID(kAudioObjectUnknown)
    var proc: AudioDeviceIOProcID?

    init?(_ processes: [AudioObjectID]) {
        let desc = CATapDescription(stereoMixdownOfProcesses: processes)
        desc.uuid = UUID()
        desc.isPrivate = true
        desc.muteBehavior = .unmuted
        guard AudioHardwareCreateProcessTap(desc, &tap) == noErr else { return nil }
        guard let fmt = prop(tap, kAudioTapPropertyFormat, AudioStreamBasicDescription()) else { close(); return nil }
        var aggDesc: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Spyglass synth tap",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapUIDKey: desc.uuid.uuidString, kAudioSubTapDriftCompensationKey: true]],
        ]
        if let uid = defaultOutputUID() { // where the synths play (the AIR 192|6 here): its clock drives the tap
            aggDesc[kAudioAggregateDeviceMainSubDeviceKey] = uid
            aggDesc[kAudioAggregateDeviceSubDeviceListKey] = [[kAudioSubDeviceUIDKey: uid]]
        }
        guard AudioHardwareCreateAggregateDevice(aggDesc as CFDictionary, &agg) == noErr else { close(); return nil }
        let rate = UInt32(prop(agg, kAudioDevicePropertyNominalSampleRate, Float64(0)) ?? fmt.mSampleRate) // the clock, not the tap's own guess
        let channels = max(Int(fmt.mChannelsPerFrame), 1)
        let interleaved = fmt.mFormatFlags & kAudioFormatFlagIsNonInterleaved == 0
        let status = AudioDeviceCreateIOProcIDWithBlock(&proc, agg, nil) { _, input, _, _, _ in
            let buffers = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: input))
            // The clock device's own inputs (the AIR 192|6's) come first; the tap is last.
            guard let first = buffers.last, let raw = first.mData else { return }
            let floats = raw.assumingMemoryBound(to: Float.self)
            let step = interleaved ? channels : 1
            let count = Int(first.mDataByteSize) / MemoryLayout<Float>.size / step
            guard count > 0 else { return }
            var chunk = Data(capacity: 8 + count * 4)
            withUnsafeBytes(of: rate.littleEndian) { chunk.append(contentsOf: $0) }
            withUnsafeBytes(of: UInt32(count).littleEndian) { chunk.append(contentsOf: $0) }
            for i in 0..<count { // the left channel (the mixdown's two sides are near enough alike)
                withUnsafeBytes(of: floats[i * step].bitPattern.littleEndian) { chunk.append(contentsOf: $0) }
            }
            send(chunk)
        }
        guard status == noErr, AudioDeviceStart(agg, proc) == noErr else { close(); return nil }
    }

    func close() {
        if let proc { AudioDeviceStop(agg, proc); AudioDeviceDestroyIOProcID(agg, proc) }
        if agg != kAudioObjectUnknown { AudioHardwareDestroyAggregateDevice(agg) }
        if tap != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tap) }
        proc = nil; agg = AudioObjectID(kAudioObjectUnknown); tap = AudioObjectID(kAudioObjectUnknown)
    }
}

let bridge = getppid()
var tapped: [AudioObjectID] = []
var current: Tap?
while true {
    let now = synthProcesses()
    if now != tapped {
        current?.close()
        current = now.isEmpty ? nil : Tap(now)
        tapped = current == nil ? [] : now // couldn't tap: try again next time
        var names: [String] = []
        for n in tapped.map(appName) where !n.isEmpty && !names.contains(n) { names.append(n) }
        sendNames(names)
        FileHandle.standardError.write("synth-tap: \(now.count) synth(s)\(current == nil && !now.isEmpty ? ", tap failed" : "")\n".data(using: .utf8)!)
    }
    if getppid() != bridge { exit(0) } // the bridge is gone
    RunLoop.current.run(until: Date(timeIntervalSinceNow: 1)) // (Core Audio needs a run loop going)
}
