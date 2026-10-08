import Foundation

/// Keeps background discovery from restoring a chat while its archive is in flight.
public struct ChatArchives {
    /// Restores the strip if discovery prunes it while an archive is pending.
    public struct TabSnapshot {
        private let workspace: String
        private let strip: TabStrip?
        private let history: [String]?

        public init(workspace: String, navigation: WorkspaceNavigation) {
            self.workspace = workspace
            strip = navigation.tabs[workspace]
            history = navigation.tabHistory[workspace]
        }

        public func restore(in navigation: inout WorkspaceNavigation) {
            navigation.tabs[workspace] = strip
            navigation.tabHistory[workspace] = history
        }
    }

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

    public mutating func restore(_ ids: [SwarmSessionID]) {
        confirmed.subtract(ids)
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
                    var visible = workspace
                    visible.sessions.removeAll { !excluded.isDisjoint(with: $0.sessions.map(\.id)) }
                    return visible
                }
            )
        }, agentsBySession: source.agentsBySession)
    }

    public static func selection(
        afterArchiving id: SwarmSessionID, selected: SwarmSessionID?, in tree: SessionsTree,
        history: [String], open: [String]
    ) -> SwarmSessionID? {
        guard let row = tree.session(id), let selected,
              row.sessions.contains(where: { $0.id == selected }) else { return selected }
        let remaining = open.filter { $0 != ChatTitle.key(row) }
        let key = TabStrip.selectionAfterClose(history: history, open: remaining)
        return tree.workspaceChats(for: row.id).first { ChatTitle.key($0.session) == key }?.id
    }

    private var hidden: Set<SwarmSessionID> {
        pending.values.reduce(confirmed) { $0.union($1) }
    }
}
