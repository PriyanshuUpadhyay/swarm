import Foundation

public struct RecentlyClosedChat: Sendable, Hashable, Identifiable {
    public var id: SwarmSessionID { chat.id }
    public let chat: SwarmProjectSession
    public let title: String
    public let workspace: String
    public let archivedAt: Int
    public let age: String
}

public enum RecentlyClosed {
    public static func list(
        chats: [SwarmProjectSession], navigation: WorkspaceNavigation,
        workspaces: [WorkspaceEntry], now: Int
    ) -> [RecentlyClosedChat] {
        chats.compactMap { chat in
            guard let archivedAt = chat.sessions.compactMap(\.archivedAt).max() else { return nil }
            let path = chat.session.cwd
            let workspace = workspaces.first { $0.id == path }.map { navigation.title(for: $0) }
                ?? navigation.names[path] ?? URL(fileURLWithPath: path).lastPathComponent
            let age = SessionRowPresentation.ageText(since: archivedAt, now: now) + " ago"
            return RecentlyClosedChat(chat: chat, title: navigation.title(for: chat),
                                      workspace: workspace, archivedAt: archivedAt, age: age)
        }.sorted {
            $0.archivedAt == $1.archivedAt ? $0.id.rawValue > $1.id.rawValue : $0.archivedAt > $1.archivedAt
        }
    }

    public static func restore(_ chat: SwarmProjectSession, bus: any SwarmBus) async throws {
        try await bus.unarchive(chat.sessions.map(\.id))
    }
}
