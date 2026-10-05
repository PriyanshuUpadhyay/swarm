import Foundation
import Testing
@testable import SwarmCore

@Suite("Step runs")
struct StepRunsTests {
    static let todoHead = "## Todo (check a box only with its evidence after the colon; `done` refuses an empty one)"
    /// The frame's text below line 1. Its revision, 9defab3576aa, comes from the kit's own rule
    /// (`step_run.revision`: SHA-1 of the bytes after line 1, cut to 12), not from the reader.
    static let frameBody = "Uses:\nSkills read: none\n\n## Rules for this step\n- [ ] a quoted rule list that is not a todo\n\n"
        + todoHead + "\n- [x] Goal: yes\n- [x] Done-when: yes\n- [ ] Lane: \n\n## Result\n"

    @Test("A flow folder with the new Uses and Revision lines reads into nodes, needs, states, stale marks, and todo counts")
    func newFlowFolder() async throws {
        let workspace = try await gitWorkspace()
        defer { try? FileManager.default.removeItem(at: workspace) }
        let head = try await Shell.check("git", ["rev-parse", "HEAD"], cwd: workspace.path).trimmed
        let run = workspace.appendingPathComponent("tmp/flow/2026-10-05-login")
        try write(run, [
            "01-frame.md": "Status: done 111111111111\n" + Self.frameBody,
            "02-design.md": "Status: skipped no UI: a CLI flag\nUses: 01-frame@9defab3576aa\n",
            "03-contracts.md": "Status: done 222222222222\nUses: 01-frame@9defab3576aa\n",
            "04-impact.md": "Status: active worker-a\nUses: 03-contracts@000000000000\n",
            "05-build.md": "Status: waiting Pick A or B?\nUses: 04-impact, 02-design\nRevision: HEAD\nTake with: x\n\n"
                + Self.todoHead + "\n- [x] Base: abc\n- [ ] Failing check: \n- [ ] Check passes: \n\n## Result\nRevision: HEAD\n",
            "06-review.md": "Status: done 333333333333\nUses: 05-build@\(head.prefix(12))\n",
            "07-close.md": "Status: open\nUses: 06-review\n",
            "brief.md": "Not a step.\n",
            "events.log": "2026-10-05T17:13:36\t01-frame\ttake a\nnot a line\n2026-10-05T17:20:00\t04-impact\ttake worker-a\n",
        ])
        let runs = try await StepRuns.scan(workspace: workspace.path, includeClosed: false)
        #expect(runs.map(\.id) == ["tmp/flow/2026-10-05-login"])
        let steps = try #require(runs.first).steps
        #expect(steps.map(\.id) == ["01-frame", "02-design", "03-contracts", "04-impact", "05-build", "06-review", "07-close"])
        let byID = Dictionary(uniqueKeysWithValues: steps.map { ($0.id, $0) })
        #expect(byID["01-frame"]?.state == .done(revision: "111111111111"))
        #expect(byID["01-frame"]?.needs == [])
        #expect(byID["01-frame"]?.todo == StepTodo(checked: 2, total: 3))
        #expect(byID["01-frame"]?.path == "tmp/flow/2026-10-05-login/01-frame.md")
        #expect(byID["02-design"]?.state == .skipped(reason: "no UI: a CLI flag"))
        #expect(byID["03-contracts"]?.stale == [])
        #expect(byID["04-impact"]?.state == .active(agent: "worker-a"))
        #expect(byID["04-impact"]?.stale == ["03-contracts"])
        #expect(byID["05-build"]?.state == .waiting(question: "Pick A or B?"))
        #expect(byID["05-build"]?.needs == ["04-impact", "02-design"])
        #expect(byID["05-build"]?.needsAssumed == false)
        #expect(byID["05-build"]?.todo == StepTodo(checked: 1, total: 3))
        #expect(byID["05-build"]?.ready == false)
        #expect(byID["06-review"]?.stale == [], "05-build's revision is HEAD, not its file hash")
        #expect(byID["07-close"]?.ready == true)
        #expect(byID["07-close"]?.todo == nil, "no todo heading means no count")
        #expect(byID["04-impact"]?.lastEvent != nil)
        #expect(byID["02-design"]?.lastEvent == nil)
        #expect(runs.first?.lastActivity == byID["04-impact"]?.lastEvent)
        #expect(runs.first?.urgency == .waiting)
        #expect(runs.first?.doneCount == 4)
        #expect(runs.first?.firstQuestion == "Pick A or B?")
        #expect(StepRuns.layers(steps) == [["01-frame"], ["02-design", "03-contracts"], ["04-impact"], ["05-build"], ["06-review"], ["07-close"]])

        try "next".write(to: workspace.appendingPathComponent("next.txt"), atomically: true, encoding: .utf8)
        try await commit(workspace, "next")
        let moved = try await StepRuns.scan(workspace: workspace.path, includeClosed: false)
        #expect(moved.first?.steps.first { $0.id == "06-review" }?.stale == ["05-build"], "a new commit makes the review stale")

        // Each scan reads the files again, so a poll sees a new line 1 at once (done-when 4).
        try write(run, ["07-close.md": "Status: active closer\nUses: 06-review@\(head.prefix(12))\n"])
        let changed = try await StepRuns.scan(workspace: workspace.path, includeClosed: false)
        #expect(changed.first?.steps.last?.state == .active(agent: "closer"))
    }

