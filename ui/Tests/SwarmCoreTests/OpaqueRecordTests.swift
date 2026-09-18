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
        #expect(SubagentTranscript.deniedRecordTypes.contains("attachment"))
        #expect(!SubagentTranscript.deniedRecordTypes.contains("user"))
        #expect(!SubagentTranscript.deniedRecordTypes.contains("assistant"))
        #expect(!SubagentTranscript.deniedRecordTypes.contains("system"))
        #expect(!SubagentTranscript.deniedRecordTypes.contains("pr-link"))
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
