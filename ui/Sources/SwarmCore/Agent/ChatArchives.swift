import Foundation

/// Keeps background discovery from restoring a chat while its archive is in flight.
public struct ChatArchives {
    private var pending: [SwarmSessionID: Set<SwarmSessionID>] = [:]
    private var confirmed: Set<SwarmSessionID> = []

    public init() {}

    public mutating func begin(_ id: SwarmSessionID, in tree: SessionsTree) -> [SwarmSessionID] {
        let ids = tree.archiveIDs(for: id)
        guard !ids.isEmpty, hidden.isDisjoint(with: ids) else { return [] }
        pending[id] = Set(ids)
        return ids
    }

    public mutating func finish(_ id: SwarmSessionID, succeeded: Bool) {
        guard let ids = pending.removeValue(forKey: id) else { return }
        if succeeded { confirmed.formUnion(ids) }
    }

    /// Call only with discovery started after the last archive completed.
    public mutating func reconcile(_ source: SessionsTree) {
        let present = source.projects.flatMap(\.workspaces).flatMap(\.sessions)
            .flatMap(\.sessions).map(\.id)
        confirmed.formIntersection(present)
    }

    /// The tree without archived rows, nor the rows in `starting` (sessions of chats being started).
    public func applying(to source: SessionsTree, hiding starting: Set<SwarmSessionID> = []) -> SessionsTree {
        let excluded = hidden.union(starting)
        return SessionsTree(projects: source.projects.map { project in
            ProjectNode(
                id: project.id, path: project.path, launchDirectory: project.launchDirectory,
                workspaces: project.workspaces.map { workspace in
                    WorkspaceNode(
                        path: workspace.path, name: workspace.name,
                        sessions: workspace.sessions.filter {
                            excluded.isDisjoint(with: $0.sessions.map(\.id))
                        }, branch: workspace.branch
                    )
                }
            )
        }, agentsBySession: source.agentsBySession)
    }

    public static func selection(
        afterArchiving id: SwarmSessionID, selected: SwarmSessionID?, in tree: SessionsTree
    ) -> SwarmSessionID? {
        guard let row = tree.session(id), let selected,
              row.sessions.contains(where: { $0.id == selected }) else { return selected }
        let tabs = tree.workspaceChats(for: row.id).map(\.id)
        guard let index = tabs.firstIndex(of: row.id) else { return nil }
        if index + 1 < tabs.count { return tabs[index + 1] }
        return index > 0 ? tabs[index - 1] : nil
    }

    private var hidden: Set<SwarmSessionID> {
        pending.values.reduce(confirmed) { $0.union($1) }
    }
}
