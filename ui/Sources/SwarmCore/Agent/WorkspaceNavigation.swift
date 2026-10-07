import Foundation

public struct WorkspaceEntry: Identifiable, Sendable {
    public var id: String { workspace.path }
    public let project: ProjectNode
    public let workspace: WorkspaceNode
    public var chats: [SwarmProjectSession] { workspace.sessions.filter { $0.totalAgents != 0 } }
    public var lastActivity: Int { chats.map(\.lastActivity).max() ?? 0 }
    public var isRunning: Bool { chats.contains { $0.isRunning == true } }
    /// Sets the row's glyph only; it never changes the row order.
    public var status: AgentStatus? { AgentStatus.aggregate(chats.compactMap(\.status)) }
    public var statusCounts: [AgentStatus: Int] {
        chats.reduce(into: [:]) { total, chat in total.merge(chat.statusCounts, uniquingKeysWith: +) }
    }
    public var folderName: String { URL(fileURLWithPath: id).lastPathComponent }

    public static func list(in tree: SessionsTree, workspaceOrder: [String: [String]] = [:]) -> [Self] {
        tree.projects.flatMap { project in
            SessionsTree.ordered(project.workspaces, order: workspaceOrder[project.path] ?? [],
                                 mainPath: project.mainWorkspacePath, hubPath: project.path)
                .map { Self(project: project, workspace: $0) }
        }
    }
}

public struct WorkspaceNavigation: Codable, Equatable, Sendable {
    public var selectedWorkspace: String?
    public var selectedChats: [String: String] = [:]
    public var pinned: Set<String> = []
    public var archived: Set<String> = []
    public var names: [String: String] = [:]
    public var chatNames: [String: String] = [:]
    public var projectNames: [String: String] = [:]
    public var workspaceOrder: [String: [String]] = [:]
    /// Project and workspace paths whose rows are collapsed. Chat children start folded;
    /// `expanded:chat:<root id>` records the exception in the same saved view state.
    public var collapsed: Set<String> = []

    public init() {}

    /// A drop can move only a known workspace within the same project.
    @discardableResult
    public mutating func moveWorkspace(_ path: String, onto target: String, in entries: [WorkspaceEntry]) -> Bool {
        guard path != target,
              let source = entries.first(where: { $0.id == path }),
              let destination = entries.first(where: { $0.id == target }),
              source.project.id == destination.project.id,
              !archived.contains(path), !archived.contains(target) else { return false }
        var paths = SessionsTree.ordered(source.project.workspaces,
                                        order: workspaceOrder[source.project.path] ?? [],
                                        mainPath: source.project.mainWorkspacePath, hubPath: source.project.path).map(\.path)
        guard let start = paths.firstIndex(of: path), let end = paths.firstIndex(of: target) else { return false }
        paths.remove(at: start)
        paths.insert(path, at: end)
        workspaceOrder[source.project.path] = paths
        return true
    }

    @discardableResult
    public mutating func pinWorkspace(_ path: String, in entries: [WorkspaceEntry]) -> Bool {
        guard entries.contains(where: { $0.id == path }), !archived.contains(path) else { return false }
        pinned.insert(path)
        return true
    }

    public mutating func renameChat(_ chat: SwarmProjectSession, to name: String) {
        chatNames[ChatTitle.key(chat)] = Self.savedName(name)
    }

    public mutating func renameProject(_ project: ProjectNode, to name: String) {
        projectNames[project.path] = Self.savedName(name)
    }

    public func projectTitle(for project: ProjectNode) -> String {
        projectNames[project.path].flatMap(Self.savedName) ?? project.name
    }

    private static func savedName(_ value: String) -> String? {
        let name = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? nil : name
    }

    public func isCollapsed(_ id: String) -> Bool {
        id.hasPrefix("chat:") ? !collapsed.contains("expanded:\(id)") : collapsed.contains(id)
    }

    public mutating func toggleCollapsed(_ id: String) {
        let key = id.hasPrefix("chat:") ? "expanded:\(id)" : id
        if collapsed.contains(key) { collapsed.remove(key) }
        else { collapsed.insert(key) }
    }

    private enum CodingKeys: String, CodingKey {
        case selectedWorkspace, selectedChats, collapsed
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        selectedWorkspace = try container.decodeIfPresent(String.self, forKey: .selectedWorkspace)
        selectedChats = try container.decodeIfPresent([String: String].self, forKey: .selectedChats) ?? [:]
        collapsed = try container.decodeIfPresent(Set<String>.self, forKey: .collapsed) ?? []
    }

    /// Under its project's header (`inProject`) a row drops the project name, and a main
    /// checkout, whose folder is named like the project, shows its branch.
    public func title(for entry: WorkspaceEntry, inProject: Bool = false) -> String {
        if let name = customName(for: entry) { return name }
        let project = projectTitle(for: entry.project)
        let folder = entry.folderName
        if folder.isEmpty { return entry.id }
        if inProject { return folder == entry.project.name ? entry.workspace.branch ?? folder : folder }
        if entry.project.name == folder { return project }
        return "\(project) / \(folder)"
    }

    public func detail(for entry: WorkspaceEntry, among entries: [WorkspaceEntry]) -> String {
        detail(for: entry, idsByTitle: idsByTitle(entries))
    }

