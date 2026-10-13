import Foundation

public enum ProfileAction: Sendable, Equatable {
    case new(String)
    case rename(from: String, to: String)
    case copy(from: String, to: String)
    case delete(String)
    case reset
    case setMinUsage(Int)

    var arguments: [String] {
        switch self {
        case .new(let name): ["new", name]
        case .rename(let from, let to): ["rename", from, to]
        case .copy(let from, let to): ["copy", from, to]
        case .delete(let name): ["delete", name]
        case .reset: ["reset"]
        case .setMinUsage(let pct): ["set-min-usage", String(pct)]
        }
    }
}
