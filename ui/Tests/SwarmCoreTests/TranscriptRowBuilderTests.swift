import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Transcript rows")
struct TranscriptRowBuilderTests {
    @Test("Rows use conversation labels")
    func labels() {
        #expect(TranscriptRow(kind: .user, text: "", eventID: "1").label(chair: "claude") == "You")
        #expect(TranscriptRow(kind: .assistant, text: "", eventID: "2").label(chair: "claude") == "Claude")
        #expect(TranscriptRow(kind: .assistant, text: "", eventID: "3").label(chair: nil) == "Chair")
        #expect(TranscriptRow(kind: .toolUse, text: "", eventID: "4").label(chair: nil) == "Tool")
    }

    @Test("New rows keep following until the user scrolls away")
    func tailFollow() {
        #expect(TranscriptTail.follows(current: true, atBottom: false, userScrolled: false))
        #expect(!TranscriptTail.follows(current: true, atBottom: false, userScrolled: true))
        #expect(!TranscriptTail.follows(current: false, atBottom: true, userScrolled: false))
        #expect(TranscriptTail.follows(current: false, atBottom: true, userScrolled: true))
    }

    @Test("Agent chunks with one UUID form one row")
    func agentChunksFold() {
        let first = TranscriptEvent.agentMessageChunk(
            text: "Hello ", meta: Meta(agentSessionID: "s1", uuid: "turn-1", timestamp: "t1")
        )
        let second = TranscriptEvent.agentMessageChunk(
            text: "world!", meta: Meta(agentSessionID: "s1", uuid: "turn-1", timestamp: "t2")
        )
        let rows = TranscriptRowBuilder.rows(from: [first, second])
        #expect(rows == [TranscriptRow(kind: .assistant, text: "Hello world!", eventID: "turn-1")])
    }

    @Test("A different UUID starts a new row")
    func distinctChunksStayApart() {
        let first = TranscriptEvent.agentMessageChunk(text: "A", meta: Meta(uuid: "one"))
        let second = TranscriptEvent.agentMessageChunk(text: "B", meta: Meta(uuid: "two"))
        #expect(TranscriptRowBuilder.rows(from: [first, second]).map(\.text) == ["A", "B"])
    }

