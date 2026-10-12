import Foundation

public enum AgentStateText {
    public static func title(status: AgentStatus, kind: String?) -> String {
        switch status {
        case .working: "Working"
        case .waiting:
            switch kind {
            case "permission": "Waiting for permission"
            case "question": "Asked a question"
            default: "Waiting for you"
            }
        case .done: "Done"
        case .failed: "Failed"
        case .ended: "Ended"
        }
    }

    public static func tooltip(agent: SwarmAgent, now: Int) -> String {
        var text = agent.status.rawValue
        if let kind = agent.stateKind, kind == "permission" || kind == "question" {
            text += " · " + kind
        }
        if let timestamp = agent.stateAtS {
            text += " since " + SessionRowPresentation.ageText(since: timestamp, now: now)
        }
        if let source = agent.stateSource, !source.isEmpty { text += " · " + source }
        if let detail = agent.stateDetail, !detail.isEmpty { text += "\n" + detail }
        return text
    }
}
