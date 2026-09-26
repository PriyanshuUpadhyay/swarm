import Foundation
import Testing
@testable import SwarmCore

@Suite("Composer file discovery", .serialized)
struct ComposerFileDiscoveryTests {
    @Test("Home folders and ancestors are rejected before Git can scan personal files")
    func broadRoots() async throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let home = root.appendingPathComponent("home")
        let music = home.appendingPathComponent("Music")
        try FileManager.default.createDirectory(at: music, withIntermediateDirectories: true)
        try "private".write(to: music.appendingPathComponent("library.txt"), atomically: true, encoding: .utf8)
        let initialized = try await Shell.run("git", ["init", "-q"], cwd: home.path)
        #expect(initialized.ok)
        let alias = root.appendingPathComponent("home-alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: home)
        let before = Shell.spawnCount
        for path in [home.path, home.path + "/", alias.path, root.path, "/"] {
            let files = await ComposerFileCatalog.discover(from: .init(root: path), homeDirectory: home.path)
            #expect(files.isEmpty)
        }
        #expect(Shell.spawnCount == before)
    }

    @Test("Project file suggestions still work for Git and non-Git folders")
    func projectRoots() async throws {
        let home = try fixture()
        defer { try? FileManager.default.removeItem(at: home) }
        let project = home.appendingPathComponent("project")
        let nested = project.appendingPathComponent("Sources")
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
        try "let value = 1".write(to: nested.appendingPathComponent("Example.swift"), atomically: true, encoding: .utf8)
        let source = ComposerMentionSource(root: project.path)
        #expect(await ComposerFileCatalog.discover(from: source, homeDirectory: home.path) == ["Sources/Example.swift"])
        let initialized = try await Shell.run("git", ["init", "-q"], cwd: project.path)
        #expect(initialized.ok)
        #expect(await ComposerFileCatalog.discover(from: source, homeDirectory: home.path) == ["Sources/Example.swift"])
    }

    private func fixture() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("composer-discovery-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root.resolvingSymlinksInPath()
    }
}
