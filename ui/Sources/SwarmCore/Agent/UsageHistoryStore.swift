import Foundation

public struct UsageHistorySample: Codable, Sendable, Hashable {
    public let provider: String
    public let account: String
    public let window: String
    public let usedPct: Double
    public let asOfSeconds: Int

    public init(provider: String, account: String, window: String, usedPct: Double, asOfSeconds: Int) {
        self.provider = provider
        self.account = account
        self.window = window
        self.usedPct = usedPct
        self.asOfSeconds = asOfSeconds
    }
}

/// Keeps usage samples in `usage-history.json` in the swarm home, at most 7 days and 512 KB.
/// A file error leaves the history in memory only; the HUD works without its trend.
public struct UsageHistoryStore: Sendable {
    private static let maxAgeSeconds = 7 * 86_400
    private let file: URL?

    /// The store does not make the folder. `swarm init` owns the swarm home.
    public init(folder: URL? = SwarmHome.dataFolder) {
        file = folder?.appendingPathComponent("usage-history.json")
    }

    public func load() -> [UsageHistorySample] {
        guard let file, let data = try? Data(contentsOf: file),
              let samples = try? JSONDecoder().decode([UsageHistorySample].self, from: data) else { return [] }
        return samples
    }

    /// Returns the saved history plus the new samples, which is the list the caller keeps.
    @discardableResult
    public func append(_ samples: [UsageHistorySample], nowSeconds: Int) -> [UsageHistorySample] {
        let merged = Self.capToByteBudget(Self.prune(Self.dedup(load() + samples), nowSeconds: nowSeconds))
        if let file, let data = try? JSONEncoder().encode(merged) { try? data.write(to: file, options: .atomic) }
        return merged
    }

    /// Only a fresh meter becomes a sample. An old or failed value must not enter the trend.
    static func samples(from meters: [SwarmUsageMeter], nowSeconds: Int) -> [UsageHistorySample] {
        meters.compactMap { meter in
            guard meter.state == .fresh, let window = meter.window, let usedPct = meter.usedPct else { return nil }
            return UsageHistorySample(
                provider: meter.provider, account: meter.account ?? "", window: window, usedPct: usedPct,
                asOfSeconds: meter.asOfSeconds ?? nowSeconds
            )
        }
    }

    static func prune(
        _ samples: [UsageHistorySample], nowSeconds: Int, maxCount: Int = 5_000
    ) -> [UsageHistorySample] {
        let fresh = samples.filter { $0.asOfSeconds >= nowSeconds - maxAgeSeconds }
            .sorted { $0.asOfSeconds < $1.asOfSeconds }
        return Array(fresh.suffix(maxCount))
    }

    /// A read of the same cached sample must not fill the history and push out new samples.
    private static func dedup(_ samples: [UsageHistorySample]) -> [UsageHistorySample] {
        struct Key: Hashable {
            let provider: String
            let account: String
            let window: String
            let asOfSeconds: Int
        }
        var seen = Set<Key>()
        return samples.filter {
            seen.insert(Key(provider: $0.provider, account: $0.account, window: $0.window, asOfSeconds: $0.asOfSeconds)).inserted
        }
    }

    /// Drops the oldest tenth until the JSON fits, so many accounts cannot grow the file without limit.
    static func capToByteBudget(_ samples: [UsageHistorySample], maxBytes: Int = 512 * 1_024) -> [UsageHistorySample] {
        var current = samples
        while !current.isEmpty, let data = try? JSONEncoder().encode(current), data.count > maxBytes {
            current = Array(current.dropFirst(max(1, current.count / 10)))
        }
        return current
    }
}
