@preconcurrency import AVFoundation
import Foundation
@preconcurrency import Speech

@MainActor
final class SpeechCaptureController: ObservableObject {
    enum State: Equatable {
        case idle
        case authorizing
        case recording
        case transcribing
        case failed(String)

        var label: String {
            switch self {
            case .idle: "Hold to talk"
            case .authorizing: "Requesting access…"
            case .recording: "Recording — release to stop"
            case .transcribing: "Finishing transcript…"
            case let .failed(message): "Speech failed: \(message)"
            }
        }
    }

    @Published private(set) var state: State = .idle
    @Published private(set) var transcript = ""
    @Published private(set) var recoveryAudioPath: String?

    private let audioEngine = AVAudioEngine()
    private let recognizer = SFSpeechRecognizer()
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?
    private var audioFile: AVAudioFile?
    private var stopRequested = false

    var isRecording: Bool { state == .recording }

    func pressBegan() {
        guard state == .idle || state.isFailed else { return }
        if let privacyFailure = Self.privacyConfigurationFailure {
            state = .failed(privacyFailure)
            return
        }
        stopRequested = false
        state = .authorizing
        Task {
            let microphoneGranted = await requestMicrophoneAccess()
            let authorization = await requestAuthorization()
            guard !stopRequested else {
                state = .idle
                return
            }
            guard microphoneGranted else {
                state = .failed("Microphone permission was not granted")
                return
            }
            guard authorization == .authorized else {
                state = .failed("Speech recognition permission was not granted")
                return
            }
            do {
                try beginRecording()
            } catch {
                state = .failed(error.localizedDescription)
            }
        }
    }

    func pressEnded() {
        stopRequested = true
        guard state == .recording else { return }
        audioEngine.stop()
        audioEngine.inputNode.removeTap(onBus: 0)
        recognitionRequest?.endAudio()
        state = .transcribing
    }

    func resetTranscript() {
        stop()
        transcript = ""
        discardRecoveryAudio()
        state = .idle
    }

    func discardRecoveryAudio() {
        if let recoveryAudioPath {
            try? FileManager.default.removeItem(atPath: recoveryAudioPath)
        }
        recoveryAudioPath = nil
    }

    func stop() {
        stopRequested = true
        if audioEngine.isRunning {
            audioEngine.stop()
            audioEngine.inputNode.removeTap(onBus: 0)
        }
        recognitionRequest?.endAudio()
        recognitionTask?.cancel()
        recognitionTask = nil
        recognitionRequest = nil
        audioFile = nil
    }

    private func beginRecording() throws {
        stop()
        stopRequested = false

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        let inputNode = audioEngine.inputNode
        let format = inputNode.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            throw SpeechCaptureError.noAudioInput
        }

        let audioURL = FileManager.default.temporaryDirectory.appendingPathComponent(
            "vela-speech-\(UUID().uuidString.lowercased()).caf"
        )
        let file = try AVAudioFile(forWriting: audioURL, settings: format.settings)
        recoveryAudioPath = audioURL.path
        recognitionRequest = request
        audioFile = file

        Self.installAudioTap(on: inputNode, format: format, request: request, file: file)
        audioEngine.prepare()
        try audioEngine.start()

        guard let recognizer, recognizer.isAvailable else {
            audioEngine.stop()
            inputNode.removeTap(onBus: 0)
            throw SpeechCaptureError.recognizerUnavailable
        }
        recognitionTask = recognizer.recognitionTask(with: request) { @Sendable [weak self] result, error in
            let text = result?.bestTranscription.formattedString
            let isFinal = result?.isFinal == true
            let errorDescription = error?.localizedDescription
            Task { @MainActor [weak self] in
                self?.receiveRecognition(
                    text: text,
                    isFinal: isFinal,
                    errorDescription: errorDescription
                )
            }
        }
        state = .recording
    }

    // TCC, Speech, and the audio render thread invoke these callbacks off the main
    // queue. A closure created in this @MainActor type inherits main-actor isolation
    // and traps the process when the framework calls it, so the callbacks below stay
    // explicitly nonisolated and hop back to the main actor themselves.
    private nonisolated static func installAudioTap(
        on inputNode: AVAudioInputNode,
        format: AVAudioFormat,
        request: SFSpeechAudioBufferRecognitionRequest,
        file: AVAudioFile
    ) {
        inputNode.installTap(onBus: 0, bufferSize: 1_024, format: format) { buffer, _ in
            request.append(buffer)
            try? file.write(from: buffer)
        }
    }

    private func receiveRecognition(
        text: String?,
        isFinal: Bool,
        errorDescription: String?
    ) {
        if let text, !text.isEmpty {
            transcript = text
        }
        if let errorDescription {
            stop()
            state = .failed(errorDescription)
        } else if isFinal {
            stop()
            state = .idle
        }
    }

    private nonisolated func requestMicrophoneAccess() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: true
        case .notDetermined: await AVCaptureDevice.requestAccess(for: .audio)
        default: false
        }
    }

    private nonisolated func requestAuthorization() async -> SFSpeechRecognizerAuthorizationStatus {
        await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { status in
                continuation.resume(returning: status)
            }
        }
    }

    // TCC aborts the process instead of returning an error when it cannot read a
    // usage description. A bare `swift run` executable still answers the usage-
    // description keys from its embedded __info_plist section, so only the bundle
    // layout distinguishes a launch that TCC will accept.
    private static var privacyConfigurationFailure: String? {
        guard Bundle.main.bundleURL.pathExtension == "app" else {
            return "Push-to-talk must be launched with scripts/run-app.sh so macOS can read its privacy settings"
        }
        let requiredKeys = [
            "NSMicrophoneUsageDescription",
            "NSSpeechRecognitionUsageDescription",
        ]
        guard requiredKeys.allSatisfy({
            Bundle.main.object(forInfoDictionaryKey: $0) as? String != nil
        }) else {
            return "This VelaApp bundle is missing microphone or Speech privacy settings"
        }
        return nil
    }
}

private enum SpeechCaptureError: LocalizedError {
    case noAudioInput
    case recognizerUnavailable

    var errorDescription: String? {
        switch self {
        case .noAudioInput: "No audio input is available"
        case .recognizerUnavailable: "Speech recognizer is unavailable"
        }
    }
}

private extension SpeechCaptureController.State {
    var isFailed: Bool {
        if case .failed = self { true } else { false }
    }
}
