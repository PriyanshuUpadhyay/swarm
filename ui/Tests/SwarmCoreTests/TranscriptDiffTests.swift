import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Saved transcript diffs")
struct TranscriptDiffTests {
    @Test("Saved Codex and AGY edits retain their inputs and results without a false diff",
          arguments: ["codex", "agy"])
    func providerEditResults(format: String) async throws {
        let binary = try #require(ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"])
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let filename = format == "codex" ? "codex-exec-edit.jsonl" : "agy-edit.jsonl"
        let fixture = repo.appendingPathComponent("packages/transcript/src/fixtures/\(filename)")
        let source = try String(contentsOf: fixture, encoding: .utf8).split(separator: "\n")
        let saved = try source.map {
            try #require(JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any])
        }
        let process = TranscriptToolProcess(binary: URL(fileURLWithPath: binary), format: format, log: fixture, follow: false)
        var records: [TranscriptRecord] = []
        for try await record in process.stream { records.append(record) }
        #expect(records.count == 2)
        guard case .toolCall(let callID, let name, let input, _, _) = records.first?.event,
              case .toolCallUpdate(let resultID, let status, let content, _) = records.last?.event else {
            Issue.record("Expected a tool call followed by its original result")
            return
        }
        #expect(callID == resultID)
        #expect(status == .completed)
        if format == "codex" {
            #expect(name == "exec")
            let call = try #require(saved[0]["payload"] as? [String: Any])
            #expect(input == .string(try #require(call["input"] as? String)))
            let result = try #require(saved[1]["payload"] as? [String: Any])
            let output = try #require(result["output"] as? [[String: String]])
            #expect(content == output.compactMap { $0["text"] }.joined())
            #expect(content.hasSuffix("{}"))
        } else {
            #expect(name == "replace_file_content")
            let calls = try #require(saved[0]["tool_calls"] as? [[String: Any]])
            let data = try JSONSerialization.data(withJSONObject: try #require(calls.first?["args"]))
            #expect(input == (try JSONDecoder().decode(JSONElement.self, from: data)))
            #expect(content == (try #require(saved[1]["content"] as? String)))
            #expect(content.contains("@@ -10,7 +10,7 @@\n"))
            #expect(content.contains("-old line\n+new line\n"))
        }
        let rows = TranscriptRowBuilder.rows(from: records)
        #expect(rows.map(\.kind) == [.toolUse])
        #expect(rows[0].eventID == callID + ":call")
        #expect(rows[0].tool?.output == content)
        #expect(rows[0].tool?.input == input)
        #expect(rows[0].tool?.state == .finished)
        #expect(rows[0].tool?.diffs.isEmpty == true)
        #expect(rows.allSatisfy { $0.diff == nil })
    }

    @Test("Real Zig output decodes and retains the original tool result")
    func savedEdit() async throws {
        let binary = try #require(ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"])
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let fixture = repo.appendingPathComponent("packages/transcript/src/fixtures/claude-edit.jsonl")
        let process = TranscriptToolProcess(binary: URL(fileURLWithPath: binary), format: "claude", log: fixture, follow: false)
        var records: [TranscriptRecord] = []
        for try await record in process.stream { records.append(record) }
        #expect(records.count == 2)
        guard case .toolCallUpdate(let id, let status, let text, _) = records.first?.event,
              case .toolDiff(let diff, let meta) = records.last?.event else {
            Issue.record("Expected a tool result followed by a structured diff")
            return
        }
        #expect(id == diff.toolCallID)
        #expect(status == .completed)
        #expect(text == "The file /workspace/example.txt has been updated successfully.")
        #expect(meta.uuid == "fixture-result")
        let rows = TranscriptRowBuilder.rows(from: records)
        #expect(rows.map(\.kind) == [.toolResult, .diff])
        #expect(rows[0].text == text)
        #expect(rows[1].text == "example.txt · +1 −0")
        #expect(rows[1].detail == "/workspace/example.txt")
        #expect(rows[1].eventID != rows[0].eventID)
        let preview = TranscriptDiffPreview(try #require(rows[1].diff))
        #expect(preview.patch.contains("@@ -445,6 +445,7 @@\n"))
        #expect(preview.patch.contains("+added line\n"))
        #expect(preview.notice == nil)
    }

    @Test("Preview caps are explicit and leave canonical lines and paths intact")
    func boundedPreview() throws {
        let path = "/workspace/quote\"\n<script>.swift"
        let longLine = "+" + String(repeating: "λ", count: 900)
        let lines = [longLine] + Array(repeating: "+new", count: 204)
        let payload: [String: Any] = [
            "type": "tool_diff", "tool_call_id": "edit-2", "path": path,
            "hunks": [["old_start": 0, "old_lines": 0, "new_start": 1, "new_lines": 205, "lines": lines]],
        ]
        let data = try JSONSerialization.data(withJSONObject: payload)
        guard case .toolDiff(let diff, _) = TranscriptEvent.decode(line: String(decoding: data, as: UTF8.self)) else {
            Issue.record("Expected structured diff")
            return
        }
        let preview = TranscriptDiffPreview(diff)
        #expect(preview.omittedLines == 5)
        #expect(preview.shortenedLines == 1)
        #expect(preview.notice?.contains("5 patch lines omitted") == true)
        #expect(preview.patch.contains("@@ -0,0 +1,200 @@"))
        #expect(preview.patch.contains("quote\\\"\\n<script>.swift"))
        #expect(diff.path == path)
        #expect(diff.hunks[0].lines == lines)
        let full = TranscriptDiffPreview(diff, full: true)
        #expect(full.notice == nil)
        #expect(full.patch.contains("@@ -0,0 +1,205 @@"))
        #expect(full.patch.contains(longLine))
        #expect(full.patch.components(separatedBy: "\n").filter { $0 == "+new" }.count == 204)
    }

    @Test("Malformed new diff events remain visible as unknown data")
    func unknownDiff() {
        let raw = #"{"type":"tool_diff","tool_call_id":"x","path":"a","hunks":"invalid"}"#
        #expect(TranscriptEvent.decode(line: raw) == .unknown(raw: raw))
    }

    @Test("Edit labels shorten only the display path")
    func toolLabel() throws {
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "edit", name: "Edit", input: .object([
                "file_path": .string("/workspace/Sources/main.swift")
            ]), status: .pending, meta: Meta()),
        ])
        #expect(rows[0].text == "Edit · main.swift")
        let detail = try #require(rows[0].detail)
        let input = try JSONDecoder().decode(JSONElement.self, from: Data(detail.utf8))
        #expect(input == .object(["file_path": .string("/workspace/Sources/main.swift")]))
    }
}
