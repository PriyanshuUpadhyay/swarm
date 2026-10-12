import Foundation

/// The one path to `swarm usage --refresh`. The HUD and the Accounts page share an instance, so a
/// provider is asked at most once per 300 seconds; the Claude usage endpoint is rate-limited.
// ponytail: the limit is in memory, so one more refresh is possible after an app restart. Seed it
// from the newest `as_of_seconds` if that matters.
public actor UsageRefresher {
    public static let minIntervalSeconds = 300

    public enum Outcome: Sendable, Equatable {
        case refreshed(SwarmUsage)
        /// No call was made. The cached usage is still the newest reading.
        case cooldown(nextInSeconds: Int)
    }

    private let now: @Sendable () -> Int
    private let run: @Sendable (String) async throws -> SwarmUsage
    private var startedAtSeconds: [String: Int] = [:]
    private var running: [String: Task<SwarmUsage, any Error>] = [:]
    /// The error of the last call, until the next call. Inside the interval it is thrown again,
    /// so a second click does not show a failed refresh as a good one.
    private var failures: [String: any Error] = [:]

    public init(
        now: @escaping @Sendable () -> Int = { Int(Date().timeIntervalSince1970) },
        run: @escaping @Sendable (String) async throws -> SwarmUsage
    ) {
        self.now = now
        self.run = run
    }

    /// A failed refresh also starts the interval, so a second click cannot repeat a failing call.
    public func refresh(provider: String) async throws -> Outcome {
        let now = now()
        if let task = running[provider] { return .refreshed(try await task.value) }
        // The check and the reservation have no suspension between them, so two callers cannot both pass.
        if let started = startedAtSeconds[provider], (0..<Self.minIntervalSeconds).contains(now - started) {
            if let failure = failures[provider] { throw failure }
            return .cooldown(nextInSeconds: Self.minIntervalSeconds - (now - started))
        }
        startedAtSeconds[provider] = now
        failures[provider] = nil
        let run = run
        let task = Task { try await run(provider) }
        running[provider] = task
        defer { running[provider] = nil }
        do {
            return .refreshed(try await task.value)
        } catch {
            failures[provider] = error
            throw error
        }
    }

    /// For a page that shows one provider's usage: the new reading, or the cached one when the
    /// limit allows no call.
    public func refreshOrCached(
        provider: String, cached: @Sendable () async throws -> [SwarmUsageMeter]
    ) async throws -> SwarmUsage {
        switch try await refresh(provider: provider) {
        case .refreshed(let usage): usage
        case .cooldown: SwarmUsage(meters: try await cached().filter { $0.provider == provider })
        }
    }
}
