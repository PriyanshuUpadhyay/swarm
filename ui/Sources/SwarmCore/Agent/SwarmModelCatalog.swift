import Foundation

/// The models each provider's CLI lists, read once per app run and shared by New Chat and the
/// profile editor. A failed read is not kept, so the next caller retries.
public actor SwarmModelCatalog {
    public static let shared = SwarmModelCatalog()

    private let load: @Sendable (String) async throws -> [SwarmModel]
    private var models: [String: [SwarmModel]] = [:]

    public init(load: @escaping @Sendable (String) async throws -> [SwarmModel] = {
        try await SwarmCLIProfileSource().models(provider: $0)
    }) {
        self.load = load
    }

    public func cached(_ provider: String) -> [SwarmModel]? { models[provider] }

    public func models(for provider: String) async throws -> [SwarmModel] {
        if let known = models[provider] { return known }
        let loaded = try await load(provider)
        models[provider] = loaded
        return loaded
    }
}
