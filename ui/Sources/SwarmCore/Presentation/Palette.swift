import Foundation

/// One command palette result, as plain values.
public struct PaletteItem: Sendable, Hashable, Identifiable {
    /// Results show in this order: Actions, Workspaces, Chats, Agents.
    public enum Group: Int, Sendable, Hashable, CaseIterable, Comparable {
        case action, workspace, chat, agent

        public var title: String {
            switch self {
            case .action: "Actions"
            case .workspace: "Workspaces"
            case .chat: "Chats"
            case .agent: "Agents"
            }
        }

        public static func < (lhs: Group, rhs: Group) -> Bool { lhs.rawValue < rhs.rawValue }
    }

    public let id: String
    public let title: String
    public let subtitle: String?
    public let group: Group
    /// The action's key, such as "⇧⌘N".
    public let shortcut: String?
    public let status: AgentStatus?
    /// Last use or activity in seconds since 1970; nil for never.
    public let recency: Int?
    public let disabledReason: String?

    public init(
        id: String, title: String, subtitle: String? = nil, group: Group,
        shortcut: String? = nil, status: AgentStatus? = nil, recency: Int? = nil, disabledReason: String? = nil
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.group = group
        self.shortcut = shortcut
        self.status = status
        self.recency = recency
        self.disabledReason = disabledReason
    }
}

public enum PaletteSearch {
    /// Every query word must be a case-insensitive substring of the title or subtitle. An empty
    /// query lists recent items, newest first, then the actions. Ties keep the input order.
    public static func rank(items: [PaletteItem], query: String) -> [PaletteItem] {
        let words = query.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !words.isEmpty else {
            let recent = items.filter { $0.recency != nil }
                .enumerated()
                .sorted { ($0.element.recency!, -$0.offset) > ($1.element.recency!, -$1.offset) }
                .map(\.element)
            return recent + items.filter { $0.group == .action && $0.recency == nil }
        }
        return items.filter { item in
            let text = item.title + " " + (item.subtitle ?? "")
            return words.allSatisfy { text.localizedCaseInsensitiveContains($0) }
        }
    }
}

/// Plain inputs for palette items; the app fills them from its own state.
public enum PaletteSource {
    public struct Workspace: Sendable, Hashable {
        public let id: String
        public let title: String
        public let detail: String
        public let status: AgentStatus?
        public let lastActivity: Int?

        public init(id: String, title: String, detail: String, status: AgentStatus?, lastActivity: Int?) {
            self.id = id
            self.title = title
            self.detail = detail
            self.status = status
            self.lastActivity = lastActivity
        }
    }

    public struct Chat: Sendable, Hashable {
        public let id: String
        public let title: String
        public let workspace: String
        public let status: AgentStatus?
        public let lastActivity: Int?

        public init(id: String, title: String, workspace: String, status: AgentStatus?, lastActivity: Int?) {
            self.id = id
            self.title = title
            self.workspace = workspace
            self.status = status
            self.lastActivity = lastActivity
        }
    }

    public struct Agent: Sendable, Hashable {
        public let id: String
        public let name: String
        public let role: String
        public let status: AgentStatus

        public init(id: String, name: String, role: String, status: AgentStatus) {
            self.id = id
            self.name = name
            self.role = role
            self.status = status
        }
    }
}

extension PaletteSource {
    public static func agents(
        _ entries: [WorkspaceEntry], agentsBySession: [SwarmSessionID: [SwarmAgent]],
        navigation: WorkspaceNavigation
    ) -> [Agent] {
        entries.flatMap { entry in
            entry.chats.flatMap { chat in
                SwarmPanePolicy.liveCells(session: chat.session, agents: agentsBySession[chat.id] ?? []).map {
                    Agent(id: SidebarRows.childID(chat: chat, agent: $0.agent),
                          name: $0.agent.id.rawValue,
                          role: $0.agent.role + " · " + navigation.title(for: chat), status: $0.agent.status)
                }
            }
        }
    }

    /// Workspaces and their chats, pinned first, then by activity, archived ones last with "Archived" in their
    /// detail, so the palette finds them from any sidebar view.
    public static func workspaces(
        _ entries: [WorkspaceEntry], navigation: WorkspaceNavigation, now: Int
    ) -> (workspaces: [Workspace], chats: [Chat]) {
        // Pinned, the rest by activity, then archived, with full "project / folder" titles: the
        // palette has no project headers to name the project.
        let idsByTitle = navigation.idsByTitle(entries)
        let byActivity = entries.sorted {
            $0.lastActivity == $1.lastActivity ? $0.id < $1.id : $0.lastActivity > $1.lastActivity
        }
        let pinned = byActivity.filter { navigation.pinned.contains($0.id) && !navigation.archived.contains($0.id) }
        let others = byActivity.filter { !navigation.pinned.contains($0.id) && !navigation.archived.contains($0.id) }
        let archived = byActivity.filter { navigation.archived.contains($0.id) }
        var workspaces: [Workspace] = []
        var chats: [Chat] = []
        for entry in pinned + others + archived {
            let row = SidebarRows.row(entry, idsByTitle: idsByTitle, navigation: navigation, now: now)
            let detail = row.archived ? [row.detail, "Archived"].filter { !$0.isEmpty }.joined(separator: " · ") : row.detail
            workspaces.append(Workspace(
                id: row.id, title: row.title, detail: detail, status: row.status,
                lastActivity: entry.lastActivity > 0 ? entry.lastActivity : nil
            ))
            chats += entry.chats.map {
                Chat(id: $0.id.rawValue, title: navigation.title(for: $0),
                     workspace: row.title, status: $0.status, lastActivity: $0.lastActivity)
            }
        }
        return (workspaces, chats)
    }
}

