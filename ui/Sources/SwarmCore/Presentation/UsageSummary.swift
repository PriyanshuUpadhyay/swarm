import Foundation

public struct SessionUsageShare: Sendable, Equatable, Identifiable {
    public let id: SwarmSessionID
    public let provider: String?
    public let tokens: Int64?
    public let costUsd: Double?
}

public struct ChainUsage: Sendable, Equatable {
    public let tokens: Int64?
    public let costUsd: Double?
    public let sessions: [SessionUsageShare]
}

public enum UsageSummary {
    /// Chair rows carry cumulative session snapshots; missing reports stay unknown, not zero.
    public static func chain(
        sessions: [SwarmSession], usageBySession: [SwarmSessionID: SwarmAgent]
    ) -> ChainUsage {
        let shares = sessions.map { session in
            let chair = usageBySession[session.id]
            return SessionUsageShare(id: session.id, provider: session.chairProvider ?? chair?.provider,
                                     tokens: chair?.tokens, costUsd: chair?.costUsd)
        }
        let tokens = shares.compactMap(\.tokens)
        let costs = shares.compactMap(\.costUsd)
        let totalTokens = tokens.reduce(Int64(0)) { total, value in
            let sum = total.addingReportingOverflow(value)
            return sum.overflow ? Int64.max : sum.partialValue
        }
        return ChainUsage(tokens: tokens.isEmpty ? nil : totalTokens,
                          costUsd: costs.isEmpty ? nil : costs.reduce(0, +), sessions: shares)
    }
}
