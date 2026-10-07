import Foundation

/// One row of the sidebar, as plain values.
public struct SidebarRow: Sendable, Hashable, Identifiable {
    public enum Kind: Sendable, Hashable { case workspace, chat, child, more }
    public let id: String
    public let kind: Kind
    public let depth: Int
    public let parentID: String?
    public let expanded: Bool
    public let hasChildren: Bool
    public let childrenSummary: String?
    public let title: String
    /// Branch, project, or a path qualifier; shown after the title in secondary text.
    public let detail: String
    public let status: AgentStatus?
    /// Agents per status, most urgent first; shown on hover.
    public let counts: [StatusCount]
    public let age: String?
    public let help: String
    public let pinned: Bool
    public let archived: Bool
    public let missing: Bool
    public let newChatEnabled: Bool
}

public struct StatusCount: Sendable, Hashable {
    public let status: AgentStatus
    public let count: Int
}

public struct SidebarSection: Sendable, Hashable, Identifiable {
    public enum Kind: Sendable, Hashable {
        case pinned
        /// Keyed by `ProjectNode.path`, which stays the same when a folder becomes a git repo.
        case project(path: String)
    }

    public let kind: Kind
    /// Unique even when two projects share a path, such as a bare clone kept as a folder.
    public let id: String
    public let title: String
    /// The most urgent status of the rows; the header shows it while collapsed.
    public let status: AgentStatus?
    public let rows: [SidebarRow]

    /// A project section's id, which also names the project for its header's "+".
    public static func id(of project: ProjectNode) -> String { "project:\(project.id)" }

    init(kind: Kind, id: String, title: String, rows: [SidebarRow]) {
        self.kind = kind
        self.id = id
        self.title = title
        self.status = AgentStatus.aggregate(rows.compactMap(\.status))
        self.rows = rows
    }
}

