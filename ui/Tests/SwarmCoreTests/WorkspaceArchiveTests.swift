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

    @Test("A missing batch session is read alone and its live agents need confirmation")
    func missingSession() async throws {
        let recorder = WorkspaceArchiveCalls(omitted: "older")
        let chats = [chat("current", older: "older")]
        let count = try await WorkspaceArchive.liveAgents(in: chats, bus: bus(recorder))
        #expect(count == 2)
        #expect(WorkspaceArchive.Confirmation(liveAgents: count).required)
        try await WorkspaceArchive.end(chats, bus: bus(recorder))
        #expect(await recorder.calls == [
            ":agents --json --all", "older:agents --json",
            ":agents --json --all", "older:agents --json",
            "current:close orchestrator", "older:close orchestrator",
        ])
    }

    @Test("An unreadable missing session refuses confirmation and stopping before any close")
    func unreadableSession() async throws {
        let recorder = WorkspaceArchiveCalls(omitted: "older", unreadable: "older")
        let chats = [chat("current", older: "older")]
        do {
            _ = try await WorkspaceArchive.liveAgents(in: chats, bus: bus(recorder))
            Issue.record("The unreadable chat must refuse the action")
        } catch {
            #expect(error.localizedDescription == "Swarm could not read the agents of “Chat”. Try again.")
        }
        await #expect(throws: (any Error).self) {
            try await WorkspaceArchive.end(chats, bus: bus(recorder))
        }
        #expect(await recorder.calls.allSatisfy { !$0.contains("close") })
    }

    @Test("An older swarm without the batch command falls back to each session")
    func olderSwarm() async throws {
        let recorder = WorkspaceArchiveCalls(batchFails: true)
        let chats = [chat("current", older: "older")]
        #expect(try await WorkspaceArchive.liveAgents(in: chats, bus: bus(recorder)) == 2)
        try await WorkspaceArchive.end(chats, bus: bus(recorder))
        #expect(await recorder.calls == [
            ":agents --json --all", "current:agents --json", "older:agents --json",
            ":agents --json --all", "current:agents --json", "older:agents --json",
            "current:close orchestrator", "older:close orchestrator",
        ])
    }

    @Test("A failed batch reads a session the cached list shows with zero agents")
    func failedBatchStaleEmptySession() async throws {
        let recorder = WorkspaceArchiveCalls(batchFails: true)
        var late = chat("current").session
        late.agents = 0
        let chats = [SwarmProjectSession(sessions: [late], title: "Chat")]
        #expect(try await WorkspaceArchive.liveAgents(in: chats, bus: bus(recorder)) == 1)
        try await WorkspaceArchive.end(chats, bus: bus(recorder))
        #expect(try await SwarmSessionCloser.confirmation(session: chats[0], bus: bus(recorder)).liveChildren == 0)
        try await SwarmSessionCloser.end(session: chats[0], bus: bus(recorder))
        #expect(await recorder.calls == [
            ":agents --json --all", "current:agents --json",
            ":agents --json --all", "current:agents --json", "current:close orchestrator",
            ":agents --json --all", "current:agents --json",
            ":agents --json --all", "current:agents --json", "current:close orchestrator",
        ])
    }

    @Test("Missing zero-agent and legacy adapter sessions need no single read")
    func emptyOrLegacySession() async throws {
        for (adapter, agents) in [("herdr" as String?, 0), (nil, 1), ("", 1)] {
            let recorder = WorkspaceArchiveCalls(omitted: "legacy")
            let legacy = SwarmSession(id: .init("legacy"), talkMode: "lane", adapter: adapter,
                                     cwd: "/repo/feature", createdAt: 1, chairLog: nil,
                                     agents: agents, messages: 0, lastMessageAt: nil)
            let chats = [SwarmProjectSession(sessions: [legacy], title: "Legacy chat")]
            #expect(try await WorkspaceArchive.liveAgents(in: chats, bus: bus(recorder)) == 0)
            try await WorkspaceArchive.end(chats, bus: bus(recorder))
            try await SwarmSessionCloser.end(session: chats[0], bus: bus(recorder))
            let confirmation = try await SwarmSessionCloser.confirmation(session: chats[0], bus: bus(recorder))
            #expect(!confirmation.required)
            #expect(confirmation.liveChildren == 0)
            #expect(confirmation.midTurnChildren == 0)
            #expect(await recorder.calls == Array(repeating: ":agents --json --all", count: 4))
        }
    }

    @Test("End chat and workspace use the same message when a session read fails")
    func endReadFailure() async throws {
        let recorder = WorkspaceArchiveCalls(omitted: "current", unreadable: "current")
        do {
            try await SwarmSessionCloser.end(session: chat("current"), bus: bus(recorder))
            Issue.record("The unreadable chat must refuse the action")
        } catch {
            #expect(error.localizedDescription == "Swarm could not read the agents of “Chat”. Try again.")
        }
        #expect(await recorder.calls == [":agents --json --all", "current:agents --json"])
    }

    @Test("Chat confirmation names an unreadable session and never closes it")
    func confirmationReadFailure() async throws {
        let recorder = WorkspaceArchiveCalls(omitted: "current", unreadable: "current")
        do {
            _ = try await SwarmSessionCloser.confirmation(session: chat("current"), bus: bus(recorder))
            Issue.record("The unreadable chat must refuse confirmation")
        } catch {
            #expect(error.localizedDescription == "Swarm could not read the agents of “Chat”. Try again.")
        }
        #expect(await recorder.calls == [":agents --json --all", "current:agents --json"])
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
    let omitted: String?
    let unreadable: String?
    let batchFails: Bool

    init(omitted: String? = nil, unreadable: String? = nil, batchFails: Bool = false) {
        self.omitted = omitted
        self.unreadable = unreadable
        self.batchFails = batchFails
    }

    func reply(_ arguments: [String], environment: [String: String]) -> ShellResult {
        calls.append((environment["SWARM_SESSION_ID"] ?? "") + ":" + arguments.joined(separator: " "))
        let row = "{\"agents\":[{\"id\":\"orchestrator\",\"role\":\"chair\",\"pane\":\"chair\",\"alive\":true}]}"
        if arguments == ["agents", "--json", "--all"] {
            if batchFails { return ShellResult(status: 1, stdout: "", stderr: "Unknown --all") }
            let ids: [String] = ["current", "older", "another"]
            let rows = ids.filter { $0 != omitted }
                .map { "\"\($0)\":\(row)" }.joined(separator: ",")
            return ShellResult(status: 0, stdout: "{\(rows)}", stderr: "")
        }
        if arguments == ["agents", "--json"] {
            if environment["SWARM_SESSION_ID"] == unreadable {
                return ShellResult(status: 1, stdout: "", stderr: "Read timed out")
            }
            return ShellResult(status: 0, stdout: row, stderr: "")
        }
        return ShellResult(status: 0, stdout: "", stderr: "")
    }
}
