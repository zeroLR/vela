import Foundation

@MainActor
public protocol LipSyncSource: AnyObject {
    var isEnabled: Bool { get set }
    func value(at: Date) -> Double
}

@MainActor
public final class MicrophoneRMSSource: LipSyncSource {
    public var isEnabled = true

    private var latestValue = 0.0
    private var latestSampleAt: Date?

    public init() {}

    public func accept(_ rms: Double, at date: Date) {
        latestValue = min(max(rms, 0), 1)
        latestSampleAt = date
    }

    public func value(at now: Date) -> Double {
        guard isEnabled,
              let latestSampleAt,
              now.timeIntervalSince(latestSampleAt) <= 0.3
        else { return 0 }
        return latestValue
    }
}

@MainActor
public final class TextCadenceLipSyncSource: LipSyncSource {
    public var isEnabled = true

    private var previousDeltaAt: Date?
    private var latestDeltaAt: Date?

    public init() {}

    public func recordTextDelta(at date: Date) {
        previousDeltaAt = latestDeltaAt
        latestDeltaAt = date
    }

    public func value(at now: Date) -> Double {
        guard isEnabled, let latestDeltaAt else { return 0 }
        let age = now.timeIntervalSince(latestDeltaAt)
        guard age >= 0, age <= 0.7 else { return 0 }
        let cadence: Double
        if let previousDeltaAt {
            let interval = latestDeltaAt.timeIntervalSince(previousDeltaAt)
            cadence = min(max((0.5 - interval) / 0.5, 0), 1)
        } else {
            cadence = 0.5
        }
        return (0.3 + 0.7 * cadence) * (1 - age / 0.7)
    }
}
