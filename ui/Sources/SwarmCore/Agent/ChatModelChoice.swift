import Foundation
import TranscriptTool

public enum ChatModelChoice {
    public static func initial(current: String?, models: [SwarmModel]) -> String {
        current.flatMap { SwarmChatLaunchPlan.validModel($0) ? $0 : nil } ?? models.first?.id ?? ""
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
