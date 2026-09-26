import Foundation
import TranscriptTool

/// Preferences are choices, not evidence of which model answered a turn.
@MainActor
public final class ChatModelPreferences {
    private let defaults: UserDefaults
    public init(defaults: UserDefaults = .standard) { self.defaults = defaults }

    public var provider: String? { defaults.string(forKey: "chat.provider") }

    public func model(for provider: String) -> String? {
        defaults.string(forKey: "chat.model.\(provider)")
    }

    public func remember(provider: String, model: String) {
        guard SwarmChatProvider.all.contains(provider), SwarmChatLaunchPlan.validModel(model) else { return }
        defaults.set(provider, forKey: "chat.provider")
        defaults.set(model, forKey: "chat.model.\(provider)")
    }
}

public enum ChatModelChoice {
    public static func initial(current: String?, saved: String?, models: [SwarmModel]) -> String {
        [current, saved].compactMap { $0 }.first(where: SwarmChatLaunchPlan.validModel)
            ?? models.first?.id ?? ""
    }

    public static func latest(in records: [TranscriptRecord]) -> String? {
        for record in records.reversed() {
            if case .sessionInfo(let kind, let value, _) = record.event,
               kind == "model", SwarmChatLaunchPlan.validModel(value), value != "<synthetic>" {
                return value
            }
        }
        return nil
    }
}

public enum ChatSwitchPhase: Sendable, Equatable {
    case preparing, summarizing, starting, waiting, delivering

    public var canCancel: Bool { self == .preparing || self == .summarizing }

    public var title: String {
        switch self {
        case .preparing: "Reading the current chat…"
        case .summarizing: "Preparing a summary…"
        case .starting: "Starting the new agent…"
        case .waiting: "Waiting for the new agent…"
        case .delivering: "Sending the summary…"
        }
    }
}
