import Foundation
import Testing
@testable import SwarmCore

@Suite("Performance log", .scratchDirectory)
struct PerfLogTests {
    @Test("retains seven dated days and ignores names it cannot prove are old", .tags(.destructive))
    func retentionFromFileName() throws {
        let calendar = calendar()
        let today = try #require(calendar.date(from: DateComponents(
            year: 2026, month: 9, day: 18, hour: 12
        )))

        #expect(PerfLog.shouldDelete(
            fileName: "perf-2026-09-10.jsonl", today: today, calendar: calendar
        ))
        #expect(!PerfLog.shouldDelete(
            fileName: "perf-2026-09-11.jsonl", today: today, calendar: calendar
        ))
        #expect(!PerfLog.shouldDelete(
            fileName: "perf-not-a-date.jsonl", today: today, calendar: calendar
        ))
        #expect(!PerfLog.shouldDelete(
            fileName: "another-2026-01-01.jsonl", today: today, calendar: calendar
        ))
    }

    @Test("nothing is written, and no directory is made, while the switch is off")
    func offWritesNothing() throws {
        let directory = URL(fileURLWithPath: TestScratch.path("perf-off"), isDirectory: true)
        let date = try #require(calendar().date(from: DateComponents(
            year: 2026, month: 9, day: 18, hour: 12
        )))
        let log = PerfLog(
            directory: directory, now: { date }, calendar: calendar(), isRecording: { false }
        )
        log.start()
        log.record(.busRead(milliseconds: 999, sessionCount: 3))
        log.flushForTesting()

        #expect(log.entriesForTesting().isEmpty)
        #expect(!FileManager.default.fileExists(atPath: directory.path))
    }

    @Test("each event writes only its named detail fields")
    func detailPrivacy() throws {
        let directory = URL(fileURLWithPath: TestScratch.path("perf-details"), isDirectory: true)
        let date = try #require(calendar().date(from: DateComponents(
            year: 2026, month: 9, day: 18, hour: 12
        )))
        let log = PerfLog(directory: directory, now: { date }, calendar: calendar())
        let events: [PerfLog.Event] = [
            .stall(milliseconds: 121, paneKind: "/Users/person/private", drawnRowCount: 14),
            .chatRead(milliseconds: 51, messageCount: 8, rowCount: 12),
            .busRead(milliseconds: 101, sessionCount: 3),
            .processCapture(milliseconds: 251, executable: "/Users/person/private/bin/tool"),
            .heartbeat(residentMemoryBytes: 123_456, uptimeSeconds: 60),
        ]
        for event in events { log.record(event) }
        log.flushForTesting()

        let entries = log.entriesForTesting()
        #expect(entries.count == 5)
        let fields: [String: Set<String>] = Dictionary(
            uniqueKeysWithValues: entries.compactMap { entry in
                guard let kind = entry["kind"] as? String,
                      let detail = entry["detail"] as? [String: Any] else { return nil }
                return (kind, Set(detail.keys))
            }
        )
        #expect(fields["stall"] == ["paneKind", "drawnRowCount"])
        #expect(fields["chatRead"] == ["messageCount", "rowCount"])
        #expect(fields["busRead"] == ["sessionCount"])
        #expect(fields["processCapture"] == ["executable"])
        #expect(fields["heartbeat"] == ["residentMemoryBytes", "uptimeSeconds"])
        let capture = entries.first { $0["kind"] as? String == "processCapture" }
        let detail = try #require(capture?["detail"] as? [String: Any])
        #expect(detail["executable"] as? String == "tool")
        let stall = try #require(entries.first { $0["kind"] as? String == "stall" })
        let stallDetail = try #require(stall["detail"] as? [String: Any])
        #expect(stallDetail["paneKind"] as? String == "unknown")
        #expect(entries.first { $0["kind"] as? String == "heartbeat" }?["ms"] == nil)
    }

    @Test("an overflowing hour stays at the line bound and records the dropped count")
    func hourlyBound() throws {
        let directory = URL(fileURLWithPath: TestScratch.path("perf-bound"), isDirectory: true)
        let date = try #require(calendar().date(from: DateComponents(
            year: 2026, month: 9, day: 18, hour: 12
        )))
        let log = PerfLog(directory: directory, now: { date }, calendar: calendar())
        for _ in 0..<2_005 {
            log.record(.busRead(milliseconds: 101, sessionCount: 1))
        }
        log.flushForTesting()
        log.finishHourForTesting()

        let entries = log.entriesForTesting()
        #expect(entries.count == 2_000)
        let dropped = try #require(entries.last)
        #expect(dropped["kind"] as? String == "dropped")
        let detail = try #require(dropped["detail"] as? [String: Any])
        #expect(detail["count"] as? Int == 6)
        #expect(Set(detail.keys) == ["count"])
        #expect(dropped["ms"] == nil)
    }

    private func calendar() -> Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        return calendar
    }
}
