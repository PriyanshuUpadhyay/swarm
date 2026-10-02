import Foundation

/// One workspace row of the sidebar, as plain values.
public struct SidebarRow: Sendable, Hashable, Identifiable {
    public let id: String
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
        search: String, showingArchive: Bool, now: Int
    ) -> [SidebarSection] {
        let visible = workspaces.filter { navigation.matches(search, entry: $0) }
        var sections: [SidebarSection] = []
        if !showingArchive {
            // Titles once for the whole list; a per-row scan made this quadratic in workspaces.
            let idsByTitle = navigation.idsByTitle(workspaces)
            let pinned = visible.filter {
                navigation.pinned.contains($0.id) && !navigation.archived.contains($0.id)
            }.map { row($0, idsByTitle: idsByTitle, navigation: navigation, now: now) }
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
            }.map { row($0, idsByTitle: idsByTitle, navigation: navigation, now: now, inProject: true) }
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

    static func row(
        _ entry: WorkspaceEntry, idsByTitle: [String: [String]],
        navigation: WorkspaceNavigation, now: Int, inProject: Bool = false
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
            id: entry.id, title: navigation.title(for: entry, inProject: inProject),
            detail: navigation.detail(for: entry, idsByTitle: idsByTitle, inProject: inProject),
            status: entry.status, counts: counts, age: age,
            help: "\(entry.project.name) · \(entry.workspace.name)\n\(entry.id)",
            pinned: navigation.pinned.contains(entry.id),
            archived: navigation.archived.contains(entry.id)
        )
    }
}
