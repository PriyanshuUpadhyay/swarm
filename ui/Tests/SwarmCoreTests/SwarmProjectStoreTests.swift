import Darwin
import Foundation
import Testing
@testable import SwarmCore

private enum InitializeFailure: Error { case rejected }

@Suite("Project folders")
@MainActor
struct SwarmProjectStoreTests {
    @Test("Create returns the first commit result and resolved project path", arguments: [FirstCommit.made, .skippedNoIdentity, .failed("Hook refused")])
    func returnsFirstCommit(firstCommit: FirstCommit) async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(root.appendingPathComponent("choices")))
        let project = root.appendingPathComponent("project")
        let store = SwarmProjectStore(choices: choices, initializeRepository: { path in
            #expect(path == project.path)
            return firstCommit
        })

        let created = try await store.create(at: project)
        #expect(created.path == project.path)
        #expect(created.firstCommit == firstCommit)
        #expect(try choices.load().projectPaths == [project.path])
    }

    @Test("Opened and created folders remain available, and a created folder is a git repo")
    func savedFolders() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer {
            try? FileManager.default.removeItem(at: root)
        }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let environment = try GitProjectInitializationTests.projectEnvironment(in: root, identity: "")
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(root.appendingPathComponent("choices")))
        let store = SwarmProjectStore(choices: choices, initializeRepository: {
            try await Git.initializeProject(at: $0, environment: environment)
        })
        let created = root.appendingPathComponent("New Project")

        #expect(try await store.create(at: created).path == created.path)
        #expect(FileManager.default.fileExists(atPath: created.path))
        #expect(Git.repositoryPaths(in: created.path) != nil)
        #expect(try await store.add(created) == created.path)
        #expect(try savedChoices(from: SwarmProjectStore(choicesFolder: root.appendingPathComponent("choices"))).projectPaths == [created.path])
        #expect(try OwnerChoicesStore(folder: root.appendingPathComponent("choices")).load().projectPaths == [created.path])
        await #expect(throws: SwarmProjectError.self) { try await store.create(at: created) }
    }

    @Test("A folder that git init turns into a repository is listed as one after its identities are forgotten")
    func gitInitChangesIdentity() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let folder = root.appendingPathComponent("notes")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let discovery = SwarmSessionDiscovery()
        guard case .folder = await discovery.identity(for: folder.path) else {
            Issue.record("a plain folder is a folder"); return
        }

        #expect(await Git.isRepository(at: folder.path) == false)
        let bare = root.appendingPathComponent("app.git")
        try await Shell.check("git", ["init", "-q", "--bare", bare.path])
        // A bare clone is no `.folder` for git, so the app must never offer git init in it.
        #expect(await Git.isRepository(at: bare.path))

        try await Git.initialize(at: folder.path)
        await discovery.forgetIdentities()
        guard case .repository = await discovery.identity(for: folder.path) else {
            Issue.record("after git init the folder is a repository"); return
        }
    }

    @Test("A rejected first commit keeps the created project and reports the cause")
    func rejectedFirstCommit() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let environment = try GitProjectInitializationTests.projectEnvironment(in: root, identity: "name = Test\nemail = test@example.com")
        let hooks = root.appendingPathComponent("hooks")
        try FileManager.default.createDirectory(at: hooks, withIntermediateDirectories: true)
        let preCommit = hooks.appendingPathComponent("pre-commit")
        try "#!/bin/sh\necho 'Commit rejected by test hook' >&2\nexit 1\n".write(to: preCommit, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: preCommit.path)
        let gitConfig = try #require(environment["GIT_CONFIG_GLOBAL"])
        try await Shell.check("git", ["config", "--file", gitConfig, "core.hooksPath", hooks.path], env: environment)
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(root.appendingPathComponent("choices")))
        let store = SwarmProjectStore(choices: choices, initializeRepository: {
            try await Git.initializeProject(at: $0, environment: environment)
        })
        let project = root.appendingPathComponent("project")

        let created = try await store.create(at: project)
        guard case .failed(let reason) = created.firstCommit else {
            Issue.record("The rejected hook must return a failed first commit")
            return
        }
        #expect(reason.contains("Commit rejected by test hook"))
        #expect(created.firstCommit.notice == "Created without a first commit. \(reason)")
        #expect(try choices.load().projectPaths == [project.path])
        #expect(try String(contentsOf: project.appendingPathComponent(".gitignore"), encoding: .utf8) == "tmp/\n")
        #expect(try await Shell.run("git", ["rev-parse", "--verify", "HEAD"], cwd: project.path, env: environment).ok == false)
        #expect(try await Shell.check("git", ["ls-files"], cwd: project.path, env: environment).trimmed == ".gitignore")
    }

    @Test("The no-identity notice requires both identity and a later first commit")
    func firstCommitNotices() {
        #expect(FirstCommit.made.notice == nil)
        #expect(FirstCommit.skippedNoIdentity.notice
            == "Created without a first commit, because git has no user.name and user.email. Set them, then commit once; until then workspaces start orphan branches.")
    }

    @Test("A bare repository outside a hub stays one project after its chat is archived")
    func remembersBareWorktree() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let seed = root.appendingPathComponent("seed")
        let bare = root.appendingPathComponent("app.git")
        let worktree = root.appendingPathComponent("main")
        try await Shell.check("git", ["init", "-q", "-b", "main", seed.path])
        try await Shell.check("git", ["-C", seed.path, "-c", "user.name=Test", "-c", "user.email=test@example.com",
                                      "commit", "-q", "--allow-empty", "-m", "base"])
        try await Shell.check("git", ["clone", "-q", "--bare", seed.path, bare.path])
        try await Shell.check("git", ["-C", bare.path, "worktree", "add", "-q", worktree.path, "main"])
        let listed = try await Git.worktrees(of: bare.path)
        let chat = SwarmSession(id: .init("bare-chat"), talkMode: "lane", adapter: "tmux-solo", cwd: worktree.path,
                                createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
        func build(_ sessions: [SwarmSession], paths: [String] = []) -> SessionsTree {
            SessionsTree.build(sessions: sessions, projectPaths: paths, repositoryPathsResolver: Git.repositoryPaths,
                                worktreeLister: { _ in listed })
        }
        let store = SwarmProjectStore(choicesFolder: try claimedChoicesFolder(root.appendingPathComponent("choices")))
        let initial = build([chat])
        #expect(initial.projects.count == 1)
        try refresh(initial, in: store, choicesFolder: root.appendingPathComponent("choices"))
        let refreshed = build([chat], paths: try savedChoices(from: store).projectPaths)
        #expect(refreshed.projects.count == 1)
        let archived = build([], paths: try savedChoices(from: store).projectPaths)
        #expect(archived.projects.count == 1)
        #expect(archived.projects.first?.id == initial.projects.first?.id)
        #expect(archived.projects.first?.path == bare.path)
        #expect(archived.projects.first?.workspaces.map(\.path) == listed.filter { !$0.isBare }.map(\.path))
    }

    @Test("A choices failure during Create leaves no new folder or initialized repository")
    func failedCreateChoices() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let target = root.appendingPathComponent("New Project")
        let store = SwarmProjectStore(choicesFolder: nil)
        await #expect(throws: OwnerChoicesError.self) { try await store.create(at: target) }
        #expect(!FileManager.default.fileExists(atPath: target.path))
    }

    @Test("A failed choices rollback keeps the original git init error")
    func failedRollbackKeepsInitializeError() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let folder = try claimedChoicesFolder(root.appendingPathComponent("choices"))
        let target = root.appendingPathComponent("new")
        var failRead = false
        let choices = OwnerChoicesStore(folder: folder, readFile: {
            if failRead { throw CocoaError(.fileReadNoPermission) }
            return try Data(contentsOf: $0)
        })
        let store = SwarmProjectStore(choices: choices, initializeRepository: { _ in
            failRead = true
            throw InitializeFailure.rejected
        })
        await #expect(throws: InitializeFailure.self) { try await store.create(at: target) }
        #expect(!FileManager.default.fileExists(atPath: target.path))
        #expect(try OwnerChoicesStore(folder: folder).load().projectPaths == [target.path])
    }

    @Test("A failed git init removes only a newly remembered project path with its empty folder")
    func failedInitialize() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(root.appendingPathComponent("choices")))
        let newProject = root.appendingPathComponent("new")
        let rememberedProject = root.appendingPathComponent("remembered")
        try choices.update { $0.projectPaths = [rememberedProject.path] }
        let store = SwarmProjectStore(choices: choices, initializeRepository: { path in
            try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: path)
            defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: path) }
            return try await Git.initializeProject(at: path)
        })
        await #expect(throws: ShellError.self) { try await store.create(at: newProject) }
        #expect(!FileManager.default.fileExists(atPath: newProject.path))
        #expect(try choices.load().projectPaths == [rememberedProject.path])
        await #expect(throws: ShellError.self) { try await store.create(at: rememberedProject) }
        #expect(!FileManager.default.fileExists(atPath: rememberedProject.path))
        #expect(try choices.load().projectPaths == [rememberedProject.path])
    }

    @Test("Remove resolves saved project paths before holding the choices lock")
    func removeResolvesBeforeLock() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: folder)
        try choices.update { $0.projectPaths = ["/repo/worktree", "/other"] }
        var resolvedPaths: [String] = []
        let store = SwarmProjectStore(choices: choices, initializeRepository: { _ in .made }, projectPathResolver: { savedPath in
            resolvedPaths.append(savedPath)
            let descriptor = open(folder.appendingPathComponent("choices.lock").path, O_RDWR)
            #expect(descriptor >= 0)
            defer { close(descriptor) }
            let acquired = flock(descriptor, LOCK_EX | LOCK_NB) == 0
            #expect(acquired)
            if acquired {
                _ = flock(descriptor, LOCK_UN)
                if savedPath == "/repo/worktree" {
                    do { try choices.update { $0.projectPaths.append("/new-project") } }
                    catch { Issue.record(error) }
                }
            }
            return savedPath == "/repo/worktree" ? "/repo" : savedPath
        })
        let saved = try store.remove("/repo", workspacePaths: ["/repo/worktree"])
        #expect(Set(resolvedPaths) == ["/repo/worktree", "/other"])
        #expect(saved.projectPaths == ["/other", "/new-project"])
        #expect(saved.removedProjects == ["/repo"])
        #expect(try choices.load() == saved)
    }

}
