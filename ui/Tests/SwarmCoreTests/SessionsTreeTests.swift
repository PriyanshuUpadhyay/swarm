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

    @Test("One repository shows sessions in two worktrees")
    func repositoryAndWorktrees() {
        let tree = build([
            session("11111111-a", cwd: "/repo/wt/main/src"),
            session("22222222-b", cwd: "/repo/wt/feature"),
        ])
        #expect(tree.projects.count == 1)
        #expect(tree.projects[0].name == "repo")
        #expect(tree.projects[0].worktrees.map(\.sessions.count) == [1, 1])
        #expect(tree.launchDirectory(for: SwarmSessionID("22222222-b")) == "/repo/wt/feature")
        #expect(tree.text(now: 61).contains("main\n    no chair 11111111 · 1m · 1 total"))
    }

    @Test("A folder has session rows directly under it")
    func folder() {
        let tree = build([session("folder-1", cwd: "/outside")])
        #expect(tree.projects[0].path == "/outside")
        #expect(tree.projects[0].worktrees.isEmpty)
        #expect(tree.projects[0].sessions.count == 1)
        #expect(tree.launchDirectory(for: SwarmSessionID("folder-1")) == "/outside")
    }

    @Test("Rows with only dead agents say ended in the sidebar and tree text")
    func ended() {
        let dead = session("dead-session", cwd: "/outside")
        let agent = SwarmAgent(
            id: .init("orchestrator"), role: "chair", pane: "%1", alive: false
        )
        let tree = build([dead], agentsBySession: [dead.id: [agent]])
        let row = tree.projects[0].sessions[0]
        #expect(SessionsTree.rowText(row, now: 61).hasPrefix("ended dead-ses"))
        #expect(SessionsTree.rowText(row, now: 61).contains("0 live · 1 total"))
        #expect(tree.text(now: 61).contains("ended dead-ses"))

        let empty = build([dead], agentsBySession: [dead.id: []])
        #expect(SessionsTree.rowText(empty.projects[0].sessions[0], now: 61)
            .hasPrefix("ended dead-ses"))

        let running = build([dead], agentsBySession: [dead.id: [agent, SwarmAgent(
            id: .init("worker"), role: "code", pane: "%2", alive: true
        )]])
        #expect(SessionsTree.rowText(running.projects[0].sessions[0], now: 61)
            .hasPrefix("no chair dead-ses"))
        #expect(SessionsTree.rowText(running.projects[0].sessions[0], now: 61)
            .contains("1 live · 2 total"))
    }

    @Test("Archived sessions and empty worktrees are hidden")
    func archived() {
        let tree = build([
            session("active", cwd: "/repo/wt/main"),
            session("archived", cwd: "/repo/wt/feature", archivedAt: 50),
        ])
        #expect(tree.projects[0].worktrees.map(\.entry.path) == ["/repo/wt/main"])
        #expect(tree.session(SwarmSessionID("archived")) == nil)
    }

    @Test("Repeated sessions in one chair make one row")
    func chairGroup() {
        let tree = build([
            session("older", cwd: "/repo/wt/main", chair: "chair"),
            session("newer", cwd: "/repo/wt/main", chair: "chair"),
        ])
        let rows = tree.projects[0].worktrees[0].sessions
        #expect(rows.count == 1)
        #expect(rows[0].sessions.count == 2)
        #expect(tree.session(SwarmSessionID("older"))?.id == rows[0].id)
        #expect(tree.retainedSelection(SwarmSessionID("older")) == SwarmSessionID("older"))
        #expect(tree.retainedSelection(SwarmSessionID("newer")) == SwarmSessionID("newer"))
        #expect(tree.retainedSelection(SwarmSessionID("gone")) == nil)
        #expect(tree.windowTitle(for: SwarmSessionID("newer")) == "repo · codex newer")
    }

    @Test("A new session remains the chat row when an older session has later activity")
    func newestSessionOwnsChat() {
        var older = session("older", cwd: "/repo/wt/main", chair: "chair")
        older.lastMessageAt = 100
        var newer = session("newer", cwd: "/repo/wt/main", chair: "chair")
        newer.createdAt = 50
        let tree = build([older, newer])
        #expect(tree.session(newer.id)?.id == newer.id)
        #expect(tree.session(newer.id)?.session == newer)
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
            session("hub", cwd: "/repo"),
            session("main", cwd: "/repo/wt/main"),
        ])
        #expect(tree.projects.count == 1)
        #expect(tree.projects[0].sessions.map(\.id) == [SwarmSessionID("hub")])
        #expect(tree.projects[0].launchDirectory == "/repo/wt/main")
        #expect(tree.launchDirectory(for: SwarmSessionID("hub")) == "/repo/wt/main")
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
        _ sessions: [SwarmSession], agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:]
    ) -> SessionsTree {
        SessionsTree.build(
            sessions: sessions, agentsBySession: agentsBySession,
            repositoryPathsResolver: { path in
                guard path == "/repo" || worktrees.contains(where: { $0.path == path }) else { return nil }
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
}
