import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Session detail")
struct SwarmSessionDetailTests {
    @Test("Tmux-solo and Herdr agents with panes can attach")
    func adapterPolicy() {
        let agent = SwarmAgent(id: .init("orchestrator"), role: "orchestrator", pane: "%1", alive: true)
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "tmux-solo"), agent: agent) == nil)
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "tmux"), agent: agent)
            == "This session's host has no attach")
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "herdr"), agent: agent) == nil)
        var unstarted = agent
        unstarted.pane = nil
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "tmux-solo"), agent: unstarted)
            == "This agent has no pane")
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "herdr"), agent: unstarted)
            == "This agent has no pane")
    }

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
        #expect(cells.map(\.kind) == [.attach, .attach])
        value.adapter = "herdr"
        #expect(SwarmPanePolicy.cells(session: value, agents: agents).first?.kind
            == .attach)
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
            == .notice("No transcript reader for this provider yet"))
        #expect(await reader.poll(session: value)
            == .notice("No transcript reader for this provider yet"))
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

    @Test("Attach keeps the selected session and adapter")
    func attachEnvironment() {
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, _, _, _, _, _ in ShellResult(status: 0, stdout: "", stderr: "")
        }
        let value = session(adapter: "tmux")
        let command = SwarmPanePolicy.attachCommand(bus: bus, session: value, agent: .init("coder"))
        #expect(command.arguments == ["attach", "coder"])
        #expect(command.environment["SWARM_SESSION_ID"] == value.id.rawValue)
        #expect(command.environment["SWARM_ADAPTER"] == "tmux")
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
