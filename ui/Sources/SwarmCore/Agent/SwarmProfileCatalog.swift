import Foundation

/// The profile list and the provider list, read when the app starts so New Chat and the profiles
/// page open without waiting on swarm. Two callers share one provider read in flight. A failed
/// read is not kept, so the next caller reads again.
public actor SwarmProfileCatalog {
    public static let shared = SwarmProfileCatalog()

    private let loadProfiles: @Sendable () async throws -> SwarmProfileList
    private let loadProviders: @Sendable () async throws -> [SwarmProvider]
    private var providerRead: Task<[SwarmProvider], any Error>?
    /// The last profile list read, for a view to show before its own read returns.
    public private(set) var cachedProfiles: SwarmProfileList?

    public init(
        profiles: @escaping @Sendable () async throws -> SwarmProfileList = {
            try await SwarmCLIProfileSource().profiles()
        },
        providers: @escaping @Sendable () async throws -> [SwarmProvider] = {
            try await SwarmCLIProfileSource().providers()
        }
    ) {
        loadProfiles = profiles
        loadProviders = providers
    }

    /// Starts both reads without waiting for them.
    public func prefetch() {
        Task { _ = try? await self.profiles() }
        Task { _ = try? await self.providers() }
    }

    /// Reads the profile file again on every call, never joining a read in flight, because a save
    /// can change the file after that read began.
    public func profiles() async throws -> SwarmProfileList {
        let list = try await loadProfiles()
        cachedProfiles = list
        return list
    }

    /// The providers do not change while the app runs, so one good read serves every caller.
    public func providers() async throws -> [SwarmProvider] {
        let read = providerRead ?? Task { try await loadProviders() }
        providerRead = read
        do {
            return try await read.value
        } catch {
            if providerRead == read { providerRead = nil }
            throw error
        }
    }
}
