import Foundation
import Testing
@testable import SwarmCore

@Suite("Workspace removal")
struct GitWorktreeRemovalTests {
    @Test("An untracked file blocks deletion, and git remove never forces it")
    func dirty() async throws {
        let fixture = try await RemovalFixture.make()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        try "keep".write(to: fixture.worktree.appendingPathComponent("dirty.txt"), atomically: true, encoding: .utf8)
        #expect(await Git.removalBlocker(worktree: fixture.worktree.path)?.contains("uncommitted") == true)
        await #expect(throws: (any Error).self) {
            try await Git.removeWorktree(fixture.worktree.path, in: fixture.repository.path)
        }
        #expect(FileManager.default.fileExists(atPath: fixture.worktree.appendingPathComponent("dirty.txt").path))
    }

    @Test("A clean worktree blocks unpushed commits with or without an upstream")
    func unpushed() async throws {
        let fixture = try await RemovalFixture.make()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        try await fixture.commit(in: fixture.worktree.path, message: "local work")
        #expect(await Git.removalBlocker(worktree: fixture.worktree.path)?.contains("no upstream") == true)
        try await Shell.check("git", ["branch", "--set-upstream-to=origin/main", "feature"], cwd: fixture.repository.path)
        #expect(await Git.removalBlocker(worktree: fixture.worktree.path)?.contains("unpushed") == true)
    }

    @Test("A pushed clean worktree is removable, with or without an upstream")
    func clean() async throws {
        let fixture = try await RemovalFixture.make()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        #expect(await Git.removalBlocker(worktree: fixture.worktree.path) == nil)
        try await Shell.check("git", ["branch", "--set-upstream-to=origin/main", "feature"], cwd: fixture.repository.path)
        #expect(await Git.removalBlocker(worktree: fixture.worktree.path) == nil)
        try await Git.removeWorktree(fixture.worktree.path, in: fixture.repository.appendingPathComponent(".git").path)
        #expect(!FileManager.default.fileExists(atPath: fixture.worktree.path))
        #expect(try await Git.worktrees(of: fixture.repository.path).allSatisfy { $0.path != fixture.worktree.path })
        #expect(FileManager.default.fileExists(atPath: fixture.repository.appendingPathComponent(".git").path))
    }

    @Test("Delete confirmation counts ignored entries and names the first path before removal")
    func ignoredItems() async throws {
        let fixture = try await RemovalFixture.make()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        #expect(try await Git.ignoredRemovalItems(worktree: fixture.worktree.path).count == 0)
        try "tmp/\n".write(to: fixture.repository.appendingPathComponent(".git/info/exclude"),
                              atomically: true, encoding: .utf8)
        let ignoredDirectory = fixture.worktree.appendingPathComponent("tmp")
        try FileManager.default.createDirectory(at: ignoredDirectory, withIntermediateDirectories: true)
        try "task record".write(to: ignoredDirectory.appendingPathComponent("task.md"), atomically: true, encoding: .utf8)
        let ignored = try await Git.ignoredRemovalItems(worktree: fixture.worktree.path)
        #expect(ignored.count == 1)
        #expect(ignored.firstPath == "tmp/")
        #expect(ignored.message == "1 ignored item, such as tmp/, is deleted too.")
        #expect(await Git.removalBlocker(worktree: fixture.worktree.path) == nil)
        try await Git.removeWorktree(fixture.worktree.path, in: fixture.repository.path)
        #expect(!FileManager.default.fileExists(atPath: ignoredDirectory.path))
    }

    @Test("A failed git inspection refuses deletion with a reason")
    func inspectionFailure() async {
        let missing = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        #expect(await Git.removalBlocker(worktree: missing)?.contains("could not check") == true)
    }
}

private struct RemovalFixture {
    let root: URL
    let repository: URL
    let worktree: URL

    static func make() async throws -> Self {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("removal-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let fixture = Self(root: root, repository: root.appendingPathComponent("repository"),
                           worktree: root.appendingPathComponent("feature"))
        let remote = root.appendingPathComponent("remote.git")
        do {
            try await Shell.check("git", ["init", "--bare", remote.path])
            try await Shell.check("git", ["init", "-b", "main", fixture.repository.path])
            try await fixture.commit(in: fixture.repository.path, message: "base")
            try await Shell.check("git", ["remote", "add", "origin", remote.path], cwd: fixture.repository.path)
            try await Shell.check("git", ["push", "-u", "origin", "main"], cwd: fixture.repository.path)
            try await Shell.check("git", ["worktree", "add", "-b", "feature", fixture.worktree.path, "main"], cwd: fixture.repository.path)
            return fixture
        } catch {
            try? FileManager.default.removeItem(at: root)
            throw error
        }
    }

    func commit(in directory: String, message: String) async throws {
        try await Shell.check("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                                      "-c", "commit.gpgsign=false", "commit", "--allow-empty", "-m", message], cwd: directory)
    }
}
