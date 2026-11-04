// Spike: record 10 s of system audio via a Core Audio process tap (macOS 14.2+)
// and write it to a WAV file. Recording starts at the first non-silent buffer,
// so the permission prompt and starting the video don't eat into the 10 s.

import AVFoundation
import AudioToolbox
import CoreAudio
import Foundation

let recordSeconds = 10.0
let waitForSoundTimeoutSeconds = 180.0
let silenceThreshold: Float = 0.0001

enum SpikeError: Error, CustomStringConvertible {
    case osStatus(String, OSStatus)
    case other(String)

    var description: String {
        switch self {
        case let .osStatus(what, status): return "\(what) failed: OSStatus \(status)"
        case let .other(message): return message
        }
    }
}

func log(_ message: String) {
    print(message)
    fflush(stdout)
}

func check(_ status: OSStatus, _ what: String) throws {
    guard status == noErr else { throw SpikeError.osStatus(what, status) }
}

func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress(
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain
    )
}

func defaultOutputDevice() throws -> AudioObjectID {
    var addr = address(kAudioHardwarePropertyDefaultOutputDevice)
    var device = AudioObjectID(kAudioObjectUnknown)
    var size = UInt32(MemoryLayout<AudioObjectID>.size)
    try check(
        AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &device),
        "read default output device"
    )
    return device
}

func stringProperty(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) throws -> String {
    var addr = address(selector)
    var value: Unmanaged<CFString>?
    var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    try check(AudioObjectGetPropertyData(object, &addr, 0, nil, &size, &value), "read string property")
    guard let value else { throw SpikeError.other("string property was empty") }
    return value.takeRetainedValue() as String
}

func tapFormat(_ tap: AudioObjectID) throws -> AudioStreamBasicDescription {
    var addr = address(kAudioTapPropertyFormat)
    var asbd = AudioStreamBasicDescription()
    var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
    try check(AudioObjectGetPropertyData(tap, &addr, 0, nil, &size, &asbd), "read tap format")
    return asbd
}

func peak(of buffer: AVAudioPCMBuffer) -> (peak: Float, sumSquares: Double, samples: Int) {
    guard let data = buffer.floatChannelData else { return (0, 0, 0) }
    let frames = Int(buffer.frameLength)
    let channels = Int(buffer.format.channelCount)
    // Interleaved data lives in one block; deinterleaved data has one block per channel.
    let blocks = buffer.format.isInterleaved ? [(data[0], frames * channels)] : (0..<channels).map { (data[$0], frames) }
    var maxValue: Float = 0
    var sum: Double = 0
    var count = 0
    for (pointer, length) in blocks {
        for i in 0..<length {
            let sample = pointer[i]
            maxValue = max(maxValue, abs(sample))
            sum += Double(sample * sample)
        }
        count += length
    }
    return (maxValue, sum, count)
}

func dbfs(_ value: Double) -> String {
    value > 0 ? String(format: "%.1f dBFS", 20 * log10(value)) : "-inf dBFS (digitale Stille)"
}

