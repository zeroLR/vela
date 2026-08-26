import Combine
import Foundation
import SwiftUI
import VelaAvatar
import VelaIPC

@MainActor
final class AppEnvironment: ObservableObject {
    let client: IPCClient
    let supervisor: CoreProcessSupervisor
    let avatar: AvatarController
    let avatarSurface: AnyView
    @Published private(set) var captureShortcutStatus = "Not installed"

    private var quickCapturePanel: QuickCapturePanelController?
    private var globalHotKey: GlobalHotKeyController?
    private var cancellables: Set<AnyCancellable> = []

    init(
        client: IPCClient = IPCClient(),
        supervisor: CoreProcessSupervisor = CoreProcessSupervisor()
    ) {
        self.client = client
        self.supervisor = supervisor
        let avatarRuntime = AvatarRuntimeFactory.debugShape()
        let avatarMapping = AvatarMappingLoader.loadDefault()
        avatar = AvatarController(
            client: client,
            runtime: avatarRuntime.runtime,
            mapping: avatarMapping.mapping,
            configurationDiagnostic: avatarMapping.diagnostic
        )
        avatarSurface = avatarRuntime.view
        supervisor.onUnexpectedExit = { [weak client] description in
            client?.disconnect(reason: description)
        }
        client.$state
            .removeDuplicates()
            .sink { [weak self] state in
                guard state == .ready else { return }
                self?.restoreWorkspace()
            }
            .store(in: &cancellables)
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
        let panel = QuickCapturePanelController(client: client, avatar: avatar)
        let hotKey = GlobalHotKeyController { [weak self] in
            self?.showQuickCapture()
        }
        do {
            try hotKey.register()
            quickCapturePanel = panel
            globalHotKey = hotKey
            captureShortcutStatus = "\(hotKey.registeredShortcutLabel) ready"
        } catch {
            quickCapturePanel = panel
            captureShortcutStatus = error.localizedDescription
        }
    }

    func showQuickCapture() {
        quickCapturePanel?.show()
    }

    @discardableResult
    func openWorkspace(root: String) -> String? {
        guard let requestID = client.openWorkspace(root: root) else { return nil }
        UserDefaults.standard.set(root, forKey: Self.lastWorkspaceKey)
        return requestID
    }

    func stop() {
        client.disconnect(reason: nil)
        supervisor.stop()
    }

    private func restoreWorkspace() {
        guard client.workspace == nil,
              let root = UserDefaults.standard.string(forKey: Self.lastWorkspaceKey),
              FileManager.default.fileExists(atPath: root) else { return }
        _ = client.openWorkspace(root: root)
    }

    private static let lastWorkspaceKey = "dev.vela.last-workspace-root"
}
