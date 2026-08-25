import Combine
import Darwin
import Foundation

@MainActor
public final class CoreProcessSupervisor: ObservableObject {
    public enum State: String, Sendable {
        case stopped
        case launching
        case running
        case degraded
    }

    @Published public private(set) var state: State = .stopped
    @Published public private(set) var executablePath: String?
    @Published public private(set) var lastExitDescription: String?

    public let socketPath: String
    public var onUnexpectedExit: ((String) -> Void)?

    private var process: Process?
    private var expectedStop = false
    private let environmentOverrides: [String: String]

    public init(socketPath: String? = nil, environmentOverrides: [String: String] = [:]) {
        self.socketPath = socketPath ?? "/tmp/vela-\(getuid()).sock"
        self.environmentOverrides = environmentOverrides
    }

    public func start() async -> Bool {
        guard process == nil else { return state == .running }
        guard let executableURL = Self.resolveCoreExecutable() else {
            state = .degraded
            lastExitDescription = "vela-core not found. Build Core or set VELA_CORE_PATH."
            return false
        }

        try? FileManager.default.removeItem(atPath: socketPath)
        let process = Process()
        process.executableURL = executableURL
        process.arguments = ["--socket", socketPath]
        process.environment = ProcessInfo.processInfo.environment.merging(environmentOverrides) { _, override in override }
        process.standardOutput = FileHandle.standardOutput
        process.standardError = FileHandle.standardError
        expectedStop = false
        state = .launching
        executablePath = executableURL.path
        lastExitDescription = nil
        self.process = process

        process.terminationHandler = { [weak self] terminatedProcess in
            let status = terminatedProcess.terminationStatus
            let reason = terminatedProcess.terminationReason == .exit ? "exit" : "signal"
            Task { @MainActor [weak self] in
                self?.processDidTerminate(
                    terminatedProcess,
                    status: status,
                    reason: reason
                )
            }
        }

        do {
            try process.run()
        } catch {
            self.process = nil
            state = .degraded
            lastExitDescription = "Could not launch vela-core: \(error)"
            return false
        }

        for _ in 0..<100 {
            if FileManager.default.fileExists(atPath: socketPath) {
                state = .running
                return true
            }
            if !process.isRunning { break }
            try? await Task.sleep(for: .milliseconds(50))
        }

        if process.isRunning {
            expectedStop = true
            process.terminate()
        }
        self.process = nil
        state = .degraded
        lastExitDescription = "vela-core did not become ready within five seconds"
        return false
    }

    public func stop() {
        guard let process else {
            state = .stopped
            return
        }
        expectedStop = true
        if process.isRunning {
            process.terminate()
        }
        self.process = nil
        state = .stopped
        try? FileManager.default.removeItem(atPath: socketPath)
    }

    public func killForDiagnostics() {
        guard let process, process.isRunning else { return }
        expectedStop = false
        process.terminate()
    }

    public static func resolveCoreExecutable(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        currentDirectory: URL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath),
        bundle: Bundle = .main
    ) -> URL? {
        let candidates: [URL?] = [
            environment["VELA_CORE_PATH"].map { URL(fileURLWithPath: $0) },
            bundle.executableURL?.deletingLastPathComponent().appendingPathComponent("vela-core"),
            bundle.bundleURL.appendingPathComponent("Contents/MacOS/vela-core"),
            currentDirectory.appendingPathComponent("core/target/debug/vela-core"),
            currentDirectory.deletingLastPathComponent().appendingPathComponent("core/target/debug/vela-core"),
        ]
        return candidates.compactMap { $0 }.first {
            FileManager.default.isExecutableFile(atPath: $0.path)
        }
    }

    private func processDidTerminate(
        _ terminatedProcess: Process,
        status: Int32,
        reason: String
    ) {
        if let currentProcess = process, currentProcess !== terminatedProcess {
            return
        }
        process = nil
        try? FileManager.default.removeItem(atPath: socketPath)
        let description = "vela-core terminated by \(reason) with status \(status)"
        lastExitDescription = description
        if expectedStop {
            state = .stopped
        } else {
            state = .degraded
            onUnexpectedExit?(description)
        }
        expectedStop = false
    }
}
