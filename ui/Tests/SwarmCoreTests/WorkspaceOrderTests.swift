import Foundation
import Testing
@testable import SwarmCore

@Suite("Workspace order")
@MainActor
struct WorkspaceOrderTests {
    private func project(bare: Bool = false, master: Bool = false) -> ProjectNode {
        let nodes = [
            WorkspaceNode(path: "/repo/alpha", name: "alpha", sessions: []),
            WorkspaceNode(path: bare ? "/repo/wt/main" : "/repo", name: "zeta", sessions: [],
                          branch: master ? "master" : "main"),
            WorkspaceNode(path: "/repo/beta", name: "beta", sessions: []),
        ] + (bare ? [WorkspaceNode(path: "/repo", name: "repo", sessions: [])] : []) + [
            WorkspaceNode(path: "/repo#removed", name: "Removed worktrees", sessions: [], isRemoved: true),
        ]
        return ProjectNode(id: .repository(commonDirectory: bare ? "/repo/.bare" : "/repo/.git"),
                           path: "/repo", launchDirectory: nodes[1].path, workspaces: nodes)
    }

    @Test("Stored paths lead, stale and duplicate paths do not disturb the remaining rows")
    func savedOrder() {
        let project = project()
        let tree = SessionsTree(projects: [project])
        #expect(WorkspaceEntry.list(in: tree).map(\.id) == ["/repo", "/repo/alpha", "/repo/beta", "/repo#removed"])
        let order = ["/gone", "/repo/beta", "/repo/beta", "/repo/alpha"]
        let entries = WorkspaceEntry.list(in: tree, workspaceOrder: ["/repo": order])
        #expect(entries.map(\.id) == ["/repo/beta", "/repo/alpha", "/repo", "/repo#removed"])
        var navigation = WorkspaceNavigation()
        navigation.workspaceOrder = ["/repo": order]
        let sections = SidebarRows.sections(projects: [project], workspaces: entries.reversed(), navigation: navigation,
                                            search: "", showingArchive: false, now: 1)
        #expect(sections.flatMap(\.rows).map(\.id) == entries.map(\.id))
    }

    @Test("Bare hubs prefer main, then master, and keep hub and removed rows last")
    func bareOrder() {
        for master in [false, true] {
            let project = project(bare: true, master: master)
            #expect(project.mainWorkspacePath == "/repo/wt/main")
            #expect(WorkspaceEntry.list(in: SessionsTree(projects: [project])).map(\.id)
                == ["/repo/wt/main", "/repo/alpha", "/repo/beta", "/repo", "/repo#removed"])
        }
        let both = ProjectNode(id: .repository(commonDirectory: "/repo/.bare"), path: "/repo", launchDirectory: "/repo/master",
                               workspaces: [
                                WorkspaceNode(path: "/repo/master", name: "master", sessions: [], branch: "master"),
                                WorkspaceNode(path: "/repo/main", name: "main", sessions: [], branch: "main"),
                               ])
        #expect(both.mainWorkspacePath == "/repo/main")
    }

    @Test("Drag moves workspaces both ways and rejects other projects, stale paths, and archives")
    func moves() {
        let project = project()
        let other = ProjectNode(id: .folder("/other"), path: "/other", launchDirectory: "/other",
                                workspaces: [WorkspaceNode(path: "/other", name: "other", sessions: [])])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project, other]))
        var navigation = WorkspaceNavigation()
        let movedBelowBeta = navigation.moveWorkspace("/repo", onto: "/repo/beta", in: entries)
        #expect(movedBelowBeta)
        #expect(navigation.workspaceOrder["/repo"] == ["/repo/alpha", "/repo/beta", "/repo", "/repo#removed"])
        let movedAboveAlpha = navigation.moveWorkspace("/repo", onto: "/repo/alpha", in: entries)
        #expect(movedAboveAlpha)
        #expect(navigation.workspaceOrder["/repo"] == ["/repo", "/repo/alpha", "/repo/beta", "/repo#removed"])
        let saved = navigation
        let rejectedOtherProject = !navigation.moveWorkspace("/repo", onto: "/other", in: entries)
        #expect(rejectedOtherProject)
        let rejectedStalePath = !navigation.moveWorkspace("/stale", onto: "/repo", in: entries)
        #expect(rejectedStalePath)
        let rejectedSamePath = !navigation.moveWorkspace("/repo", onto: "/repo", in: entries)
        #expect(rejectedSamePath)
        #expect(navigation == saved)
        navigation.archived.insert("/repo/beta")
        let rejectedArchivedTarget = !navigation.moveWorkspace("/repo", onto: "/repo/beta", in: entries)
        #expect(rejectedArchivedTarget)
        let rejectedArchivedPin = !navigation.pinWorkspace("/repo/beta", in: entries)
        #expect(rejectedArchivedPin)
        let rejectedStalePin = !navigation.pinWorkspace("/stale", in: entries)
        #expect(rejectedStalePin)
        let pinnedWorkspace = navigation.pinWorkspace("/repo", in: entries)
        #expect(pinnedWorkspace)
        #expect(navigation.pinned == ["/repo"])
    }

    @Test("Order and pins use choices while the Pinned fold uses view defaults across restarts")
    func persistence() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "WorkspaceOrderTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var navigation = WorkspaceNavigation()
        navigation.workspaceOrder = ["/repo": ["/repo/beta", "/repo"]]
        navigation.pinned = ["/repo/beta"]
        navigation.toggleCollapsed("pinned")
        WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(folder)).save(navigation)
        let loaded = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(folder)).load()
        #expect(loaded == navigation)
        #expect(loaded.isCollapsed("pinned"))
        #expect(try OwnerChoicesStore(folder: folder).load().workspaceOrder == navigation.workspaceOrder)
        let viewData = try #require(defaults.data(forKey: "workspaces.navigation"))
        let state = try #require(JSONSerialization.jsonObject(with: viewData) as? [String: Any])
        #expect(state["workspaceOrder"] == nil)
        #expect(state["pinned"] == nil)
        let project = project()
        let section = try #require(SidebarRows.sections(projects: [project], workspaces: WorkspaceEntry.list(in: SessionsTree(projects: [project])),
                                                       navigation: loaded, search: "", showingArchive: false, now: 1).first)
        #expect(section.collapseID == "pinned")
        #expect(loaded.collapsed.contains(section.collapseID))
    }

    @Test("Refresh keeps workspace order while a folder is missing")
    func retainOrder() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let existing = folder.appendingPathComponent("project/workspace")
        try FileManager.default.createDirectory(at: existing, withIntermediateDirectories: true)
        let suite = "WorkspaceOrderTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        let projectPath = existing.deletingLastPathComponent().path
        var navigation = WorkspaceNavigation()
        navigation.workspaceOrder = [projectPath: [existing.path, folder.appendingPathComponent("gone").path]]
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(folder.appendingPathComponent("choices")))
        store.save(navigation)
        let refreshed = try store.reloadChoices(navigation)
        #expect(refreshed.workspaceOrder == navigation.workspaceOrder)
        #expect(store.load().workspaceOrder == refreshed.workspaceOrder)
    }
}