public enum SidebarRows {
    /// Pinned, then one section per project in tree order, even an empty one, so its "+" stays
    /// reachable. The archive view shows only projects with archived rows. Rows keep the
    /// workspace order, so a status change sets a row's glyph but never moves the row.
    public static func sections(
        projects: [ProjectNode], workspaces: [WorkspaceEntry], navigation: WorkspaceNavigation,
        search: String, showingArchive: Bool, now: Int,
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:], expandedLists: Set<String> = []
    ) -> [SidebarSection] {
        let visible = workspaces.filter { navigation.matches(search, entry: $0) }
        var sections: [SidebarSection] = []
        if !showingArchive {
            // Titles once for the whole list; a per-row scan made this quadratic in workspaces.
            let idsByTitle = navigation.idsByTitle(workspaces)
            let pinned = visible.filter {
                navigation.pinned.contains($0.id) && !navigation.archived.contains($0.id)
            }.flatMap {
                rows($0, idsByTitle: idsByTitle, navigation: navigation, now: now,
                     agentsBySession: agentsBySession, expandedLists: expandedLists)
            }
            if !pinned.isEmpty { sections.append(SidebarSection(kind: .pinned, id: "pinned", title: "Pinned", rows: pinned)) }
        }
        let names = Dictionary(grouping: projects.map(\.name), by: { $0 }).mapValues(\.count)
        for project in projects {
            let members = workspaces.filter { $0.project.id == project.id }
            let idsByTitle = navigation.idsByTitle(members, inProject: true)
            let rows = visible.filter { entry in
                entry.project.id == project.id && (showingArchive
                    ? navigation.archived.contains(entry.id)
                    : !navigation.pinned.contains(entry.id) && !navigation.archived.contains(entry.id))
            }.flatMap {
                rows($0, idsByTitle: idsByTitle, navigation: navigation, now: now, inProject: true,
                     agentsBySession: agentsBySession, expandedLists: expandedLists)
            }
            if rows.isEmpty, showingArchive || !search.isEmpty { continue }
            let parent = URL(fileURLWithPath: project.path).deletingLastPathComponent().lastPathComponent
            let title = names[project.name, default: 0] > 1 && !parent.isEmpty
                ? "\(project.name) — \(parent)" : project.name
            sections.append(SidebarSection(
                kind: .project(path: project.path), id: SidebarSection.id(of: project), title: title, rows: rows
            ))
        }
        return sections
    }

    /// A continuation can change the current session id without changing this row's identity.
    public static func chatID(_ chat: SwarmProjectSession) -> String {
        let root = chat.sessions.min {
            $0.createdAt == $1.createdAt ? $0.id.rawValue < $1.id.rawValue : $0.createdAt < $1.createdAt
        }!
        return "chat:\(root.id.rawValue)"
    }

    private static func rows(
        _ entry: WorkspaceEntry, idsByTitle: [String: [String]], navigation: WorkspaceNavigation,
        now: Int, inProject: Bool = false, agentsBySession: [SwarmSessionID: [SwarmAgent]],
        expandedLists: Set<String>
    ) -> [SidebarRow] {
        var result = [row(entry, idsByTitle: idsByTitle, navigation: navigation, now: now,
                          inProject: inProject, agentsBySession: agentsBySession)]
        guard !navigation.isCollapsed(entry.id) else { return result }
        let chats = entry.chats.sorted {
            $0.lastActivity == $1.lastActivity ? chatID($0) < chatID($1) : $0.lastActivity > $1.lastActivity
        }
        let shown = expandedLists.contains(entry.id) ? chats.count : min(5, chats.count)
        for chat in chats.prefix(shown) {
            let id = chatID(chat)
            let expanded = !navigation.isCollapsed(id)
            let children = chat.sessions.flatMap { session in
                (agentsBySession[session.id] ?? [])
                    .filter { $0.id != SwarmPanePolicy.chair && $0.id.rawValue != session.chairID?.rawValue }
                    .sorted { $0.id < $1.id }
                    .map { (session.id, $0) }
            }
            let agents = agentsBySession[chat.id]
            let status = expanded && !children.isEmpty
                ? agents?.first { $0.id == SwarmPanePolicy.chair || $0.id.rawValue == chat.session.chairID?.rawValue }?.status
                : AgentStatus.aggregate((agents ?? []).map(\.status) + children.map { $0.1.status }) ?? chat.status
            let counts = chat.statusCounts.map { StatusCount(status: $0.key, count: $0.value) }
                .sorted { $0.status.urgency > $1.status.urgency }
            let presentation = SessionRowPresentation.make(
                ChatRow(session: chat, workspace: entry.workspace.name, workspacePath: entry.id), now: now
            )
            let waiting = children.filter { $0.1.status == .waiting }.count
            result.append(SidebarRow(
                id: id, kind: .chat, depth: 1, parentID: entry.id, expanded: expanded,
                hasChildren: !children.isEmpty,
                childrenSummary: children.isEmpty ? nil : "\(children.count) agents · \(waiting) waiting",
                title: presentation.title, detail: "", status: status, counts: counts, age: presentation.age,
                help: "\(presentation.title)\n\(entry.id)", pinned: navigation.pinned.contains(entry.id),
                archived: navigation.archived.contains(entry.id), missing: false, newChatEnabled: false
            ))
            if expanded {
                result += children.map { sessionID, agent in
                    SidebarRow(
                        id: "child:\(sessionID.rawValue)/\(agent.id.rawValue)", kind: .child, depth: 2,
                        parentID: id, expanded: false, hasChildren: false, childrenSummary: nil,
                        title: agent.id.rawValue, detail: agent.role, status: agent.status, counts: [], age: nil,
                        help: "\(agent.id.rawValue) · \(agent.role)", pinned: navigation.pinned.contains(entry.id),
                        archived: navigation.archived.contains(entry.id), missing: false, newChatEnabled: false
                    )
                }
            }
        }
        if shown < chats.count {
            result.append(SidebarRow(
                id: "more:\(entry.id)", kind: .more, depth: 1, parentID: entry.id,
                expanded: false, hasChildren: false, childrenSummary: nil,
                title: "… \(chats.count - shown) more", detail: "", status: nil, counts: [], age: nil,
                help: "Show all chats", pinned: navigation.pinned.contains(entry.id),
                archived: navigation.archived.contains(entry.id), missing: false, newChatEnabled: false
            ))
        }
        return result
    }

    static func row(
        _ entry: WorkspaceEntry, idsByTitle: [String: [String]],
        navigation: WorkspaceNavigation, now: Int, inProject: Bool = false,
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:]
    ) -> SidebarRow {
        let age = entry.chats.max { $0.lastActivity < $1.lastActivity }.map {
            SessionRowPresentation.make(
                ChatRow(session: $0, workspace: entry.workspace.name, workspacePath: entry.id), now: now
            ).age
        }
        let counts = entry.statusCounts
            .map { StatusCount(status: $0.key, count: $0.value) }
            .sorted { $0.status.urgency > $1.status.urgency }
        return SidebarRow(
            id: entry.id, kind: .workspace, depth: 0, parentID: nil,
            expanded: !navigation.isCollapsed(entry.id), hasChildren: !entry.chats.isEmpty, childrenSummary: nil,
            title: entry.workspace.isRemoved
                ? entry.workspace.name : navigation.title(for: entry, inProject: inProject),
            detail: ([navigation.detail(for: entry, idsByTitle: idsByTitle, inProject: inProject)]
                + (entry.workspace.missing ? ["folder missing"] : [])
                + (entry.workspace.mark.map { [$0.rawValue] } ?? [])).joined(separator: " · "),
            status: AgentStatus.aggregate(entry.chats.compactMap(\.status) + entry.chats.flatMap {
                $0.sessions.flatMap { (agentsBySession[$0.id] ?? []).map(\.status) }
            }), counts: counts, age: age,
            help: "\(entry.project.name) · \(entry.workspace.name)\n\(entry.id)",
            pinned: navigation.pinned.contains(entry.id),
            archived: navigation.archived.contains(entry.id),
            missing: entry.workspace.missing, newChatEnabled: entry.workspace.canStartChat
        )
    }
}
