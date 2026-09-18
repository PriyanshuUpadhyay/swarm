/// What one swarm agent pane draws.
public enum SwarmAgentPaneState: Sendable, Hashable {
    public enum Attachment: Sendable, Hashable {
        case notStarted
        case running
        case exited
    }

    case noLivePane
    case agentEnded
    case terminal
    case reattach

    public init(isAlive: Bool?, attachment: Attachment) {
        switch isAlive {
        case nil:
            self = .noLivePane
        case false:
            self = .agentEnded
        case true:
            self = switch attachment {
            case .notStarted: .terminal
            case .running: .terminal
            case .exited: .reattach
            }
        }
    }
}