    /// Titles that compare equal ignoring case in the user's locale share a key, as
    /// `localizedCaseInsensitiveCompare` would decide ("Straße" and "STRASSE").
    static func titleKey(_ title: String) -> String {
        title.folding(options: [.caseInsensitive], locale: .current)
    }

    /// Workspace ids under each case-folded title, computed once for a whole list.
    public func idsByTitle(_ entries: [WorkspaceEntry], inProject: Bool = false) -> [String: [String]] {
        Dictionary(grouping: entries.map { (Self.titleKey(title(for: $0, inProject: inProject)), $0.id) }, by: \.0)
            .mapValues { $0.map(\.1) }
    }

    public func detail(
        for entry: WorkspaceEntry, idsByTitle: [String: [String]], inProject: Bool = false
    ) -> String {
        var parts: [String] = []
        let title = title(for: entry, inProject: inProject)
        let duplicates = (idsByTitle[Self.titleKey(title)] ?? []).filter { $0 != entry.id }
        if !duplicates.isEmpty {
            parts.append(pathQualifier(for: entry.id, others: duplicates))
        }
        let project = projectTitle(for: entry.project)
        if !inProject, customName(for: entry) != nil, !parts.contains(project) {
            parts.append(project)
        }
        if let branch = entry.workspace.branch,
           !parts.contains(branch), branch != title,
           customName(for: entry) != nil || branch != entry.folderName {
            parts.append(branch)
        }
        let count = entry.chats.count
        parts.append(count == 1 ? "1 chat" : "\(count) chats")
        return parts.joined(separator: " · ")
    }

    private func customName(for entry: WorkspaceEntry) -> String? {
        guard let name = names[entry.id]?.trimmingCharacters(in: .whitespacesAndNewlines),
              !name.isEmpty else { return nil }
        return name
    }

    private func pathQualifier(for path: String, others: [String]) -> String {
        let components = URL(fileURLWithPath: path).pathComponents
        let otherComponents = others.map { URL(fileURLWithPath: $0).pathComponents }
        for length in 1...components.count {
            let suffix = components.suffix(length)
            if otherComponents.allSatisfy({ !$0.suffix(length).elementsEqual(suffix) }) {
                return length == components.count ? path : suffix.joined(separator: "/")
            }
        }
        return path
    }

    public func selectedChat(in entry: WorkspaceEntry) -> SwarmProjectSession? {
        if let saved = selectedChats[entry.id],
           let chat = SwarmSessionListing.chat(SwarmSessionID(saved), in: entry.chats) {
            return chat
        }
        return entry.chats.first
    }

    public mutating func select(_ entry: WorkspaceEntry, chat: SwarmSessionID? = nil) {
        selectedWorkspace = entry.id
        if let id = chat ?? selectedChat(in: entry)?.id {
            selectedChats[entry.id] = id.rawValue
        }
    }

    // Archive is a UI filter. Bus sessions and worktree files remain available for restore.
    public mutating func archive(_ path: String) {
        archived.insert(path)
        if selectedWorkspace == path { selectedWorkspace = nil }
    }

    public func matches(_ query: String, entry: WorkspaceEntry) -> Bool {
        query.isEmpty || [title(for: entry), projectTitle(for: entry.project), entry.workspace.name, entry.id]
            .contains { $0.localizedCaseInsensitiveContains(query) }
            || entry.chats.contains { ChatTitle.title($0, appName: chatNames[ChatTitle.key($0)]).localizedCaseInsensitiveContains(query) }
    }
}

@MainActor
public final class WorkspaceNavigationStore {
    private let defaults: UserDefaults
    private let key = "workspaces.navigation"
    private let choices: OwnerChoicesStore

    public init(defaults: UserDefaults = .standard, choicesFolder: URL? = SwarmHome.dataFolder) {
        self.defaults = defaults
        choices = OwnerChoicesStore(folder: choicesFolder)
    }

    public func load() -> WorkspaceNavigation {
        var value = defaults.data(forKey: key)
            .flatMap { try? JSONDecoder().decode(WorkspaceNavigation.self, from: $0) } ?? WorkspaceNavigation()
        if let saved = try? choices.load() {
            value.pinned = saved.pinned
            value.archived = saved.archived
            value.names = saved.names
            value.chatNames = saved.chatNames
            value.projectNames = saved.projectNames
            value.workspaceOrder = saved.workspaceOrder
        }
        return value
    }

    public func save(_ value: WorkspaceNavigation) {
        try? choices.update {
            $0.pinned = value.pinned
            $0.archived = value.archived
            $0.names = value.names
            $0.chatNames = value.chatNames
            $0.projectNames = value.projectNames
            $0.workspaceOrder = value.workspaceOrder
        }
        guard let data = try? JSONEncoder().encode(value) else { return }
        defaults.set(data, forKey: key)
    }

    public func pruneMissingFolders(_ value: WorkspaceNavigation) throws -> WorkspaceNavigation {
        try choices.update { $0.pruneMissingFolders() }
        let saved = try choices.load()
        var pruned = value
        pruned.pinned = saved.pinned
        pruned.archived = saved.archived
        pruned.names = saved.names
        pruned.chatNames = saved.chatNames
        pruned.projectNames = saved.projectNames
        pruned.workspaceOrder = saved.workspaceOrder
        pruned.selectedChats = pruned.selectedChats.filter { OwnerChoices.folderExists($0.key) }
        save(pruned)
        return pruned
    }
}
