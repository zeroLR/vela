import Carbon.HIToolbox
import Foundation

enum GlobalHotKeyError: Error, CustomStringConvertible {
    case eventHandler(OSStatus)
    case registration(OSStatus)

    var description: String {
        switch self {
        case let .eventHandler(status): "Could not install hotkey handler (\(status))"
        case let .registration(status): "Could not register a Vela hotkey (\(status))"
        }
    }
}

final class GlobalHotKeyController: @unchecked Sendable {
    private var eventHandler: EventHandlerRef?
    private var hotKeys: [EventHotKeyRef] = []
    private let action: @MainActor @Sendable () -> Void
    private(set) var registeredShortcutLabel = ""

    init(action: @escaping @MainActor @Sendable () -> Void) {
        self.action = action
    }

    func register() throws {
        guard hotKeys.isEmpty else { return }
        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed)
        )
        let handlerStatus = InstallEventHandler(
            GetApplicationEventTarget(),
            { _, event, userData in
                guard let event, let userData else { return OSStatus(eventNotHandledErr) }
                let controller = Unmanaged<GlobalHotKeyController>
                    .fromOpaque(userData)
                    .takeUnretainedValue()
                var identifier = EventHotKeyID()
                let parameterStatus = GetEventParameter(
                    event,
                    EventParamName(kEventParamDirectObject),
                    EventParamType(typeEventHotKeyID),
                    nil,
                    MemoryLayout<EventHotKeyID>.size,
                    nil,
                    &identifier
                )
                guard parameterStatus == noErr,
                      identifier.signature == GlobalHotKeyController.signature,
                      GlobalHotKeyController.shortcuts.contains(where: { $0.id == identifier.id })
                else { return OSStatus(eventNotHandledErr) }
                NSLog("Vela global hotkey received [id=%u]", identifier.id)
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

        var labels: [String] = []
        var lastFailure = OSStatus(eventHotKeyExistsErr)
        for shortcut in Self.shortcuts {
            var hotKey: EventHotKeyRef?
            let registrationStatus = RegisterEventHotKey(
                shortcut.keyCode,
                shortcut.modifiers,
                EventHotKeyID(signature: Self.signature, id: shortcut.id),
                GetApplicationEventTarget(),
                0,
                &hotKey
            )
            if registrationStatus == noErr, let hotKey {
                hotKeys.append(hotKey)
                labels.append(shortcut.label)
            } else {
                lastFailure = registrationStatus
            }
        }
        guard !hotKeys.isEmpty else {
            if let eventHandler {
                RemoveEventHandler(eventHandler)
                self.eventHandler = nil
            }
            throw GlobalHotKeyError.registration(lastFailure)
        }
        registeredShortcutLabel = labels.joined(separator: " / ")
        NSLog("Vela global hotkeys ready [%@]", registeredShortcutLabel)
    }

    deinit {
        for hotKey in hotKeys { UnregisterEventHotKey(hotKey) }
        if let eventHandler { RemoveEventHandler(eventHandler) }
    }

    private static let signature: OSType = 0x5645_4C41 // VELA
    private static let shortcuts: [(
        id: UInt32,
        keyCode: UInt32,
        modifiers: UInt32,
        label: String
    )] = [
        (1, UInt32(kVK_Space), UInt32(optionKey), "⌥Space"),
        (2, UInt32(kVK_ANSI_V), UInt32(controlKey | optionKey), "⌃⌥V"),
    ]
}
