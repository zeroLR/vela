import Foundation
import VelaIPC

@MainActor
final class AppEnvironment: ObservableObject {
    let client: IPCClient
    let supervisor: CoreProcessSupervisor

    init(
        client: IPCClient = IPCClient(),
        supervisor: CoreProcessSupervisor = CoreProcessSupervisor()
    ) {
        self.client = client
        self.supervisor = supervisor
        supervisor.onUnexpectedExit = { [weak client] description in
            client?.disconnect(reason: description)
        }
    }

    func start() async {
        guard await supervisor.start() else { return }
        client.connect(socketPath: supervisor.socketPath)
    }

    func restart() async {
        client.disconnect(reason: nil)
        supervisor.stop()
        try? await Task.sleep(for: .milliseconds(150))
        await start()
    }

    func stop() {
        client.disconnect(reason: nil)
        supervisor.stop()
    }
}
