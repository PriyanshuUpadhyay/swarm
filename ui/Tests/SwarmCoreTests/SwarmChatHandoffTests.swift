import Foundation
import Testing
@testable import SwarmCore

@Suite("Chat handoff")
struct SwarmChatHandoffTests {
    @Test("A chair can receive context before its first log exists")
    func readyBeforeLog() {
        let session = SwarmSession(
            id: SwarmSessionID("new"), talkMode: "lane", adapter: "tmux-solo",
            cwd: "/work", createdAt: 1, chairProvider: "codex",
            chairID: SwarmChairID("codex-session"), chairLog: nil,
            agents: 1, messages: 0, lastMessageAt: nil
        )
        #expect(SwarmChatHandoff.isReady(session, provider: "codex"))
        #expect(!SwarmChatHandoff.isReady(session, provider: "claude"))
    }

    @Test("A summary is used only after the new turn ends")
    func completedSummary() {
        let old = TranscriptRow(kind: .assistant, text: "old answer", eventID: "old")
        let request = TranscriptRow(kind: .user, text: SwarmChatHandoff.request, eventID: "request")
        let answer = TranscriptRow(kind: .assistant, text: "files and next step", eventID: "answer")
        var ended = TranscriptRow(kind: .result, text: "done", eventID: "end")
        ended.endsTurn = true
        #expect(SwarmChatHandoff.completedSummary(in: [old, request, answer], after: 1) == nil)
        #expect(SwarmChatHandoff.completedSummary(in: [old, request, answer, ended], after: 1) == "files and next step")
    }

    @Test("A closed pane can still carry recent messages")
    func recentContext() {
        let rows = [
            TranscriptRow(kind: .user, text: "Fix the menu", eventID: "1"),
            TranscriptRow(kind: .toolResult, text: "secret tool output", eventID: "2"),
            TranscriptRow(kind: .assistant, text: "I found the filter", eventID: "3"),
        ]
        let context = SwarmChatHandoff.recentContext(in: rows)
        #expect(context?.contains("User: Fix the menu") == true)
        #expect(context?.contains("Agent: I found the filter") == true)
        #expect(context?.contains("secret tool output") == false)
    }
}

