import Foundation
import Testing
import TranscriptTool

@Suite("TranscriptToolProcess lifecycle")
struct TranscriptToolProcessTests {
    // `stop` runs on the main thread from a view's deinit. If it waits for the child, the wait
    // spins the main run loop inside a SwiftUI update and AttributeGraph aborts the app.
    @Test("stop returns without waiting for a child that ignores SIGTERM")
    func stopDoesNotWait() async throws {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("transcript-stop-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let stubborn = dir.appendingPathComponent("stubborn")
        try "#!/bin/sh\ntrap '' TERM\necho ready\nfor i in 1 2 3; do sleep 1; done\n"
            .write(to: stubborn, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: stubborn.path)

        let tool = TranscriptToolProcess(
            binary: stubborn, format: "claude", log: dir.appendingPathComponent("log"), follow: true
        )
        var records = tool.events.makeAsyncIterator()
        #expect(try await records.next()?.rawLine == "ready")

        let elapsed = ContinuousClock().measure { tool.stop() }

        #expect(elapsed < .milliseconds(500))
        if let pid = tool.processIdentifier { kill(pid, SIGKILL) }
    }
}
