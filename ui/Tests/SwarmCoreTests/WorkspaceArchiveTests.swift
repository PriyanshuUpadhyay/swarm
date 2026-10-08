import Foundation
import Testing
@testable import SwarmCore

@Suite("Archived workspace policy")
struct WorkspaceArchiveTests {
    @Test("Archive checks chairs as well as children and ends every chat-group session")
    func endAgents() async throws {
        let recorder = WorkspaceArchiveCalls()
        let chats = [chat("current", older: "older"), chat("another")]
        let bus = bus(recorder)
        #expect(try await WorkspaceArchive.liveAgents(in: chats, bus: bus) == 3)
        try await WorkspaceArchive.end(chats, bus: bus)
        #expect(await recorder.calls == [
            ":agents --json --all", ":agents --json --all",
            "current:close orchestrator", "older:close orchestrator", "another:close orchestrator",
        ])
        #expect(await recorder.calls.allSatisfy { !$0.contains("session archive") })
    }

    @Test("Archive and delete require the same confirmation for any live agent")
    func confirmationRule() {
        #expect(!WorkspaceArchive.Confirmation(liveAgents: 0).required)
        #expect(WorkspaceArchive.Confirmation(liveAgents: 1).required)
        #expect(WorkspaceArchive.Confirmation(liveAgents: 3).liveAgents == 3)
    }

    @Test("Archived workspace selection keeps archive state, blocks new chats and has a reason")
    func readOnly() throws {
        let tree = source()
        let entry = try #require(WorkspaceEntry.list(in: tree).first { $0.id == "/repo/feature" })
        var navigation = WorkspaceNavigation()
        navigation.archive(entry.id)
        navigation.select(entry, chat: .init("current"))
        #expect(navigation.archived.contains(entry.id))
        #expect(navigation.selectedChat(in: entry)?.id == .init("current"))
        #expect(!navigation.canStartChat(in: entry))
        #expect(navigation.readOnlyReason(in: entry.id)?.contains("Restore") == true)
        let rows = SidebarRows.sections(projects: tree.projects, workspaces: WorkspaceEntry.list(in: tree),
                                        navigation: navigation, search: "", showingArchive: true, now: 10)
            .flatMap(\.rows)
        #expect(rows.first { $0.id == entry.id }?.newChatEnabled == false)
        navigation.archived.remove(entry.id)
        #expect(navigation.canStartChat(in: entry))
        #expect(navigation.readOnlyReason(in: entry.id) == nil)
    }

    @Test("A waiting agent in an older session supplies the Archived glyph; active workspaces do not")
    func glyph() {
        let entries = WorkspaceEntry.list(in: source())
        let waiting = SwarmAgent(id: .init("child"), role: "child", pane: "child", alive: true, state: "waiting")
        let done = SwarmAgent(id: .init("done"), role: "child", pane: "done", alive: true, state: "done")
        var navigation = WorkspaceNavigation()
        #expect(SidebarRows.archivedStatus(entries, navigation: navigation, agentsBySession: [.init("older"): [waiting]]) == nil)
        navigation.archive("/repo/feature")
        #expect(SidebarRows.archivedStatus(entries, navigation: navigation,
                                          agentsBySession: [.init("older"): [waiting], .init("current"): [done]]) == .waiting)
        #expect(SidebarRows.archivedStatus(entries, navigation: navigation,
                                          agentsBySession: [.init("current"): [done]]) == nil)
    }

    @Test("Delete is offered only for present repository worktrees outside the main checkout")
    func deletePolicy() throws {
        let entries = WorkspaceEntry.list(in: source())
        #expect(entries.first { $0.id == "/repo" }?.canDelete == false)
        #expect(entries.first { $0.id == "/repo/feature" }?.canDelete == true)
        #expect(entries.first { $0.id == "/repo/missing" }?.canDelete == false)
        let plain = ProjectNode(id: .folder("/folder"), path: "/folder", launchDirectory: "/folder",
                                workspaces: [WorkspaceNode(path: "/folder", name: "folder", sessions: [])])
        #expect(WorkspaceEntry.list(in: SessionsTree(projects: [plain])).first?.canDelete == false)
    }

    private func source() -> SessionsTree {
        SessionsTree(projects: [ProjectNode(id: .repository(commonDirectory: "/repo/.git"), path: "/repo",
            launchDirectory: "/repo", workspaces: [
                WorkspaceNode(path: "/repo", name: "repo", sessions: []),
                WorkspaceNode(path: "/repo/feature", name: "feature", sessions: [chat("current", older: "older")]),
                WorkspaceNode(path: "/repo/missing", name: "missing", sessions: [], missing: true),
            ])])
    }

    private func chat(_ id: String, older: String? = nil) -> SwarmProjectSession {
        let ids = [id] + (older.map { [$0] } ?? [])
        return SwarmProjectSession(sessions: ids.map {
            SwarmSession(id: .init($0), talkMode: "lane", adapter: "herdr", cwd: "/repo/feature",
                         createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
        }, title: "Chat")
    }

    private func bus(_ recorder: WorkspaceArchiveCalls) -> SwarmCLIBus {
        SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await recorder.reply(arguments, environment: environment)
        }
    }
}

private actor WorkspaceArchiveCalls {
    private(set) var calls: [String] = []
    func reply(_ arguments: [String], environment: [String: String]) -> ShellResult {
        calls.append((environment["SWARM_SESSION_ID"] ?? "") + ":" + arguments.joined(separator: " "))
        if arguments == ["agents", "--json", "--all"] {
            let row = "{\"agents\":[{\"id\":\"orchestrator\",\"role\":\"chair\",\"pane\":\"chair\",\"alive\":true}]}"
            return ShellResult(status: 0, stdout: "{\"current\":\(row),\"older\":\(row),\"another\":\(row)}", stderr: "")
        }
        return ShellResult(status: 0, stdout: "", stderr: "")
    }
}
