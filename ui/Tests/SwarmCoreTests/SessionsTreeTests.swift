import Foundation
import Testing
@testable import SwarmCore

@Suite("Sessions tree")
struct SessionsTreeTests {
    private let common = "/repo/.bare"
    private let worktrees = [
        WorktreeEntry(path: "/repo/wt/main", branch: "main"),
        WorktreeEntry(path: "/repo/wt/feature", branch: "feature"),
    ]

    @Test("One repository shows two worktrees and its hub as workspaces")
    func repositoryAndWorktrees() {
        let tree = build([
            session("11111111-a", cwd: "/repo/wt/main/src"),
            session("22222222-b", cwd: "/repo/wt/feature"),
            session("33333333-c", cwd: "/repo/.bare"),
        ])
        #expect(tree.projects.count == 1)
        #expect(tree.projects[0].name == "repo")
        #expect(tree.projects[0].workspaces.map(\.name) == [".bare", "feature", "main"])
        #expect(tree.projects[0].workspaces.map(\.sessions.count) == [1, 1, 1])
        #expect(tree.launchDirectory(for: "/repo/wt/feature") == "/repo/wt/feature")
    }

    @Test("A folder is one workspace")
    func folder() {
        let tree = build([session("folder-1", cwd: "/outside")])
        #expect(tree.projects[0].path == "/outside")
        #expect(tree.projects[0].workspaces.map(\.name) == ["outside"])
        #expect(tree.projects[0].workspaces[0].sessions.count == 1)
        #expect(tree.launchDirectory(for: "/outside") == "/outside")
    }

    @Test("Resolved titles name their rows and missing titles fall back")
    func resolvedTitles() {
        let named = session("named", cwd: "/outside")
        let fallback = session("fallback", cwd: "/outside")
        let tree = build([named, fallback], titles: [named.id: "Repair the sidebar"])
        let titles = Dictionary(uniqueKeysWithValues: tree.projects[0].workspaces[0].sessions.map {
            ($0.id, $0.title)
        })
        #expect(titles == [named.id: "Repair the sidebar", fallback.id: "Chat"])
    }

    @Test("Session rows have clear titles, captions, and states")
    func rowPresentation() {
        let live = projectSession("live-title", title: "Build sidebar", provider: "claude", running: true)
        #expect(SessionRowPresentation.make(live, now: 7_201)
            == SessionRowPresentation(title: "Build sidebar", caption: "claude · 2h", state: .live))

