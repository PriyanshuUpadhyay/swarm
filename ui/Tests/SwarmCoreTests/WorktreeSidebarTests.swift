import Foundation
import Testing
@testable import SwarmCore

@Suite("Worktree sidebar")
struct WorktreeSidebarTests {
    @Test("Missing, locked, and detached worktrees show their state")
    func worktreeStates() throws {
        let entries = [
            WorktreeEntry(path: "/repo/wt/missing", branch: "missing", pruneReason: "folder gone"),
            WorktreeEntry(path: "/repo/wt/locked", branch: "locked", lockReason: "keep"),
            WorktreeEntry(path: "/repo/wt/detached", isDetached: true),
        ]
        let tree = build([], entries: entries)
        let rows = sidebarRows(tree)
        #expect(try #require(rows.first { $0.id == entries[0].path }).detail.contains("folder missing"))
        #expect(try #require(rows.first { $0.id == entries[1].path }).detail.contains("locked"))
        #expect(try #require(rows.first { $0.id == entries[2].path }).detail.contains("detached"))
        let workspaces = try #require(tree.projects.first).workspaces
        #expect(workspaces.first { $0.path == entries[0].path }?.missing == true)
        #expect(workspaces.first { $0.path == entries[1].path }?.mark == .locked)
        #expect(workspaces.first { $0.path == entries[2].path }?.mark == .detached)
        #expect(rows.first { $0.id == entries[0].path }?.newChatEnabled == false)
        #expect(rows.first { $0.id == entries[1].path }?.newChatEnabled == true)
        #expect(rows.first { $0.id == entries[2].path }?.newChatEnabled == true)
    }

    @Test("Prune removes a missing worktree and keeps a locked worktree")
    func pruneWorktree() async throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let repo = root.appendingPathComponent("repo")
        let missing = root.appendingPathComponent("missing")
        let locked = root.appendingPathComponent("locked")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try await Shell.check("git", ["init", "-b", "main", repo.path])
        try await Shell.check("git", [
            "-c", "user.name=Test", "-c", "user.email=test@example.com",
            "commit", "--allow-empty", "-m", "base",
        ], cwd: repo.path)
        try await Shell.check("git", ["worktree", "add", "-b", "missing", missing.path], cwd: repo.path)
        try await Shell.check("git", ["worktree", "add", "-b", "locked", locked.path], cwd: repo.path)
        try await Shell.check("git", ["worktree", "lock", locked.path], cwd: repo.path)
        try FileManager.default.removeItem(at: missing)
        let before = try await Git.worktrees(of: repo.path)
        let missingEntry = try #require(before.first { $0.branch == "missing" })
        #expect(missingEntry.isPrunable)
        try await Git.pruneWorktrees(in: repo.appendingPathComponent(".git").path)
        let entries = try await Git.worktrees(of: repo.path)
        #expect(!entries.contains { $0.path == missingEntry.path })
        let lockedEntry = try #require(entries.first { $0.branch == "locked" })
        #expect(lockedEntry.isLocked)
        await #expect(throws: (any Error).self) { try await Git.pruneWorktrees(in: root.path) }
    }

    private func build(_ sessions: [SwarmSession], entries: [WorktreeEntry]) -> SessionsTree {
        SessionsTree.build(
            sessions: sessions, projectPaths: ["/repo"],
            repositoryPathsResolver: { _ in
                GitRepositoryPaths(gitDirectory: "/repo/.bare", commonDirectory: "/repo/.bare")
            },
            worktreeLister: { _ in entries }
        )
    }

    private func sidebarRows(_ tree: SessionsTree) -> [SidebarRow] {
        SidebarRows.sections(
            projects: tree.projects, workspaces: WorkspaceEntry.list(in: tree),
            navigation: WorkspaceNavigation(), search: "", showingArchive: false, now: 10
        ).flatMap(\.rows)
    }
}
