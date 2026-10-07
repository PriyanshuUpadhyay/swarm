import Foundation
import Testing
@testable import SwarmCore

@Suite("Worktree sidebar")
struct WorktreeSidebarTests {
    @Test("A hub-root chat gives Files the hub folder")
    func hubRootFiles() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        for folder in [".bare", "wt/main", "wt/feature"] {
            try FileManager.default.createDirectory(
                at: root.appendingPathComponent(folder), withIntermediateDirectories: true
            )
        }
        try "hub file".write(to: root.appendingPathComponent("hub.txt"), atomically: true, encoding: .utf8)
        try "git metadata".write(to: root.appendingPathComponent(".bare/internal.txt"), atomically: true, encoding: .utf8)
        let tree = SessionsTree.build(
            sessions: [
                session("hub", cwd: root.path),
                session("main", cwd: root.path + "/wt/main/src"),
                session("feature", cwd: root.path + "/wt/feature/src"),
            ], projectPaths: [root.path],
            repositoryPathsResolver: Git.repositoryPaths,
            worktreeLister: { common in
                let hub = URL(fileURLWithPath: common).deletingLastPathComponent().path
                return [
                    WorktreeEntry(path: common, isBare: true),
                    WorktreeEntry(path: hub + "/wt/main", branch: "main"),
                    WorktreeEntry(path: hub + "/wt/feature", branch: "feature"),
                ]
            }
        )
        let project = try #require(tree.projects.first)
        let selected = try #require(WorkspaceEntry.list(in: tree).first { $0.chats.contains { $0.id.rawValue == "hub" } })
        #expect(selected.id == project.path)
        let files = try await WorkspaceFiles.list(in: selected.id)
        #expect(files.entries.contains { $0.name == "hub.txt" })
        #expect(!files.entries.contains { $0.name == "internal.txt" })
        #expect(try await WorkspaceFiles.preview(in: selected.id, path: "hub.txt") == .text("hub file"))
        #expect(tree.launchDirectory(for: SwarmSessionID("main")) == project.path + "/wt/main")
        #expect(tree.launchDirectory(for: SwarmSessionID("feature")) == project.path + "/wt/feature")
    }

    @Test("Gone worktree chats share one removed row per project")
    func removedWorktrees() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let projects = [root.appendingPathComponent("first"), root.appendingPathComponent("second")]
        for project in projects {
            for folder in [".bare", "wt/main", "existing"] {
                try FileManager.default.createDirectory(
                    at: project.appendingPathComponent(folder), withIntermediateDirectories: true
                )
            }
        }
        let sessions = projects.flatMap { project in
            [
                session("\(project.lastPathComponent)-gone-one", cwd: project.path + "/wt/gone-one"),
                session("\(project.lastPathComponent)-gone-two", cwd: project.path + "/wt/gone-two/src"),
                session("\(project.lastPathComponent)-hub", cwd: project.path),
                session("\(project.lastPathComponent)-existing", cwd: project.path + "/existing"),
                session("\(project.lastPathComponent)-listed", cwd: project.path + "/wt/main/src"),
                session("\(project.lastPathComponent)-prunable", cwd: project.path + "/wt/prunable"),
            ]
        }
        let tree = SessionsTree.build(
            sessions: sessions, projectPaths: projects.map(\.path),
            repositoryPathsResolver: Git.repositoryPaths,
            worktreeLister: { common in
                let project = URL(fileURLWithPath: common).deletingLastPathComponent().path
                return [
                    WorktreeEntry(path: project + "/wt/main", branch: "main"),
                    WorktreeEntry(path: project + "/wt/prunable", branch: "prunable", pruneReason: "gone"),
                ]
            }
        )
        #expect(tree.projects.count == 2)
        for project in tree.projects {
            let removed = try #require(project.workspaces.first { $0.id == project.path + "#removed" })
            #expect(removed.name == "Removed worktrees")
            #expect(Set(removed.sessions.map { $0.id.rawValue }) == [
                "\(project.name)-gone-one", "\(project.name)-gone-two",
            ])
            let row = try #require(sidebarRows(tree).first { $0.id == removed.id })
            #expect(row.title == "Removed worktrees")
            #expect(!row.newChatEnabled)
            #expect(!row.missing)
            let hub = try #require(project.workspaces.first { $0.sessions.contains { $0.id.rawValue == "\(project.name)-hub" } })
            #expect(Set(hub.sessions.map { $0.id.rawValue }) == ["\(project.name)-hub", "\(project.name)-existing"])
            let main = try #require(project.workspaces.first { $0.path == project.path + "/wt/main" })
            #expect(main.sessions.map { $0.id.rawValue } == ["\(project.name)-listed"])
            let prunable = try #require(project.workspaces.first { $0.path == project.path + "/wt/prunable" })
            #expect(prunable.sessions.map { $0.id.rawValue } == ["\(project.name)-prunable"])
        }
    }

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

    @Test("Prune lists both missing worktrees and keeps a locked missing worktree")
    func pruneWorktree() async throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let repo = root.appendingPathComponent("repo")
        let missing = root.appendingPathComponent("missing")
        let otherMissing = root.appendingPathComponent("other missing")
        let locked = root.appendingPathComponent("locked")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try await Shell.check("git", ["init", "-b", "main", repo.path])
        try await Shell.check("git", [
            "-c", "user.name=Test", "-c", "user.email=test@example.com",
            "commit", "--allow-empty", "-m", "base",
        ], cwd: repo.path)
        try await Shell.check("git", ["worktree", "add", "-b", "missing", missing.path], cwd: repo.path)
        try await Shell.check("git", ["worktree", "add", "-b", "other-missing", otherMissing.path], cwd: repo.path)
        try await Shell.check("git", ["worktree", "add", "-b", "locked", locked.path], cwd: repo.path)
        try await Shell.check("git", ["worktree", "lock", locked.path], cwd: repo.path)
        try FileManager.default.removeItem(at: missing)
        try FileManager.default.removeItem(at: otherMissing)
        try FileManager.default.removeItem(at: locked)
        let before = try await Git.worktrees(of: repo.path)
        let missingEntry = try #require(before.first { $0.branch == "missing" })
        #expect(missingEntry.isPrunable)
        let missingFolders = Set(before.filter(\.isPrunable).map { URL(fileURLWithPath: $0.path).lastPathComponent })
        #expect(missingFolders == [missing.lastPathComponent, otherMissing.lastPathComponent])
        try await Git.pruneWorktrees(in: repo.appendingPathComponent(".git").path)
        let entries = try await Git.worktrees(of: repo.path)
        #expect(!entries.contains { $0.path == missingEntry.path })
        #expect(!entries.contains { $0.branch == "other-missing" })
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

    private func session(_ id: String, cwd: String) -> SwarmSession {
        SwarmSession(
            id: SwarmSessionID(id), talkMode: "lane", adapter: "tmux-solo", cwd: cwd,
            createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil
        )
    }

    private func sidebarRows(_ tree: SessionsTree) -> [SidebarRow] {
        SidebarRows.sections(
            projects: tree.projects, workspaces: WorkspaceEntry.list(in: tree),
            navigation: WorkspaceNavigation(), search: "", showingArchive: false, now: 10
        ).flatMap(\.rows)
    }
}
