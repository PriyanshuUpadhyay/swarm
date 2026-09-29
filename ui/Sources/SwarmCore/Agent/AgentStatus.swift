import Foundation

/// The one status every surface shows for an agent (contract C2).
public enum AgentStatus: String, Sendable, Hashable, CaseIterable {
    case working, waiting, done, failed, ended

    /// Not alive is ended. An alive agent with no known state is idle, so done.
    public init(alive: Bool?, state: String?) {
        if alive == false {
            self = .ended
        } else {
            self = state.flatMap(Self.init(rawValue:)).flatMap { $0 == .ended ? nil : $0 } ?? .done
        }
    }

    /// Higher is more urgent: waiting, failed, working, done, ended.
    public var urgency: Int {
        switch self {
        case .waiting: 4
        case .failed: 3
        case .working: 2
        case .done: 1
        case .ended: 0
        }
    }

    /// The most urgent status, or nil when there is none.
    public static func aggregate(_ statuses: some Sequence<AgentStatus>) -> AgentStatus? {
        statuses.max { $0.urgency < $1.urgency }
    }
}

extension SwarmAgent {
    public var status: AgentStatus { AgentStatus(alive: alive, state: state) }
}