public enum PaletteChatAction: String, Sendable, CaseIterable {
    case closeTab, endChat, archiveChat, renameChat, reopenChat, switchModel

    public var title: String {
        switch self {
        case .closeTab: "Close tab"
        case .endChat: "End chat"
        case .archiveChat: "Archive chat"
        case .renameChat: "Rename chat"
        case .reopenChat: "Reopen closed chat"
        case .switchModel: "Switch model"
        }
    }

    public static func offered(selectedChat: Bool) -> [Self] {
        selectedChat ? allCases : [.reopenChat]
    }
}

/// Stored separately from workspace navigation because action recency belongs to the palette.
public enum PaletteRecentActions {
    public static let key = "palette.recentActions"

    public static func load(defaults: UserDefaults = .standard) -> [String: Int] {
        guard let data = defaults.data(forKey: key) else { return [:] }
        return (try? JSONDecoder().decode([String: Int].self, from: data)) ?? [:]
    }

    public static func save(_ values: [String: Int], defaults: UserDefaults = .standard) {
        if let data = try? JSONEncoder().encode(values) { defaults.set(data, forKey: key) }
    }
}

public enum PaletteItems {
    /// App actions that belong in the palette. ⌘K itself, per-tab picks, focus moves, and find
    /// stepping stay keys only.
    public static let actions: [AppKey] = [
        .newWorkspace, .newChat, .newProject, .nextWorkspace, .previousWorkspace, .nextTab, .previousTab,
        .zoom, .focusComposer, .toggleSidebar, .moveSidebar,
    ] + (1...6).map(AppKey.sidebarView) + [.showChanges, .find, .stop]

    /// Item ids carry their group, so a workspace and a chat with the same id stay apart.
    public static func build(
        actions: [AppKey] = actions, sidebarViews: [String],
        workspaces: [PaletteSource.Workspace], chats: [PaletteSource.Chat],
        agents: [PaletteSource.Agent], recentActions: [String: Int] = [:],
        selectedChat: Bool = false, switchModelDisabledReason: String? = nil
    ) -> [PaletteItem] {
        actions.map { key in
            PaletteItem(
                id: "action:\(key)", title: key.title(sidebarViews: sidebarViews), group: .action,
                shortcut: key.chord.displayText, recency: recentActions["action:\(key)"]
            )
        }
        + PaletteChatAction.offered(selectedChat: selectedChat).map { action in
            PaletteItem(
                id: "chatAction:" + action.rawValue, title: action.title, group: .action,
                shortcut: action == .reopenChat ? AppKey.recentlyClosed.chord.displayText : nil,
                recency: recentActions["chatAction:" + action.rawValue],
                disabledReason: action == .switchModel ? switchModelDisabledReason : nil
            )
        }
        + workspaces.map {
            PaletteItem(
                id: "workspace:\($0.id)", title: $0.title, subtitle: $0.detail.isEmpty ? nil : $0.detail,
                group: .workspace, status: $0.status, recency: $0.lastActivity
            )
        }
        + chats.map {
            PaletteItem(
                id: "chat:\($0.id)", title: $0.title, subtitle: $0.workspace, group: .chat,
                status: $0.status, recency: $0.lastActivity
            )
        }
        + agents.map {
            PaletteItem(id: "agent:\($0.id)", title: $0.name, subtitle: $0.role, group: .agent, status: $0.status)
        }
    }
}

extension AppKey {
    /// The action's name in the palette. Sidebar views take their names from the app.
    public func title(sidebarViews: [String]) -> String {
        switch self {
        case .newChat: "New Chat"
        case .recentlyClosed: "Recently closed…"
        case .newWorkspace: "New Workspace"
        case .newProject: "New Project…"
        case .nextWorkspace: "Next Workspace"
        case .previousWorkspace: "Previous Workspace"
        case .selectTab(let number): "Chat \(number)"
        case .closeTab: "Close tab"
        case .closeWindow: "Close Window"
        case .previousRecentChat: "Previous Recent Chat"
        case .nextRecentChat: "Next Recent Chat"
        case .lastTab: "Last Chat"
        case .nextTab: "Next Chat"
        case .previousTab: "Previous Chat"
        case .moveFocus(let direction): "Focus \(String(describing: direction).capitalized)"
        case .zoom: "Zoom Pane"
        case .focusComposer: "Focus Composer"
        case .toggleSidebar: "Toggle Sidebar"
        case .moveSidebar: "Move Sidebar to Other Side"
        case .sidebarView(let number):
            sidebarViews.indices.contains(number - 1) ? "Show \(sidebarViews[number - 1])" : "Sidebar View \(number)"
        case .showChanges: "Show Changes"
        case .search: "Search"
        case .find: "Find"
        case .findNext: "Find Next"
        case .findPrevious: "Find Previous"
        case .stop: "Stop the Chair"
        }
    }
}

extension KeyChord {
    /// The key as the menu shows it, such as "⌥⌘→" or "⇧⌘N".
    public var displayText: String {
        var text = ""
        if modifiers.contains(.control) { text += "⌃" }
        if modifiers.contains(.option) { text += "⌥" }
        if modifiers.contains(.shift) { text += "⇧" }
        if modifiers.contains(.command) { text += "⌘" }
        switch key {
        case .character(let character): text += String(character).uppercased()
        case .returnKey: text += "↩"
        case .tab: text += "Tab"
        case .escape: text += "⎋"
        case .left: text += "←"
        case .right: text += "→"
        case .up: text += "↑"
        case .down: text += "↓"
        }
        return text
    }
}
