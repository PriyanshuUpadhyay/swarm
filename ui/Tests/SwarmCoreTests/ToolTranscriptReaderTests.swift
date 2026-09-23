import Foundation
import Testing
@testable import SwarmCore
import TranscriptTool
import Darwin

@Suite("ToolTranscriptReader integration suite")
struct ToolTranscriptReaderTests {
    @Test("Reads Codex rollout and streams appended line via readIfChanged")
    func codexRolloutFollowing() async throws {
        let toolEnv = ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"]
        let binaryPath = try #require(toolEnv, "tool not built")
        let binaryURL = URL(fileURLWithPath: binaryPath)

        let scratchDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("tool-reader-test-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: scratchDirectory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: scratchDirectory) }

        let rolloutFile = scratchDirectory.appendingPathComponent("rollout-test-session.jsonl")

        let initialRollout = """
        {"type":"session_meta","payload":{"id":"session-1","cwd":"/work"}}
        {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"List files"}]}}
        {"type":"response_item","payload":{"type":"function_call","call_id":"call-1","name":"ls","arguments":"{}"}}
        """ + "\n"

        try Data(initialRollout.utf8).write(to: rolloutFile)

        let reader = ToolTranscriptReader(
            binary: binaryURL,
            format: "codex",
            log: rolloutFile,
        )

        let initialTranscript = try await reader.read()
        #expect(initialTranscript.count >= 2)

        let appendedLine = """
        {"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"fileA.swift\\nfileB.swift"}}
        """ + "\n"

        let fileHandle = try FileHandle(forWritingTo: rolloutFile)
        try fileHandle.seekToEnd()
        try fileHandle.write(contentsOf: Data(appendedLine.utf8))
        try fileHandle.close()

        var changedTranscript: [TranscriptEvent]?
        for _ in 0..<40 {
            if let update = try await reader.readIfChanged() {
                changedTranscript = update
                break
            }
            try await Task.sleep(for: .milliseconds(50))
        }

        let updated = try #require(changedTranscript)
        #expect(updated.contains { if case .toolCallUpdate(let id, _, _, _) = $0 { return id == "call-1" }; return false })
    }

    @Test("Dropping a reader stops and reaps its follow process")
    func readerStopsProcess() async throws {
        let binaryPath = try #require(
            ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"], "tool not built"
        )
        let log = FileManager.default.temporaryDirectory
            .appendingPathComponent("tool-reader-stop-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try Data(#"{"type":"session_meta","payload":{"id":"session-1","cwd":"/work"}}"#.utf8)
            .write(to: log)

        var reader: ToolTranscriptReader? = ToolTranscriptReader(
            binary: URL(fileURLWithPath: binaryPath), format: "codex", log: log
        )
        _ = try await reader?.read()
        let pid = try #require(await reader?.processIdentifier())
        #expect(processExists(pid))

        let droppedAt = ContinuousClock.now
        reader = nil
        #expect(droppedAt.duration(to: .now) < .seconds(2))
        for _ in 0..<40 where processExists(pid) {
            try await Task.sleep(for: .milliseconds(50))
        }
        #expect(!processExists(pid))
    }

    private func processExists(_ pid: Int32) -> Bool {
        errno = 0
        return kill(pid, 0) == 0 || errno != ESRCH
    }
}
