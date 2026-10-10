import Foundation
import Testing
@testable import SwarmCore

@Suite("Sidebar step runs")
struct SidebarRunsTests {
    private func chat(_ id: String, path: String, time: Int) -> SwarmProjectSession {
        SwarmProjectSession(sessions: [SwarmSession(
            id: .init(id), talkMode: "lane", adapter: "tmux-solo", cwd: path,
            createdAt: time, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil
        )], title: id)
    }

    private func entries(_ path: String) -> [WorkspaceEntry] {
        let project = ProjectNode(id: .folder(path), path: path, launchDirectory: path, workspaces: [
            WorkspaceNode(path: path, name: "workspace", sessions: [
                chat("old", path: path, time: 1), chat("new", path: path, time: 2)
            ]),
            WorkspaceNode(path: path + "/other", name: "other", sessions: [chat("other", path: path + "/other", time: 3)])
        ])
        return WorkspaceEntry.list(in: SessionsTree(projects: [project]))
    }

    private var agents: [SwarmSessionID: [SwarmAgent]] {
        Dictionary(uniqueKeysWithValues: ["old", "new", "other"].map {
            (SwarmSessionID($0), [SwarmAgent(id: .init("coder"), role: "code", pane: nil, alive: true)])
        })
    }

    @Test("Waiting runs reach the workspace and folded project; active agents link only to the newest local chat")
    func waitingAndActive() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        for (name, status) in [("wait-a", "waiting Pick a route?"), ("wait-b", "waiting Confirm?"), ("active", "active coder")] {
            let run = folder.appendingPathComponent("tmp/flow/" + name)
            try FileManager.default.createDirectory(at: run, withIntermediateDirectories: true)
            try "Status: \(status)\nUses: none\n".write(to: run.appendingPathComponent("01-build.md"), atomically: true, encoding: .utf8)
        }
        let runs = try await StepRuns.scan(workspace: folder.path, includeClosed: false).runs
        let entries = entries(folder.path)
        let sections = SidebarRows.sections(
            projects: [entries[0].project], workspaces: entries, navigation: .init(), search: "",
            showingArchive: false, now: 100, agentsBySession: agents, runsByWorkspace: [folder.path: runs]
        )
        let rows = sections.flatMap(\.rows)
        let workspace = try #require(rows.first { $0.id == folder.path })
        #expect(workspace.run?.urgency == .waiting)
        #expect(workspace.run?.skill == "flow")
        #expect(workspace.run?.step == "01-build")
        #expect(workspace.run?.firstQuestion != nil)
        #expect(workspace.fields.contains { $0.field == .steps && $0.text == "2 runs waiting" })
        #expect(workspace.status == .waiting)
        #expect(sections.first?.status == .waiting)
        #expect(rows.first { $0.id == "chat:new" }?.runStep?.runID == "tmp/flow/active")
        #expect(rows.first { $0.id == "chat:new" }?.fields.contains { $0.field == .steps && $0.text == "flow · 01 build" } == true)
        #expect(RowFields.stepsByChat(in: entries[0], runs: runs, agentsBySession: agents) == [.init("new"): "flow · 01 build"])
        #expect(rows.first { $0.id == "chat:old" }?.runStep == nil)
        #expect(rows.first { $0.id == "chat:other" }?.runStep == nil)
        let waiting = runs.filter { $0.urgency == .waiting }
        #expect(SidebarRows.runSummary(waiting.prefix(1).map { $0 }) == "1 run waits")
        #expect(SidebarRows.runSummary(runs.filter { $0.urgency == .active }) == "flow · 01 build")
        var navigation = WorkspaceNavigation()
        navigation.collapsed = [WorkspaceNavigation.workspaceCollapseID(folder.path), WorkspaceNavigation.workspaceCollapseID(folder.path + "/other")]
        #expect(Set(SidebarRows.runWorkspaces(entries, navigation: navigation)) == Set(entries.map(\.id)))
        navigation.pinned = [folder.path]
        #expect(Set(SidebarRows.runWorkspaces(entries, navigation: navigation)) == Set(entries.map(\.id)))
        navigation.archived = [folder.path]
        #expect(SidebarRows.runWorkspaces(entries, navigation: navigation) == [folder.path + "/other"])
    }

    @Test("A failed run scan returns an empty current read with a notice instead of keeping old runs")
    func failedScanNotice() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        try "not a directory".write(to: folder.appendingPathComponent("tmp"), atomically: true, encoding: .utf8)
        let read = await SidebarRows.scanRuns(in: [folder.path])
        #expect(read[folder.path]?.runs == [])
        #expect(read[folder.path]?.notice != nil)
        try FileManager.default.removeItem(at: folder.appendingPathComponent("tmp"))
        let recovered = await SidebarRows.scanRuns(in: [folder.path])
        #expect(recovered[folder.path]?.runs == [])
        #expect(recovered[folder.path]?.notice == nil)
    }

    @Test("Sidebar scans keep the real listing cut and unreadable folder notices")
    func scanListingNotices() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let crowded = folder.appendingPathComponent("crowded")
        let unreadable = folder.appendingPathComponent("unreadable")
        let locked = unreadable.appendingPathComponent("tmp/flow/locked")
        try FileManager.default.createDirectory(at: crowded.appendingPathComponent("tmp"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: locked, withIntermediateDirectories: true)
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: locked.path)
            try? FileManager.default.removeItem(at: folder)
        }
        for index in 0...2_000 {
            try Data().write(to: crowded.appendingPathComponent("tmp/note-\(index).txt"))
        }
        try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: locked.path)
        let read = await SidebarRows.scanRuns(in: [crowded.path, unreadable.path])
        #expect(read[crowded.path]?.cut == ["tmp"])
        #expect(read[crowded.path]?.notice != nil)
        #expect(read[unreadable.path]?.unreadable == ["tmp/flow/locked"])
        #expect(read[unreadable.path]?.notice != nil)
    }

    @Test("Partial run scans show unknown run state on the workspace and project fields")
    func partialScanNotice() throws {
        let entries = entries("/repo")
        var navigation = WorkspaceNavigation()
        navigation.fields.project = [.title, .steps]
        for scan in [StepRunScan(cut: ["tmp/flow"]), StepRunScan(unreadable: ["tmp/flow/run"])] {
            let notice = try #require(scan.notice)
            let sections = SidebarRows.sections(
                projects: [entries[0].project], workspaces: entries, navigation: navigation, search: "",
                showingArchive: false, now: 100, runsByWorkspace: ["/repo": scan.runs],
                runNoticesByWorkspace: ["/repo": notice]
            )
            let workspace = try #require(sections.flatMap(\.rows).first { $0.id == "/repo" })
            #expect(workspace.fields.contains { $0.field == .steps && $0.text == "Runs unknown" })
            #expect(sections.first?.fields.contains { $0.field == .steps && $0.text == "Runs unknown" } == true)
            let waiting = StepRun(id: "tmp/flow/wait", skill: "flow", name: "wait", closed: false, steps: [
                StepNode(id: "01-pick", path: "tmp/flow/wait/01-pick.md", state: .waiting(question: "Pick?"),
                         error: nil, needs: [], needsAssumed: false, stale: [], ready: false, todo: nil, lastEvent: nil)
            ], lastActivity: nil)
            let partial = SidebarRows.sections(
                projects: [entries[0].project], workspaces: entries, navigation: navigation, search: "",
                showingArchive: false, now: 100, runsByWorkspace: ["/repo": [waiting]],
                runNoticesByWorkspace: ["/repo": notice]
            )
            let partialWorkspace = try #require(partial.flatMap(\.rows).first { $0.id == "/repo" })
            #expect(partialWorkspace.status == .waiting)
            #expect(partialWorkspace.fields.contains { $0.field == .steps && $0.text == "1 run waits (partial)" })
            #expect(partial.first?.fields.contains { $0.field == .steps && $0.text == "1 run waits (partial)" } == true)
        }
    }

    @Test("Run scans include folded empty workspaces and exclude missing, removed, and archived workspaces")
    func runScanTargets() {
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo", workspaces: [
            WorkspaceNode(path: "/repo", name: "repo", sessions: []),
            WorkspaceNode(path: "/repo/missing", name: "missing", sessions: [], missing: true),
            WorkspaceNode(path: "/repo#removed", name: "Removed worktrees", sessions: [], isRemoved: true),
            WorkspaceNode(path: "/repo/archive", name: "archived", sessions: [])
        ])
        var navigation = WorkspaceNavigation()
        navigation.toggleCollapsed("/repo")
        navigation.archived = ["/repo/archive"]
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        #expect(SidebarRows.runWorkspaces(entries, navigation: navigation) == ["/repo"])
    }

    @Test("Chat rows and tabs choose the newest run when urgency is equal")
    func newestRunSteps() throws {
        let entries = entries("/repo")
        func run(_ name: String, time: TimeInterval) -> StepRun {
            StepRun(id: "tmp/flow/" + name, skill: "flow", name: name, closed: false, steps: [
                StepNode(id: "01-" + name, path: "tmp/flow/" + name + "/01-" + name + ".md",
                         state: .active(agent: "coder"), error: nil, needs: [], needsAssumed: false,
                         stale: [], ready: false, todo: nil, lastEvent: nil)
            ], lastActivity: Date(timeIntervalSince1970: time))
        }
        let runs = [run("a-old", time: 100), run("z-new", time: 200)]
        let rows = SidebarRows.sections(
            projects: [entries[0].project], workspaces: entries, navigation: .init(), search: "",
            showingArchive: false, now: 100, agentsBySession: agents, runsByWorkspace: ["/repo": runs]
        ).flatMap(\.rows)
        let chat = try #require(rows.first { $0.id == "chat:new" })
        let steps = RowFields.stepsByChat(in: entries[0], runs: runs, agentsBySession: agents)
        #expect(chat.runStep?.stepName == "01 z-new")
        #expect(steps[.init("new")] == "flow · 01 z-new")
        #expect(chat.fields.first { $0.field == .steps }?.text == steps[.init("new")])
    }

    @Test("Workspace and chat run links select the correct run, and active steps link back to the same chat")
    func jumpMapping() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let runFolder = folder.appendingPathComponent("tmp/flow/build")
        try FileManager.default.createDirectory(at: runFolder, withIntermediateDirectories: true)
        try "Status: active coder\nUses: none\n".write(to: runFolder.appendingPathComponent("01-build.md"), atomically: true, encoding: .utf8)
        let runs = try await StepRuns.scan(workspace: folder.path, includeClosed: false).runs
        let entries = entries(folder.path)
        let cache = [folder.path: runs]
        let workspace = try #require(SidebarRows.runDestination(
            for: folder.path, in: entries, runsByWorkspace: cache, agentsBySession: agents
        ))
        let linked = try #require(SidebarRows.runDestination(
            for: "chat:new", in: entries, runsByWorkspace: cache, agentsBySession: agents
        ))
        #expect(workspace.workspaceID == folder.path)
        #expect(workspace.run.id == "tmp/flow/build")
        #expect(linked == workspace)
        let step = try #require(linked.run.steps.first)
        #expect(SidebarRows.chat(for: step, in: entries[0], agentsBySession: agents)?.id == .init("new"))
        #expect(SidebarRows.chat(for: step, in: entries[0], agentsBySession: [:]) == nil)
        for id in ["chat:old", "chat:other", "chat:gone", "more:" + folder.path, "child:new/coder"] {
            #expect(SidebarRows.runDestination(for: id, in: entries, runsByWorkspace: cache, agentsBySession: agents) == nil)
        }
        #expect(SidebarRows.runDestination(for: "chat:new", in: entries, runsByWorkspace: [:], agentsBySession: agents) == nil)
        let chain = SwarmProjectSession(sessions: [
            chat("next", path: folder.path, time: 4).session,
            chat("root", path: folder.path, time: 1).session
        ], title: "Continued")
        let entry = WorkspaceEntry(project: entries[0].project, workspace: WorkspaceNode(
            path: folder.path, name: "workspace", sessions: [chain]
        ))
        #expect(SidebarRows.chat(for: step, in: entry, agentsBySession: [.init("root"): agents[.init("old")]!])?.id == .init("next"))
        let continued = SidebarRows.runDestination(for: "chat:root", in: [entry], runsByWorkspace: cache,
                                                  agentsBySession: [.init("root"): agents[.init("old")]!])
        #expect(continued?.run.id == linked.run.id)
    }
}
