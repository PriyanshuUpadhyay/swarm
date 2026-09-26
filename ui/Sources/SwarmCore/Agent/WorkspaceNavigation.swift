import Foundation

public struct WorkspaceEntry: Identifiable, Sendable {
    public var id: String { workspace.path }
    public let project: ProjectNode
    public let workspace: WorkspaceNode
    public var chats: [SwarmProjectSession] { workspace.sessions.filter { $0.totalAgents != 0 } }
    public var lastActivity: Int { chats.map(\.lastActivity).max() ?? 0 }
    public var isRunning: Bool { chats.contains { $0.isRunning == true } }
    public var folderName: String { URL(fileURLWithPath: id).lastPathComponent }

    public static func list(in tree: SessionsTree) -> [Self] {
        tree.projects.flatMap { project in
            project.workspaces.map { Self(project: project, workspace: $0) }
        }.sorted {
            if $0.lastActivity != $1.lastActivity { return $0.lastActivity > $1.lastActivity }
            return $0.id < $1.id
        }
    }
}

public struct WorkspaceNavigation: Codable, Equatable, Sendable {
    public var selectedWorkspace: String?
    public var selectedChats: [String: String] = [:]
    public var pinned: Set<String> = []
    public var archived: Set<String> = []
    public var names: [String: String] = [:]

    public init() {}

    public func title(for entry: WorkspaceEntry) -> String {
        if let name = customName(for: entry) { return name }
        let project = entry.project.name
        let folder = entry.folderName
        if project == folder { return folder.isEmpty ? entry.id : folder }
        return "\(project) / \(folder)"
    }

    public func detail(for entry: WorkspaceEntry, among entries: [WorkspaceEntry]) -> String {
        var parts: [String] = []
        let duplicates = entries.filter {
            $0.id != entry.id && title(for: $0).localizedCaseInsensitiveCompare(title(for: entry)) == .orderedSame
        }
        if !duplicates.isEmpty {
            parts.append(pathQualifier(for: entry.id, others: duplicates.map(\.id)))
        }
        if customName(for: entry) != nil, !parts.contains(entry.project.name) {
            parts.append(entry.project.name)
        }
        if let branch = entry.workspace.branch,
           !parts.contains(branch),
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
        query.isEmpty || [title(for: entry), entry.project.name, entry.workspace.name, entry.id]
            .contains { $0.localizedCaseInsensitiveContains(query) }
            || entry.chats.contains { $0.title.localizedCaseInsensitiveContains(query) }
    }
}

@MainActor
public final class WorkspaceNavigationStore {
    private let defaults: UserDefaults
    private let key = "workspaces.navigation"

    public init(defaults: UserDefaults = .standard) { self.defaults = defaults }

    public func load() -> WorkspaceNavigation {
        guard let data = defaults.data(forKey: key),
              let value = try? JSONDecoder().decode(WorkspaceNavigation.self, from: data) else {
            return WorkspaceNavigation()
        }
        return value
    }

    public func save(_ value: WorkspaceNavigation) {
        guard let data = try? JSONEncoder().encode(value) else { return }
        defaults.set(data, forKey: key)
    }
}
