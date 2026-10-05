import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Tool run fold")
struct ToolRunFoldTests {
    private func prompt(_ id: String = "prompt") -> TranscriptRow {
        TranscriptRow(kind: .user, text: "Fix the build", eventID: id)
    }

    private func tool(
        _ id: String, name: String = "Read", state: TranscriptToolActivity.State = .finished,
        duration: Double? = 1, command: String? = nil
    ) -> TranscriptRow {
        var row = TranscriptRow(kind: .toolUse, text: name, eventID: id)
        row.tool = TranscriptToolActivity(
            name: name, input: .object([:]), state: state, command: command, duration: duration
        )
        return row
    }

    private func reply(_ id: String) -> TranscriptRow {
        TranscriptRow(kind: .assistant, text: "I found the cause.", eventID: id)
    }

    private func ring(_ id: String, startsTurn: Bool = false) -> TranscriptRow {
        var row = TranscriptRow(kind: .system, text: "swarm: new message. Run swarm inbox", eventID: id)
        row.systemKind = TranscriptSystemKind.swarmRing
        row.startsTurn = startsTurn
        return row
    }

    private func thought(_ id: String) -> TranscriptRow {
        TranscriptRow(kind: .thought, text: "plan", eventID: id)
    }

    private func hidden(_ id: String) -> TranscriptRow {
        var row = TranscriptRow(kind: .system, text: "<system-reminder>", eventID: id)
        row.systemKind = TranscriptSystemKind.injected
        return row
    }

    private func turnEnd(_ id: String = "turn-end") -> TranscriptRow {
        var row = TranscriptRow(kind: .result, text: "completed", eventID: id)
        row.endsTurn = true
        return row
    }

    private func foldedIDs(_ rows: [TranscriptRow]) -> [[String]] {
        ToolRunFold.items(in: rows).compactMap {
            if case .fold(let group) = $0 { group.map(\.eventID) } else { nil }
        }
    }

    @Test("Two steps between prose fold, and the fold's id differs from its first step's")
    func twoStepsFold() {
        let rows = [prompt(), tool("read"), tool("grep"), reply("answer"), turnEnd()]
        #expect(foldedIDs(rows) == [["read", "grep"]])
        #expect(ToolRunFold.items(in: rows).map(\.id) == ["prompt", "fold:read", "answer", "turn-end"])
    }

    @Test("One tool alone between prose stays a plain row")
    func singleToolStays() {
        let rows = [prompt(), tool("read"), reply("answer")]
        #expect(foldedIDs(rows).isEmpty)
        #expect(ToolRunFold.items(in: rows).count == rows.count)
    }

    @Test("Mid-turn rings, thoughts, and failed or waiting tools fold with the tools; a failed step opens its fold")
    func stepsFoldTogether() {
        let rows = [
            prompt(), tool("sleep", name: "sleep"), ring("ring"), thought("plan"),
            tool("py", name: "exec", state: .failed, command: "python3 -"), tool("inbox", name: "exec", state: .waiting, command: "swarm inbox"),
        ]
        let folds = ToolRunFold.items(in: rows).compactMap { if case .fold(let group) = $0 { group } else { nil } }
        #expect(folds.map { $0.map(\.eventID) } == [["sleep", "ring", "plan", "py", "inbox"]])
        #expect(ToolRunFold.defaultExpanded(folds[0]))
        #expect(!ToolRunFold.defaultExpanded([tool("a"), tool("b")]))
    }

    @Test("Rings and thoughts alone do not fold, and a ring that starts a turn is prose")
    func ringsWithoutToolsStay() {
        #expect(foldedIDs([prompt(), ring("one"), ring("two"), thought("plan")]).isEmpty)
        #expect(foldedIDs([tool("read"), ring("start", startsTurn: true), tool("grep")]).isEmpty)
    }

