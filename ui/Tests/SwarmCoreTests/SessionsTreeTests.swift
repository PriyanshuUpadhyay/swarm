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
        #expect(tree.text(now: 61).contains("main\n    no chair 11111111 · 1m · 1 agents"))
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
        #expect(tree.text(now: 61).contains("ended dead-ses"))

        let empty = build([dead], agentsBySession: [dead.id: []])
        #expect(SessionsTree.rowText(empty.projects[0].sessions[0], now: 61)
            .hasPrefix("ended dead-ses"))

        let running = build([dead], agentsBySession: [dead.id: [agent, SwarmAgent(
            id: .init("worker"), role: "code", pane: "%2", alive: true
        )]])
        #expect(SessionsTree.rowText(running.projects[0].sessions[0], now: 61)
            .hasPrefix("no chair dead-ses"))
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
    }

    @Test("The bare repository is one project for its linked worktrees")
    func bareLayout() {
        let tree = build([
            session("first", cwd: "/repo/wt/main/a"),
            session("second", cwd: "/repo/wt/feature/b"),
        ])
        #expect(tree.projects.map(\.id) == [.repository(commonDirectory: common)])
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
                guard worktrees.contains(where: { $0.path == path }) else { return nil }
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
