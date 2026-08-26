import SwiftUI

@MainActor
public struct AvatarRuntimeSurface {
    public let runtime: any AvatarRuntime
    public let view: AnyView

    private init(runtime: any AvatarRuntime, view: AnyView) {
        self.runtime = runtime
        self.view = view
    }

    static func debugShape() -> AvatarRuntimeSurface {
        let runtime = DebugShapeAvatarRuntime()
        return AvatarRuntimeSurface(
            runtime: runtime,
            view: AnyView(DebugShapeAvatarView(runtime: runtime))
        )
    }
}

@MainActor
public enum AvatarRuntimeFactory {
    public static func debugShape() -> AvatarRuntimeSurface {
        AvatarRuntimeSurface.debugShape()
    }
}

@MainActor
public final class DebugShapeAvatarRuntime: AvatarRuntime, ObservableObject {
    @Published public private(set) var state: AvatarState = .idle
    @Published public private(set) var lipSync: Double = 0
    @Published public private(set) var isLoaded = false

    private(set) var expression: String?
    private(set) var motion: String?
    private(set) var lookAt = SIMD2<Double>(repeating: 0)

    public init() {}

    public func load() throws {
        isLoaded = true
    }

    public func unload() {
        isLoaded = false
    }

    public func setState(_ state: AvatarState) throws {
        self.state = state
    }

    public func setExpression(_ expression: String?) throws {
        self.expression = expression
    }

    public func playMotion(_ motion: String) throws {
        self.motion = motion
    }

    public func setLipSync(_ value: Double) throws {
        lipSync = min(max(value, 0), 1)
    }

    public func lookAt(x: Double, y: Double) throws {
        lookAt = SIMD2(min(max(x, -1), 1), min(max(y, -1), 1))
    }
}

private struct DebugShapeAvatarView: View {
    @ObservedObject var runtime: DebugShapeAvatarRuntime

    var body: some View {
        VStack(spacing: 8) {
            ZStack {
                Circle()
                    .fill(color.opacity(0.20))
                    .overlay(Circle().stroke(color.opacity(0.55), lineWidth: 2))
                VStack(spacing: 7) {
                    Image(systemName: symbol)
                        .font(.system(size: 32, weight: .semibold))
                    Capsule()
                        .fill(color)
                        .frame(width: 28, height: 4 + 14 * runtime.lipSync)
                }
                .foregroundStyle(color)
            }
            .frame(width: 92, height: 92)
            .animation(.easeInOut(duration: 0.18), value: runtime.state)
            Text(runtime.state.rawValue.capitalized)
                .font(.caption.bold())
                .foregroundStyle(color)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Avatar \(runtime.state.rawValue)")
    }

    private var color: Color {
        switch runtime.state {
        case .idle: .secondary
        case .listening: .purple
        case .thinking: .orange
        case .speaking: .blue
        case .success: .green
        case .error: .red
        }
    }

    private var symbol: String {
        switch runtime.state {
        case .idle: "moon.zzz.fill"
        case .listening: "waveform"
        case .thinking: "ellipsis.circle.fill"
        case .speaking: "text.bubble.fill"
        case .success: "checkmark.circle.fill"
        case .error: "exclamationmark.triangle.fill"
        }
    }
}