func run(outputURL: URL) throws {
    let outputDevice = try defaultOutputDevice()
    let outputUID = try stringProperty(outputDevice, kAudioDevicePropertyDeviceUID)
    let outputName = (try? stringProperty(outputDevice, kAudioObjectPropertyName)) ?? "?"
    log("Ausgabegeraet: \(outputName) (\(outputUID))")

    // Empty exclude list: tap the mix of every process.
    let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
    description.uuid = UUID()
    description.name = "Handy System Audio Spike"
    description.isPrivate = true
    description.muteBehavior = .unmuted

    var tapID = AudioObjectID(kAudioObjectUnknown)
    try check(AudioHardwareCreateProcessTap(description, &tapID), "AudioHardwareCreateProcessTap")
    defer { AudioHardwareDestroyProcessTap(tapID) }

    var asbd = try tapFormat(tapID)
    guard let format = AVAudioFormat(streamDescription: &asbd) else {
        throw SpikeError.other("tap format not supported by AVAudioFormat")
    }
    log("Tap-Format: \(format.sampleRate) Hz, \(format.channelCount) Kanaele, interleaved=\(format.isInterleaved)")

    let aggregateDescription: [String: Any] = [
        kAudioAggregateDeviceNameKey: "Handy System Audio Spike",
        kAudioAggregateDeviceUIDKey: UUID().uuidString,
        kAudioAggregateDeviceMainSubDeviceKey: outputUID,
        kAudioAggregateDeviceIsPrivateKey: true,
        kAudioAggregateDeviceIsStackedKey: false,
        kAudioAggregateDeviceTapAutoStartKey: true,
        kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID]],
        kAudioAggregateDeviceTapListKey: [
            [kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: description.uuid.uuidString],
        ],
    ]
    var aggregateID = AudioObjectID(kAudioObjectUnknown)
    try check(
        AudioHardwareCreateAggregateDevice(aggregateDescription as CFDictionary, &aggregateID),
        "AudioHardwareCreateAggregateDevice"
    )
    defer { AudioHardwareDestroyAggregateDevice(aggregateID) }

    let fileSettings: [String: Any] = [
        AVFormatIDKey: kAudioFormatLinearPCM,
        AVSampleRateKey: format.sampleRate,
        AVNumberOfChannelsKey: format.channelCount,
        AVLinearPCMBitDepthKey: 16,
        AVLinearPCMIsFloatKey: false,
        AVLinearPCMIsBigEndianKey: false,
    ]
    let targetFrames = AVAudioFramePosition(recordSeconds * format.sampleRate)

    // All of this state is touched only on ioQueue.
    let ioQueue = DispatchQueue(label: "spike.io")
    let done = DispatchSemaphore(value: 0)
    var file: AVAudioFile?
    var writeError: Error?
    var framesWritten: AVAudioFramePosition = 0
    var overallPeak: Float = 0
    var sumSquares: Double = 0
    var sampleCount = 0
    var silentPhasePeak: Float = 0
    var buffersSeen = 0
    var finished = false

    var procID: AudioDeviceIOProcID?
    try check(
        AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, ioQueue) { _, inputData, _, _, _ in
            guard !finished,
                  let buffer = AVAudioPCMBuffer(pcmFormat: format, bufferListNoCopy: inputData, deallocator: nil)
            else { return }
            let stats = peak(of: buffer)
            buffersSeen += 1
            if file == nil {
                silentPhasePeak = max(silentPhasePeak, stats.peak)
                guard stats.peak > silenceThreshold else { return }
                log("Ton erkannt, nehme \(Int(recordSeconds)) s auf ...")
                do {
                    file = try AVAudioFile(
                        forWriting: outputURL,
                        settings: fileSettings,
                        commonFormat: .pcmFormatFloat32,
                        interleaved: format.isInterleaved
                    )
                } catch {
                    writeError = error
                    finished = true
                    done.signal()
                    return
                }
            }
            do {
                try file?.write(from: buffer)
            } catch {
                writeError = error
                finished = true
                done.signal()
                return
            }
            framesWritten += AVAudioFramePosition(buffer.frameLength)
            overallPeak = max(overallPeak, stats.peak)
            sumSquares += stats.sumSquares
            sampleCount += stats.samples
            if framesWritten >= targetFrames {
                finished = true
                done.signal()
            }
        },
        "AudioDeviceCreateIOProcIDWithBlock"
    )
    guard let procID else { throw SpikeError.other("no IOProc ID returned") }
    defer { AudioDeviceDestroyIOProcID(aggregateID, procID) }

    try check(AudioDeviceStart(aggregateID, procID), "AudioDeviceStart")
    log("Aufnahme laeuft. Warte bis zu \(Int(waitForSoundTimeoutSeconds)) s auf Ton, bitte jetzt ein Video abspielen.")

    let timedOut = done.wait(timeout: .now() + waitForSoundTimeoutSeconds + recordSeconds) == .timedOut
    AudioDeviceStop(aggregateID, procID)

    try ioQueue.sync {
        finished = true
        file = nil // closes the WAV file
        if let writeError { throw writeError }
        if framesWritten == 0 {
            log("ERGEBNIS: STUMM. In \(Int(waitForSoundTimeoutSeconds)) s kam nur Stille an.")
            log("Puffer empfangen: \(buffersSeen), hoechster Pegel: \(dbfs(Double(silentPhasePeak)))")
            log("Moegliche Gruende: Berechtigung Systemaudioaufnahme fehlt, oder es lief kein Ton.")
            return
        }
        let seconds = Double(framesWritten) / format.sampleRate
        let rms = sampleCount > 0 ? (sumSquares / Double(sampleCount)).squareRoot() : 0
        log(String(format: "Aufgenommen: %.1f s%@", seconds, timedOut ? " (Zeitlimit erreicht)" : ""))
        log("Spitzenpegel: \(dbfs(Double(overallPeak)))")
        log("Durchschnittspegel (RMS): \(dbfs(rms))")
        log("ERGEBNIS: TON KOMMT AN. Datei: \(outputURL.path)")
    }
}

let outputPath = CommandLine.arguments.count > 1
    ? CommandLine.arguments[1]
    : FileManager.default.currentDirectoryPath + "/system-audio-test.wav"

do {
    try run(outputURL: URL(fileURLWithPath: outputPath))
    exit(0)
} catch {
    log("FEHLER: \(error)")
    exit(1)
}
