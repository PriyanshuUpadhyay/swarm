import Foundation
import Testing
@testable import SwarmCore

@Suite("Stopped interactive chats")
struct InteractiveChatTranscriptTests {
    private let sessionID = SessionID("swarm-chat")
    private let providerID = "01a0af2b-a378-7973-8f3c-347b5fe7c818"

    @Test("A terminal chat survives every process exit")
    func exitDisposition() {
        for exit in [TerminalExit.exited(0), .exited(1), .killed(SIGTERM), .unknown] {
            #expect(InteractiveChatLifecycle.disposition(for: exit, isTerminalChat: true) == .keepChat)
        }
        #expect(InteractiveChatLifecycle.disposition(
            for: .exited(0), isTerminalChat: false
        ) == .closePane)
    }

    @Test("Presence and launch state decide the stopped label")
    func state() {
        #expect(InteractiveChatLifecycle.state(agentIsPresent: true, launchIsPending: false) == .running)
        #expect(InteractiveChatLifecycle.state(agentIsPresent: false, launchIsPending: true) == .starting)
        #expect(InteractiveChatLifecycle.state(agentIsPresent: false, launchIsPending: false) == .stopped)
        #expect(InteractiveChatLifecycle.State.stopped.label == "Stopped")
        #expect(InteractiveChatLifecycle.workspaceLabel(for: [.stopped, .stopped]) == "Stopped")
        #expect(InteractiveChatLifecycle.workspaceLabel(for: [.stopped, .running]) == nil)
        #expect(InteractiveChatLifecycle.workspaceLabel(
            for: [InteractiveChatLifecycle.State]()
        ) == nil)
    }

    @Test("Claude has a provider id before launch and Codex reports its own")
    func initialIdentity() {
        #expect(InteractiveChatLifecycle.initialProviderSessionID(
            for: .claudeCode, sessionID: sessionID
        ) == sessionID.rawValue)
        #expect(InteractiveChatLifecycle.initialProviderSessionID(
            for: .codex, sessionID: sessionID
        ) == nil)
        #expect(InteractiveChatLifecycle.resumeSessionID("  native-id  ") == "native-id")
        #expect(InteractiveChatLifecycle.resumeSessionID("  ") == nil)
    }

    @Test("Claude and Codex logs are found across named profiles")
    func paths() throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: home) }
        let claude = home.appendingPathComponent(".claude-work/projects/-tmp", isDirectory: true)
        let codex = home.appendingPathComponent(".codex-work/sessions/2026/09/17", isDirectory: true)
        try FileManager.default.createDirectory(at: claude, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: codex, withIntermediateDirectories: true)
        let claudeLog = claude.appendingPathComponent(providerID + ".jsonl")
        let codexLog = codex.appendingPathComponent("rollout-2026-09-17T12-00-00-\(providerID).jsonl")
        try Data().write(to: claudeLog)
        try Data().write(to: codexLog)

        #expect(InteractiveChatTranscript.path(
            agent: .claudeCode, providerSessionID: providerID, home: home
        )?.resolvingSymlinksInPath() == claudeLog.resolvingSymlinksInPath())
        #expect(InteractiveChatTranscript.path(
            agent: .codex, providerSessionID: providerID, home: home
        )?.resolvingSymlinksInPath() == codexLog.resolvingSymlinksInPath())
        #expect(InteractiveChatTranscript.path(
            agent: .codex, providerSessionID: "../escape", home: home
        ) == nil)
    }

    /// A Codex chair's log was read as a Claude log, so the swarm session pane showed one raw
    /// `session_meta`, `event_msg` or `response_item` row per line and no conversation.
    @Test("A Codex chair's log is read as a Codex rollout")
    func codexChairLog() async throws {
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent("chair-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: file) }
        try Data("""
            {"type":"session_meta","payload":{"id":"safe-id","cwd":"/Users/example/work"}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Testing swarm"}]}}
            {"type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","id":"answer","content":[{"type":"output_text","text":"Test received."}]}}

            """.utf8).write(to: file)
        let reader = ChairTranscriptOutput.reader(
            path: file.path, sessionID: sessionID, provider: "codex", chairID: SwarmChairID(providerID)
        )
        let transcript = try await #require(reader).read()
        #expect(transcript.messages.map(\.kind) == [.user, .assistantText])
    }

    /// The bubble a chat draws the moment the key goes down. A CLI writes the prompt to its log
    /// when its turn starts, so without this the message sat nowhere for a second or more.
    @Test("a sent message draws as a user row before the CLI writes it")
    func sentRowReadsBack() {
        let row = InteractiveChatTranscript.sentRow("say ok", sessionID: sessionID, seq: 7)
        #expect(row.kind == .user)
        #expect(row.seq == 7)
        #expect(UserTurnPrompt.text(in: row.payload) == "say ok")
    }

    @Test("Codex rollout keeps prose, thinking and every agent action nobody has coded for")
    func codexRows() throws {
        let transcript = InteractiveChatTranscript.parseCodex(
            """
            {"type":"session_meta","payload":{"id":"safe-id","cwd":"/Users/example/work"}}
            {"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"Hidden rule"}]}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /Users/example/work"},{"type":"input_text","text":"<environment_context>hidden</environment_context>"}]}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"How do CDC systems work?"}]}}
            {"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"Hidden thought"}]}}
            {"type":"response_item","payload":{"type":"custom_tool_call","name":"search","input":"{}"}}
            {"type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","id":"answer","content":[{"type":"output_text","text":"They read database changes."}]}}
            """,
            sessionID: sessionID,
            providerSessionID: providerID
        )

        // Dropped: `session_meta` is session state, the `developer` message is an instruction
        // Codex sent and nobody said, and the AGENTS.md turn is scaffolding every part of which
        // `hidesCodexSystemText` names. Kept: the thinking with a readable summary, and a
        // `custom_tool_call` nobody has written a case for, folded.
        #expect(transcript.messages.map(\.kind) == [.user, .thinking, .system, .assistantText])
        #expect(UserTurnPrompt.text(in: transcript.messages[0].payload) == "How do CDC systems work?")
        #expect(OpaqueRecord.read(transcript.messages[2].payload)?.title == "response_item")

        let answer = try #require(AgentEvent.decode(
            line: String(decoding: transcript.messages[3].payload, as: UTF8.self)
        ))
        guard case .assistantText(let block) = answer else {
            Issue.record("Expected assistant text")
            return
        }
        #expect(block.text == "They read database changes.")
    }
}
