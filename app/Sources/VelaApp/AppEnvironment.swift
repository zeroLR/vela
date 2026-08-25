import Foundation
import VelaIPC

@MainActor
final class AppEnvironment: ObservableObject {
    let client: IPCClient
    let supervisor: CoreProcessSupervisor
    @Published private(set) var captureShortcutStatus = "Not installed"

    private var quickCapturePanel: QuickCapturePanelController?
    private var globalHotKey: GlobalHotKeyController?

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

    func installCaptureUI() {
        guard globalHotKey == nil else { return }
        let panel = QuickCapturePanelController(client: client)
        let hotKey = GlobalHotKeyController { [weak self] in
            self?.showQuickCapture()
        }
        do {
            try hotKey.register()
            quickCapturePanel = panel
            globalHotKey = hotKey
            captureShortcutStatus = "⌥Space ready"
        } catch {
            quickCapturePanel = panel
            captureShortcutStatus = error.localizedDescription
        }
    }

    func showQuickCapture() {
        quickCapturePanel?.show()
    }

    func stop() {
        client.disconnect(reason: nil)
        supervisor.stop()
    }
}