    @Test("A hidden row does not count toward the two steps and does not change the fold's id")
    func hiddenRowsDoNotCount() {
        #expect(foldedIDs([prompt(), hidden("reminder"), tool("read"), reply("answer")]).isEmpty)
        let shown = ToolRunFold.items(in: [prompt(), hidden("reminder"), tool("read"), tool("grep")])
        let filtered = ToolRunFold.items(in: [prompt(), tool("read"), tool("grep")])
        #expect(shown.map(\.id) == filtered.map(\.id))
        #expect(shown.map(\.id) == ["prompt", "fold:read"])
    }

    @Test("The summary counts commands, waits, other tools by name, rings, and failures, and sums tool time")
    func summary() {
        let rows = [
            tool("c1", name: "exec", duration: 3, command: "swarm inbox"),
            tool("c2", name: "Bash", state: .failed, duration: 2, command: "false"),
            tool("w1", name: "sleep", duration: 15), ring("r1"), ring("r2"),
            tool("read", duration: 0.5), tool("read-2", duration: 0.5), tool("grep", name: "Grep", duration: 1),
            hidden("reminder"),
        ]
        let summary = ToolRunFold.summary(of: rows)
        #expect(summary.text == "2 commands · 1 wait · Read ×2 · Grep · 2 rings · 1 failed")
        #expect(summary.duration == 22)
        #expect(!summary.isRunning)
        #expect(summary.accessibilityLabel == "Steps: 2 commands, 1 wait, Read 2, Grep, 2 rings, 1 failed, 22 seconds")
        #expect(ToolRunFold.summary(of: rows + [tool("bash", name: "Bash", duration: nil, command: "ls")]).duration == nil)
    }

    @Test("A running fold names its newest step")
    func runningSummary() {
        let rows = [tool("w1", name: "sleep", duration: 9), tool("inbox", name: "exec", state: .waiting, duration: nil, command: "swarm inbox")]
        let summary = ToolRunFold.summary(of: rows)
        #expect(summary.isRunning)
        #expect(summary.latestTitle == "swarm inbox")
        #expect(summary.text == "1 command · 1 wait")
        #expect(summary.accessibilityLabel == "Steps: 1 command, 1 wait, running, now swarm inbox")
    }

    /// ADR 0047's invariant I1 over every sequence of up to 6 rows of 8 row types (about 300,000):
    /// the folded items hold every row once, in order, and each fold is a whole run of steps
    /// with 2 or more shown steps and a tool.
    @Test("Folding never drops, repeats, or reorders a row")
    func foldIsLossless() {
        let makers: [(String) -> TranscriptRow] = [
            reply, { tool($0) }, { tool($0, state: .failed) }, { tool($0, state: .waiting) },
            { ring($0) }, { ring($0, startsTurn: true) }, thought, hidden,
        ]
        var failures = 0
        for length in 0...6 {
            var digits = Array(repeating: 0, count: length)
            while true {
                let rows = digits.enumerated().map { makers[$0.element]("r\($0.offset)") }
                let items = ToolRunFold.items(in: rows)
                let flat = items.flatMap { item -> [TranscriptRow] in
                    switch item {
                    case .row(let row): [row]
                    case .fold(let group): group
                    }
                }
                var valid = flat.map(\.eventID) == rows.map(\.eventID)
                for (position, item) in items.enumerated() {
                    guard case .fold(let group) = item else { continue }
                    valid = valid && group.allSatisfy(ToolRunFold.isStep)
                        && group.filter { !$0.isHiddenByDefault }.count >= 2
                        && group.contains { $0.tool != nil }
                    for neighbor in [position - 1, position + 1] where items.indices.contains(neighbor) {
                        if case .row(let row) = items[neighbor] { valid = valid && !ToolRunFold.isStep(row) } else { valid = false }
                    }
                }
                if !valid { failures += 1 }
                guard let next = digits.lastIndex(where: { $0 < makers.count - 1 }) else { break }
                digits[next] += 1
                for later in (next + 1)..<length { digits[later] = 0 }
            }
        }
        #expect(failures == 0)
    }
}
