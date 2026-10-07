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
        #expect(workspace.runSummary == "2 runs waiting")
        #expect(workspace.status == .waiting)
        #expect(sections.first?.status == .waiting)
        #expect(rows.first { $0.id == "chat:new" }?.runStep?.runID == "tmp/flow/active")
        #expect(rows.first { $0.id == "chat:old" }?.runStep == nil)
        #expect(rows.first { $0.id == "chat:other" }?.runStep == nil)
        let waiting = runs.filter { $0.urgency == .waiting }
        #expect(SidebarRows.runSummary(waiting.prefix(1).map { $0 }) == "1 run waits")
        #expect(SidebarRows.runSummary(runs.filter { $0.urgency == .active }) == "flow · 01 build")
        var navigation = WorkspaceNavigation()
        navigation.collapsed = [folder.path, folder.path + "/other"]
        #expect(SidebarRows.runWorkspaces(entries, navigation: navigation).isEmpty)
        navigation.pinned = [folder.path]
        #expect(SidebarRows.runWorkspaces(entries, navigation: navigation) == [folder.path])
        navigation.archived = [folder.path]
        #expect(SidebarRows.runWorkspaces(entries, navigation: navigation).isEmpty)
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
