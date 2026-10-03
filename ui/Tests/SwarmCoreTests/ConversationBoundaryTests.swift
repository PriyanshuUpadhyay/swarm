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

    @Test("The clear preamble goes only before the first user row, and command output with text stays")
    func clearPreamble() {
        let system = { (text: String) in TranscriptRow(kind: .system, text: text, eventID: text) }
        let caveat = system("<local-command-caveat>Caveat</local-command-caveat>")
        let modelOutput = system("<local-command-stdout>Set model to opus</local-command-stdout>")
        let fresh = TranscriptRow(kind: .user, text: "Start fresh", eventID: "u2")
        let rows = ConversationBoundary.withoutClearPreamble([
            caveat, system("<command-name>/clear</command-name>"),
            system("<local-command-stdout></local-command-stdout>"), modelOutput, fresh, caveat,
        ])
        #expect(rows == [modelOutput, fresh, caveat])
    }

    @Test("A /clear command that took its empty output still drops with its caveat")
    func clearPreambleWithLinkedOutput() throws {
        let built = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command_output", text: "<local-command-caveat>Caveat</local-command-caveat>", meta: Meta(uuid: "caveat")),
            .systemMessage(kind: "command", text: "<command-name>/clear</command-name>", meta: Meta(uuid: "clear", parentUUID: "caveat")),
            .systemMessage(kind: "command_output", text: "<local-command-stdout></local-command-stdout>", meta: Meta(uuid: "stdout", parentUUID: "clear")),
            .userMessageChunk(text: "Start fresh", meta: Meta(uuid: "prompt")),
        ])
        try #require(built.count == 3)
        #expect(built[1].command?.output == "")
        #expect(ConversationBoundary.withoutClearPreamble(built).map(\.kind) == [.user])
    }

    @Test("A marker after the first 30 records does not count")
    func lateMarker() {
        let late = Array(repeating: ordinaryMessage, count: 30) + [hook("SessionStart:clear")]
        #expect(!ConversationBoundary.isClear(late))
    }
}