    @Test("An older folder with empty Uses lines still draws, with an assumed edge from the previous step")
    func oldFlowFolder() async throws {
        let workspace = try fixture()
        defer { try? FileManager.default.removeItem(at: workspace) }
        let run = workspace.appendingPathComponent("tmp/flow/2026-10-01-old")
        try write(run, [
            "01-frame.md": "Status: done 111111111111\n" + Self.frameBody,
            "02-design.md": "Status: active worker-b\nUses: 01-frame@9defab3576aa\n",
            "03-contracts.md": "Status: open\nUses:\n",
            "04-impact.md": "Status: open\nUses: none\n",
            "05-build.md": "Status: open\n",
        ])
        try write(workspace.appendingPathComponent("tmp/flow/_closed/2026-09-30-closed"), ["01-frame.md": "Status: done x\nUses:\n"])
        try write(workspace.appendingPathComponent("tmp/flow/notes"), ["01-intro.md": "# Intro\n"])
        try write(workspace.appendingPathComponent("tmp/deliver"), [:])
        try "task".write(to: workspace.appendingPathComponent("tmp/deliver/2026-10-01-old.md"), atomically: true, encoding: .utf8)

        let runs = try await StepRuns.scan(workspace: workspace.path, includeClosed: false)
        #expect(runs.map(\.id) == ["tmp/flow/2026-10-01-old"], "a folder with no Status line is not a run")
        let steps = try #require(runs.first).steps
        #expect(steps.map(\.needs) == [[], ["01-frame"], ["02-design"], [], ["04-impact"]])
        #expect(steps.map(\.needsAssumed) == [false, false, true, false, true])
        #expect(steps[1].stale == [])
        #expect(steps[2].ready == false)
        #expect(steps[3].ready == true)
        #expect(runs.first?.urgency == .active)

        let all = try await StepRuns.scan(workspace: workspace.path, includeClosed: true)
        #expect(all.map(\.id).sorted() == ["tmp/flow/2026-10-01-old", "tmp/flow/_closed/2026-09-30-closed"])
        #expect(all.first { $0.closed }?.name == "2026-09-30-closed")
    }

    @Test("A malformed step file is an error node in its place, and the rest of the run still reads")
    func malformedStep() async throws {
        let workspace = try fixture()
        defer { try? FileManager.default.removeItem(at: workspace) }
        let run = workspace.appendingPathComponent("tmp/research/2026-10-05-topic")
        try write(run, [
            "01-question.md": "Status: done abc\nUses: \n",
            "02-local.md": "garbage line one\nUses: 01-question\n",
            "03-web.md": "Status: open\nUses: 01-question\n",
            "04-report.md": "Status: thinking about it\nUses: 02-local, 03-web\n",
        ])
        try Data([0xff, 0xfe, 0x00]).write(to: run.appendingPathComponent("05-binary.md"))
        let steps = try #require(try await StepRuns.scan(workspace: workspace.path, includeClosed: false).first).steps
        #expect(steps.map(\.id) == ["01-question", "02-local", "03-web", "04-report", "05-binary"])
        #expect(steps[1].state == nil && steps[1].error == "Line 1 is not a status")
        #expect(steps[4].state == nil && steps[4].error != nil)
        #expect(steps[2].state == .open && steps[2].ready)
        #expect(steps[3].state == .other(word: "thinking", rest: "about it"), "a new status word shows as text")
        #expect(steps[3].ready == false)
        #expect(StepRuns.layers(steps) == [["01-question", "05-binary"], ["02-local", "03-web"], ["04-report"]])
    }

    @Test("A step file read while an agent rewrites it (empty) is read once more before it counts as an error")
    func halfWrittenStep() async throws {
        let workspace = try fixture()
        defer { try? FileManager.default.removeItem(at: workspace) }
        let run = workspace.appendingPathComponent("tmp/flow/2026-10-05-race")
        try write(run, ["01-frame.md": "Status: done x\nUses:\n", "02-design.md": ""])
        let scan = Task { try await StepRuns.scan(workspace: workspace.path, includeClosed: false) }
        try await Task.sleep(for: .milliseconds(20))
        try write(run, ["02-design.md": "Status: open\nUses: 01-frame\n"])
        #expect(try await scan.value.first?.steps.last?.state == .open)
    }

    @Test("No tmp folder is no runs, not an error")
    func noTmp() async throws {
        let workspace = try fixture()
        defer { try? FileManager.default.removeItem(at: workspace) }
        #expect(try await StepRuns.scan(workspace: workspace.path, includeClosed: true).isEmpty)
    }

    @Test("Layers drop an edge that closes a cycle and an edge to a missing step")
    func layersCycle() {
        func node(_ id: String, _ needs: [String]) -> StepNode {
            StepNode(id: id, path: id, state: .open, error: nil, needs: needs, needsAssumed: false,
                     stale: [], ready: false, todo: nil, lastEvent: nil)
        }
        let steps = [node("01-a", ["03-c"]), node("02-b", ["01-a", "09-gone"]), node("03-c", ["02-b"])]
        #expect(StepRuns.layers(steps) == [["01-a"], ["02-b"], ["03-c"]])
    }

    private func write(_ folder: URL, _ files: [String: String]) throws {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        for (name, text) in files {
            try text.write(to: folder.appendingPathComponent(name), atomically: true, encoding: .utf8)
        }
    }

    private func gitWorkspace() async throws -> URL {
        let workspace = try fixture()
        try await Shell.check("git", ["init", "-q", "-b", "main", workspace.path])
        try await commit(workspace, "base")
        return workspace
    }

    private func commit(_ workspace: URL, _ message: String) async throws {
        try await Shell.check("git", [
            "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-q", "--allow-empty", "-m", message,
        ], cwd: workspace.path)
    }

    private func fixture() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("step-runs-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root.resolvingSymlinksInPath()
    }
}
