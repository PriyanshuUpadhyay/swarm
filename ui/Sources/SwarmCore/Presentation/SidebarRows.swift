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
    public var id: String { title }
    public let title: String
    public let rows: [SidebarRow]
}

public enum SidebarRows {
    /// Pinned, then My workspaces; or Archived alone. Rows keep the workspace order, so a status
    /// change sets a row's glyph but never moves the row.
    public static func sections(
        workspaces: [WorkspaceEntry], navigation: WorkspaceNavigation,
        search: String, showingArchive: Bool, now: Int
    ) -> [SidebarSection] {
        let visible = workspaces.filter { navigation.matches(search, entry: $0) }
        func rows(_ keep: (WorkspaceEntry) -> Bool) -> [SidebarRow] {
            visible.filter(keep).map { row($0, among: workspaces, navigation: navigation, now: now) }
        }
        if showingArchive {
            return [SidebarSection(title: "Archived", rows: rows { navigation.archived.contains($0.id) })]
        }
        return [
            SidebarSection(title: "Pinned", rows: rows {
                navigation.pinned.contains($0.id) && !navigation.archived.contains($0.id)
            }),
            SidebarSection(title: "My workspaces", rows: rows {
                !navigation.pinned.contains($0.id) && !navigation.archived.contains($0.id)
            }),
        ]
    }

    static func row(
        _ entry: WorkspaceEntry, among workspaces: [WorkspaceEntry],
        navigation: WorkspaceNavigation, now: Int
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
            id: entry.id, title: navigation.title(for: entry),
            detail: navigation.detail(for: entry, among: workspaces),
            status: entry.status, counts: counts, age: age,
            help: "\(entry.project.name) · \(entry.workspace.name)\n\(entry.id)",
            pinned: navigation.pinned.contains(entry.id),
            archived: navigation.archived.contains(entry.id)
        )
    }
}
