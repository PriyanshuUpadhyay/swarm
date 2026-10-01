import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Session detail")
struct SwarmSessionDetailTests {
    @Test("The grid includes only live agents and sorts by creation time")
    func agentCells() {
        var value = session(adapter: "tmux-solo")
        value.chairID = .init("other-chair")
        let agents = [
            SwarmAgent(id: .init("dead"), role: "code", pane: "%1", alive: false, createdAt: 1),
            SwarmAgent(id: .init("later"), role: "code", pane: "%2", alive: true, createdAt: 3),
            SwarmAgent(id: .init("orchestrator"), role: "chair", pane: "%3", alive: true),
            SwarmAgent(id: .init("early"), role: "code", pane: "%4", alive: true, createdAt: 2),
            SwarmAgent(id: .init("other-chair"), role: "chair", pane: "%5", alive: true),
            SwarmAgent(id: .init("no-pane"), role: "code", pane: nil, alive: nil, createdAt: 4),
        ]
        let cells = SwarmPanePolicy.cells(session: value, agents: agents)
        #expect(cells.map(\.id.rawValue) == ["early", "later"])
    }

    @Test("The pane column exists only while a child agent is live")
    func liveChildAgents() {
        let value = session(adapter: "tmux-solo")
        let ended = [SwarmAgent(id: .init("coder"), role: "code", pane: "%1", alive: false)]
        let live = [SwarmAgent(id: .init("coder"), role: "code", pane: "%1", alive: true)]
        #expect(!SwarmPanePolicy.hasLiveChildAgents(session: value, agents: ended))
        #expect(SwarmPanePolicy.hasLiveChildAgents(session: value, agents: live))
    }

