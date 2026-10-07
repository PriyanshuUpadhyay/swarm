import Foundation
import Testing
@testable import SwarmCore

@Suite("Projects that stay")
@MainActor
struct ProjectRetentionTests {
    @Test("Choices failures keep refresh values and report the same error once")
    func choicesFailureKeepsRefresh() throws {
        let suite = "ProjectRetentionTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let projects = SwarmProjectStore(choicesFolder: nil)
        let navigationStore = WorkspaceNavigationStore(defaults: defaults, choicesFolder: nil)
        let tree = build(sessions: [], paths: ["/repo"])
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/repo/main"]
        navigation.selectedChats = ["/repo/main": "chat"]
        var errors: [String] = []
        let refreshed = projects.refreshChoices(
            shown: tree.projects, navigation: navigation, navigationStore: navigationStore,
            reportError: { errors.append($0) }
        )
        #expect(refreshed == navigation)
        #expect(errors == ["SWARM_HOME is set but empty"])
        #expect(tree.projects.map(\.path) == ["/repo"])
        _ = projects.refreshChoices(
            shown: tree.projects, navigation: navigation, navigationStore: navigationStore,
            reportError: { errors.append($0) }
        )
        #expect(errors.count == 1)
    }

    @Test("A CLI project stays with every worktree after its last chat is archived")
    func archivedCLIProject() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = SwarmProjectStore(choicesFolder: folder)
        var chat = SwarmSession(
            id: .init("cli-chat"), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo/main",
            createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil
        )
        let first = build(sessions: [chat])
        try store.rememberShown(first.projects)
        chat.archivedAt = 2
        let restored = SwarmProjectStore(choicesFolder: folder)
        let archived = build(sessions: [chat], paths: restored.paths())
        #expect(archived.projects.map(\.path) == ["/repo"])
        #expect(archived.projects.first?.chats.isEmpty == true)
        #expect(Set(archived.projects.first?.workspaces.map(\.path) ?? []) == ["/repo/main", "/repo/feature"])
        #expect(WorkspaceEntry.list(in: archived).count == 2)
    }

    @Test("A removed project stays hidden despite new sessions and saved import paths")
    func removedProject() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = SwarmProjectStore(choicesFolder: folder)
        try store.rememberShown(build(sessions: [], paths: ["/repo"]).projects)
        try store.remove("/repo")
        let restored = SwarmProjectStore(choicesFolder: folder)
        #expect(restored.paths().isEmpty)
        #expect(restored.removedPaths() == ["/repo"])
        let chat = SwarmSession(
            id: .init("new-chat"), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo/main",
            createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil
        )
        #expect(build(sessions: [chat], paths: ["/repo/main"], removed: restored.removedPaths()).projects.isEmpty)
        #expect(SessionsTree.build(
            sessions: [], projectPaths: ["/folder"], removed: ["/folder"],
            repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] }
        ).projects.isEmpty)
    }

    @Test("Import clears a removed project and preserves its folder")
    func importRemoved() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let store = SwarmProjectStore(choicesFolder: root.appendingPathComponent("choices"))
        let path = try await store.add(project)
        try store.remove(path)
        #expect(store.removedPaths() == [path])
        #expect(try await store.add(project) == path)
        #expect(store.removedPaths().isEmpty)
        #expect(store.paths() == [path])
        #expect(FileManager.default.fileExists(atPath: path))
    }

    @Test("Import of a repository subfolder clears its project ignore entry")
    func importWorkspace() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let project = root.appendingPathComponent("project")
        let workspace = project.appendingPathComponent("workspace")
        try FileManager.default.createDirectory(at: workspace, withIntermediateDirectories: true)
        try await Git.initialize(at: project.path)
        let store = SwarmProjectStore(choicesFolder: root.appendingPathComponent("choices"))
        let path = try await store.add(workspace)
        let canonical = ProjectNode.projectPath(for: SwarmSessionDiscovery.identity(
            for: path, repositoryPathsResolver: Git.repositoryPaths
        ))
        try store.remove(canonical)
        #expect(store.paths().isEmpty)
        #expect(store.removedPaths() == [canonical])
        _ = try await store.add(workspace)
        #expect(store.removedPaths().isEmpty)
        #expect(store.paths() == [path])
    }

    @Test("Create clears a project ignore entry")
    func createRemoved() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let project = root.appendingPathComponent("project")
        let store = SwarmProjectStore(choicesFolder: root.appendingPathComponent("choices"))
        try store.remove(project.path)
        let path = try await store.create(at: project)
        #expect(store.removedPaths().isEmpty)
        #expect(store.paths() == [path])
    }

    @Test("Prune keeps keys for existing folders even when a git listing is empty")
    func pruneMissingFolders() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "ProjectRetentionTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: root)
        }
        let existing = root.appendingPathComponent("project").path
        let gone = root.appendingPathComponent("gone").path
        try FileManager.default.createDirectory(atPath: existing, withIntermediateDirectories: true)
        let file = root.appendingPathComponent("file").path
        try Data().write(to: URL(fileURLWithPath: file))
        let folder = root.appendingPathComponent("choices")
        var navigation = WorkspaceNavigation()
        navigation.pinned = [existing, gone, file]
        navigation.archived = [existing, gone]
        navigation.names = [existing: "Keep", gone: "Drop"]
        navigation.selectedChats = [existing: "keep-chat", gone: "drop-chat"]
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: folder)
        store.save(navigation)
        try OwnerChoicesStore(folder: folder).update {
            $0.projectPaths = [existing, gone]
            $0.workspaceOrder = [existing: [existing, gone], gone: [existing]]
        }
        let tree = SessionsTree.build(
            sessions: [], projectPaths: [existing],
            repositoryPathsResolver: { _ in GitRepositoryPaths(gitDirectory: existing + "/.git", commonDirectory: existing + "/.git") },
            worktreeLister: { _ in [] }
        )
        #expect(tree.projects.first?.workspaces.isEmpty == true)
        let pruned = try store.pruneMissingFolders(navigation)
        #expect(pruned.pinned == [existing])
        #expect(pruned.archived == [existing])
        #expect(pruned.names == [existing: "Keep"])
        #expect(pruned.selectedChats == [existing: "keep-chat"])
        #expect(store.load() == pruned)
        let choices = try OwnerChoicesStore(folder: folder).load()
        #expect(choices.workspaceOrder == [existing: [existing]])
        #expect(choices.projectPaths == [existing, gone])
    }

    private func build(sessions: [SwarmSession], paths: [String] = [], removed: Set<String> = []) -> SessionsTree {
        SessionsTree.build(
            sessions: sessions, projectPaths: paths, removed: removed,
            repositoryPathsResolver: { _ in GitRepositoryPaths(gitDirectory: "/repo/.git", commonDirectory: "/repo/.git") },
            worktreeLister: { _ in [WorktreeEntry(path: "/repo/main", branch: "main"), WorktreeEntry(path: "/repo/feature", branch: "feature")] }
        )
    }
}
