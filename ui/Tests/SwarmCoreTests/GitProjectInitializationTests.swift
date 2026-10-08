import Foundation
import Testing
@testable import SwarmCore

@Suite("Project initialization")
struct GitProjectInitializationTests {
    @Test("A configured identity makes one clean first commit and ignores tmp")
    func createsFirstCommit() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let environment = try projectEnvironment(in: root, identity: "name = Test\nemail = test@example.com")
        let project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)

        #expect(try await Git.initializeProject(at: project.path, environment: environment) == .made)
        #expect(try await Shell.check("git", ["rev-list", "--count", "HEAD"], cwd: project.path, env: environment).trimmed == "1")
        #expect(try await Shell.check("git", ["log", "-1", "--format=%s"], cwd: project.path, env: environment).trimmed == "Create project")
        #expect(try await Shell.check("git", ["ls-files"], cwd: project.path, env: environment).trimmed == ".gitignore")
        try FileManager.default.createDirectory(at: project.appendingPathComponent("tmp"), withIntermediateDirectories: false)
        try "step output".write(to: project.appendingPathComponent("tmp/step.txt"), atomically: true, encoding: .utf8)
        #expect(try await Shell.check("git", ["check-ignore", "tmp/step.txt"], cwd: project.path, env: environment).trimmed == "tmp/step.txt")
        #expect(try await Shell.check("git", ["status", "--porcelain"], cwd: project.path, env: environment).trimmed.isEmpty)
    }

    @Test("The first two workspaces share the created project history")
    func sharesFirstCommit() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let environment = try projectEnvironment(in: root, identity: "name = Test\nemail = test@example.com")
        let project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        #expect(try await Git.initializeProject(at: project.path, environment: environment) == .made)
        let head = try await Shell.check("git", ["rev-parse", "HEAD"], cwd: project.path, env: environment).trimmed
        for name in ["First workspace", "Second workspace"] {
            let workspace = try await GitTaskWorktree.create(
                named: name, in: project.path, commonDirectory: project.appendingPathComponent(".git").path,
                under: root.appendingPathComponent("workspaces").path
            )
            #expect(try await Shell.check("git", ["rev-parse", "HEAD"], cwd: workspace, env: environment).trimmed == head)
        }
    }

    @Test("A missing name or email leaves gitignore without a commit", arguments: ["", "name = Test", "email = test@example.com"])
    func skipsWithoutIdentity(identity: String) async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let environment = try projectEnvironment(in: root, identity: identity)
        let project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)

        #expect(try await Git.initializeProject(at: project.path, environment: environment) == .skippedNoIdentity)
        #expect(try String(contentsOf: project.appendingPathComponent(".gitignore"), encoding: .utf8) == "tmp/\n")
        #expect(try await Shell.run("git", ["rev-parse", "--verify", "HEAD"], cwd: project.path, env: environment).ok == false)
        #expect(try await Shell.check("git", ["ls-files"], cwd: project.path, env: environment).trimmed.isEmpty)
    }

    @Test("Existing ignore rules stay intact and tmp is added only once", arguments: ["build/", "build/\n", "build/\ntmp/\n", "tmp/\r\nbuild/\r\n"])
    func preservesIgnoreRules(original: String) async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let environment = try projectEnvironment(in: root, identity: "")
        let project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let ignore = project.appendingPathComponent(".gitignore")
        try Data(original.utf8).write(to: ignore)

        #expect(try await Git.initializeProject(at: project.path, environment: environment) == .skippedNoIdentity)
        let expected = original.components(separatedBy: .newlines).contains("tmp/")
            ? original : original + (original.hasSuffix("\n") ? "" : "\n") + "tmp/\n"
        #expect(try Data(contentsOf: ignore) == Data(expected.utf8))
    }

    private func projectEnvironment(in root: URL, identity: String) throws -> [String: String] {
        let home = root.appendingPathComponent("home")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try ("[user]\n" + identity + "\n").write(to: home.appendingPathComponent(".gitconfig"), atomically: true, encoding: .utf8)
        return ["HOME": home.path, "XDG_CONFIG_HOME": home.path, "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": home.appendingPathComponent(".gitconfig").path]
    }
}
