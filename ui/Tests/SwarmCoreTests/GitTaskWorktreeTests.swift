import Foundation
import Testing
@testable import SwarmCore

@Suite("Task worktrees")
struct GitTaskWorktreeTests {
    @Test("A task gets its own branch and working tree")
    func createsWorktree() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("project")
        let tasks = root.appendingPathComponent("tasks")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try await Shell.check("git", ["init", "-b", "main", source.path])
        try await Shell.check("git", [
            "-c", "user.name=Test", "-c", "user.email=test@example.com",
            "commit", "--allow-empty", "-m", "base",
        ], cwd: source.path)
        let mainHead = try await Shell.check("git", ["rev-parse", "HEAD"], cwd: source.path).trimmed
        try await Shell.check("git", ["checkout", "-b", "feature"], cwd: source.path)
        try "feature".write(to: source.appendingPathComponent("feature.txt"), atomically: true, encoding: .utf8)
        try await Shell.check("git", ["add", "feature.txt"], cwd: source.path)
        try await Shell.check("git", [
            "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-m", "feature",
        ], cwd: source.path)

        let discovery = SwarmSessionDiscovery()
        let bus = SwarmCLIBus(environment: [:], cwd: root.path) { _, _, _, _, _, _ in
            ShellResult(status: 0, stdout: "{}", stderr: "")
        }
        _ = try await discovery.tree(sessions: [], projectPaths: [source.path], bus: bus)
        let taskPath = try await GitTaskWorktree.create(
            named: "Fix sidebar", in: source.path,
            commonDirectory: source.appendingPathComponent(".git").path, under: tasks.path
        )
        let common = try #require(Git.repositoryPaths(in: source.path)).commonDirectory
        await discovery.forgetWorktrees(for: common)
        let refreshed = try await discovery.tree(sessions: [], projectPaths: [source.path, taskPath], bus: bus)
        #expect(refreshed.projects.flatMap(\.workspaces).contains { $0.path == taskPath })
        let listed = try await Git.worktrees(of: source.path)
        let task = try #require(listed.first { $0.path == taskPath })
        #expect(task.branch?.hasPrefix("swarm/fix-sidebar-") == true)
        #expect(URL(fileURLWithPath: taskPath).deletingLastPathComponent().lastPathComponent == "tasks")
        #expect(FileManager.default.fileExists(atPath: taskPath + "/.git"))
        #expect(try await Shell.check("git", ["rev-parse", "HEAD"], cwd: taskPath).trimmed == mainHead)
        #expect(!FileManager.default.fileExists(atPath: taskPath + "/feature.txt"))

        let nextPath = try await GitTaskWorktree.create(
            named: "Fix sidebar", in: source.path,
            commonDirectory: source.appendingPathComponent(".git").path, under: tasks.path
        )
        let nextListed = try await Git.worktrees(of: source.path)
        let next = try #require(nextListed.first { $0.path == nextPath })
        #expect(next.branch != task.branch)
        #expect(nextPath != taskPath)
        #expect(try await Shell.check("git", ["rev-parse", "HEAD"], cwd: nextPath).trimmed == mainHead)

        let bare = root.appendingPathComponent(".bare")
        try await Shell.check("git", ["clone", "--bare", source.path, bare.path])
        let bareTaskPath = try await GitTaskWorktree.create(
            named: "Review", in: bare.path, commonDirectory: bare.path,
            under: root.appendingPathComponent("wt").path
        )
        #expect(try await Shell.check("git", ["rev-parse", "HEAD"], cwd: bareTaskPath).trimmed == mainHead)
    }

    @Test("A task in a repository with no commit starts an orphan branch")
    func createsOrphanWorktree() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        try await Git.initialize(at: project.path)

        let taskPath = try await GitTaskWorktree.create(
            named: "First task", in: project.path,
            commonDirectory: project.appendingPathComponent(".git").path,
            under: root.appendingPathComponent("tasks").path
        )
        let task = try #require(try await Git.worktrees(of: project.path).first { $0.path == taskPath })
        #expect(task.branch?.hasPrefix("swarm/first-task-") == true)
        // An unborn branch has no commit for HEAD to name.
        await #expect(throws: (any Error).self) {
            try await Shell.check("git", ["rev-parse", "--verify", "HEAD"], cwd: taskPath)
        }
    }
}
