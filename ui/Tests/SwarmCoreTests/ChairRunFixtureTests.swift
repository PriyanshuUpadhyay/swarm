import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

/// Redacted chair runs (packages/transcript/src/fixtures/*-chair-run.jsonl) through the real Zig
/// binary, the row builder, and the fold (ADR 0047).
@Suite("Chair run fixtures")
struct ChairRunFixtureTests {
    private func records(_ format: String) async throws -> [TranscriptRecord] {
        let binary = try #require(ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"])
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let fixture = repo.appendingPathComponent("packages/transcript/src/fixtures/\(format)-chair-run.jsonl")
        let process = TranscriptToolProcess(binary: URL(fileURLWithPath: binary), format: format, log: fixture, follow: false)
        var records: [TranscriptRecord] = []
        for try await record in process.stream { records.append(record) }
        return records
    }

    /// "user", "assistant", "fold(…)" for each item of the default (hidden rows off) view.
    private func shape(_ rows: [TranscriptRow]) -> [String] {
        ToolRunFold.items(in: rows.filter { !$0.isHiddenByDefault }).map { item in
            switch item {
            case .row(let row): row.kind.rawValue
            case .fold(let group): "fold(\(ToolRunFold.summary(of: group).text))"
            }
        }
    }

    @Test("Codex: rings are ring rows, exec rows show their cmd, failures show, and tool runs fold between prose")
    func codex() async throws {
        let rows = TranscriptRowBuilder.rows(from: try await records("codex"), isCodex: true)
        #expect(rows.filter { $0.systemKind == TranscriptSystemKind.swarmRing }.map(\.eventID) == ["ring-1:ring", "ring-2:ring"])
        #expect(!rows.contains { $0.kind == .user && $0.text.hasPrefix("swarm: new message") })
        let exec = rows.compactMap(\.tool).filter { $0.name == "exec" }
        #expect(exec.map(\.headerTitle) == [
            "swarm inbox", "python3 - <<'PY'", "swarm roles get council.claude",
            "python3 /home/owner/scripts/ensure-council-access.py --council-dir /tmp/councils/test-1",
            #"text(await tools.apply_patch("*** Begin Patch\n*** Update File: /workspace/notes.txt\n@@\n-old line\n+new line\n*** End Patch"));"#,
        ])
        #expect(exec[1].command == "python3 - <<'PY'\nraise SystemExit(1)\nPY")
        #expect(exec[2].command == "swarm roles get council.claude\nswarm roles get council.gpt")
        #expect(exec.map(\.state) == [.finished, .failed, .finished, .failed, .finished])
        #expect(exec.map(\.exitCode) == [nil, 1, nil, nil, nil])
        #expect(shape(rows) == [
            "user", "assistant", "fold(2 commands · 1 wait · 1 ring · 1 failed)",
            "assistant", "fold(3 commands · 1 ring · 1 failed)", "assistant", "result",
        ])
    }

    @Test("Claude: a ring is a ring row mid-turn, and the run with a failed build folds open")
    func claude() async throws {
        let rows = TranscriptRowBuilder.rows(from: try await records("claude"))
        let ring = try #require(rows.first { $0.systemKind == TranscriptSystemKind.swarmRing })
        #expect(ring.eventID == "u-ring:ring")
        #expect(!ring.startsTurn)
        #expect(shape(rows) == ["user", "assistant", "fold(2 commands · Read · 1 ring · 1 failed)", "assistant", "result"])
        let fold = ToolRunFold.items(in: rows.filter { !$0.isHiddenByDefault }).compactMap {
            if case .fold(let group) = $0 { group } else { nil }
        }
        #expect(fold.map { ToolRunFold.isExpanded($0, overrides: [:], shownAsRows: []) } == [true])
        #expect(rows.compactMap(\.tool).map(\.exitCode) == [1, nil, nil])
    }

    @Test("AGY: a ring inside the request wrapper is a ring row, folds with the tools mid-turn, and starts a turn after a final reply")
    func agy() async throws {
        let rows = TranscriptRowBuilder.rows(from: try await records("agy"))
        let rings = rows.filter { $0.systemKind == TranscriptSystemKind.swarmRing }
        #expect(rings.map(\.eventID) == ["4:ring", "8:ring"])
        #expect(rings.map(\.startsTurn) == [false, true])
        #expect(shape(rows) == [
            "user", "assistant", "fold(1 command · view_file · 1 ring)", "assistant", "result",
            "system", "toolUse", "assistant", "result",
        ])
    }

    /// ADR 0047 I2 and I3: with hidden rows shown and every fold open, the rows name each event
    /// that has a row exactly once, rows start in log order, and each name finds its raw entry.
    @Test("Expanded folds hold every source event of the fixture once, and Show Source finds each one",
          arguments: ["codex", "claude", "agy"])
    func foldsHoldEverySource(format: String) async throws {
        let records = try await records(format)
        let rows = TranscriptRowBuilder.rows(from: records, isCodex: format == "codex")
        let expanded = ToolRunFold.items(in: rows).flatMap { item -> [TranscriptRow] in
            switch item {
            case .row(let row): [row]
            case .fold(let group): group
            }
        }
        #expect(expanded.map(\.eventID) == rows.map(\.eventID))
        let withRow = records.indices.filter { TranscriptRowBuilder.row(from: records[$0].event, index: $0) != nil }
        let sources = expanded.flatMap(\.sourceIDs)
        #expect(sources.count == withRow.count)
        #expect(Set(sources) == Set(withRow.map { RawTranscriptEntry.id(index: $0) }))
        let starts = expanded.compactMap { $0.sourceIDs.first }.compactMap { Int($0.dropFirst("raw-".count)) }
        #expect(starts == starts.sorted() && Set(starts).count == starts.count)
        let raw = TranscriptDebugData.entries(from: records)
        for row in expanded {
            #expect(TranscriptSource.entries(for: row, in: raw).map(\.id) == row.sourceIDs.sorted {
                Int($0.dropFirst(4)) ?? 0 < Int($1.dropFirst(4)) ?? 0
            })
        }
    }
}
