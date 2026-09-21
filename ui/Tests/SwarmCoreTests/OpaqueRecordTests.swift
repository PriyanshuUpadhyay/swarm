import Testing
import Foundation
@testable import SwarmCore

/// Nothing a provider writes may leave the chat without a trace.
///
/// The rule these tests pin is narrow and load-bearing: a line the reader has no case for becomes
/// exactly one row, and a line that is pure app state becomes none. Before this, the reader kept
/// `user` and `assistant` and returned nothing for the other twenty record types measured across
/// 446 real captures, which is why `/compact` left a bare message and `pr-link` left nothing.
@Suite struct OpaqueRecordTests {
    private static let sessionID = SessionID("opaque-tests")

    private func chairRows(_ lines: [String]) -> [Message] {
        SubagentTranscript.parseChair(
            lines.joined(separator: "\n"), sessionID: Self.sessionID
        ).messages
    }

    @Test("a record type nobody has written code for still becomes one row")
    func anUnknownTypeSurvives() {
        let rows = chairRows([
            #"{"type":"assistant","message":{"content":[{"type":"text","text":"done"}]}}"#,
            #"{"type":"future_thing","uuid":"a","detail":{"note":"shipped today"}}"#,
        ])

        #expect(rows.count == 2)
        #expect(rows[0].kind == .assistantText)
        #expect(rows[1].kind == .system)

        let record = OpaqueRecord.read(rows[1].payload)
        #expect(record?.title == "future_thing")
        // The bytes are kept whole, because the row is the only place the reader can still see
        // what arrived.
        #expect(record?.detail.contains("shipped today") == true)
    }

    @Test("pr-link is the real new type this was measured against")
    func theMeasuredNewTypeSurvives() {
        let rows = chairRows([
            #"{"type":"pr-link","url":"https://example.test/pull/1"}"#,
        ])

        #expect(rows.count == 1)
        #expect(OpaqueRecord.read(rows[0].payload)?.title == "pr-link")
    }

    @Test("a slash command leaves a named row instead of a bare message")
    func compactLeavesARow() {
        let rows = chairRows([
            #"{"type":"user","message":{"content":"<command-name>/compact</command-name>"}}"#,
        ])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .system)
        let record = OpaqueRecord.read(rows[0].payload)
        #expect(record?.title == "local command")
        // The wrapper tag comes off, so the collapsed line shows the command and not the markup.
        #expect(record?.summary.hasPrefix("/compact") == true)
    }

    @Test("app state is denied by name and never reaches a row")
    func deniedTypesAreDropped() {
        // These four are 40% of all lines across the 446 captures. Denying them by name is what
        // keeps the opaque row from burying the conversation it is meant to complete.
        let rows = chairRows([
            #"{"type":"attachment","content":"a deferred tool list"}"#,
            #"{"type":"last-prompt","text":"..."}"#,
            #"{"type":"mode","mode":"auto"}"#,
            #"{"type":"permission-mode","mode":"acceptEdits"}"#,
        ])

        #expect(rows.isEmpty)
    }

    @Test("every denied type is one the capture actually contained")
    func theDenyListIsMeasuredNotGuessed() {
        // Guarding against a future edit that denies a type by taste. Each of these was counted;
        // `user`, `assistant`, `system` and `pr-link` are the four kept on purpose.
        #expect(TranscriptMapping.claudeDeniedTypes.contains("attachment"))
        #expect(!TranscriptMapping.claudeDeniedTypes.contains("user"))
        #expect(!TranscriptMapping.claudeDeniedTypes.contains("assistant"))
        #expect(!TranscriptMapping.claudeDeniedTypes.contains("system"))
        #expect(!TranscriptMapping.claudeDeniedTypes.contains("pr-link"))
    }

    /// The published schema is what another renderer reads, so it may not drift from the rules.
    ///
    /// Compared as parsed JSON rather than as text, because the file is written by hand and the
    /// encoder's spacing is not the thing under test.
    @Test("the published schema says what the mapping does")
    func schemaFileMatchesTheRules() throws {
        // Resolved, because Tools/test-core.sh runs the suite from a directory of symlinks and
        // #filePath then names a path whose parents are the mirror rather than the checkout.
        let url = URL(fileURLWithPath: #filePath)
            .resolvingSymlinksInPath()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("docs/transcript-blocks.json")
        let onDisk = JSONValue.parse(try Data(contentsOf: url))
        let published = JSONValue.parse(Data(TranscriptMapping.schemaJSON.utf8))

        #expect(onDisk == published, "docs/transcript-blocks.json is stale")
    }

    @Test("a hook line is not a chat row")
    func hookLinesStayOut() {
        // A permission hook is answered live, never replayed from history, so its line must not
        // arrive as a row that looks answerable. `TranscriptNoise` hides it in the view; this
        // pins that the reader still keeps the bytes rather than inventing speech for them.
        let rows = chairRows([
            #"{"type":"hook_event","hook_event_name":"PermissionRequest","tool_name":"Bash"}"#,
        ])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .system)
        #expect(OpaqueRecord.read(rows[0].payload)?.title == "hook_event")
    }

    @Test("a block type nobody has a case for keeps its own name")
    func anUnknownBlockSurvives() {
        let rows = chairRows([
            #"{"type":"assistant","message":{"content":[{"type":"server_tool_use","id":"x"}]}}"#,
        ])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .system)
        #expect(OpaqueRecord.read(rows[0].payload)?.title == "server_tool_use")
    }

    @Test("bytes that are not JSON make no row at all")
    func rubbishMakesNoRow() {
        #expect(OpaqueRecord.read(Data("not json".utf8)) == nil)
    }
}