    @Test("Known tool output joins its call, while permission and errors remain distinct")
    func actionRows() {
        let meta = Meta(uuid: "event-1")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "call-1", name: "ls", input: .object([:]), status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "call-1", status: .completed, content: "one\ntwo", meta: meta),
            .elicitation(toolCallID: "ask-1", questions: [Question(question: "Proceed?")], meta: meta),
            .error(message: "failed", meta: meta),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .permission, .error])
        #expect(Set(rows.map(\.eventID)).count == rows.count)
        #expect(rows[0].eventID == "call-1:call")
        #expect(rows[0].tool?.output == "one\ntwo")
        #expect(rows[0].tool?.state == .finished)
    }

    @Test("Tool calls show a short label and keep input in detail")
    func toolSummary() {
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "one", name: "exec", input: .object([
                "description": .string("List files"), "command": .string("ls\npwd")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "two", name: "exec", input: .object([
                "command": .string("pwd\nls")
            ]), status: .pending, meta: Meta()),
        ])
        #expect(rows.map(\.text) == ["exec · List files", "exec · pwd"])
        #expect(rows[0].detail?.contains("\"command\"") == true)
    }

    @Test("Codex and AGY command shapes give distinct collapsed labels")
    func providerCommandLabels() {
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "codex", name: "exec", input: .object([
                "cmd": .string("git status")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "agy", name: "exec", input: .object([
                "CommandLine": .string("pwd")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "file", name: "view_file", input: .object([
                "TargetFile": .string("/work/source.swift")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "summary", name: "view_file", input: .object([
                "toolSummary": .string("Read source"), "AbsolutePath": .string("/work/source.swift")
            ]), status: .pending, meta: Meta()),
        ])
        #expect(rows.map(\.text) == [
            "exec · git status", "exec · pwd", "view_file · source.swift", "view_file · Read source"
        ])
    }

    @Test("Low value notices and empty system rows are hidden by default")
    func hiddenRows() {
        let rows = TranscriptRowBuilder.rows(from: [
            .sessionInfo(kind: "title", value: "Chat", meta: Meta()),
            .hookResult(kind: "hook_success", hookEvent: "", hookName: "check", toolCallID: "", exitCode: 0, meta: Meta()),
            .systemMessage(kind: "system", text: "  ", meta: Meta()),
            .systemMessage(kind: "system", text: "Ready", meta: Meta()),
        ])
        #expect(rows.map(\.isHiddenByDefault) == [true, true, true, false])
    }

    @Test("Interleaved calls keep their own output and their call order")
    func interleavedCalls() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "a", name: "exec", input: .object(["command": .string("first")]), status: .pending, meta: meta),
            .toolCall(toolCallID: "b", name: "exec", input: .object(["command": .string("second")]), status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "b", status: .completed, content: "B result", meta: meta),
            .toolCallUpdate(toolCallID: "a", status: .completed, content: "A result", meta: meta),
        ])
        #expect(rows.map(\.eventID) == ["a:call", "b:call"])
        #expect(rows.map { $0.tool?.output } == ["A result", "B result"])
        #expect(rows.map { $0.tool?.command } == ["first", "second"])
    }

    @Test("Updates are snapshots and confirmed diffs join across event order")
    func snapshotsAndDiffs() {
        let meta = Meta(agentSessionID: "s")
        let before = diff("a", path: "/work/first.txt")
        let after = diff("a", path: "/work/second.txt")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCallUpdate(toolCallID: "a", status: .pending, content: "partial", meta: meta),
            before,
            .toolCall(toolCallID: "a", name: "edit", input: .object(["file_path": .string("/work/first.txt")]), status: .pending, meta: meta),
            .agentMessageChunk(text: "I edited it.", meta: Meta(uuid: "answer")),
            .toolCallUpdate(toolCallID: "a", status: .completed, content: "final output", meta: meta),
            after,
        ])
        #expect(rows.map(\.kind) == [.toolUse, .assistant])
        #expect(rows[0].tool?.output == "final output")
        #expect(rows[0].tool?.state == .finished)
        #expect(rows[0].tool?.diffs.map(\.path) == ["/work/first.txt", "/work/second.txt"])
        #expect(rows[0].tool?.path == "/work/first.txt")
        let sourceInput = try? JSONDecoder().decode(
            JSONElement.self, from: Data((rows[0].detail ?? "").utf8)
        )
        #expect(sourceInput == .object(["file_path": .string("/work/first.txt")]))
        #expect(rows[1].text == "I edited it.")
    }

    @Test("A page without a call keeps each result and diff visible")
    func missingCallAndPageBoundary() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "a", name: "edit", input: .null, status: .pending, meta: meta),
            .page(start: 0, end: 10),
            .toolCallUpdate(toolCallID: "a", status: .completed, content: "page result", meta: meta),
            diff("a", path: "/work/page.txt"),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .toolResult, .diff])
        #expect(rows[0].tool?.state == .waiting)
        #expect(rows[0].tool?.output == nil)
        #expect(rows[1].text == "page result")
        #expect(rows[2].diff?.path == "/work/page.txt")
    }

    @Test("Missing and ambiguous IDs do not attach results to a call")
    func ambiguousIDs() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "dup", name: "first", input: .null, status: .pending, meta: meta),
            .toolCall(toolCallID: "dup", name: "second", input: .null, status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "dup", status: .completed, content: "ambiguous", meta: meta),
            .toolCall(toolCallID: "", name: "third", input: .null, status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "", status: .completed, content: "missing ID", meta: meta),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .toolUse, .toolResult, .toolUse, .toolResult])
        #expect(Set(rows.map(\.eventID)).count == rows.count)
        #expect(rows[0].tool?.output == nil)
        #expect(rows[1].tool?.output == nil)
        #expect(rows[2].text == "ambiguous")
        #expect(rows[2].toolStatus == .completed)
        #expect(rows[4].text == "missing ID")
        #expect(rows[4].toolStatus == .completed)
    }

    @Test("A blank session cannot resolve a reused tool ID")
    func blankSessionIsAmbiguous() {
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "same", name: "one", input: .null, status: .pending, meta: Meta(agentSessionID: "one")),
            .toolCall(toolCallID: "same", name: "two", input: .null, status: .pending, meta: Meta(agentSessionID: "two")),
            .toolCallUpdate(toolCallID: "same", status: .failed, content: "unknown owner", meta: Meta()),
            diff("same", path: "/work/ambiguous.txt", session: ""),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .toolUse, .toolResult, .diff])
        #expect(rows[0].tool?.diffs.isEmpty == true)
        #expect(rows[1].tool?.diffs.isEmpty == true)
        #expect(rows[2].toolStatus == .failed)
        #expect(rows[2].text == "unknown owner")
    }

    @Test("Session and turn boundaries limit matching when IDs repeat")
    func scopeBoundaries() {
        let first = Meta(agentSessionID: "one")
        let second = Meta(agentSessionID: "two")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "same", name: "first", input: .null, status: .pending, meta: first),
            .toolCallUpdate(toolCallID: "same", status: .completed, content: "one", meta: first),
            .toolCall(toolCallID: "same", name: "second", input: .null, status: .pending, meta: second),
            .toolCallUpdate(toolCallID: "same", status: .failed, content: "two", meta: second),
            .turnEnded(durationMs: nil, reason: .completed, meta: second),
            .toolCall(toolCallID: "same", name: "third", input: .null, status: .pending, meta: second),
            .toolCallUpdate(toolCallID: "same", status: .completed, content: "three", meta: second),
        ])
        #expect(rows.filter { $0.kind == .toolUse }.map { $0.tool?.output } == ["one", "two", "three"])
        #expect(rows.filter { $0.kind == .toolUse }.map { $0.tool?.state } == [.finished, .failed, .finished])
        #expect(Set(rows.map(\.eventID)).count == rows.count)
    }

    @Test("Failed, aborted, and completed turns give unresolved calls clear states")
    func endStates() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "a", name: "a", input: .null, status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "a", status: .failed, content: "error text", meta: meta),
            .toolCall(toolCallID: "b", name: "b", input: .null, status: .pending, meta: meta),
            .turnEnded(durationMs: nil, reason: .aborted, meta: meta),
            .toolCall(toolCallID: "c", name: "c", input: .null, status: .pending, meta: meta),
            .turnEnded(durationMs: nil, reason: .completed, meta: meta),
            .toolCall(toolCallID: "d", name: "d", input: .null, status: .pending, meta: meta),
        ])
        #expect(rows.filter { $0.kind == .toolUse }.map { $0.tool?.state } == [.failed, .interrupted, .unreported, .waiting])
        #expect(rows.first?.tool?.output == "error text")
    }

    private func diff(_ id: String, path: String, session: String = "s") -> TranscriptEvent {
        .decode(line: """
        {"type":"tool_diff","tool_call_id":"\(id)","path":"\(path)","hunks":[],"meta":{"session_id":"\(session)"}}
        """)
    }
}

