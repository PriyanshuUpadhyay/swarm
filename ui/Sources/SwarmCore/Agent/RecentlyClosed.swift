import Foundation

public struct RecentlyClosedChat: Sendable, Hashable, Identifiable {
    public var id: SwarmSessionID { chat.id }
    public let chat: SwarmProjectSession
    public let title: String
    public let workspace: String
    public let archivedAt: Int
    public let age: String
}

/// A missing handoff can wait for discovery; a restore waits only for the next completed list.
public enum PendingChatSelection: Sendable, Hashable {
    case handoff(SwarmSessionID)
    case restored(SwarmSessionID)

    public var id: SwarmSessionID {
        switch self {
        case .handoff(let id), .restored(let id): id
        }
    }

    public var isRestoring: Bool {
        if case .restored = self { return true }
        return false
    }

    /// Call only after a completed refresh does not list the pending chat.
    public static func settleMissingAfterRefresh(
        _ selection: Self?
    ) -> (pending: Self?, clearSelection: Bool, notice: String?) {
        guard selection?.isRestoring == true else { return (selection, false, nil) }
        return (nil, true, RecentlyClosed.restoredButNotListed)
    }
}

public enum RecentlyClosed {
    public static let restoredButNotListed = "The chat was reopened, but it is not in the workspace list. Check the sidebar after the list refreshes."

    public struct Listing: Sendable {
        /// Keep this in sync with ARCHIVED_SESSIONS_LIMIT in src/store.rs.
        public static let cliArchivedLimit = 50

        public let chats: [RecentlyClosedChat]
        public let notice: String?

        public init(chats: [RecentlyClosedChat], archivedSessionCount: Int) {
            self.chats = chats
            notice = archivedSessionCount == Self.cliArchivedLimit
                ? "Showing the newest \(Self.cliArchivedLimit) closed sessions." : nil
        }
    }

    /// A superseded discovery gets one retry; a second supersession leaves selection pending.
    @MainActor
    public static func refreshRestoredChat(
        refresh: () async throws -> Bool, isListed: () -> Bool
    ) async throws -> Bool {
        for _ in 0..<2 {
            let completed: Bool
            do { completed = try await refresh() }
            catch let error as CancellationError { throw error }
            catch {
                throw SwarmProfileError.failed("The chat was reopened, but the list could not refresh. \(error.localizedDescription)")
            }
            if isListed() { return true }
            if completed {
                throw SwarmProfileError.failed(restoredButNotListed)
            }
        }
        return false
    }

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
