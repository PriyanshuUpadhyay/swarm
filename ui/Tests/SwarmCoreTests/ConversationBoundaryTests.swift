import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Conversation boundary")
struct ConversationBoundaryTests {
    private func record(_ event: TranscriptEvent) -> TranscriptRecord {
        TranscriptRecord(event: event, rawLine: "")
    }

    private func hook(_ name: String) -> TranscriptRecord {
        record(.hookResult(
            kind: "hook_success", hookEvent: "SessionStart", hookName: name, toolCallID: "",
            exitCode: 0, meta: Meta()
        ))
    }

    private let ordinaryMessage = TranscriptRecord(
        event: .userMessageChunk(text: "Fix the bug", meta: Meta()), rawLine: ""
    )

    @Test("A SessionStart:clear hook result marks a clear")
    func clearHook() {
        #expect(ConversationBoundary.isClear([hook("SessionStart:clear"), ordinaryMessage]))
    }

    @Test("A /clear command record marks a clear")
    func clearCommand() {
        let command = record(.systemMessage(
            kind: "command",
            text: "<command-name>/clear</command-name>\n<command-message>clear</command-message>",
            meta: Meta()
        ))
        #expect(ConversationBoundary.isClear([ordinaryMessage, command]))
    }

    @Test("A startup hook, another command, and an ordinary log are not clears")
    func notClears() {
        let compact = record(.systemMessage(
            kind: "command", text: "<command-name>/compact</command-name>", meta: Meta()
        ))
        #expect(!ConversationBoundary.isClear([hook("SessionStart:startup"), ordinaryMessage]))
        #expect(!ConversationBoundary.isClear([compact, ordinaryMessage]))
        #expect(!ConversationBoundary.isClear([ordinaryMessage]))
    }

    @Test("A marker after the first 30 records does not count")
    func lateMarker() {
        let late = Array(repeating: ordinaryMessage, count: 30) + [hook("SessionStart:clear")]
        #expect(!ConversationBoundary.isClear(late))
    }
}
