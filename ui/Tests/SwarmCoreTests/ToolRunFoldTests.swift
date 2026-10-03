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
        duration: Double? = 1
    ) -> TranscriptRow {
        var row = TranscriptRow(kind: .toolUse, text: name, eventID: id)
        row.tool = TranscriptToolActivity(name: name, input: .object([:]), state: state, duration: duration)
        return row
    }

    private func reply(_ id: String) -> TranscriptRow {
        TranscriptRow(kind: .assistant, text: "I found the cause.", eventID: id)
    }

    private func turnEnd(_ id: String = "turn-end") -> TranscriptRow {
        var row = TranscriptRow(kind: .result, text: "completed", eventID: id)
        row.endsTurn = true
        return row
    }

    private func foldedIDs(_ rows: [TranscriptRow], pinned: Set<String> = []) -> [[String]] {
        ToolRunFold.items(in: rows, pinned: pinned).compactMap {
            if case .fold(let group) = $0 { group.map(\.eventID) } else { nil }
        }
    }

    @Test("Three finished tools in an ended turn fold, and the fold takes the first tool's id")
    func threeFinishedToolsFold() {
        let rows = [prompt(), tool("read"), tool("grep"), tool("edit"), turnEnd()]
        #expect(foldedIDs(rows) == [["read", "grep", "edit"]])
        let items = ToolRunFold.items(in: rows, pinned: [])
        #expect(items.map(\.id) == ["prompt", "read", "turn-end"])
    }

    @Test("Two finished tools stay as rows")
    func twoToolsDoNotFold() {
        let rows = [prompt(), tool("read"), tool("grep"), turnEnd()]
        #expect(foldedIDs(rows).isEmpty)
        #expect(ToolRunFold.items(in: rows, pinned: []).count == rows.count)
    }

    @Test("A failed tool breaks a run and stays its own row")
    func failedToolBreaksRun() {
        let rows = [
            prompt(), tool("read"), tool("grep"), tool("build", name: "Bash", state: .failed),
            tool("edit"), tool("test"), turnEnd(),
        ]
        #expect(foldedIDs(rows).isEmpty)
    }

    @Test("Waiting, interrupted, and unreported tools break a run", arguments: [
        TranscriptToolActivity.State.waiting, .interrupted, .unreported,
    ])
    func unfinishedToolBreaksRun(state: TranscriptToolActivity.State) {
        let rows = [prompt(), tool("read"), tool("odd", state: state), tool("grep"), tool("edit"), turnEnd()]
        #expect(foldedIDs(rows) == [])
    }

    @Test("Agent text between tools breaks a run")
    func textBreaksRun() {
        let rows = [prompt(), tool("read"), tool("grep"), reply("reply"), tool("edit"), tool("test"), turnEnd()]
        #expect(foldedIDs(rows).isEmpty)
    }

    @Test("A turn with no ending never folds, and its tools count as open")
    func runningTurnNeverFolds() {
        let rows = [prompt(), tool("read"), tool("grep"), tool("edit")]
        #expect(foldedIDs(rows).isEmpty)
        #expect(ToolRunFold.openTurnToolIDs(in: rows) == ["read", "grep", "edit"])
        #expect(ToolRunFold.openTurnToolIDs(in: rows + [turnEnd()]).isEmpty)
    }

    @Test("Only the ended turn folds when the next turn is still running")
    func onlyEndedTurnFolds() {
        let rows = [
            prompt("first"), tool("read"), tool("grep"), tool("edit"), turnEnd(),
            prompt("second"), tool("read-2"), tool("grep-2"), tool("edit-2"),
        ]
        #expect(foldedIDs(rows) == [["read", "grep", "edit"]])
        #expect(ToolRunFold.openTurnToolIDs(in: rows) == ["read-2", "grep-2", "edit-2"])
    }

    @Test("A pinned tool keeps its run from folding")
    func pinnedToolPreventsFold() {
        let rows = [prompt(), tool("read"), tool("grep"), tool("edit"), turnEnd()]
        #expect(foldedIDs(rows, pinned: ["grep"]).isEmpty)
    }

    @Test("The summary counts tool names in first-seen order and sums the durations")
    func summaryAndDuration() {
        let rows = [
            tool("read", duration: 2), tool("grep", name: "Grep", duration: 0.5),
            tool("read-again", duration: 1.5), tool("edit", name: "Edit", duration: 4),
        ]
        #expect(ToolRunFold.summary(of: rows) == "Read ×2 · Grep · Edit")
        #expect(ToolRunFold.totalDuration(of: rows) == 8)
        #expect(ToolRunFold.totalDuration(of: rows + [tool("bash", name: "Bash", duration: nil)]) == nil)
    }
}
