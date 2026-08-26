import AppKit
import SwiftUI
import VelaAvatar
import VelaIPC

@MainActor
final class QuickCapturePanelController {
    private let client: IPCClient
    private let avatar: AvatarController
    private var panel: NSPanel?

    init(client: IPCClient, avatar: AvatarController) {
        self.client = client
        self.avatar = avatar
    }

    func show() {
        if let panel, panel.isVisible {
            NSApp.activate(ignoringOtherApps: true)
            panel.orderFrontRegardless()
            panel.makeKeyAndOrderFront(nil)
            return
        }

        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 480),
            styleMask: [.titled, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        panel.title = "Quick Capture"
        panel.level = .floating
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentView = NSHostingView(
            rootView: QuickCaptureView(client: client, avatar: avatar) { [weak panel] in
                panel?.orderOut(nil)
            }
        )
        panel.center()
        self.panel = panel
        NSLog("Vela Quick Capture panel requested")
        NSApp.activate(ignoringOtherApps: true)
        panel.orderFrontRegardless()
        panel.makeKeyAndOrderFront(nil)
    }
}

private struct QuickCaptureView: View {
    @ObservedObject var client: IPCClient
    @ObservedObject var avatar: AvatarController
    let dismiss: () -> Void

    @StateObject private var speech: SpeechCaptureController
    @State private var rawText = ""
    @State private var selectedIntent: CaptureIntent?
    @State private var source: CaptureSource = .text
    @State private var startedAtMilliseconds = Self.nowMilliseconds
    @State private var isPressingSpeech = false
    @State private var submitted = false
    @State private var knownCaptureIDs: Set<String> = []
    @State private var result: CaptureRecord?
    @FocusState private var textFocused: Bool

    init(client: IPCClient, avatar: AvatarController, dismiss: @escaping () -> Void) {
        self.client = client
        self.avatar = avatar
        self.dismiss = dismiss
        _speech = StateObject(wrappedValue: SpeechCaptureController(
            onRecordingChanged: { avatar.setListening($0) },
            onMicrophoneRMS: { avatar.setMicrophoneRMS($0) }
        ))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Quick Capture").font(.title2.bold())
                Text("⌥Space · fallback ⌃⌥V")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Spacer()
                if client.workspace == nil {
                    Text("Open a workspace first").foregroundStyle(.orange)
                }
            }

            TextEditor(text: $rawText)
                .font(.system(.body, design: .rounded))
                .focused($textFocused)
                .frame(minHeight: 105)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
                .disabled(submitted)

            HStack {
                Picker("Route", selection: $selectedIntent) {
                    Text("Auto classify").tag(nil as CaptureIntent?)
                    ForEach(CaptureIntent.allCases, id: \.self) { intent in
                        Text(intentLabel(intent)).tag(intent as CaptureIntent?)
                    }
                }
                .frame(width: 210)
                .disabled(submitted)

                Text(speech.state.label)
                    .font(.caption)
                    .foregroundStyle(speech.isRecording ? .red : .secondary)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .help(speech.state.label)
                Text("Avatar: \(avatar.state.rawValue.capitalized)")
                    .font(.caption)
                    .foregroundStyle(avatar.state == .listening ? .purple : .secondary)
                Capsule()
                    .fill(avatar.state == .listening ? Color.purple : .secondary)
                    .frame(width: 24, height: 4 + 14 * avatar.lipSyncValue)
                Spacer()
                speechButton
            }

            if let recoveryPath = speech.recoveryAudioPath,
               case .failed = speech.state {
                Text("Recoverable audio: \(recoveryPath)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
            }

            if let result {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Routed to \(result.routedPath ?? "capture record only")")
                        .font(.headline)
                    Text("Suggested \(intentLabel(result.suggestedIntent)); current \(intentLabel(result.intent))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    HStack {
                        Text("Correct route:").font(.caption)
                        ForEach(CaptureIntent.allCases, id: \.self) { intent in
                            Button(intentLabel(intent)) {
                                client.correctCapture(id: result.id, intent: intent)
                            }
                            .buttonStyle(.link)
                            .disabled(intent == result.intent)
                        }
                    }
                }
                .padding(8)
                .background(Color.green.opacity(0.10))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }

            Divider()
            CurrentStateView(
                state: client.currentState,
                entryLimit: 2,
                reload: { client.loadCurrentState() }
            )

            Spacer()
            HStack {
                Button("Cancel", role: .cancel) { cancel() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                if result != nil {
                    Button("Done") { finish() }
                        .keyboardShortcut(.defaultAction)
                } else {
                    Button("Capture") { submit() }
                        .keyboardShortcut(.defaultAction)
                        .disabled(
                            submitted
                                || client.workspace == nil
                                || rawText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        )
                }
            }
        }
        .padding(18)
        .onAppear {
            textFocused = true
            client.loadCurrentState()
        }
        .onChange(of: speech.transcript) { _, transcript in
            guard !transcript.isEmpty else { return }
            rawText = transcript
            source = .speech
        }
        .onChange(of: client.captures) { _, captures in
            guard submitted else { return }
            if let capture = captures.first(where: { !knownCaptureIDs.contains($0.id) }) {
                result = capture
            } else if let currentID = result?.id,
                      let updated = captures.first(where: { $0.id == currentID }) {
                result = updated
            }
        }
    }

    private var speechButton: some View {
        Text(speech.isRecording ? "Release" : "Hold to Talk")
            .font(.caption.bold())
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(speech.isRecording ? Color.red.opacity(0.2) : Color.accentColor.opacity(0.15))
            .clipShape(Capsule())
            .contentShape(Capsule())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        guard !isPressingSpeech, !submitted else { return }
                        isPressingSpeech = true
                        source = .speech
                        speech.pressBegan()
                    }
                    .onEnded { _ in
                        isPressingSpeech = false
                        speech.pressEnded()
                    }
            )
            .accessibilityLabel("Push to talk")
    }

    private func submit() {
        knownCaptureIDs = Set(client.captures.map(\.id))
        submitted = client.submitCapture(
            rawText: rawText,
            source: source,
            intent: selectedIntent,
            startedAtMilliseconds: startedAtMilliseconds
        ) != nil
    }

    private func cancel() {
        if !submitted, (!rawText.isEmpty || speech.recoveryAudioPath != nil) {
            _ = client.abandonCapture(
                rawText: rawText,
                source: source,
                startedAtMilliseconds: startedAtMilliseconds
            )
        }
        finish()
    }

    private func finish() {
        speech.stop()
        avatar.setListening(false)
        avatar.setMicrophoneRMS(0)
        if result != nil {
            speech.discardRecoveryAudio()
        }
        dismiss()
    }

    private func intentLabel(_ intent: CaptureIntent) -> String {
        switch intent {
        case .note: "Note"
        case .idea: "Idea"
        case .todo: "Todo"
        case .workUpdate: "Work Update"
        case .unknown: "Inbox"
        }
    }

    private static var nowMilliseconds: UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1_000)
    }
}
