import Foundation
import Testing
@testable import SwarmCore

@Suite("Row field choices")
struct RowFieldsTests {
    @Test("Agent launch and usage keys decode, including a zero cost")
    func agentKeys() throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let json = #"{"id":"chair","role":"chat","profile":"chat","runner":"chat#1","model":"gpt-6","effort":"high","account":"work","costUsd":0,"tokens":12345}"#
        let agent = try decoder.decode(SwarmAgent.self, from: Data(json.utf8))
        #expect(agent.profile == "chat")
        #expect(agent.runner == "chat#1")
        #expect(agent.model == "gpt-6")
        #expect(agent.effort == "high")
        #expect(agent.account == "work")
        #expect(agent.costUsd == 0)
        #expect(agent.tokens == 12345)
        let old = try decoder.decode(SwarmAgent.self, from: Data(#"{"id":"chair","role":"chat"}"#.utf8))
        #expect(old.profile == nil && old.runner == nil && old.model == nil)
        #expect(old.effort == nil && old.account == nil && old.costUsd == nil && old.tokens == nil)
        let null = try decoder.decode(SwarmAgent.self, from: Data(#"{"id":"chair","role":"chat","tokens":null,"costUsd":null}"#.utf8))
        #expect(null.tokens == nil && null.costUsd == nil)
    }

    @Test("Missing lists use the bundled defaults and an empty list stays empty")
    func defaults() throws {
        let choices = try JSONDecoder().decode(OwnerChoices.self, from: Data("{}".utf8))
        #expect(choices.fields == RowFieldLists())
        #expect(choices.fields.project == [.title, .status])
        #expect(choices.fields.workspace == [.status, .title, .branch, .children, .age, .steps])
        #expect(choices.fields.chat == [.status, .title, .age, .steps, .children, .unread, .tokens])
        #expect(choices.fields.tab == [.status, .title, .provider, .unread, .tokens])
        let empty = try JSONDecoder().decode(OwnerChoices.self, from: Data(#"{"fields":{"chat":[]}}"#.utf8))
        #expect(empty.fields.chat.isEmpty)
        #expect(empty.fields.workspace == choices.fields.workspace)
    }

    @Test("Unknown names are skipped without losing known fields or their order")
    func unknownNames() throws {
        let json = #"{"fields":{"chat":["cost","future","model","title","tokens"],"tab":["unknown"]}}"#
        let choices = try JSONDecoder().decode(OwnerChoices.self, from: Data(json.utf8))
        #expect(choices.fields.chat == [.cost, .model, .title, .tokens])
        #expect(choices.fields.tab.isEmpty)
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: JSONEncoder().encode(choices)) == choices)
    }
}

@Suite("Row field presentation")
struct RowFieldPresentationTests {
    private func chat(time: Int = 90) -> SwarmProjectSession {
        SwarmProjectSession(sessions: [SwarmSession(
            id: .init("chat"), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo",
            createdAt: time, chairProvider: "codex", chairLog: nil, agents: 1, messages: 0, lastMessageAt: time
        )], title: "Fix rows", status: .done)
    }

    @Test("All surfaces use their selected fields in order and omit absent values")
    func orderedFields() throws {
        let chat = chat()
        var chair = SwarmAgent(id: .init("orchestrator"), role: "chat", pane: "%0", alive: true, provider: "codex")
        chair.model = "gpt-6"
        chair.effort = "high"
        chair.tokens = 1200
        chair.costUsd = 0
        chair.prompt = SwarmPrompt(id: "q", question: "Which route?", choices: [])
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo", workspaces: [
            WorkspaceNode(path: "/repo", name: "repo", sessions: [chat], branch: "feature")
        ])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        let agents = [chat.id: [chair]]
        var navigation = WorkspaceNavigation()
        navigation.names["/repo"] = "repo"
        navigation.fields.project = [.tokens, .title, .model, .cost]
        navigation.fields.workspace = [.model, .dirty, .title, .branch, .tokens, .cost]
        navigation.fields.chat = [.cost, .tokens, .effort, .question, .provider, .title]
        navigation.fields.tab = navigation.fields.chat
        let sections = SidebarRows.sections(
            projects: [project], workspaces: entries, navigation: navigation, search: "", showingArchive: false,
            now: 100, agentsBySession: agents, workspaceFields: ["/repo": RowWorkspaceFields(dirtyCount: 2)]
        )
        #expect(sections[0].fields.map(\.text) == ["1200 tokens", "repo", "gpt-6", "$0.00"])
        #expect(sections[0].rows[0].fields.map(\.text) == ["gpt-6", "2 dirty", "repo", "feature", "1200 tokens", "$0.00"])
        #expect(sections[0].rows[1].fields.map(\.text) == ["$0.00", "1200 tokens", "high", "Which route?", "codex", "Fix rows"])
        let tabs = ChatTab.tabs([ChatRow(session: chat, workspace: "repo", workspacePath: "/repo")], closing: [], now: 100,
                                navigation: navigation, agentsBySession: agents)
        #expect(tabs[0].fields.map(\.field) == navigation.fields.tab)
        #expect(tabs[0].fields.map(\.text) == sections[0].rows[1].fields.map(\.text))
        navigation.names = [:]
        navigation.fields.workspace = [.branch]
        let branchOnly = SidebarRows.sections(projects: [project], workspaces: entries, navigation: navigation,
                                              search: "", showingArchive: false, now: 100)
        #expect(branchOnly[0].rows[0].fields.map(\.text) == ["feature"])
        let noUsage = ChatTab.tabs([ChatRow(session: chat, workspace: "repo", workspacePath: "/repo")], closing: [], now: 100,
                                   navigation: navigation)
        #expect(noUsage[0].fields.map(\.field) == [.provider, .title])
    }

    @Test("Unread uses the stable chat key and survives defaults without entering choices")
    @MainActor
    func unread() throws {
        let original = chat()
        var navigation = WorkspaceNavigation()
        #expect(!navigation.isUnread(original))
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo", workspaces: [
            WorkspaceNode(path: "/repo", name: "repo", sessions: [original])
        ])
        let entry = WorkspaceEntry.list(in: SessionsTree(projects: [project]))[0]
        navigation.select(entry, now: 90)
        #expect(!navigation.isUnread(original))
        #expect(navigation.isUnread(chat(time: 91)))
        let row = SidebarRows.sections(projects: [project], workspaces: [entry], navigation: navigation,
                                       search: "", showingArchive: false, now: 100)[0].rows[1]
        #expect(!row.fields.contains { $0.field == .unread })
        let tabs = ChatTab.tabs([ChatRow(session: chat(time: 91), workspace: "repo", workspacePath: "/repo")],
                                closing: [], now: 100, navigation: navigation)
        #expect(tabs[0].fields.contains { $0.field == .unread })
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "RowFieldsTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite); try? FileManager.default.removeItem(at: root) }
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(root))
        navigation.fields.tab = [.title, .unread]
        store.save(navigation)
        #expect(store.load().lastSeen == ["chat": 90])
        #expect(store.load().fields == navigation.fields)
        let choices = try Data(contentsOf: root.appendingPathComponent("choices.json"))
        let keys = try #require(JSONSerialization.jsonObject(with: choices) as? [String: Any])
        #expect(keys["lastSeen"] == nil)
        let viewState = try #require(defaults.data(forKey: "workspaces.navigation"))
        #expect((try JSONSerialization.jsonObject(with: viewState) as? [String: Any])?["fields"] == nil)
    }

    @Test("A refresh at the activity time keeps a later selection time")
    func refreshSeenTime() {
        var navigation = WorkspaceNavigation()
        navigation.markSeen(chat(time: 80), now: 90)
        navigation.markSeen(chat(time: 80), now: 80)
        #expect(navigation.lastSeen == ["chat": 90])
    }

    @Test("First sight records every chat once and later activity becomes unread")
    func firstSightBaseline() {
        var navigation = WorkspaceNavigation()
        navigation.recordFirstSight([chat(time: 80)])
        #expect(navigation.lastSeen == ["chat": 80])
        #expect(!navigation.isUnread(chat(time: 80)))
        navigation.recordFirstSight([chat(time: 81)])
        #expect(navigation.lastSeen == ["chat": 80])
        #expect(navigation.isUnread(chat(time: 81)))
    }

    @Test("A continued chat uses the original last-seen key")
    func continuedUnread() {
        let root = chat().session
        let next = SwarmSession(id: .init("next"), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo", createdAt: 95,
                                chairLog: nil, agents: 1, messages: 0, lastMessageAt: 95, continuationOf: root.id)
        let continued = SwarmProjectSession(sessions: [next, root], title: "Continued")
        var navigation = WorkspaceNavigation()
        navigation.lastSeen = ["chat": 94]
        #expect(navigation.isUnread(continued))
        navigation.markSeen(continued, now: 95)
        #expect(!navigation.isUnread(continued))
        #expect(navigation.lastSeen == ["chat": 95])
    }

    @Test("A requested field controls reads for empty, populated, and missing workspaces")
    func requestedPaths() {
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo", workspaces: [
            WorkspaceNode(path: "/repo", name: "repo", sessions: [chat()]),
            WorkspaceNode(path: "/repo/empty", name: "empty", sessions: []),
            WorkspaceNode(path: "/repo/missing", name: "missing", sessions: [], missing: true)
        ])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        var fields = RowFieldLists()
        #expect(RowFields.requestedPaths(for: .dirty, entries: entries, fields: fields).isEmpty)
        fields.tab = [.dirty]
        #expect(RowFields.requestedPaths(for: .dirty, entries: entries, fields: fields) == ["/repo"])
        fields.project = [.dirty]
        #expect(Set(RowFields.requestedPaths(for: .dirty, entries: entries, fields: fields)) == ["/repo", "/repo/empty"])
    }

    @Test("Dirty count counts each path once and refreshes only requested workspace paths")
    func dirtyRefresh() async {
        let recorder = DirtyRecorder()
        let cache = RowFieldCache(inspect: { path in await recorder.inspect(path) })
        let values = await cache.refresh(paths: ["/repo", "/repo"], now: Date(timeIntervalSince1970: 100))
        #expect(values["/repo"]?.dirtyCount == 1)
        _ = await cache.refresh(paths: ["/repo"], now: Date(timeIntervalSince1970: 109))
        #expect(await recorder.paths == ["/repo"])
        _ = await cache.refresh(paths: ["/other"], now: Date(timeIntervalSince1970: 109))
        _ = await cache.refresh(paths: ["/repo"], now: Date(timeIntervalSince1970: 110))
        #expect(await recorder.paths == ["/repo", "/other", "/repo"])
    }
}

private actor DirtyRecorder {
    var paths: [String] = []
    func inspect(_ path: String) -> GitWorkspaceSnapshot {
        paths.append(path)
        return GitWorkspaceSnapshot(root: path, branch: "feature", head: "abc", files: [
            GitChange(path: "same.txt", status: "M", layer: .staged),
            GitChange(path: "same.txt", status: "M", layer: .unstaged)
        ], refs: [], readAt: Date(timeIntervalSince1970: 100))
    }
}