@Suite("Model switch lifecycle")
struct ModelSwitchLifecycleTests {
    @Test("Cancelling before launch cannot create or link a session")
    func cancelBeforeLaunch() async throws {
        let log = try fixture()
        defer { try? FileManager.default.removeItem(at: log) }
        let calls = HandoffCalls()
        let plan = try #require(SwarmChatLaunchPlan(
            directory: "/tmp", provider: "codex", model: "gpt-6-sol", account: nil
        ))
        let task = Task {
            try await SwarmChatHandoff.start(plan, after: row(log), bus: bus(calls)) { phase in
                if phase == .starting { withUnsafeCurrentTask { $0?.cancel() } }
            }
        }
        do {
            _ = try await task.value
            Issue.record("Cancelled switch completed")
        } catch is CancellationError { }
        #expect(await calls.arguments == [["agents", "--json"]])
    }

    @Test("Retry cannot interrupt a summary that is still running")
    func activeSource() async throws {
        let log = try fixture()
        defer { try? FileManager.default.removeItem(at: log) }
        let handle = try FileHandle(forWritingTo: log)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Summary still running"}]}}"#.appending("\n").utf8))
        try handle.close()
        let calls = HandoffCalls(alive: true)
        let plan = try #require(SwarmChatLaunchPlan(
            directory: "/tmp", provider: "codex", model: "gpt-6-sol", account: nil
        ))
        do {
            _ = try await SwarmChatHandoff.start(plan, after: row(log), bus: bus(calls))
            Issue.record("Switch should wait for the active reply")
        } catch let error as SwarmProfileError {
            #expect(error.message.contains("Wait for the current reply"))
        }
        #expect(await calls.arguments == [["agents", "--json"]])
    }

    @Test("A switch reports stages and links only after context delivery")
    func successfulHandoff() async throws {
        let log = try fixture()
        defer { try? FileManager.default.removeItem(at: log) }
        let calls = HandoffCalls()
        let plan = try #require(SwarmChatLaunchPlan(
            directory: "/tmp", provider: "codex", model: "gpt-6-sol", account: nil
        ))
        let id = try await SwarmChatHandoff.start(plan, after: row(log), bus: bus(calls)) { phase in
            await calls.progress(phase)
        }
        #expect(id.rawValue == "new")
        #expect(await calls.phases == [.preparing, .starting, .waiting, .delivering])
        #expect(await calls.arguments.suffix(2) == [["type", "orchestrator"], ["session", "continue", "new", "old"]])
        #expect(await calls.delivered?.contains("Keep the user's files") == true)
        #expect(!ChatSwitchPhase.starting.canCancel)
        #expect(ChatSwitchPhase.summarizing.canCancel)
    }

    @Test("Failed context delivery keeps the original chat separate")
    func deliveryFailure() async throws {
        let log = try fixture()
        defer { try? FileManager.default.removeItem(at: log) }
        let calls = HandoffCalls(failDelivery: true)
        let plan = try #require(SwarmChatLaunchPlan(
            directory: "/tmp", provider: "codex", model: "gpt-6-sol", account: nil
        ))
        do {
            _ = try await SwarmChatHandoff.start(plan, after: row(log), bus: bus(calls))
            Issue.record("Delivery should fail")
        } catch let error as SwarmProfileError {
            #expect(error.message.contains("delivery failed"))
        }
        #expect(await !calls.arguments.contains { $0.starts(with: ["session", "continue"]) })
    }

    private func fixture() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("handoff-\(UUID().uuidString).jsonl")
        try Data("""
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Keep the user's files"}]}}
            {"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Ready"}]}}
            {"type":"event_msg","payload":{"type":"task_complete"}}
            """.appending("\n").utf8).write(to: url)
        return url
    }

    private func row(_ log: URL) -> SwarmProjectSession {
        SwarmProjectSession(sessions: [SwarmSession(
            id: .init("old"), talkMode: "lane", adapter: "tmux-solo", cwd: "/tmp", createdAt: 1,
            chairProvider: "codex", chairLog: log.path, agents: 1, messages: 0, lastMessageAt: nil
        )], title: "Test", isRunning: false)
    }

    private func bus(_ calls: HandoffCalls) -> SwarmCLIBus {
        SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, _, stdin, _ in await calls.reply(arguments, stdin: stdin)
        }
    }
}

private actor HandoffCalls {
    var arguments: [[String]] = []
    var phases: [ChatSwitchPhase] = []
    var delivered: String?
    let failDelivery: Bool
    let alive: Bool
    init(failDelivery: Bool = false, alive: Bool = false) {
        self.failDelivery = failDelivery
        self.alive = alive
    }
    func progress(_ phase: ChatSwitchPhase) { phases.append(phase) }
    func reply(_ arguments: [String], stdin: String?) -> ShellResult {
        self.arguments.append(arguments)
        let output: String
        switch arguments.first {
        case "agents": output = alive
            ? #"{"agents":[{"id":"orchestrator","role":"chat","pane":"%old","alive":true}]}"#
            : #"{"agents":[]}"#
        case "launch": output = "%test\n"
        case "sessions": output = """
            {"sessions":[{"id":"new","talk_mode":"lane","adapter":"tmux-solo","cwd":"/tmp","created_at":2,"chair_provider":"codex","chair_id":"ready","agents":1,"messages":0}]}
            """
        case "type":
            delivered = stdin
            if failDelivery { return ShellResult(status: 1, stdout: "", stderr: "delivery failed") }
            output = ""
        case "session" where arguments.contains("new"): output = "new\n"
        default: output = ""
        }
        return ShellResult(status: 0, stdout: output, stderr: "")
    }
}
