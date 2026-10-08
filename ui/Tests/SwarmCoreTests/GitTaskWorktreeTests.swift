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
            WorkspaceRequest(name: "Fix sidebar", start: .newBranch(base: "main"), prefix: "swarm/"), in: source.path,
            commonDirectory: source.appendingPathComponent(".git").path, under: tasks.path
        )
        let common = try #require(Git.repositoryPaths(in: source.path)).commonDirectory
        await discovery.forgetWorktrees(for: common)
        let refreshed = try await discovery.tree(sessions: [], projectPaths: [source.path, taskPath], bus: bus)
        #expect(refreshed.projects.flatMap(\.workspaces).contains { $0.path == taskPath })
        let listed = try await Git.worktrees(of: source.path)
        let task = try #require(listed.first { $0.path == taskPath })
        #expect(task.branch == "swarm/fix-sidebar")
        #expect(URL(fileURLWithPath: taskPath).deletingLastPathComponent().lastPathComponent == "tasks")
        #expect(FileManager.default.fileExists(atPath: taskPath + "/.git"))
        #expect(try await Shell.check("git", ["rev-parse", "HEAD"], cwd: taskPath).trimmed == mainHead)
        #expect(!FileManager.default.fileExists(atPath: taskPath + "/feature.txt"))

        await #expect(throws: GitTaskWorktreeError.self) {
            try await GitTaskWorktree.create(
                WorkspaceRequest(name: "Fix sidebar", start: .newBranch(base: "main"), prefix: "swarm/"), in: source.path,
                commonDirectory: source.appendingPathComponent(".git").path, under: tasks.path
            )
        }

        let bare = root.appendingPathComponent(".bare")
        try await Shell.check("git", ["clone", "--bare", source.path, bare.path])
        let bareTaskPath = try await GitTaskWorktree.create(
            WorkspaceRequest(name: "Review", start: .newBranch(base: "main"), prefix: "swarm/"), in: bare.path, commonDirectory: bare.path,
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
            WorkspaceRequest(name: "First task", start: .newBranch(base: ""), prefix: "swarm/"), in: project.path,
            commonDirectory: project.appendingPathComponent(".git").path,
            under: root.appendingPathComponent("tasks").path
        )
        let task = try #require(try await Git.worktrees(of: project.path).first { $0.path == taskPath })
        #expect(task.branch == "swarm/first-task")
        // An unborn branch has no commit for HEAD to name.
        await #expect(throws: (any Error).self) {
            try await Shell.check("git", ["rev-parse", "--verify", "HEAD"], cwd: taskPath)
        }
    }

    @Test("Branch names follow Git validation without a suffix")
    func validatesBranchNames() async throws {
        #expect(GitTaskWorktree.branchName("Fix Sidebar!", prefix: "swarm/") == "swarm/fix-sidebar")
        #expect(GitTaskWorktree.branchName("!!!", prefix: "") == "task")
        #expect(GitTaskWorktree.branchName(" ", prefix: "swarm/") == nil)
        #expect(GitTaskWorktree.branchName(String(repeating: "x", count: 60), prefix: "") == String(repeating: "x", count: 40))
        for prefix in ["swarm/", "", "-", "/", "bad//", ".hidden/", "name.lock/", "bad../", "bad@{/", "bad /", "bad\\/", "bad~/", "bad^/", "bad:/", "bad?/", "bad*/", "bad[/", "bad\n/", "é/"] {
            let candidate = prefix + "fix-sidebar"
            let git = try await Shell.run("git", ["check-ref-format", "--branch", candidate])
            #expect((GitTaskWorktree.branchName("Fix Sidebar", prefix: prefix) != nil) == git.ok)
        }
    }

    @Test("Local branches keep their name and commit")
    func existingLocalBranch() async throws {
        let fixture = try await WorktreeFixture()
        defer { fixture.remove() }
        try await fixture.git(["branch", "fix/local"])
        let path = try await fixture.create("Local work", start: .existingBranch("fix/local"))
        #expect(try await fixture.git(["branch", "--show-current"], at: path) == "fix/local")
        #expect(try await fixture.git(["rev-parse", "HEAD"], at: path) == fixture.baseCommit)
    }

    @Test("A branch keeps its name beside a tag of the same name")
    func branchAndTag() async throws {
        let fixture = try await WorktreeFixture()
        defer { fixture.remove() }
        try await fixture.git(["branch", "v1"])
        try await fixture.git(["tag", "v1"])
        let references = try await GitTaskWorktree.references(in: fixture.common)
        #expect(references.local.contains("v1"))
        #expect(!references.local.contains("heads/v1"))
        let path = try await fixture.create("Release", start: .existingBranch("v1"))
        #expect(try await fixture.git(["branch", "--show-current"], at: path) == "v1")
    }

    @Test("The main worktree holds main and the first free branch is selected")
    func heldBranches() async throws {
        let fixture = try await WorktreeFixture()
        defer { fixture.remove() }
        try await fixture.git(["branch", "fix/free"])
        let references = try await GitTaskWorktree.references(in: fixture.common)
        #expect(references.held == ["main"])
        #expect(references.availableBranches.first == "fix/free")
        #expect(references.bases.first == "main")
        #expect(!NewWorkspaceForm.canCreate(
            WorkspaceRequest(name: "Held", start: .existingBranch("main"), prefix: "swarm/"), references: references
        ))
    }

    @Test("The raw Git check ends a sleeping command at its deadline")
    func rawGitTimeout() async throws {
        let fixture = try await WorktreeFixture()
        defer { fixture.remove() }
        await #expect(throws: ShellFailure.timedOut(command: try #require(Shell.which("git")))) {
            try await Git.checkRaw(["-c", "alias.wait=!sleep 2", "wait"], in: fixture.project.path, timeout: .milliseconds(100))
        }
        #expect(GitTaskWorktreeError.pullRequestFetchTimedOut(7).localizedDescription
            == "Fetching pull request #7 from origin took longer than 60 s.")
    }

    @Test("A new branch uses the selected base and an empty prefix")
    func selectedBase() async throws {
        let fixture = try await WorktreeFixture()
        defer { fixture.remove() }
        try await fixture.git(["checkout", "-b", "feature"])
        try await fixture.git(["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--allow-empty", "-m", "feature"])
        let featureHead = try await fixture.git(["rev-parse", "HEAD"])
        let path = try await GitTaskWorktree.create(
            WorkspaceRequest(name: "From feature", start: .newBranch(base: "feature"), prefix: ""),
            in: fixture.project.path, commonDirectory: fixture.common,
            under: fixture.root.appendingPathComponent("workspaces").path
        )
        #expect(try await fixture.git(["rev-parse", "HEAD"], at: path) == featureHead)
        #expect(try await fixture.git(["branch", "--show-current"], at: path) == "from-feature")
        #expect(URL(fileURLWithPath: path).lastPathComponent == "from-feature")
    }

    @Test("Origin branches work offline and track the known head while PRs fetch theirs")
    func remoteStarts() async throws {
        let fixture = try await WorktreeFixture()
        defer { fixture.remove() }
        let origin = fixture.root.appendingPathComponent("origin")
        try await Shell.check("git", ["clone", fixture.project.path, origin.path])
        try await fixture.git(["remote", "add", "origin", origin.path])
        try await fixture.git(["checkout", "-b", "fix/remote"], at: origin.path)
        try await fixture.git(["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--allow-empty", "-m", "remote"], at: origin.path)
        try await fixture.git(["fetch", "origin"])
        try await fixture.git(["remote", "set-head", "origin", "main"])
        let trackedHead = try await fixture.git(["rev-parse", "origin/fix/remote"])
        try await fixture.git(["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--allow-empty", "-m", "latest"], at: origin.path)
        let remoteHead = try await fixture.git(["rev-parse", "HEAD"], at: origin.path)
        try await fixture.git(["update-ref", "refs/pull/7/head", remoteHead], at: origin.path)
        try await fixture.git(["remote", "set-url", "origin", fixture.root.appendingPathComponent("offline-origin").path])
        let remotePath = try await fixture.create("Remote work", start: .existingBranch("origin/fix/remote"))
        #expect(try await fixture.git(["rev-parse", "HEAD"], at: remotePath) == trackedHead)
        #expect(try await fixture.git(["rev-parse", "--abbrev-ref", "@{upstream}"], at: remotePath) == "origin/fix/remote")
        try await fixture.git(["remote", "set-url", "origin", origin.path])
        let pullPath = try await fixture.create("Review seven", start: .pullRequest(7))
        #expect(try await fixture.git(["rev-parse", "HEAD"], at: pullPath) == remoteHead)
        #expect(try await fixture.git(["branch", "--show-current"], at: pullPath) == "swarm/review-seven")
        let refs = try await GitTaskWorktree.references(in: fixture.common)
        #expect(refs.defaultBranch == "main")
        #expect(refs.local.first == "main")
        #expect(refs.remote.contains("origin/fix/remote"))
        #expect(!refs.remote.contains("origin/HEAD"))
        #expect(refs.bases.first == "main")
        try await fixture.git(["branch", "-m", "main", "trunk"])
        try await fixture.git(["remote", "set-head", "origin", "fix/remote"])
        let remoteDefault = try await GitTaskWorktree.references(in: fixture.common)
        #expect(remoteDefault.defaultBranch == "fix/remote")
        #expect(remoteDefault.bases.first == "fix/remote")
        try await fixture.git(["branch", "release"], at: origin.path)
        try await fixture.git(["fetch", "origin"])
        try await fixture.git(["remote", "set-head", "origin", "release"])
        let remoteOnlyDefault = try await GitTaskWorktree.references(in: fixture.common)
        #expect(remoteOnlyDefault.defaultBranch == "origin/release")
        #expect(remoteOnlyDefault.remote.first == "origin/release")
        #expect(remoteOnlyDefault.bases.first == "origin/release")
    }

    @Test("An existing folder is refused without changing it")
    func refusesExistingFolder() async throws {
        let fixture = try await WorktreeFixture()
        defer { fixture.remove() }
        let folder = fixture.root.appendingPathComponent("workspaces/occupied")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let marker = folder.appendingPathComponent("keep.txt")
        try "keep".write(to: marker, atomically: true, encoding: .utf8)
        do {
            _ = try await fixture.create("Occupied", start: .newBranch(base: "main"))
            Issue.record("An existing folder must refuse creation")
        } catch GitTaskWorktreeError.folderExists(let path) {
            #expect(path == folder.path)
        }
        #expect(try String(contentsOf: marker, encoding: .utf8) == "keep")
        #expect(try await fixture.git(["branch", "--list", "swarm/occupied"]) == "")
    }

    @Test("An unborn repository refuses existing branches and pull requests")
    func unbornStarts() async throws {
        let fixture = try await WorktreeFixture(committed: false)
        defer { fixture.remove() }
        let refs = try await GitTaskWorktree.references(in: fixture.common)
        #expect(refs.defaultBranch == nil)
        #expect(refs.bases.isEmpty)
        for start in [WorkspaceStart.existingBranch("main"), .pullRequest(7)] {
            await #expect(throws: GitTaskWorktreeError.self) {
                try await fixture.create("Refused", start: start)
            }
        }
        #expect(!FileManager.default.fileExists(atPath: fixture.root.appendingPathComponent("workspaces").path))
        let spawnsBeforeRefusal = Shell.spawnCount
        do {
            _ = try await GitTaskWorktree.create(
                WorkspaceRequest(name: "Invalid", start: .newBranch(base: ""), prefix: "bad//"),
                in: fixture.project.path, commonDirectory: fixture.common,
                under: fixture.root.appendingPathComponent("workspaces").path
            )
            Issue.record("An invalid branch must refuse creation")
        } catch GitTaskWorktreeError.invalidBranchName {
            #expect(Shell.spawnCount == spawnsBeforeRefusal)
        }
    }

}


private struct WorktreeFixture {
    let root: URL
    let project: URL
    let baseCommit: String
    var common: String { project.appendingPathComponent(".git").path }

    init(committed: Bool = true) async throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try await Shell.check("git", ["init", "-b", "main", project.path])
        if committed {
            try await Shell.check("git", ["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--allow-empty", "-m", "base"], cwd: project.path)
            baseCommit = try await Shell.check("git", ["rev-parse", "HEAD"], cwd: project.path).trimmed
        } else {
            baseCommit = ""
        }
    }

    @discardableResult
    func git(_ arguments: [String], at path: String? = nil) async throws -> String {
        try await Shell.check("git", arguments, cwd: path ?? project.path).trimmed
    }

    func create(_ name: String, start: WorkspaceStart) async throws -> String {
        try await GitTaskWorktree.create(
            WorkspaceRequest(name: name, start: start, prefix: "swarm/"),
            in: project.path, commonDirectory: common, under: root.appendingPathComponent("workspaces").path
        )
    }

    func remove() { try? FileManager.default.removeItem(at: root) }
}