    @Test("Agent creation time decodes from the CLI")
    func agentCreationTime() throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let agent = try decoder.decode(SwarmAgent.self, from: Data(
            #"{"id":"coder","role":"code","pane":"%2","alive":true,"created_at":42}"#.utf8
        ))
        #expect(agent.createdAt == 42)
    }

    @Test("A missing chair log stays in retry state")
    func missingLog() async {
        var value = session(adapter: "herdr")
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in true }) == .waiting)
        value.chairLog = "/tmp/not-written.jsonl"
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in false }) == .waiting)
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in true })
            == .ready(log: URL(fileURLWithPath: value.chairLog!), format: "claude"))
        let reader = SwarmChairTranscript()
        #expect(await reader.poll(session: value) == .waiting)
        value.chairProvider = nil
        #expect(await reader.poll(session: value, chairProvider: "agy")
            == .waiting)
        #expect(await reader.poll(session: value)
            == .notice("No transcript reader for this provider yet"))
    }

    @Test("A known AGY log uses the existing translator and retains failed tool output")
    func agyLog() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("agy-ui-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try Data("""
            {"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-09-26T00:00:00Z","tool_calls":[{"name":"run_command","args":{"CommandLine":"swift test"}}]}
            {"step_index":2,"source":"MODEL","type":"RUN_COMMAND","status":"ERROR","created_at":"2026-09-26T00:00:01Z","content":"One check failed."}
            """.appending("\n").utf8).write(to: log)
        var value = session(adapter: "herdr")
        value.chairProvider = "agy"
        value.chairLog = log.path
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in true })
            == .ready(log: log, format: "agy"))
        let reader = SwarmChairTranscript(binary: binary)
        guard case .rows(let rows, let raw) = await reader.poll(session: value) else {
            Issue.record("AGY log did not reach the UI reader")
            return
        }
        #expect(rows.count == 1)
        #expect(rows.first?.tool?.command == "swift test")
        #expect(rows.first?.tool?.state == .failed)
        #expect(rows.first?.tool?.output == "One check failed.")
        #expect(raw.count == 2)
    }

    @Test("A missing log explains whether the chat ended")
    func missingLogMessage() {
        #expect(ChairTranscriptSnapshot.waitingMessage(isRunning: true)
            == "The chair has not written its log yet")
        #expect(ChairTranscriptSnapshot.waitingMessage(isRunning: false)
            == "This chat ended before its log was found")
    }

    @Test("The real transcript tool makes rows from a chair log")
    func fixtureLog() async throws {
        let binary = try #require(TranscriptToolProcess.bundled, "tool not built")
        let log = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-chair-fixture-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        var value = session(adapter: "herdr")
        value.chairProvider = "codex"
        value.chairLog = log.path
        let reader = SwarmChairTranscript(binary: binary)
        #expect(await reader.poll(session: value) == .waiting)
        try Data("""
            {"type":"session_meta","payload":{"id":"session-1","cwd":"/work"}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"List files"}]}}
            """.appending("\n").utf8).write(to: log)
        let snapshot = await reader.poll(session: value)
        guard case .rows(let rows, _) = snapshot else {
            Issue.record("The tool did not return rows")
            return
        }
        #expect(rows.contains { $0.kind == .user && $0.text.contains("List files") })
        try FileManager.default.removeItem(at: log)
        guard case .rows(let retained, _) = await reader.poll(session: value) else {
            Issue.record("A missing log cleared the loaded transcript")
            return
        }
        #expect(retained == rows)
    }

    @Test("The chair reports the actual Claude model through the transcript tool")
    func actualModel() async throws {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("model-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try Data(#"{"type":"assistant","uuid":"a","message":{"model":"claude-opus-4-6","content":[{"type":"text","text":"Ready"}]}}"#.appending("\n").utf8).write(to: log)
        var value = session(adapter: "tmux-solo")
        value.chairLog = log.path
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))
        _ = await reader.poll(session: value)
        #expect(await reader.currentModel == "claude-opus-4-6")
    }

    @Test("A transcript starts after its reader becomes available")
    func readerRetry() async throws {
        let realBinary = try #require(TranscriptToolProcess.bundled)
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-reader-retry-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let binary = directory.appendingPathComponent("transcript")
        let log = directory.appendingPathComponent("rollout.jsonl")
        try Data("""
            {"type":"session_meta","payload":{"id":"session-1","cwd":"/work"}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"List files"}]}}
            """.appending("\n").utf8).write(to: log)
        var value = session(adapter: "herdr")
        value.chairProvider = "codex"
        value.chairLog = log.path
        let reader = SwarmChairTranscript(binary: binary)

        guard case .unavailable = await reader.poll(session: value) else {
            Issue.record("The missing reader did not report a failure")
            return
        }
        try FileManager.default.copyItem(at: realBinary, to: binary)
        guard case .rows(let rows, _) = await reader.poll(session: value) else {
            Issue.record("The reader did not retry")
            return
        }
        #expect(rows.contains { $0.kind == .user && $0.text.contains("List files") })
    }

    @Test("An answer names the agent, the question it saw, and the choice, in the session")
    func answerArguments() async throws {
        let calls = CloseCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await calls.reply(arguments: arguments, environment: environment)
        }
        let value = session(adapter: "tmux-solo")
        let prompt = SwarmPrompt(id: "015475c015dc4eae", question: "Do you want to proceed?", choices: ["Yes", "No"])
        try await bus.answer(prompt, choice: 1, to: .init("seat"), in: value)
        #expect(await calls.arguments == [["answer", "seat", "015475c015dc4eae", "1"]])
        #expect(await calls.adapters == ["tmux-solo"])
    }

    @Test("Hook status and plan decode the CLI's answers, and setup sends the plan's digest")
    func hooksStatusPlanAndSetup() async throws {
        let calls = CloseCalls()
        let plan = #"""
            {"digest":"d1","files":[{"path":"/h/.codex/config.toml","diff":"--- /h/.codex/config.toml\n+++ /h/.codex/config.toml\n@@ -1,1 +1,3 @@\n model = 1\n+[a]\n+b = 2\n"}],
             "conflicts":[{"file":"/h/.gemini/config/hooks.json","entry":"group \"swarm\"","found":"{}","wanted":"{\"Stop\":[]}","fix":"rename or delete it"}]}
            """#
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            _ = await calls.reply(arguments: arguments, environment: environment)
            let stdout = arguments.contains("--plan") ? plan : #"{"codex":true,"agy":false}"#
            return ShellResult(status: 0, stdout: stdout, stderr: "")
        }
        let status = try await bus.hooksStatus()
        #expect(status == SwarmHooksStatus(codex: true, agy: false))
        #expect(!status.isSetUp)

        let decoded = try await bus.hooksPlan()
        #expect(decoded.digest == "d1")
        #expect(decoded.files.map { [$0.added, $0.removed] } == [[2, 0]])
        #expect(decoded.files.first?.patch.hasPrefix(
            #"diff --git "a/h/.codex/config.toml" "b/h/.codex/config.toml"\#n--- /h/.codex/config.toml\#n"#
        ) == true)
        #expect(decoded.conflicts.first?.entry == #"group "swarm""#)
        #expect(!decoded.isSetUp && !decoded.canApply)
        #expect(SwarmHooksPlan(digest: "d", files: decoded.files, conflicts: []).canApply)
        #expect(SwarmHooksPlan(digest: "d", files: [], conflicts: []).isSetUp)
        #expect(decoded.summary == "1 file to change, 1 conflict.")
        #expect(SwarmHooksPlan(digest: "d", files: decoded.files + decoded.files, conflicts: []).summary
            == "2 files to change, 0 conflicts.")
        #expect(SwarmHooksPlan(digest: "d", files: [], conflicts: []).summary == "Swarm's hooks are already set up.")
        let plusLine = SwarmHooksPlan.File(path: "/f", diff: "--- /f\n+++ /f\n@@ -0,0 +1 @@\n+++ x\n")
        #expect(plusLine.added == 1 && plusLine.removed == 0)

        try await bus.setUpHooks(digest: decoded.digest)
        #expect(await calls.arguments == [
            ["hooks", "status", "--json"],
            ["hooks", "setup", "--plan", "--json"],
            ["hooks", "setup", "--digest", "d1"],
        ])
    }

    @Test("A child's chat reads the log its hooks reported and waits before one exists")
    func childTranscript() async throws {
        let transcript = SwarmChairTranscript()
        #expect(await transcript.poll(childLog: nil, provider: "claude") == .waiting)
        #expect(await transcript.poll(childLog: "/missing/child.jsonl", provider: "codex") == .waiting)
        #expect(await transcript.poll(childLog: "/missing/child.jsonl", provider: "gemini")
            == .notice("No transcript reader for this provider yet"))
    }

    @Test("Close uses the session adapter and closes live children before the chair")
    func closeOrderAndAdapter() async throws {
        let calls = CloseCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await calls.reply(arguments: arguments, environment: environment)
        }
        let value = session(adapter: "herdr")

        try await SwarmSessionCloser.close(value, bus: bus)

        #expect(await calls.arguments == [
            ["agents", "--json"],
            ["close", "child-b"],
            ["close", "child-a"],
            ["close", "orchestrator"],
            ["session", "archive", value.id.rawValue],
        ])
        #expect(await calls.adapters == ["herdr", "herdr", "herdr", "herdr", "tmux-solo"])
    }

    private func session(adapter: String) -> SwarmSession {
        SwarmSession(
            id: .init("01a0c8e6-7afc-7544-95cc-37c77567c776"),
            talkMode: "lane", adapter: adapter, cwd: "/tmp", createdAt: 1,
            chairProvider: "claude", chairID: nil, chairLog: nil,
            agents: 1, messages: 0, lastMessageAt: nil
        )
    }
}

private actor CloseCalls {
    private(set) var arguments: [[String]] = []
    private(set) var adapters: [String] = []

    func reply(arguments: [String], environment: [String: String]) -> ShellResult {
        self.arguments.append(arguments)
        adapters.append(environment["SWARM_ADAPTER"] ?? "")
        if arguments == ["agents", "--json"] {
            return ShellResult(status: 0, stdout: """
                {"agents":[
                  {"id":"orchestrator","role":"chair","pane":"%1","alive":true},
                  {"id":"child-b","role":"code","pane":"%2","alive":true},
                  {"id":"dead","role":"test","pane":null,"alive":false},
                  {"id":"child-a","role":"review","pane":"%3","alive":true}
                ]}
                """, stderr: "")
        }
        return ShellResult(status: 0, stdout: "", stderr: "")
    }
}
