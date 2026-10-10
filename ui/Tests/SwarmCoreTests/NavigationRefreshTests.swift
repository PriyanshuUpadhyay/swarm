import Foundation
import Testing
@testable import SwarmCore

private final class ViewDefaults: UserDefaults, @unchecked Sendable {
    var writes = 0

    override func set(_ value: Any?, forKey key: String) {
        writes += 1
        super.set(value, forKey: key)
    }
}

@Suite("Navigation refresh")
@MainActor
struct NavigationRefreshTests {
    @Test("A steady refresh reads choices once and writes no unchanged view state")
    func steadyRefresh() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "NavigationRefreshTests.\(UUID().uuidString)"
        let defaults = try #require(ViewDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var reads = 0
        let choices = OwnerChoicesStore(folder: folder, readFile: {
            reads += 1
            return try Data(contentsOf: $0)
        })
        try choices.update { $0.projectPaths = ["/repo"] }
        let projects = SwarmProjectStore(choices: choices)
        let store = WorkspaceNavigationStore(defaults: defaults, choices: choices)
        let navigation = store.load()
        #expect(store.save(navigation) == nil)
        defaults.writes = 0
        reads = 0
        let saved = projects.loadChoices(reportError: { _ in Issue.record("Choices load failed") })
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo", workspaces: [])
        var errors: [String] = []
        let refreshed = projects.refreshChoices(shown: [project], navigation: navigation, saved: saved,
                                                navigationStore: store, reportError: { errors.append($0.message) })
        #expect(errors.isEmpty)
        #expect(store.save(refreshed) == nil)
        #expect(reads == 1)
        #expect(defaults.writes == 0)
        var selected = refreshed
        selected.selectedWorkspace = "/repo"
        #expect(store.save(selected) == nil)
        #expect(reads == 1)
        #expect(defaults.writes == 1)
        #expect(store.save(selected) == nil)
        #expect(defaults.writes == 1)
    }
    @Test("A new project needs one extra locked read and keeps another process's changes")
    func newProjectRefresh() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "NavigationRefreshTests.\(UUID().uuidString)"
        let defaults = try #require(ViewDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var reads = 0
        let choices = OwnerChoicesStore(folder: folder, readFile: {
            reads += 1
            return try Data(contentsOf: $0)
        })
        try choices.update { $0.projectPaths = ["/existing"] }
        let projects = SwarmProjectStore(choices: choices)
        let store = WorkspaceNavigationStore(defaults: defaults, choices: choices)
        let navigation = store.load()
        reads = 0
        let saved = projects.loadChoices(reportError: { _ in Issue.record("Choices load failed") })
        try OwnerChoicesStore(folder: folder).update { $0.names["/external/workspace"] = "Other process" }
        let project = ProjectNode(id: .folder("/new"), path: "/new", launchDirectory: "/new", workspaces: [])
        var errors: [String] = []
        let refreshed = projects.refreshChoices(shown: [project], navigation: navigation, saved: saved,
                                                navigationStore: store, reportError: { errors.append($0.message) })
        #expect(errors.isEmpty)
        #expect(store.save(refreshed) == nil)
        #expect(reads == 2)
        #expect(refreshed.names == ["/external/workspace": "Other process"])
        #expect(try OwnerChoicesStore(folder: folder).load().projectPaths == ["/existing", "/new"])
    }

    @Test("An owner save during tree discovery survives the earlier choices snapshot")
    func ownerSaveDuringDiscovery() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "NavigationRefreshTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var failRead = false
        let choices = OwnerChoicesStore(folder: folder, readFile: {
            if failRead { throw CocoaError(.fileReadNoPermission) }
            return try Data(contentsOf: $0)
        })
        try choices.update { $0.projectPaths = ["/repo"] }
        let projects = SwarmProjectStore(choices: choices)
        let store = WorkspaceNavigationStore(defaults: defaults, choices: choices)
        var navigation = store.load()
        let choicesRevision = store.choicesRevision
        let saved = projects.loadChoices(reportError: { Issue.record("\($0.message)") })
        navigation.pinned = ["/repo"]
        navigation.names = ["/repo": "Owner name"]
        #expect(store.save(navigation) == nil)
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo", workspaces: [])
        let refreshed = projects.refreshChoices(shown: [project], navigation: navigation, saved: saved,
                                                loadedAtRevision: choicesRevision, navigationStore: store, reportError: { Issue.record("\($0.message)") })
        #expect(refreshed.pinned == ["/repo"])
        #expect(refreshed.names == ["/repo": "Owner name"])
        #expect(try choices.load().pinned == refreshed.pinned)
        navigation.names["/repo"] = "Later owner name"
        #expect(store.save(navigation) == nil)
        failRead = true
        var failures: [String] = []
        let kept = projects.refreshChoices(shown: [project], navigation: navigation, saved: saved,
                                           loadedAtRevision: choicesRevision, navigationStore: store,
                                           reportError: { failures.append($0.message) })
        #expect(kept == navigation)
        #expect(failures.first?.hasPrefix("Could not load sidebar choices.") == true)
    }

    @Test("Stale navigation writes keep choices changed by the other store")
    func independentStoreChanges() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let firstSuite = "NavigationRefreshTests.\(UUID().uuidString)"
        let secondSuite = "NavigationRefreshTests.\(UUID().uuidString)"
        let firstDefaults = try #require(UserDefaults(suiteName: firstSuite))
        let secondDefaults = try #require(UserDefaults(suiteName: secondSuite))
        defer {
            firstDefaults.removePersistentDomain(forName: firstSuite)
            secondDefaults.removePersistentDomain(forName: secondSuite)
            try? FileManager.default.removeItem(at: folder)
        }
        let firstStore = WorkspaceNavigationStore(defaults: firstDefaults, choicesFolder: folder)
        let secondStore = WorkspaceNavigationStore(defaults: secondDefaults, choicesFolder: folder)
        var first = firstStore.load()
        var second = secondStore.load()
        first.pinned = ["/first"]
        first.names = ["/first": "First"]
        #expect(firstStore.save(first) == nil)
        second.pinned = ["/second"]
        second.names = ["/second": "Second"]
        #expect(secondStore.save(second) == nil)
        let saved = try OwnerChoicesStore(folder: folder).load()
        #expect(saved.pinned == ["/first", "/second"])
        #expect(saved.names == ["/first": "First", "/second": "Second"])
        second.pinned.remove("/second")
        second.names.removeValue(forKey: "/second")
        #expect(secondStore.save(second) == nil)
        let remaining = try OwnerChoicesStore(folder: folder).load()
        #expect(remaining.pinned == ["/first"])
        #expect(remaining.names == ["/first": "First"])
    }

    @Test("A failed load keeps known project paths and removed projects and reports a load error once")
    func failedLoadKeepsLastGoodSnapshot() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "NavigationRefreshTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        try OwnerChoicesStore(folder: folder).update {
            $0.projectPaths = ["/kept"]
            $0.removedProjects = ["/removed"]
            $0.pinned = ["/kept"]
        }
        var failRead = false
        let choices = OwnerChoicesStore(folder: folder, readFile: {
            if failRead { throw CocoaError(.fileReadNoPermission) }
            return try Data(contentsOf: $0)
        })
        let projects = SwarmProjectStore(choices: choices)
        let store = WorkspaceNavigationStore(defaults: defaults, choices: choices)
        var errors: [String] = []
        let good = try #require(projects.loadChoices(reportError: { if choices.alerts.report($0) { errors.append($0.message) } }))
        failRead = true
        let retained = projects.loadChoices(reportError: { if choices.alerts.report($0) { errors.append($0.message) } })
        #expect(retained == good)
        #expect(projects.choicesLoadFailed)
        let session = SwarmSession(id: .init("removed-chat"), talkMode: "lane", adapter: "tmux-solo", cwd: "/removed",
                                   createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
        let tree = SessionsTree.build(sessions: [session], projectPaths: retained?.projectPaths ?? [],
                                      removed: retained?.removedProjects ?? [],
                                      repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] })
        #expect(tree.projects.map(\.path) == ["/kept"])
        #expect(errors.first?.hasPrefix("Could not load sidebar choices.") == true)
        failRead = false
        let recovered = projects.loadChoices(reportError: { if choices.alerts.report($0) { errors.append($0.message) } })
        _ = projects.refreshChoices(shown: [], navigation: WorkspaceNavigation(), saved: recovered,
                                    navigationStore: store, reportError: { if choices.alerts.report($0) { errors.append($0.message) } })
        #expect(!projects.choicesLoadFailed)
        failRead = true
        _ = projects.loadChoices(reportError: { if choices.alerts.report($0) { errors.append($0.message) } })
        #expect(errors.count == 2)
    }

    @Test("A corrupt choices file whose backup cannot move does not stop the tree and reports once")
    func corruptChoicesKeepsTree() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: folder.path)
            try? FileManager.default.removeItem(at: folder)
        }
        let file = folder.appendingPathComponent("choices.json")
        let corrupt = Data("{broken".utf8)
        try corrupt.write(to: file)
        try Data().write(to: folder.appendingPathComponent("choices.lock"))
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: folder.path)
        let projects = SwarmProjectStore(choicesFolder: folder)
        var alerts = OwnerChoicesAlerts()
        var errors: [String] = []
        let saved = projects.loadChoices(reportError: { if alerts.report($0) { errors.append($0.message) } })
        let session = SwarmSession(id: .init("visible"), talkMode: "lane", adapter: "tmux-solo", cwd: "/visible",
                                   createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
        let tree = SessionsTree.build(sessions: [session], projectPaths: saved?.projectPaths ?? [],
                                      removed: saved?.removedProjects ?? [],
                                      repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] })
        #expect(tree.session(session.id) != nil)
        #expect(saved == nil)
        _ = projects.loadChoices(reportError: { if alerts.report($0) { errors.append($0.message) } })
        #expect(errors.count == 1)
        #expect(try Data(contentsOf: file) == corrupt)
    }

}
