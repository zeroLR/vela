import Foundation

public struct AvatarAdapterMapping: Codable, Equatable, Sendable {
    public let values: [String: String]

    public init(values: [String: String]) {
        self.values = values
    }
}

public struct AvatarMappingConfiguration: Codable, Equatable, Sendable {
    public static let builtIn = AvatarMappingConfiguration(
        stateExpressions: [:],
        stateMotions: [:],
        adapters: [:]
    )

    public let stateExpressions: [AvatarState: String]
    public let stateMotions: [AvatarState: String]
    public let adapters: [String: AvatarAdapterMapping]

    public init(
        stateExpressions: [AvatarState: String],
        stateMotions: [AvatarState: String],
        adapters: [String: AvatarAdapterMapping]
    ) {
        self.stateExpressions = stateExpressions
        self.stateMotions = stateMotions
        self.adapters = adapters
    }

    enum CodingKeys: String, CodingKey {
        case stateExpressions = "state_expressions"
        case stateMotions = "state_motions"
        case adapters
    }
}

public struct AvatarMappingLoadResult: Equatable, Sendable {
    public let mapping: AvatarMappingConfiguration
    public let diagnostic: String?

    public init(mapping: AvatarMappingConfiguration, diagnostic: String?) {
        self.mapping = mapping
        self.diagnostic = diagnostic
    }
}

public enum AvatarMappingLoader {
    public static var defaultURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("dev.vela.app/avatar/mapping.json")
    }

    public static func loadDefault() -> AvatarMappingLoadResult {
        load(from: defaultURL)
    }

    public static func load(from url: URL) -> AvatarMappingLoadResult {
        guard FileManager.default.fileExists(atPath: url.path) else {
            return AvatarMappingLoadResult(mapping: .builtIn, diagnostic: nil)
        }
        do {
            let data = try Data(contentsOf: url)
            return AvatarMappingLoadResult(
                mapping: try JSONDecoder().decode(AvatarMappingConfiguration.self, from: data),
                diagnostic: nil
            )
        } catch {
            return AvatarMappingLoadResult(
                mapping: .builtIn,
                diagnostic: "Avatar mapping ignored: \(error.localizedDescription)"
            )
        }
    }
}
