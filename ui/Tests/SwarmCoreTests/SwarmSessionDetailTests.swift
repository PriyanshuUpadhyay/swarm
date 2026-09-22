import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Session detail")
struct SwarmSessionDetailTests {
    @Test("Only tmux-solo agents with panes can attach")
    func adapterPolicy() {
        let agent = SwarmAgent(id: .init("orchestrator"), role: "orchestrator", pane: "%1", alive: true)
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "tmux-solo"), agent: agent) == nil)
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "tmux"), agent: agent)
            == "This session's host has no attach")
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "herdr"), agent: agent)
            == "This session's host has no attach")
        var unstarted = agent
        unstarted.pane = nil
        #expect(SwarmPanePolicy.unavailableReason(session: session(adapter: "tmux-solo"), agent: unstarted)
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
            == .notice("This session's host has no attach"))
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
        guard case .rows(let rows) = snapshot else {
            Issue.record("The tool did not return rows")
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

    private func session(adapter: String) -> SwarmSession {
        SwarmSession(
            id: .init("01a0c8e6-7afc-7544-95cc-37c77567c776"),
            talkMode: "lane", adapter: adapter, cwd: "/tmp", createdAt: 1,
            chairProvider: "claude", chairID: nil, chairLog: nil,
            agents: 1, messages: 0, lastMessageAt: nil
        )
    }
}
