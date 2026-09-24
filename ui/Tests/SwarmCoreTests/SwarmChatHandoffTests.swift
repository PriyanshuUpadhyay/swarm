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