/// The same rule on the Codex side, where the hole was larger.
///
/// The rollout reader kept `response_item/message` and nothing else, which across 1,286 real
/// rollouts is 13% of the lines. Every command Codex ran and every result it got back, half the
/// file, reached the pane as nothing at all.
@Suite struct CodexOpaqueRecordTests {
    private static let sessionID = SessionID("codex-opaque")

    private func rows(_ lines: [String]) -> [Message] {
        InteractiveChatTranscript.parseCodex(
            lines.joined(separator: "\n"),
            sessionID: Self.sessionID,
            providerSessionID: "01a0b3d5-e175-7782-9cd5-000000000000"
        ).messages
    }

    @Test("a call and its output become a paired tool row")
    func callsBecomeToolRows() {
        // The shapes are taken from a real rollout: a call carries `name`, `call_id` and
        // `arguments` as a JSON STRING, and its output carries `call_id` and `output`.
        let rows = rows([
            #"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"call_1","arguments":"{\"cmd\":\"ls\"}"}}"#,
            #"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"a.txt"}}"#,
        ])

        #expect(rows.map(\.kind) == [.toolUse, .toolResult])
        // Paired by the same id, which is what lets one row draw the call and its result together.
        #expect(rows[0].refID == "call_1")
        #expect(rows[1].refID == "call_1")
    }

    @Test("a call's arguments are unwrapped from the string Codex sends")
    func argumentsAreParsed() throws {
        let rows = rows([
            #"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"c","arguments":"{\"cmd\":\"ls -la\"}"}}"#,
        ])

        let row = try #require(rows.first)
        let json = try #require(JSONValue.parse(row.payload))
        let block = try #require(json["message"]?["content"]?.arrayValue?.first)
        // An escaped blob in the row would make the tool row unreadable, so the string is parsed
        // back into an object on the way in.
        #expect(block["input"]?["cmd"]?.stringValue == "ls -la")
        #expect(block["name"]?.stringValue == "exec_command")
    }

    @Test("a response item nobody has a case for still becomes one row")
    func unknownItemsSurvive() {
        let rows = rows([
            #"{"type":"response_item","payload":{"type":"web_search_call","id":"w1","query":"swift"}}"#,
        ])

        #expect(rows.map(\.kind) == [.system])
        #expect(OpaqueRecord.read(rows[0].payload)?.title == "response_item")
    }

    /// The owner saw `world_state`, `token_usage_record` and three `developer` messages drawn as
    /// rows of bare type names around a two-line answer.
    @Test("session state and injected instructions make no row")
    func deniedCodexTypesAreDropped() {
        // `event_msg` repeats what the response items already say and counts tokens, 1,692 lines
        // of it. `reasoning` is dropped for a different reason: all 564 measured are encrypted
        // with an empty summary, so a row for one would hold nothing.
        let rows = rows([
            #"{"type":"event_msg","payload":{"type":"token_count","total":900}}"#,
            #"{"type":"event_msg","payload":{"type":"agent_message","message":"hello"}}"#,
            #"{"type":"turn_context","cwd":"/tmp"}"#,
            #"{"type":"session_meta","id":"s"}"#,
            #"{"type":"world_state","full":true,"state":{}}"#,
            #"{"type":"token_usage_record","usage":{}}"#,
            #"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<skills_instructions>"}]}}"#,
            #"{"type":"response_item","payload":{"type":"reasoning","summary":[],"encrypted_content":"x"}}"#,
        ])

        #expect(rows.isEmpty)
    }

    /// The other half of the deny-list. A record type nobody has coded for must be visible, or the
    /// next `world_state` disappears for a month before anybody notices it is missing.
    @Test("a record type nobody has a case for still becomes one row")
    func unknownRecordsSurvive() {
        let rows = rows([
            #"{"type":"future_record","state":{"a":1}}"#,
        ])

        #expect(rows.map(\.kind) == [.system])
        #expect(OpaqueRecord.read(rows[0].payload)?.title == "future_record")
    }

    @Test("an ordinary message is still an ordinary message")
    func messagesAreUnchanged() {
        let rows = rows([
            #"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"run it"}]}}"#,
            #"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}}"#,
        ])

        #expect(rows.map(\.kind) == [.user, .assistantText])
    }
}
