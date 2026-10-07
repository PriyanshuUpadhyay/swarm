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
    var ownerChoices = OwnerChoices()
    public var pinned: Set<String> {
        get { ownerChoices.pinned }
        set { ownerChoices.pinned = newValue }
    }
    public var archived: Set<String> {
        get { ownerChoices.archived }
        set { ownerChoices.archived = newValue }
    }
    public var names: [String: String] {
        get { ownerChoices.names }
        set { ownerChoices.names = newValue }
    }
    public var chatNames: [String: String] {
        get { ownerChoices.chatNames }
        set { ownerChoices.chatNames = newValue }
    }
    public var projectNames: [String: String] {
        get { ownerChoices.projectNames }
        set { ownerChoices.projectNames = newValue }
    }
    public var workspaceOrder: [String: [String]] {
        get { ownerChoices.workspaceOrder }
        set { ownerChoices.workspaceOrder = newValue }
    }
    /// Project and workspace folds have separate keys. Chat children start folded;
    /// `expanded:<chat row id>` records the exception in the same saved view state.
    public var collapsed: Set<String> = []
    public var lastSeen: [String: Int] = [:]
    public var fields: RowFieldLists {
        get { ownerChoices.fields }
        set { ownerChoices.fields = newValue }
    }

    public init() {}

    public func workspaceMoveTarget(_ path: String, by offset: Int, in entries: [WorkspaceEntry]) -> String? {
        guard offset == -1 || offset == 1,
              let source = entries.first(where: { $0.id == path }), !archived.contains(path) else { return nil }
        let siblings = SessionsTree.ordered(source.project.workspaces,
                                           order: workspaceOrder[source.project.path] ?? [],
                                           mainPath: source.project.mainWorkspacePath, hubPath: source.project.path)
            .map(\.path).filter { !archived.contains($0) && pinned.contains($0) == pinned.contains(path) }
        guard let index = siblings.firstIndex(of: path), siblings.indices.contains(index + offset) else { return nil }
        return siblings[index + offset]
    }

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

    public func title(for chat: SwarmProjectSession) -> String {
        ChatTitle.title(chat, appName: chatNames[ChatTitle.key(chat)])
    }

    public mutating func renameChat(_ chat: SwarmProjectSession, to name: String) {
        chatNames[ChatTitle.key(chat)] = ChatTitle.nonblank(name)
    }

    public mutating func renameProject(_ project: ProjectNode, to name: String) {
        projectNames[project.path] = ChatTitle.nonblank(name)
    }

    public func projectTitle(for project: ProjectNode) -> String {
        ChatTitle.nonblank(projectNames[project.path]) ?? project.name
    }

    public static func projectCollapseID(_ path: String) -> String { "project:\(path)" }
    public static func workspaceCollapseID(_ path: String) -> String { "workspace:\(path)" }

    private static func collapseKey(_ id: String) -> String {
        if id.hasPrefix(SidebarRows.chatPrefix) { return "expanded:\(id)" }
        return id.hasPrefix("/") ? workspaceCollapseID(id) : id
    }

    public func isCollapsed(_ id: String) -> Bool {
        let stored = collapsed.contains(Self.collapseKey(id))
        return id.hasPrefix(SidebarRows.chatPrefix) ? !stored : stored
    }

    public mutating func toggleCollapsed(_ id: String) {
        let key = Self.collapseKey(id)
        if collapsed.contains(key) { collapsed.remove(key) }
        else { collapsed.insert(key) }
    }

    // Owner choices live in choices.json; defaults contain only view state.
    private enum CodingKeys: String, CodingKey {
        case selectedWorkspace, selectedChats, collapsed, lastSeen
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        selectedWorkspace = try container.decodeIfPresent(String.self, forKey: .selectedWorkspace)
        selectedChats = try container.decodeIfPresent([String: String].self, forKey: .selectedChats) ?? [:]
        collapsed = try container.decodeIfPresent(Set<String>.self, forKey: .collapsed) ?? []
        lastSeen = try container.decodeIfPresent([String: Int].self, forKey: .lastSeen) ?? [:]
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

    public func isUnread(_ chat: SwarmProjectSession) -> Bool {
        guard let seen = lastSeen[ChatTitle.key(chat)] else { return false }
        return chat.lastActivity > seen
    }

    /// First sight sets the baseline; only later activity makes a chat unread.
    public mutating func recordFirstSight(_ chats: [SwarmProjectSession]) {
        let listed = Set(chats.map(ChatTitle.key))
        lastSeen = lastSeen.filter { listed.contains($0.key) }
        for chat in chats where lastSeen[ChatTitle.key(chat)] == nil {
            lastSeen[ChatTitle.key(chat)] = chat.lastActivity
        }
    }

    public mutating func markSeen(_ chat: SwarmProjectSession, now: Int = Int(Date().timeIntervalSince1970)) {
        let key = ChatTitle.key(chat)
        lastSeen[key] = max(lastSeen[key] ?? 0, now, chat.lastActivity)
    }

    public mutating func select(_ entry: WorkspaceEntry, chat: SwarmSessionID? = nil, now: Int = Int(Date().timeIntervalSince1970)) {
        selectedWorkspace = entry.id
        if let id = chat ?? selectedChat(in: entry)?.id {
            selectedChats[entry.id] = id.rawValue
            if let selected = SwarmSessionListing.chat(id, in: entry.chats) { markSeen(selected, now: now) }
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
            || entry.chats.contains { title(for: $0).localizedCaseInsensitiveContains(query) }
    }
}

@MainActor
public final class WorkspaceNavigationStore {
    private let defaults: UserDefaults
    private let key = "workspaces.navigation"
    private let choices: OwnerChoicesStore
    private let encodeViewState: (WorkspaceNavigation) throws -> Data
    private var lastChoices = OwnerChoices()
    public private(set) var choicesRevision = 0
    public var savedChoices: OwnerChoices { lastChoices }

    public convenience init(defaults: UserDefaults = .standard, choicesFolder: URL? = SwarmHome.dataFolder) {
        self.init(defaults: defaults, choices: OwnerChoicesStore(folder: choicesFolder))
    }

    public convenience init(defaults: UserDefaults, choices: OwnerChoicesStore) {
        self.init(defaults: defaults, choices: choices, encodeViewState: {
            let encoder = JSONEncoder()
            encoder.outputFormatting = .sortedKeys
            return try encoder.encode($0)
        })
    }

    init(defaults: UserDefaults, choices: OwnerChoicesStore,
         encodeViewState: @escaping (WorkspaceNavigation) throws -> Data) {
        self.defaults = defaults
        self.choices = choices
        self.encodeViewState = encodeViewState
    }

    public func load() -> WorkspaceNavigation {
        let value = defaults.data(forKey: key)
            .flatMap { try? JSONDecoder().decode(WorkspaceNavigation.self, from: $0) } ?? WorkspaceNavigation()
        do {
            return adopt(try choices.load(waitForLock: true), into: value)
        } catch {
            choices.alerts.report(OwnerChoicesFailure(error.localizedDescription, operation: .load))
            return value
        }
    }

    @discardableResult
    public func save(_ value: WorkspaceNavigation) -> OwnerChoicesFailure? {
        let data: Data
        do { data = try encodeViewState(value) }
        catch { return OwnerChoicesFailure(error.localizedDescription, operation: .saveViewState) }
        if data != defaults.data(forKey: key) { defaults.set(data, forKey: key) }
        choices.resolveAlerts(.saveViewState)
        guard value.ownerChoices != lastChoices else { return nil }
        choicesRevision += 1
        do {
            try choices.update { $0.applyWorkspaceChanges(from: lastChoices, to: value.ownerChoices) }
            lastChoices = value.ownerChoices
        } catch { return OwnerChoicesFailure(error.localizedDescription, operation: .save) }
        return nil
    }

    public func adopt(_ saved: OwnerChoices, into value: WorkspaceNavigation) -> WorkspaceNavigation {
        // Adopt the snapshot as the baseline for the next owner save.
        lastChoices = saved
        var refreshed = value
        refreshed.ownerChoices = saved
        return refreshed
    }

}

public enum SidebarDrop {
    public static func folder(in urls: [URL]) -> URL? {
        urls.first { $0.isFileURL && OwnerChoices.folderExists($0.path) }
    }
}
