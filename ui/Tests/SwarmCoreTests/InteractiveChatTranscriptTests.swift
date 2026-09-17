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

    @Test("Codex rollout keeps only user and assistant prose")
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

        #expect(transcript.messages.map(\.kind) == [.user, .assistantText])
        #expect(UserTurnPrompt.text(in: transcript.messages[0].payload) == "How do CDC systems work?")
        let answer = try #require(AgentEvent.decode(
            line: String(decoding: transcript.messages[1].payload, as: UTF8.self)
        ))
        guard case .assistantText(let block) = answer else {
            Issue.record("Expected assistant text")
            return
        }
        #expect(block.text == "They read database changes.")
    }
}
