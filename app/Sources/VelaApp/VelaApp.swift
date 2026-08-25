import AppKit
import SwiftUI

@main
struct VelaApp: App {
    @StateObject private var environment = AppEnvironment()

    var body: some Scene {
        WindowGroup {
            DiagnosticsView(environment: environment)
                .task { await environment.start() }
                .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
                    environment.stop()
                }
        }
        .windowStyle(.titleBar)
    }
}
