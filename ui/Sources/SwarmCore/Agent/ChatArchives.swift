import Foundation

/// Keeps background discovery from restoring a chat while its archive is in flight.
public struct ChatArchives {
    /// Restores only the failed chat; other tab edits made during the archive stay in place.
    public struct TabSnapshot {
        private let workspace: String
        private let key: String
        private let index: Int?
        private let group: TabGroup?
        private let memberIndex: Int?
        private let historyIndex: Int?

        public init(key: String, workspace: String, navigation: WorkspaceNavigation) {
            self.workspace = workspace
            self.key = key
            let strip = navigation.tabs[workspace]
            index = strip?.open.firstIndex(of: key)
            group = strip?.groups.first { $0.members.contains(key) }
            memberIndex = group?.members.firstIndex(of: key)
            historyIndex = navigation.tabHistory[workspace]?.firstIndex(of: key)
        }

        public func restore(in navigation: inout WorkspaceNavigation) {
            guard let index else { return }
            var strip = navigation.tabs[workspace] ?? TabStrip()
            strip.open.removeAll { $0 == key }
            for groupIndex in strip.groups.indices { strip.groups[groupIndex].members.removeAll { $0 == key } }
            // Closing the only member removes its group; that is not an owner deletion.
            if let group, group.members == [key], !strip.groups.contains(where: { $0.id == group.id }) {
                var restored = group
                restored.members = []
                strip.groups.append(restored)
            }
            var position = min(index, strip.open.count)
            if let groupIndex = strip.groups.firstIndex(where: { $0.id == group?.id }), let memberIndex {
                let members = strip.groups[groupIndex].members
                if memberIndex < members.count, let next = strip.open.firstIndex(of: members[memberIndex]) {
                    position = next
                } else if let last = members.last, let lastIndex = strip.open.firstIndex(of: last) {
                    position = lastIndex + 1
                }
                strip.groups[groupIndex].members.append(key)
            }
            strip.open.insert(key, at: position)
            navigation.tabs[workspace] = strip.pruned(to: Set(strip.open))
            if let historyIndex {
                var history = navigation.tabHistory[workspace] ?? []
                history.removeAll { $0 == key }
                history.insert(key, at: min(historyIndex, history.count))
                navigation.tabHistory[workspace] = history
            }
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
