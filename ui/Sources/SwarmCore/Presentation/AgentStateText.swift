import Foundation

public enum AgentStateText {
    public static func tooltip(agent: SwarmAgent, now: Int) -> String {
        var text = agent.status.rawValue
        if let timestamp = agent.stateAtS {
            text += " since " + SessionRowPresentation.ageText(since: timestamp, now: now)
        }
        if let source = agent.stateSource, !source.isEmpty { text += " · " + source }
        if let detail = agent.stateDetail, !detail.isEmpty { text += "\n" + detail }
        return text
    }
}
