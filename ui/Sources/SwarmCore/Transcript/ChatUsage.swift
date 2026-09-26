import Foundation
import TranscriptTool

/// One provider session. Costs are cumulative snapshots; context describes the last reported call.
public struct ChatUsage: Sendable, Equatable {
    public private(set) var context: TranscriptUsage?
    public private(set) var cost: TranscriptUsage?
    public private(set) var contextTimestamp = ""
    public private(set) var costTimestamp = ""
    public private(set) var contextNotice = "No usage report in the loaded transcript."
    private var model: String?

    public init() {}

    public mutating func ingest(_ event: TranscriptEvent) {
        switch event {
        case .usage(let value, let meta) where value.kind == "context":
            context = value
            contextTimestamp = meta.timestamp
            contextNotice = value.contextTokens == nil ? "Context tokens were not reported." : ""
        case .usage(let value, let meta) where value.kind == "cost":
            cost = value
            costTimestamp = meta.timestamp
        case .systemMessage(let kind, _, _) where kind == "compaction":
            clearContext("Waiting for a usage report after compaction.")
        case .sessionInfo(let kind, let value, _) where kind == "model":
            if let model, model != value { clearContext("Waiting for a usage report from this model.") }
            model = value
        default:
            break
        }
    }

    private mutating func clearContext(_ notice: String) {
        context = nil
        contextTimestamp = ""
        contextNotice = notice
    }

    /// Full reported window, without guessing the provider's compaction reserve.
    public var remainingPercent: Int? {
        guard let used = context?.contextTokens, let capacity = context?.contextCapacityTokens else { return nil }
        return Int((Double(max(0, capacity - used)) / Double(capacity) * 100).rounded())
    }

    public var contextLabel: String {
        if let remainingPercent { return "Context ~\(remainingPercent)% left" }
        if let tokens = context?.contextTokens { return "Context \(tokens.formatted()) tokens" }
        return "Usage unavailable"
    }

    public var costLabel: String? {
        guard let amount = cost?.costUSD else { return nil }
        let prefix = switch cost?.costCompleteness {
        case "complete": "Est."
        case "partial": "Partial est."
        default: "Est. (coverage unknown)"
        }
        return "\(prefix) " + amount.formatted(.currency(code: "USD"))
    }

    public var summary: String {
        [contextLabel, costLabel].compactMap { $0 }.joined(separator: " · ")
    }
}