        let untitled = projectSession("abcdefgh-more", title: "", provider: "codex", running: true)
        #expect(SessionRowPresentation.make(untitled, now: 7_201)
            == SessionRowPresentation(title: "codex abcdefgh", caption: "codex · 2h", state: .live))

        let ended = projectSession("ended-session", title: "", provider: "codex", running: false)
        #expect(SessionRowPresentation.make(ended, now: 7_201)
            == SessionRowPresentation(title: "codex ended-se", caption: "ended · 2h", state: .ended))

        let noChair = projectSession("missing-chair", title: "", provider: nil, running: true)
        #expect(SessionRowPresentation.make(noChair, now: 7_201)
            == SessionRowPresentation(title: "Chat missing-", caption: "no chair · 2h", state: .noChair))

        let children = projectSession(
            "child-agents", title: "Team", provider: "claude", running: true, liveAgents: 3
        )
        #expect(SessionRowPresentation.make(children, now: 7_201)
            == SessionRowPresentation(title: "Team", caption: "claude · 2h · 2 agents", state: .live))
    }

    @Test("Agent state feeds the sidebar presentation and tree text")
    func agentState() {
        let dead = session("dead-session", cwd: "/outside")
        let agent = SwarmAgent(
            id: .init("orchestrator"), role: "chair", pane: "%1", alive: false
        )
        let tree = build([dead], agentsBySession: [dead.id: [agent]])
        #expect(SessionRowPresentation.make(
            tree.projects[0].workspaces[0].sessions[0], now: 61
        ).caption == "ended · 1m")
        #expect(tree.text(now: 61).contains("outside · ended · 1m"))

        let empty = build([dead], agentsBySession: [dead.id: []])
        #expect(SessionRowPresentation.make(
            empty.projects[0].workspaces[0].sessions[0], now: 61
        ).caption == "ended · 1m")

        let running = build([dead], agentsBySession: [dead.id: [agent, SwarmAgent(
            id: .init("worker"), role: "code", pane: "%2", alive: true
        )]])
        #expect(SessionRowPresentation.make(
            running.projects[0].workspaces[0].sessions[0], now: 61
        ).caption == "no chair · 1m · 1 agent")
    }

    @Test("A missing session provider comes from the chair, then the first agent")
    func providerFallback() {
        let item = session("provider-session", cwd: "/outside")
        let worker = SwarmAgent(
            id: .init("worker"), role: "code", pane: "%2", alive: true, provider: "claude"
        )
        let chair = SwarmAgent(
            id: .init("orchestrator"), role: "chair", pane: "%1", alive: true, provider: "codex"
        )
        let withChair = build([item], agentsBySession: [item.id: [worker, chair]])
        #expect(SessionRowPresentation.make(
            withChair.projects[0].workspaces[0].sessions[0], now: 61
        ).caption == "codex · 1m · 1 agent")
        #expect(withChair.windowTitle(for: "/outside") == "outside · codex provider")

        let withoutChair = build([item], agentsBySession: [item.id: [worker]])
        #expect(SessionRowPresentation.make(
            withoutChair.projects[0].workspaces[0].sessions[0], now: 61
        ).caption == "claude · 1m")
    }

    @Test("Live, no-chair, and ended rows sort by state then recent activity")
    func rowOrdering() {
        let rows = [
            projectSession("ended-new", provider: "codex", running: false, activity: 50),
            projectSession("no-chair", provider: nil, running: true, activity: 40),
            projectSession("live-old", provider: "codex", running: true, activity: 10),
            projectSession("ended-old", provider: "codex", running: false, activity: 20),
            projectSession("live-new", provider: "codex", running: true, activity: 30),
        ]
        #expect(SessionsTree.ordered(rows).map(\.id.rawValue)
            == ["live-new", "live-old", "no-chair", "ended-new", "ended-old"])
    }

    @Test("Workspaces sort by state and then recent activity")
    func workspaceOrdering() {
        let workspaces = [
            WorkspaceNode(path: "/ended", name: "ended", sessions: [
                projectSession("ended", provider: "codex", running: false, activity: 50),
            ]),
            WorkspaceNode(path: "/no-chair", name: "no-chair", sessions: [
                projectSession("no-chair", provider: nil, running: true, activity: 60),
            ]),
            WorkspaceNode(path: "/live-old", name: "live-old", sessions: [
                projectSession("live-old", provider: "codex", running: true, activity: 10),
            ]),
            WorkspaceNode(path: "/live-new", name: "live-new", sessions: [
                projectSession("live-new", provider: "codex", running: true, activity: 30),
            ]),
        ]
        #expect(SessionsTree.ordered(workspaces).map(\.name)
            == ["live-new", "live-old", "no-chair", "ended"])
    }

    @Test("Archived sessions and empty worktrees are hidden")
    func archived() {
        let tree = build([
            session("active", cwd: "/repo/wt/main"),
            session("archived", cwd: "/repo/wt/feature", archivedAt: 50),
        ])
        #expect(tree.projects[0].workspaces.map(\.path) == ["/repo/wt/main"])
        #expect(tree.session("/repo/wt/feature") == nil)
    }

    @Test("Repeated sessions in one chair make one row")
    func chairGroup() {
        let tree = build([
            session("older", cwd: "/repo/wt/main", chair: "chair"),
            session("newer", cwd: "/repo/wt/main", chair: "chair"),
        ])
        let workspace = tree.projects[0].workspaces[0]
        let rows = workspace.sessions
        #expect(rows.count == 1)
        #expect(rows[0].sessions.count == 2)
        #expect(tree.session(workspace.id)?.id == rows[0].id)
        #expect(tree.retainedSelection(workspace.id) == workspace.id)
        #expect(tree.retainedSelection("/gone") == nil)
        #expect(tree.windowTitle(for: workspace.id) == "repo · codex older")
    }

    @Test("A new session remains the chat row when an older session has later activity")
    func newestSessionOwnsChat() {
        var older = session("older", cwd: "/repo/wt/main", chair: "chair")
        older.lastMessageAt = 100
        var newer = session("newer", cwd: "/repo/wt/main", chair: "chair")
        newer.createdAt = 50
        let tree = build([older, newer])
        #expect(tree.session("/repo/wt/main")?.id == newer.id)
        #expect(tree.session("/repo/wt/main")?.session == newer)
    }

    @Test("The bare repository is one project for its linked worktrees")
    func bareLayout() {
        let tree = build([
            session("first", cwd: "/repo/wt/main/a"),
            session("second", cwd: "/repo/wt/feature/b"),
        ])
        #expect(tree.projects.map(\.id) == [.repository(commonDirectory: common)])
    }

    @Test("A hub session stays in its repository and launches from main")
    func hubSession() {
        let tree = build([
            session("hub", cwd: "/repo/.bare"),
            session("main", cwd: "/repo/wt/main"),
        ])
        #expect(tree.projects.count == 1)
        #expect(tree.projects[0].workspaces.map(\.name) == [".bare", "main"])
        #expect(tree.projects[0].launchDirectory == "/repo/wt/main")
        #expect(tree.launchDirectory(for: "/repo/.bare") == "/repo/.bare")
    }

    @Test("Tree text prints projects and workspace captions only")
    func treeText() {
        let tree = build([
            session("hub", cwd: "/repo/.bare"),
            session("main", cwd: "/repo/wt/main"),
        ])
        #expect(tree.text(now: 61) == """
            repo
              .bare · no chair · 1m
              main · no chair · 1m
            """)
    }

    @Test("A .bare directory identifies its repository")
    func bareDirectory() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent(".bare"), withIntermediateDirectories: true
        )
        #expect(Git.repositoryPaths(in: root.path)?.commonDirectory == root.appendingPathComponent(".bare").path)
    }

    @Test("A .git pointer into .bare identifies the same repository")
    func barePointer() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let gitDirectory = root.appendingPathComponent(".bare/worktrees/main")
        let worktree = root.appendingPathComponent("wt/main")
        try FileManager.default.createDirectory(at: gitDirectory, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: worktree, withIntermediateDirectories: true)
        try "../..\n".write(to: gitDirectory.appendingPathComponent("commondir"), atomically: true, encoding: .utf8)
        try "gitdir: \(gitDirectory.path)\n".write(
            to: worktree.appendingPathComponent(".git"), atomically: true, encoding: .utf8
        )
        #expect(Git.repositoryPaths(in: worktree.path)?.commonDirectory == root.appendingPathComponent(".bare").path)
    }

    @Test("The agent list keeps provider and pane state")
    func agentList() throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let list = try decoder.decode(SwarmAgentList.self, from: Data(#"{"agents":[{"id":"coder","role":"code","provider":"codex","pane":"p1","alive":true}]}"#.utf8))
        #expect(list.agents[0].provider == "codex")
        #expect(list.agents[0].alive == true)
    }

    private func build(
        _ sessions: [SwarmSession], agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:],
        titles: [SwarmSessionID: String] = [:]
    ) -> SessionsTree {
        SessionsTree.build(
            sessions: sessions, agentsBySession: agentsBySession, titles: titles,
            repositoryPathsResolver: { path in
                guard path.hasPrefix("/repo") else { return nil }
                return GitRepositoryPaths(gitDirectory: common + "/worktrees/test", commonDirectory: common)
            },
            worktreeLister: { _ in worktrees }
        )
    }

    private func session(
        _ id: String, cwd: String, chair: String? = nil, archivedAt: Int? = nil
    ) -> SwarmSession {
        SwarmSession(
            id: SwarmSessionID(id), talkMode: "lane", adapter: "tmux-solo", cwd: cwd,
            createdAt: 1, chairProvider: chair == nil ? nil : "codex",
            chairID: chair.map(SwarmChairID.init), chairLog: nil,
            agents: 1, messages: 0, lastMessageAt: nil, archivedAt: archivedAt
        )
    }

    private func projectSession(
        _ id: String, title: String = "Chat", provider: String?, running: Bool?,
        liveAgents: Int? = nil, activity: Int? = nil
    ) -> SwarmProjectSession {
        var item = session(id, cwd: "/outside")
        item.lastMessageAt = activity
        return SwarmProjectSession(
            sessions: [item], title: title, isRunning: running,
            liveAgents: liveAgents, provider: provider
        )
    }
}
