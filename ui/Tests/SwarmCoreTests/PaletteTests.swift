import Foundation
import Testing
@testable import SwarmCore

@Suite("Command palette")
struct PaletteTests {
    private let items = PaletteItems.build(
        sidebarViews: ["Workspaces", "Files", "Changes", "PR", "Usage", "Runs"],
        workspaces: [
            PaletteSource.Workspace(id: "/work/atlas", title: "atlas-api", detail: "main", status: .waiting, lastActivity: 300),
            PaletteSource.Workspace(id: "/work/docs", title: "docs-site", detail: "", status: nil, lastActivity: nil),
        ],
        chats: [
            PaletteSource.Chat(id: "chat-billing", title: "Fix billing export", workspace: "atlas-api", status: .working, lastActivity: 500),
        ],
        agents: [PaletteSource.Agent(id: "reviewer", name: "reviewer", role: "review", status: .failed)]
    )

    @Test("The selected chat offers all seven actions and Home keeps Reopen")
    func chatActions() {
        let selected = PaletteItems.build(sidebarViews: [], workspaces: [], chats: [], agents: [],
                                          selectedChat: true, switchModelDisabledReason: "Wait for the reply.")
        let actions = selected.filter { $0.id.hasPrefix("chatAction:") }
        #expect(actions.map(\.title) == ["Close Tab", "End chat", "Archive chat", "Rename chat",
                                         "Reopen closed chat", "Open in New Window", "Switch model"])
        #expect(actions.map(\.id) == PaletteChatAction.allCases.map { "chatAction:" + $0.rawValue })
        #expect(actions.last?.disabledReason == "Wait for the reply.")
        #expect(actions.dropLast().allSatisfy { $0.disabledReason == nil })
        #expect(actions.first { $0.id == "chatAction:reopenChat" }?.shortcut == "⇧⌘T")
        #expect(actions.first { $0.id == "chatAction:openInNewWindow" }?.shortcut == nil)
        #expect(items.filter { $0.id.hasPrefix("chatAction:") }.map(\.id) == ["chatAction:reopenChat"])
    }

    @Test("Action recency survives new defaults and uses its own key")
    func savedRecentActions() throws {
        let suite = "PaletteTests." + UUID().uuidString
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let expected = ["action:zoom": 900, "chatAction:renameChat": 1000]
        #expect(PaletteRecentActions.load(defaults: defaults).isEmpty)
        defaults.set(Data("navigation".utf8), forKey: "workspaces.navigation")
        PaletteRecentActions.save(expected, defaults: defaults)
        let relaunched = try #require(UserDefaults(suiteName: suite))
        #expect(PaletteRecentActions.load(defaults: relaunched) == expected)
        #expect(defaults.data(forKey: "workspaces.navigation") == Data("navigation".utf8))
        let saved = try #require(defaults.data(forKey: PaletteRecentActions.key))
        #expect(try JSONDecoder().decode([String: Int].self, from: saved) == expected)
        let listed = PaletteItems.build(sidebarViews: [], workspaces: [], chats: [], agents: [],
                                        recentActions: PaletteRecentActions.load(defaults: relaunched), selectedChat: true)
        #expect(PaletteSearch.rank(items: listed, query: "").prefix(2).map(\.id)
            == ["chatAction:renameChat", "action:zoom"])
        defaults.set(Data("broken".utf8), forKey: PaletteRecentActions.key)
        #expect(PaletteRecentActions.load(defaults: defaults).isEmpty)
    }

    @Test("Agents from every chat keep distinct ids and route through the sidebar selection")
    func everyChatsAgents() throws {
        func session(_ id: String) -> SwarmSession {
            SwarmSession(id: .init(id), talkMode: "lane", adapter: "herdr", cwd: "/repo", createdAt: 1,
                         chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
        }
        let firstChat = SwarmProjectSession(sessions: [session("first-chat")], title: "First chat")
        let secondChat = SwarmProjectSession(sessions: [session("second-chat")], title: "Second chat")
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo",
                                  workspaces: [WorkspaceNode(path: "/repo", name: "repo", sessions: [firstChat, secondChat])])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        let reviewer = SwarmAgent(id: .init("reviewer"), role: "review", pane: "pane", alive: true, state: "waiting")
        let ended = SwarmAgent(id: .init("finished"), role: "code", pane: "pane", alive: false)
        let closed = SwarmAgent(id: .init("closed"), role: "code", pane: nil, alive: nil)
        let chair = SwarmAgent(id: SwarmPanePolicy.chair, role: "chair", pane: "pane", alive: true)
        let bySession = [firstChat.id: [reviewer, ended, chair], secondChat.id: [reviewer, closed]]
        let agents = PaletteSource.agents(entries, agentsBySession: bySession, navigation: .init())
        #expect(Set(agents.map(\.id)) == ["child:first-chat/reviewer", "child:second-chat/reviewer"])
        #expect(agents.allSatisfy { $0.status == .waiting })
        let listed = PaletteItems.build(sidebarViews: [], workspaces: [], chats: [], agents: agents)
        let destination = try #require(PaletteSearch.rank(items: listed, query: "reviewer Second").first)
        #expect(destination.id == "agent:child:second-chat/reviewer")
        let selection = try #require(SidebarRows.selection(for: String(destination.id.dropFirst("agent:".count)),
                                                          in: entries, agentsBySession: bySession))
        #expect(selection.chatID == secondChat.id)
        #expect(selection.agentSessionID == secondChat.id)
        #expect(selection.agentID == reviewer.id)
        #expect(selection.workspaceID == "/repo")
    }

    @Test("Every query word must appear in the title or subtitle, in any case")
    func words() {
        #expect(PaletteSearch.rank(items: items, query: "BILL export").map(\.id) == ["chat:chat-billing"])
        #expect(PaletteSearch.rank(items: items, query: "atlas").map(\.id) == ["workspace:/work/atlas", "chat:chat-billing"])
        #expect(PaletteSearch.rank(items: items, query: "atlas main").map(\.id) == ["workspace:/work/atlas"])
        #expect(PaletteSearch.rank(items: items, query: "review").map(\.id) == ["agent:reviewer"])
        #expect(PaletteSearch.rank(items: items, query: "nothing like this").isEmpty)
    }

    @Test("An empty query lists recent items, newest first, then the actions")
    func emptyQuery() {
        let ranked = PaletteSearch.rank(items: items, query: "  ")
        #expect(ranked.prefix(2).map(\.id) == ["chat:chat-billing", "workspace:/work/atlas"])
        #expect(ranked.dropFirst(2).allSatisfy { $0.group == .action })
        #expect(ranked.count == 2 + PaletteItems.actions.count + 1)
        #expect(!ranked.contains { $0.id == "workspace:/work/docs" })
    }

    @Test("Actions carry their menu key; sidebar views take the app's names")
    func actions() {
        let newWorkspace = items.first { $0.id == "action:newWorkspace" }
        #expect(newWorkspace?.title == "New Workspace")
        #expect(newWorkspace?.shortcut == "⌘N")
        #expect(items.first { $0.id == "action:newChat" }?.shortcut == "⌘T")
        let newProject = items.first { $0.id == "action:newProject" }
        #expect(newProject?.title == "New Project…")
        #expect(newProject?.shortcut == "⇧⌘N")
        #expect(items.first { $0.id == "action:sidebarView(2)" }?.title == "Show Files")
        #expect(items.first { $0.id == "action:sidebarView(2)" }?.shortcut == "⌥⌘2")
        #expect(items.first { $0.id == "action:sidebarView(6)" }?.title == "Show Runs")
        #expect(KeyChord(.right, [.option, .command]).displayText == "⌥⌘→")
        #expect(KeyChord(.down, [.control, .command]).displayText == "⌃⌘↓")
        #expect(!items.contains { $0.id == "action:search" })
        #expect(items.first { $0.group == .agent }?.status == .failed)
    }

    @Test("A recently run action is listed with the recents")
    func recentAction() {
        let withRecent = PaletteItems.build(
            sidebarViews: [], workspaces: [], chats: [], agents: [], recentActions: ["action:zoom": 900]
        )
        #expect(PaletteSearch.rank(items: withRecent, query: "").first?.id == "action:zoom")
    }
}

@Suite("Open timing summary")
struct OpenTimingTests {
    @Test("p50 and p95 use the nearest rank")
    func summary() {
        let samples = (1...20).map(Double.init)
        let result = SwarmOpenScript.summary(samples)
        #expect(result?.p50 == 10)
        #expect(result?.p95 == 19)
        #expect(result?.max == 20)
        #expect(SwarmOpenScript.summary([]) == nil)
    }
}
