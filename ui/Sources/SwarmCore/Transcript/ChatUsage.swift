import Foundation
import TranscriptTool

/// One provider session. Context describes the last reported call.
public struct ChatUsage: Sendable, Equatable {
    public private(set) var context: TranscriptUsage?
    public private(set) var contextTimestamp = ""
    public private(set) var contextNotice = "No usage report in the loaded transcript."
    private var model: String?

    public init() {}

    public mutating func ingest(_ event: TranscriptEvent) {
        switch event {
        case .usage(let value, let meta) where value.kind == "context":
            context = value
            contextTimestamp = meta.timestamp
            contextNotice = value.contextTokens == nil ? "Context tokens were not reported." : ""
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

    /// Nil when no context count is reported; the usage sidebar still shows the details.
    public func contextMeter(locale: Locale = .current) -> ComposerContextMeter? {
        guard let tokens = context?.contextTokens.map(Int.init) else { return nil }
        if let remainingPercent {
            return ComposerContextMeter(
                contextTokens: tokens, remainingPercent: remainingPercent,
                label: "\(remainingPercent)% left",
                accessibilityLabel: "Context \(100 - remainingPercent) percent used, \(remainingPercent) percent left"
            )
        }
        return ComposerContextMeter(
            contextTokens: tokens, remainingPercent: nil,
            label: tokens.formatted(.number.notation(.compactName).locale(locale)),
            accessibilityLabel: "Context \(tokens.formatted(.number.locale(locale))) tokens"
        )
    }
}

/// What the composer bar shows for the context window, as plain values.
public struct ComposerContextMeter: Equatable, Sendable {
    public var contextTokens: Int?
    /// Set only when the provider reports the window capacity.
    public var remainingPercent: Int?
    public var label: String
    public var accessibilityLabel: String
}
