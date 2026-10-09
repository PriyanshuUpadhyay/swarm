import Foundation

public struct RecentWorkRow: Sendable, Hashable, Identifiable {
    public let id: SwarmSessionID
    public let title: String
    public let workspace: String
    public let workspacePath: String
    public let age: String
    public let state: SessionRowPresentation.State
    public let status: AgentStatus?
}

public struct FirstRunStep: Sendable, Hashable, Identifiable {
    public enum ID: String, Sendable, Hashable { case importProject, runSetup, startChat }
    public let id: ID
    public let title: String
    public let done: Bool
}

public enum HomeModel {
    public static func recentWork(
        tree: SessionsTree, navigation: WorkspaceNavigation, now: Int, limit: Int = 8
    ) -> [RecentWorkRow] {
        let entries = WorkspaceEntry.list(in: tree)
        let candidates: [(WorkspaceEntry, SwarmProjectSession)] = entries.flatMap { entry in
            entry.chats.map { (entry, $0) }
        }
        let chats = candidates.sorted {
            $0.1.lastActivity == $1.1.lastActivity
                ? $0.1.id.rawValue < $1.1.id.rawValue : $0.1.lastActivity > $1.1.lastActivity
        }
        return chats.prefix(max(0, limit)).map { entry, chat in
            let presentation = SessionRowPresentation.make(
                ChatRow(session: chat, workspace: entry.workspace.name, workspacePath: entry.id), now: now
            )
            return RecentWorkRow(
                id: chat.id, title: navigation.title(for: chat), workspace: navigation.title(for: entry),
                workspacePath: entry.id, age: presentation.age, state: presentation.state, status: chat.status
            )
        }
    }

    public static func firstRunSteps(hasProject: Bool, isSetUp: Bool, hasChat: Bool) -> [FirstRunStep] {
        guard !hasProject || !isSetUp || !hasChat else { return [] }
        return [
            FirstRunStep(id: .importProject, title: "Import a project", done: hasProject),
            FirstRunStep(id: .runSetup, title: "Run setup", done: isSetUp),
            FirstRunStep(id: .startChat, title: "Start a chat", done: hasChat),
        ]
    }
}
