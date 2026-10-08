import Foundation

/// One chat tab, as plain values.
public struct ChatTab: Sendable, Hashable, Identifiable {
    public let id: String
    public let title: String
    public let status: AgentStatus?
    /// One letter for the chat's provider, such as C for Claude or X for Codex.
    public let badge: String?
    /// A live chat can be closed; an ended one or one already closing cannot.
    public let canClose: Bool
    public var canHide: Bool { pending == nil }
    /// Set for a chat the app is starting (ADR 0035). Such a tab offers no close or archive.
    public var pending: Pending? = nil
    public var fields: [RowFieldValue] = []
    public var group: TabGroup? = nil
    public var children: ChildCount? = nil

    public enum Pending: Sendable, Hashable { case starting, failed, closing }

    /// Starts lead the stored open set. A start hides its listed row so a chat never has two tabs.
    public static func tabs(
        _ chats: [ChatRow], strip: TabStrip, pending: [PendingChat] = [], closing: Set<SwarmSessionID>, now: Int,
        navigation: WorkspaceNavigation = .init(),
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:], workspaceFields: [String: RowWorkspaceFields] = [:],
        branches: [String: String] = [:], stepsByChat: [SwarmSessionID: String] = [:]
    ) -> [ChatTab] {
        let starting = Set(pending.compactMap(\.session))
        let byKey = Dictionary(chats.map { (ChatTitle.key($0.session), $0) }, uniquingKeysWith: { first, _ in first })
        return pending.reversed().map { chat in
            let pending: Pending = switch chat.state {
            case .starting, .launched: .starting
            case .failed: .failed
            case .closing: .closing
            }
            return ChatTab(
                id: chat.tabID, title: "New chat", status: nil, badge: nil, canClose: false,
                pending: pending
            )
        } + strip.open.compactMap { byKey[$0] }.filter { !starting.contains($0.id) }.map { chat in
            let title = navigation.title(for: chat.session)
            return ChatTab(
                id: ChatTitle.key(chat.session),
                title: title,
                status: chat.session.status,
                badge: chat.session.provider.map(badge),
                canClose: SessionRowPresentation.make(chat, now: now).state == .live
                    && !closing.contains(chat.id),
                fields: RowFields.chatContext(
                    chat.session, title: title,
                    navigation: navigation, now: now, agentsBySession: agentsBySession,
                    branch: branches[chat.workspacePath], workspace: workspaceFields[chat.workspacePath] ?? .init(),
                    steps: stepsByChat[chat.id]
                ).values(navigation.fields.tab),
                group: strip.groups.first { $0.members.contains(ChatTitle.key(chat.session)) },
                children: ChildCount.make(session: chat.session.session, agents: agentsBySession[chat.id] ?? [])
            )
        }
    }

    public struct ChildCount: Sendable, Hashable {
        public let count: Int
        public let waiting: Int
        public let firstWaiting: SwarmAgentID?
        public var text: String { "\(count)" + (waiting == 0 ? "" : " · \(waiting) waiting") }

        public static func make(session: SwarmSession, agents: [SwarmAgent]) -> Self? {
            let children = SwarmPanePolicy.liveCells(session: session, agents: agents)
                .map(\.agent).sorted { $0.id < $1.id }
            guard !children.isEmpty else { return nil }
            let waiting = children.filter { $0.status == .waiting }
            return Self(count: children.count, waiting: waiting.count, firstWaiting: waiting.first?.id)
        }
    }

    public func waitingChildSelection(
        in workspaces: [WorkspaceEntry], agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) -> SidebarSelection? {
        guard let child = children?.firstWaiting else { return nil }
        for entry in workspaces {
            if let chat = entry.chats.first(where: { ChatTitle.key($0) == id }),
               let agent = agentsBySession[chat.id]?.first(where: { $0.id == child && $0.status == .waiting }) {
                return SidebarRows.selection(for: SidebarRows.childID(chat: chat, agent: agent),
                                             in: workspaces, agentsBySession: agentsBySession)
            }
        }
        return nil
    }

    public struct Run: Sendable, Hashable, Identifiable {
        public var group: TabGroup?
        public var tabs: [ChatTab]
        public var id: String { group.map { "group:\($0.id)" } ?? "tab:\(tabs[0].id)" }
    }

    public static func runs(_ tabs: [ChatTab]) -> [Run] {
        var runs: [Run] = []
        for tab in tabs {
            if let group = tab.group, runs.last?.group?.id == group.id {
                runs[runs.count - 1].tabs.append(tab)
            } else {
                runs.append(Run(group: tab.group, tabs: [tab]))
            }
        }
        return runs
    }

    public static func badge(_ provider: String) -> String {
        switch provider.lowercased() {
        case "codex": "X"
        case "agy": "A"
        default: provider.prefix(1).uppercased()
        }
    }

    /// Only tabs crossing the right edge are in the menu; tabs left of the viewport stay out.
    public static func overflow(_ tabs: [ChatTab], trailingEdges: [String: Double], viewportWidth: Double) -> [ChatTab] {
        guard viewportWidth > 0 else { return [] }
        return tabs.filter { (trailingEdges[$0.id] ?? 0) > viewportWidth }
    }
}
