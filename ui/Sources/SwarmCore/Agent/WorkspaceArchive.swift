import Foundation

public enum WorkspaceArchive {
    public static func liveAgents(in chats: [SwarmProjectSession], bus: any SwarmBus) async throws -> Int {
        var count = 0
        for chat in chats {
            for session in chat.sessions {
                count += try await bus.agents(in: session).filter { $0.alive == true }.count
            }
        }
        return count
    }

    public static func end(_ chats: [SwarmProjectSession], bus: any SwarmBus) async throws {
        for chat in chats { try await SwarmSessionCloser.end(session: chat, bus: bus) }
    }
}
