import Foundation

public enum WorkspaceArchive {
    public struct Confirmation: Sendable, Hashable {
        public let liveAgents: Int
        public var required: Bool { liveAgents > 0 }
        public init(liveAgents: Int) { self.liveAgents = liveAgents }
    }

    public static func liveAgents(in chats: [SwarmProjectSession], bus: any SwarmBus) async throws -> Int {
        let agents = try await bus.agentsBySession()
        return chats.flatMap(\.sessions).reduce(0) { count, session in
            count + (agents[session.id] ?? []).filter { $0.alive == true }.count
        }
    }

    public static func end(_ chats: [SwarmProjectSession], bus: any SwarmBus) async throws {
        let agents = try await bus.agentsBySession()
        try await SwarmSessionCloser.end(sessions: chats.flatMap(\.sessions), agentsBySession: agents, bus: bus)
    }
}
