import Carbon.HIToolbox
import Foundation

enum GlobalHotKeyError: Error, CustomStringConvertible {
    case eventHandler(OSStatus)
    case registration(OSStatus)

    var description: String {
        switch self {
        case let .eventHandler(status): "Could not install hotkey handler (\(status))"
        case let .registration(status): "Could not register ⌥Space (\(status))"
        }
    }
}

final class GlobalHotKeyController: @unchecked Sendable {
    private var eventHandler: EventHandlerRef?
    private var hotKey: EventHotKeyRef?
    private let action: @MainActor @Sendable () -> Void

    init(action: @escaping @MainActor @Sendable () -> Void) {
        self.action = action
    }

    func register() throws {
        guard hotKey == nil else { return }
        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed)
        )
        let handlerStatus = InstallEventHandler(
            GetApplicationEventTarget(),
            { _, _, userData in
                guard let userData else { return OSStatus(eventNotHandledErr) }
                let controller = Unmanaged<GlobalHotKeyController>
                    .fromOpaque(userData)
                    .takeUnretainedValue()
                Task { @MainActor in controller.action() }
                return noErr
            },
            1,
            &eventType,
            Unmanaged.passUnretained(self).toOpaque(),
            &eventHandler
        )
        guard handlerStatus == noErr else {
            throw GlobalHotKeyError.eventHandler(handlerStatus)
        }

        let identifier = EventHotKeyID(signature: Self.signature, id: 1)
        let registrationStatus = RegisterEventHotKey(
            UInt32(kVK_Space),
            UInt32(optionKey),
            identifier,
            GetApplicationEventTarget(),
            0,
            &hotKey
        )
        guard registrationStatus == noErr else {
            if let eventHandler {
                RemoveEventHandler(eventHandler)
                self.eventHandler = nil
            }
            throw GlobalHotKeyError.registration(registrationStatus)
        }
    }

    deinit {
        if let hotKey { UnregisterEventHotKey(hotKey) }
        if let eventHandler { RemoveEventHandler(eventHandler) }
    }

    private static let signature: OSType = 0x5645_4C41 // VELA
}
