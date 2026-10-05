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

    @Test("A closed run's review is judged against the commit its build recorded, so a later commit does not make it stale")
    func closedRunAfterNextCommit() async throws {
        let workspace = try await gitWorkspace()
        defer { try? FileManager.default.removeItem(at: workspace) }
        let built = try await Shell.check("git", ["rev-parse", "HEAD"], cwd: workspace.path).trimmed.prefix(12)
        let steps = [
            "05-build.md": "Status: done \(built)\nUses:\nRevision: HEAD\n",
            "06-review.md": "Status: done 333333333333\nUses: 05-build@\(built)\n",
        ]
        try write(workspace.appendingPathComponent("tmp/flow/_closed/2026-10-01-shipped"), steps)
        try write(workspace.appendingPathComponent("tmp/flow/2026-10-02-open"), steps)
        try await commit(workspace, "next")
        let runs = try await StepRuns.scan(workspace: workspace.path, includeClosed: true)
        func review(closed: Bool) -> StepNode? { runs.first { $0.closed == closed }?.steps.last }
        #expect(review(closed: true)?.stale == [])
        #expect(review(closed: false)?.stale == ["05-build"], "an open run still follows HEAD")
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

    @Test("A CRLF step file reads as the kit reads it, with universal newlines")
    func crlfStep() async throws {
        let workspace = try fixture()
        defer { try? FileManager.default.removeItem(at: workspace) }
        let run = workspace.appendingPathComponent("tmp/flow/2026-10-05-crlf")
        // 903962a3eed6 is the kit's revision of "Uses:\n\nbody\n", the CRLF file's text below line 1.
        try write(run, [
            "01-frame.md": "Status: done x\r\nUses:\r\n\r\nbody\r\n",
            "02-design.md": "Status: done y\r\nUses: 01-frame@903962a3eed6\r\n",
        ])
        let steps = try #require(try await StepRuns.scan(workspace: workspace.path, includeClosed: false).first).steps
        #expect(steps.map(\.state) == [.done(revision: "x"), .done(revision: "y")])
        #expect(steps[1].needs == ["01-frame"])
        #expect(steps[1].stale == [])
    }

    @Test("A need named twice on the Uses line is one need")
    func repeatedNeed() async throws {
        let workspace = try fixture()
        defer { try? FileManager.default.removeItem(at: workspace) }
        try write(workspace.appendingPathComponent("tmp/flow/2026-10-05-twice"), [
            "01-frame.md": "Status: done x\nUses:\n",
            "02-design.md": "Status: open\nUses: 01-frame, 01-frame@abc\n",
        ])
        let steps = try #require(try await StepRuns.scan(workspace: workspace.path, includeClosed: false).first).steps
        #expect(steps[1].needs == ["01-frame"])
    }

    @Test("A run with no events.log takes its newest step file time, so a just-started run sorts first")
    func runWithNoLog() async throws {
        let workspace = try fixture()
        defer { try? FileManager.default.removeItem(at: workspace) }
        try write(workspace.appendingPathComponent("tmp/flow/2026-10-05-started"), ["01-frame.md": "Status: open\nUses:\n"])
        try write(workspace.appendingPathComponent("tmp/flow/2026-09-28-logged"), [
            "01-frame.md": "Status: done x\nUses:\n",
            "events.log": "2000-01-01T10:00:00\t01-frame\tdone x\n",
        ])
        let runs = try await StepRuns.scan(workspace: workspace.path, includeClosed: false)
        #expect(runs.map(\.name) == ["2026-10-05-started", "2026-09-28-logged"])
        #expect(runs.first?.lastActivity != nil)
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

    @Test("VoiceOver reads each node's state and each run's most urgent state in words, not only as a color")
    func spokenLabels() {
        func node(_ id: String, _ state: StepState?, ready: Bool = false, stale: [String] = [], todo: StepTodo? = nil) -> StepNode {
            StepNode(id: id, path: id, state: state, error: state == nil ? "Line 1 is not a status" : nil, needs: [],
                     needsAssumed: false, stale: stale, ready: ready, todo: todo, lastEvent: nil)
        }
        let waiting = node("03-contracts", .waiting(question: "Graph from Uses, table, or a file?"), todo: StepTodo(checked: 2, total: 4))
        #expect(waiting.spokenLabel == "03 contracts, waiting: Graph from Uses, table, or a file?, 2 of 4 todos")
        #expect(node("04-impact", .active(agent: "worker-a")).spokenLabel == "04 impact, active: worker-a")
        #expect(node("07-close", .open).spokenLabel == "07 close, open")
        #expect(node("07-close", .open, ready: true).spokenLabel == "07 close, ready")
        #expect(node("06-review", .done(revision: "abc"), stale: ["05-build"]).spokenLabel == "06 review, done, stale: 05-build changed")
        #expect(node("02-local", nil).spokenLabel == "02 local, can't read: Line 1 is not a status")

        func run(_ steps: [StepNode]) -> StepRun {
            StepRun(id: "tmp/flow/2026-10-05-login", skill: "flow", name: "2026-10-05-login", closed: false, steps: steps, lastActivity: nil)
        }
        let blocked = run([node("01-frame", .done(revision: "a")), node("02-design", .blocked(reason: "tests fail")), node("03-contracts", .open)])
        #expect(blocked.spokenLabel == "flow run 2026-10-05-login, blocked, 1 of 3 done")
        #expect(run([node("01-frame", .done(revision: "a")), waiting]).spokenLabel
            == "flow run 2026-10-05-login, waiting: Graph from Uses, table, or a file?, 1 of 2 done")
    }

    @Test("A chosen closed run is not gone while the scan leaves closed runs out")
    func chosenClosedRun() {
        let closedID = "tmp/flow/_closed/2026-09-30-old"
        #expect(!StepRuns.isGone(closedID, closed: true, from: [], includeClosed: false))
        #expect(StepRuns.isGone(closedID, closed: true, from: [], includeClosed: true))
        #expect(StepRuns.isGone("tmp/flow/2026-10-05-moved", closed: false, from: [], includeClosed: false))
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
