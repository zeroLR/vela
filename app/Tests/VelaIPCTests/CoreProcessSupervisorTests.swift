import Foundation
import Testing
@testable import VelaIPC

@Suite("Core executable discovery")
struct CoreProcessSupervisorTests {
    @Test("VELA_CORE_PATH has priority")
    @MainActor
    func explicitExecutablePath() throws {
        let knownExecutable = URL(fileURLWithPath: "/bin/echo")
        let resolved = CoreProcessSupervisor.resolveCoreExecutable(
            environment: ["VELA_CORE_PATH": knownExecutable.path],
            currentDirectory: URL(fileURLWithPath: "/private/tmp")
        )
        #expect(resolved == knownExecutable)
    }
}
