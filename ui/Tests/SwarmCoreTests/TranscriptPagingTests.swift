import Foundation
import Testing
@testable import SwarmCore
import TranscriptTool

@Suite("Transcript paging")
struct TranscriptPagingTests {
    private func line(_ number: Int) -> String {
        #"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"message-"# + String(number) + #""}]}}"# + "\n"
    }

    private func log() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("paging-\(UUID().uuidString).jsonl")
    }

    private func texts(_ records: [TranscriptRecord]) -> [String] {
        records.compactMap { if case .userMessageChunk(let text, _) = $0.event { text } else { nil } }
    }

    @Test("Initial read is bounded; older pages preserve every message and row identity")
    func pages() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let file = log()
        defer { try? FileManager.default.removeItem(at: file) }
        try Data((0..<245).map(line).joined().utf8).write(to: file)
        let reader = ToolTranscriptReader(binary: binary, format: "codex", log: file)
        let first = try await reader.read()
        #expect(texts(first) == (145..<245).map { "message-\($0)" })
        #expect(await reader.hasOlder)
        let firstRows = TranscriptRowBuilder.rows(from: first)
        let second = try await reader.loadOlder()
        #expect(texts(second) == (45..<245).map { "message-\($0)" })
        let start = await reader.historyStartIndex
        let secondRows = TranscriptRowBuilder.rows(from: second, indexOffset: start)
        #expect(Array(secondRows.suffix(firstRows.count)) == firstRows)
        let all = try await reader.loadOlder()
        #expect(texts(all) == (0..<245).map { "message-\($0)" })
        #expect(!(await reader.hasOlder))
        let again = try await reader.loadOlder()
        #expect(texts(again) == texts(all))
    }

    @Test("Appending during a page read neither loses nor duplicates output")
    func appendWhilePaging() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let file = log()
        defer { try? FileManager.default.removeItem(at: file) }
        try Data((0..<200).map(line).joined().utf8).write(to: file)
        let reader = ToolTranscriptReader(binary: binary, format: "codex", log: file)
        _ = try await reader.read()
        async let page = reader.loadOlder()
        let handle = try FileHandle(forWritingTo: file)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(line(200).utf8))
        try handle.close()
        _ = try await page
        var records = try await reader.read()
        for _ in 0..<40 where texts(records).last != "message-200" {
            try await Task.sleep(for: .milliseconds(50))
            records = try await reader.read()
        }
        #expect(texts(records) == (0...200).map { "message-\($0)" })
        #expect(!(await reader.hasOlder))
    }

    @Test("A failed history read retains the cursor and can be retried")
    func retryHistory() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let file = log()
        defer { try? FileManager.default.removeItem(at: file) }
        let data = Data((0..<150).map(line).joined().utf8)
        try data.write(to: file)
        let reader = ToolTranscriptReader(binary: binary, format: "codex", log: file)
        _ = try await reader.read()
        let cursor = await reader.olderOffset
        try FileManager.default.removeItem(at: file)
        await #expect(throws: (any Error).self) { try await reader.loadOlder() }
        #expect(await reader.olderOffset == cursor)
        #expect(texts(try await reader.read()).count == 100)
        try data.write(to: file)
        let all = try await reader.loadOlder()
        #expect(texts(all) == (0..<150).map { "message-\($0)" })
    }

    @Test("Empty logs complete the initial window without the three-second fallback")
    func empty() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let file = log()
        defer { try? FileManager.default.removeItem(at: file) }
        try Data().write(to: file)
        let reader = ToolTranscriptReader(binary: binary, format: "claude", log: file)
        let start = ContinuousClock.now
        #expect(try await reader.read().isEmpty)
        #expect(start.duration(to: .now) < .seconds(2))
        #expect(!(await reader.hasOlder))
    }

    @Test("Page size counts source lines and keeps all events from a large record")
    func multipleEvents() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let file = log()
        defer { try? FileManager.default.removeItem(at: file) }
        let content = (0..<120).map { ["type": "tool_use", "id": "call-\($0)", "name": "Read", "input": [:]] as [String: Any] }
        let object: [String: Any] = ["type": "assistant", "sessionId": "test", "uuid": "many", "message": ["content": content]]
        var data = try JSONSerialization.data(withJSONObject: object)
        data.append(0x0a)
        try data.write(to: file)
        let reader = ToolTranscriptReader(binary: binary, format: "claude", log: file, messageLimit: 1)
        let records = try await reader.read()
        #expect(records.count == 120)
        #expect(!(await reader.hasOlder))
    }

    @Test("Tool calls join across loaded pages")
    func boundaryTool() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let file = log()
        defer { try? FileManager.default.removeItem(at: file) }
        let old = #"{"type":"response_item","payload":{"type":"function_call","call_id":"boundary","name":"shell","arguments":"{\"command\":\"true\"}"}}"# + "\n"
        let result = #"{"type":"response_item","payload":{"type":"function_call_output","call_id":"boundary","output":"done"}}"# + "\n"
        try Data((old + result).utf8).write(to: file)
        let reader = ToolTranscriptReader(binary: binary, format: "codex", log: file, messageLimit: 1)
        let initial = TranscriptRowBuilder.rows(from: try await reader.read())
        #expect(initial.count == 1)
        #expect(initial.first?.kind == .toolResult)
        let all = TranscriptRowBuilder.rows(from: try await reader.loadOlder())
        #expect(all.count == 1)
        #expect(all.first?.tool?.output == "done")
        #expect(all.first?.tool?.state == .finished)
    }
}
