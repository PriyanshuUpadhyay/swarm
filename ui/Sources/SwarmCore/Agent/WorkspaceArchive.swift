import Foundation

public enum WorkspaceArchive {
    public struct Confirmation: Sendable, Hashable {
        public let liveAgents: Int
        public var required: Bool { liveAgents > 0 }
        public init(liveAgents: Int) { self.liveAgents = liveAgents }
    }

    public static func liveAgents(in chats: [SwarmProjectSession], bus: any SwarmBus) async throws -> Int {
        let agents = try await SwarmSessionCloser.agents(in: chats, listing: try? await bus.agentsBySession(), bus: bus)
        return chats.flatMap(\.sessions).reduce(0) { count, session in
            count + (agents[session.id] ?? []).filter { $0.alive == true }.count
        }
    }

    public static func end(_ chats: [SwarmProjectSession], bus: any SwarmBus) async throws {
        try await SwarmSessionCloser.end(chats: chats, agentsBySession: try? await bus.agentsBySession(), bus: bus)
    }
}
