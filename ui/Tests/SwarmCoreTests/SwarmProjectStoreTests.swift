import Foundation
import Testing
@testable import SwarmCore

@Suite("Project folders")
@MainActor
struct SwarmProjectStoreTests {
    @Test("Opened and created folders remain available, and a created folder is a git repo")
    func savedFolders() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer {
            try? FileManager.default.removeItem(at: root)
        }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let store = SwarmProjectStore(choicesFolder: try claimedChoicesFolder(root.appendingPathComponent("choices")))
        let created = root.appendingPathComponent("New Project")

        #expect(try await store.create(at: created) == created.path)
        #expect(FileManager.default.fileExists(atPath: created.path))
        #expect(Git.repositoryPaths(in: created.path) != nil)
        #expect(try await store.add(created) == created.path)
        #expect(SwarmProjectStore(choicesFolder: try claimedChoicesFolder(root.appendingPathComponent("choices"))).paths() == [created.path])
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
}