@Suite("Transcript debug data")
struct TranscriptDebugDataTests {
    @Test("Keeps raw lines and labels rendered, hidden, and omitted events")
    func entries() {
        let records = [
            TranscriptRecord(
                event: .userMessageChunk(text: "Hello", meta: Meta(uuid: "one")),
                rawLine: #"{"type":"user_message_chunk","text":"Hello"}"#
            ),
            TranscriptRecord(
                event: .sessionInfo(kind: "title", value: "Chat", meta: Meta(uuid: "two")),
                rawLine: #"{"type":"session_info","kind":"title","value":"Chat"}"#
            ),
            TranscriptRecord(
                event: .turnStarted(meta: Meta(uuid: "three")),
                rawLine: #"{"type":"turn_started"}"#
            ),
        ]

        let entries = TranscriptDebugData.entries(from: records)
        #expect(entries.map(\.rowKind) == ["user", "hidden", "no row"])
        #expect(entries[0].rawLine == records[0].rawLine)
        #expect(entries[0].displayText.contains("\n"))
    }

    @Test("A turn is active from the user's last message until a turn ends")
    func chairTurn() {
        let question = TranscriptRow(kind: .user, text: "hi", eventID: "question")
        let answer = TranscriptRow(kind: .assistant, text: "hello", eventID: "answer")
        var ended = TranscriptRow(kind: .result, text: "completed", eventID: "ended")
        ended.endsTurn = true
        let decision = TranscriptRow(kind: .result, text: "allow", eventID: "decision")

        #expect(!ChairTurn.isActive([]))
        #expect(ChairTurn.isActive([question, answer]))
        #expect(ChairTurn.isActive([question, decision]))
        #expect(!ChairTurn.isActive([question, answer, ended]))
        #expect(ChairTurn.isActive([question, answer, ended, question]))
    }
}
