import Foundation
import Testing
@testable import SwarmCore

@Suite("End and archive chats")
struct ChatEndTests {
    @Test("End covers linked sessions, closes all children before chairs, and never archives")
    func groupOrder() async throws {
        let recorder = ChatEndCalls()
        let chat = group()
        try await SwarmSessionCloser.end(session: chat, bus: bus(recorder))
        #expect(await recorder.calls == [
            "current:agents --json", "older:agents --json",
            "current:close child", "current:close no-pane", "older:close child", "older:close no-pane",
            "current:close orchestrator", "older:close orchestrator",
        ])
    }

    @Test("Archive ends the group before sending every group id")
    func archiveOrder() async throws {
        let recorder = ChatEndCalls()
        try await SwarmSessionCloser.archive(session: group(), bus: bus(recorder))
        #expect(await recorder.calls.last == ":session archive current older")
        #expect(await recorder.calls.count == 9)
    }

    @Test("A close failure stops archive and reaches the caller")
    func archiveFailure() async throws {
        let recorder = ChatEndCalls(failClose: true)
        await #expect(throws: (any Error).self) {
            try await SwarmSessionCloser.archive(session: group(), bus: bus(recorder))
        }
        #expect(await recorder.calls == [
            "current:agents --json", "older:agents --json", "current:close child",
        ])
    }

    @Test("Only live children require confirmation, including a missing pane and waiting")
    func confirmationRule() async throws {
        let chair = SwarmAgent(id: SwarmPanePolicy.chair, role: "chair", pane: "chair", alive: true, state: "working")
        let dead = SwarmAgent(id: .init("dead"), role: "child", pane: "old", alive: false, state: "working")
        let notStarted = SwarmAgent(id: .init("registered"), role: "child", pane: nil, alive: nil)
        #expect(!SwarmSessionCloser.Confirmation(agents: [chair, dead, notStarted]).required)
        let idle = SwarmAgent(id: .init("idle"), role: "child", pane: "idle", alive: true, state: "done")
        let waiting = SwarmAgent(id: .init("waiting"), role: "child", pane: "waiting", alive: true, state: "waiting")
        let unknown = SwarmAgent(id: .init("unknown"), role: "child", pane: "unknown", alive: nil)
        let noPane = SwarmAgent(id: .init("no-pane"), role: "child", pane: nil, alive: true)
        let confirmation = SwarmSessionCloser.Confirmation(agents: [chair, idle, waiting, unknown, noPane, dead])
        #expect(confirmation.required)
        #expect(confirmation.liveChildren == 3)
        #expect(confirmation.midTurnChildren == 1)
        #expect(SwarmSessionCloser.Confirmation(agents: [noPane]).required)
        #expect(SwarmSessionCloser.Confirmation(agents: [idle]).message ==
                "1 agent still runs, and 0 are mid-turn. Ending stops them.")
        let recorder = ChatEndCalls()
        let fresh = try await SwarmSessionCloser.confirmation(session: group(), bus: bus(recorder))
        #expect(fresh.liveChildren == 4)
        #expect(fresh.midTurnChildren == 2)
        #expect(await recorder.calls == ["current:agents --json", "older:agents --json"])
    }

    private func group() -> SwarmProjectSession {
        SwarmProjectSession(sessions: ["current", "older"].map { id in
            SwarmSession(id: .init(id), talkMode: "lane", adapter: "herdr", cwd: "/fixture",
                         createdAt: 1, chairLog: nil, agents: 3, messages: 0, lastMessageAt: nil)
        }, title: "Chat")
    }

    private func bus(_ recorder: ChatEndCalls) -> SwarmCLIBus {
        SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await recorder.reply(arguments, environment: environment)
        }
    }
}

private actor ChatEndCalls {
    private(set) var calls: [String] = []
    let failClose: Bool
    init(failClose: Bool = false) { self.failClose = failClose }

    func reply(_ arguments: [String], environment: [String: String]) -> ShellResult {
        calls.append((environment["SWARM_SESSION_ID"] ?? "") + ":" + arguments.joined(separator: " "))
        if arguments == ["agents", "--json"] {
            return ShellResult(status: 0, stdout: """
                {"agents":[
                  {"id":"orchestrator","role":"chair","pane":"chair","alive":true},
                  {"id":"child","role":"child","pane":"child","alive":true,"state":"working"},
                  {"id":"no-pane","role":"child","pane":null,"alive":true},
                  {"id":"dead","role":"child","pane":null,"alive":false}
                ]}
                """, stderr: "")
        }
        if failClose, arguments.first == "close" {
            return ShellResult(status: 1, stdout: "", stderr: "Close refused")
        }
        return ShellResult(status: 0, stdout: "", stderr: "")
    }
}
