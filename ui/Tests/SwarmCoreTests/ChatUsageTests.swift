import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Reported chat usage")
struct ChatUsageTests {
    @Test("Snapshots do not add costs, and context resets do not erase session cost")
    func snapshotsAndScope() {
        var usage = ChatUsage()
        #expect(usage.remainingPercent == nil)
        #expect(usage.costLabel == nil)
        usage.ingest(.decode(line: #"{"type":"session_info","kind":"model","value":"one"}"#))
        let context = TranscriptEvent.decode(line: #"{"type":"usage","source":"codex","kind":"context","context_tokens":80,"context_capacity_tokens":100,"input_tokens":70,"cache_read_tokens":60,"output_tokens":10}"#)
        usage.ingest(context)
        #expect(usage.remainingPercent == 20)
        let cost = TranscriptEvent.decode(line: #"{"type":"usage","source":"claude","kind":"cost","cost_usd":4.5,"cost_completeness":"partial"}"#)
        usage.ingest(cost)
        usage.ingest(cost)
        #expect(usage.cost?.costUSD == 4.5)
        #expect(usage.costLabel?.hasPrefix("Partial est.") == true)
        usage.ingest(.decode(line: #"{"type":"system_message","kind":"compaction","text":"summary"}"#))
        #expect(usage.context == nil)
        #expect(usage.contextNotice.contains("compaction"))
        #expect(usage.cost?.costUSD == 4.5)
        usage.ingest(context)
        usage.ingest(.decode(line: #"{"type":"session_info","kind":"model","value":"two"}"#))
        #expect(usage.context == nil)
        #expect(ChatUsage().cost == nil)
    }

    @Test("Missing, invalid and reported zero values remain distinct")
    func missingAndZero() {
        var usage = ChatUsage()
        usage.ingest(.decode(line: #"{"type":"usage","source":"claude","kind":"context","context_tokens":0,"context_capacity_tokens":null}"#))
        #expect(usage.context?.contextTokens == 0)
        #expect(usage.remainingPercent == nil)
        usage.ingest(.decode(line: #"{"type":"usage","source":"codex","kind":"context","context_tokens":-1,"context_capacity_tokens":0}"#))
        #expect(usage.context?.contextTokens == nil)
        #expect(usage.remainingPercent == nil)
        usage.ingest(.decode(line: #"{"type":"usage","source":"claude","kind":"cost","cost_usd":0,"cost_completeness":"complete"}"#))
        #expect(usage.cost?.costUSD == 0)
        #expect(usage.costLabel?.hasPrefix("Est.") == true)
        usage.ingest(.decode(line: #"{"type":"usage","source":"claude","kind":"cost","cost_usd":-1}"#))
        #expect(usage.costLabel == nil)
    }

    @Test("Live history retains metadata; initial page honestly reports its omission")
    func liveTrimAndInitialTail() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("usage-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        let initial = #"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":80,"output_tokens":20,"total_tokens":120},"total_token_usage":{"input_tokens":9000},"model_context_window":200}}}"# + "\n"
        try Data(initial.utf8).write(to: log)
        let reader = ToolTranscriptReader(binary: binary, format: "codex", log: log)
        _ = try await reader.read()
        #expect(await reader.usage.remainingPercent == 40)
        #expect(await reader.usage.context?.inputTokens == 100)
        let events = (0..<600).map {
            #"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"event"# + String($0) + #""}]}}"#
        }.joined(separator: "\n") + "\n"
        let file = try FileHandle(forWritingTo: log)
        try file.seekToEnd()
        try file.write(contentsOf: Data(events.utf8))
        try file.close()
        var records: [TranscriptRecord] = []
        for _ in 0..<80 {
            if let next = try await reader.readIfChanged() { records = next }
            if records.last?.rawLine.contains("event599") == true { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        #expect(records.last?.rawLine.contains("event599") == true)
        #expect(records.count == 601)
        #expect(records.contains { if case .usage = $0.event { true } else { false } })
        #expect(await reader.usage.context?.contextTokens == 120)
        let reopened = ToolTranscriptReader(binary: binary, format: "codex", log: log)
        _ = try await reopened.read()
        #expect(await reopened.usage.context == nil)
        #expect(await reopened.usage.contextNotice == "No usage report in the loaded transcript.")
    }
}
