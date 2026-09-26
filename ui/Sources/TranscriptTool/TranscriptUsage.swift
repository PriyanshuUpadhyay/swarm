import Foundation

/// Provider snapshots, never deltas. Input includes cache reads and writes after normalization.
public struct TranscriptUsage: Sendable, Hashable, Decodable {
    public let source: String
    public let kind: String
    public let contextTokens: Int64?
    public let contextCapacityTokens: Int64?
    public let inputTokens: Int64?
    public let outputTokens: Int64?
    public let cacheReadTokens: Int64?
    public let cacheWriteTokens: Int64?
    public let sessionInputTokens: Int64?
    public let sessionOutputTokens: Int64?
    public let costUSD: Double?
    public let costCompleteness: String

    enum CodingKeys: String, CodingKey {
        case source, kind
        case contextTokens = "context_tokens"
        case contextCapacityTokens = "context_capacity_tokens"
        case inputTokens = "input_tokens"
        case outputTokens = "output_tokens"
        case cacheReadTokens = "cache_read_tokens"
        case cacheWriteTokens = "cache_write_tokens"
        case sessionInputTokens = "session_input_tokens"
        case sessionOutputTokens = "session_output_tokens"
        case costUSD = "cost_usd"
        case costCompleteness = "cost_completeness"
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        func count(_ key: CodingKeys) -> Int64? {
            guard let value = try? values.decode(Int64.self, forKey: key), value >= 0 else { return nil }
            return value
        }
        source = (try? values.decode(String.self, forKey: .source)) ?? ""
        kind = (try? values.decode(String.self, forKey: .kind)) ?? ""
        contextTokens = count(.contextTokens)
        contextCapacityTokens = count(.contextCapacityTokens).flatMap { $0 > 0 ? $0 : nil }
        inputTokens = count(.inputTokens)
        outputTokens = count(.outputTokens)
        cacheReadTokens = count(.cacheReadTokens)
        cacheWriteTokens = count(.cacheWriteTokens)
        sessionInputTokens = count(.sessionInputTokens)
        sessionOutputTokens = count(.sessionOutputTokens)
        let cost = try? values.decode(Double.self, forKey: .costUSD)
        costUSD = cost.flatMap { $0.isFinite && $0 >= 0 ? $0 : nil }
        costCompleteness = (try? values.decode(String.self, forKey: .costCompleteness)) ?? "unknown"
    }
}
