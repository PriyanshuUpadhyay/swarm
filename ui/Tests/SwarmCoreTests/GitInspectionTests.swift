import Darwin
import Foundation
import Testing
@testable import SwarmCore

@Suite("Workspace inspection")
struct GitInspectionTests {
    @Test("Local layers and branch comparison show different changes with literal paths")
    func distinctScopes() async throws {
        let root = try await repository()
        defer { try? FileManager.default.removeItem(at: root) }
        let name = ":(glob)*.txt"
        try "base\n".write(to: root.appendingPathComponent(name), atomically: true, encoding: .utf8)
        try await git(["--literal-pathspecs", "add", "--", name], root)
        try await commit(root, "base")
        try await git(["checkout", "-b", "feature"], root)
        try "base\ncommitted\n".write(to: root.appendingPathComponent(name), atomically: true, encoding: .utf8)
        try await git(["--literal-pathspecs", "add", "--", name], root)
        try await commit(root, "feature")
        try "base\ncommitted\nstaged\n".write(to: root.appendingPathComponent(name), atomically: true, encoding: .utf8)
        try await git(["--literal-pathspecs", "add", "--", name], root)
        try "base\ncommitted\nstaged\nunstaged\n".write(to: root.appendingPathComponent(name), atomically: true, encoding: .utf8)
        let unusual = "space\tand\nline.txt"
        try "new".write(to: root.appendingPathComponent(unusual), atomically: true, encoding: .utf8)
        try await git(["config", "diff.external", "/bin/false"], root)
        let snapshot = try await Git.inspect(in: root.path)
        #expect(snapshot.branch == "feature")
        #expect(snapshot.files.contains { $0.path == unusual && $0.layer == .untracked })
        let staged = try #require(snapshot.files.first { $0.path == name && $0.layer == .staged })
        let unstaged = try #require(snapshot.files.first { $0.path == name && $0.layer == .unstaged })
        let stagedPatch = try await Git.localPatch(staged, in: snapshot)
        #expect(stagedPatch.contains("+staged"))
        #expect(!stagedPatch.contains("+unstaged"))
        #expect(try await Git.localPatch(unstaged, in: snapshot).contains("+unstaged"))
        let comparison = try await Git.compareBranch(in: snapshot, baseRef: "refs/heads/main")
        #expect(comparison.files.map(\.path) == [name])
        let branchPatch = try await Git.branchPatch(try #require(comparison.files.first), comparison: comparison, in: snapshot)
        #expect(branchPatch.contains("+committed"))
        #expect(!branchPatch.contains("+staged"))
        #expect(!branchPatch.contains("+unstaged"))
        try await git(["branch", "-D", "main"], root)
        await #expect(throws: (any Error).self) { try await Git.compareBranch(in: snapshot, baseRef: "refs/heads/main") }
    }

    @Test("Unborn repositories retain local work and detached HEAD remains explicit")
    func unbornAndDetached() async throws {
        let root = try await repository()
        defer { try? FileManager.default.removeItem(at: root) }
        try "staged".write(to: root.appendingPathComponent("one"), atomically: true, encoding: .utf8)
        try await git(["add", "one"], root)
        try "new".write(to: root.appendingPathComponent("two"), atomically: true, encoding: .utf8)
        let unborn = try await Git.inspect(in: root.path)
        #expect(unborn.head == nil)
        #expect(unborn.files.count == 2)
        let staged = try #require(unborn.files.first { $0.layer == .staged })
        #expect(try await Git.localPatch(staged, in: unborn).contains("+staged"))
        try await commit(root, "first")
        try await git(["checkout", "--detach", "HEAD"], root)
        let detached = try await Git.inspect(in: root.path)
        #expect(detached.branch == nil)
        #expect(detached.head != nil)
        #expect(detached.files.contains { $0.layer == .untracked })
        #expect(try await Git.compareBranch(in: detached, baseRef: "refs/heads/main").files.isEmpty)
    }

    @Test("Conflicts are separate from staged and unstaged files")
    func conflicts() async throws {
        let root = try await repository()
        defer { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("conflict")
        try "base\n".write(to: file, atomically: true, encoding: .utf8)
        try await git(["add", "."], root)
        try await commit(root, "base")
        try await git(["checkout", "-b", "other"], root)
        try "other\n".write(to: file, atomically: true, encoding: .utf8)
        try await git(["add", "."], root)
        try await commit(root, "other")
        try await git(["checkout", "main"], root)
        try "main\n".write(to: file, atomically: true, encoding: .utf8)
        try await git(["add", "."], root)
        try await commit(root, "main")
        let merge = try await Shell.run("git", ["merge", "other"], cwd: root.path)
        #expect(!merge.ok)
        let snapshot = try await Git.inspect(in: root.path)
        #expect(snapshot.files.count == 1)
        #expect(snapshot.files.first?.layer == .conflicted)
        #expect(try await Git.localPatch(try #require(snapshot.files.first), in: snapshot).contains("<<<<<<<"))
    }

    @Test("Untracked preview refuses symlinks, directory escapes, binary data and large files")
    func boundedPreview() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("preview-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let rootPath = root.resolvingSymlinksInPath().path
        try FileManager.default.createSymbolicLink(atPath: root.path + "/link", withDestinationPath: "/etc/passwd")
        try FileManager.default.createSymbolicLink(atPath: root.path + "/directory", withDestinationPath: "/etc")
        #expect(try UntrackedPreview.read("link", in: rootPath).contains("symbolic link"))
        #expect(try UntrackedPreview.read("directory/passwd", in: rootPath).contains("symbolic link"))
        #expect(try UntrackedPreview.read("../outside", in: rootPath).contains("cannot be previewed"))
        let huge = root.appendingPathComponent("huge")
        _ = FileManager.default.createFile(atPath: huge.path, contents: nil)
        let handle = try FileHandle(forWritingTo: huge)
        try handle.truncate(atOffset: 100 * 1024 * 1024)
        try handle.close()
        #expect(try UntrackedPreview.read("huge", in: rootPath).contains("256 KiB"))
        try Data([0, 1, 2]).write(to: root.appendingPathComponent("binary"))
        #expect(try UntrackedPreview.read("binary", in: rootPath) == "Binary file. No text preview.")
        try Data("hello\n".utf8).write(to: root.appendingPathComponent("text"))
        #expect(try UntrackedPreview.read("text", in: rootPath).contains("+hello"))
    }

    @Test("Cancellation stops the actual command process; output caps fail rather than truncate")
    func processBounds() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("cancel-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("wait.sh")
        let pidFile = root.appendingPathComponent("pid")
        try "printf '%s' \"$$\" > \"$1\"\nexec /bin/sleep 30\n".write(to: script, atomically: true, encoding: .utf8)
        let task = Task { try await Shell.run("/bin/sh", [script.path, pidFile.path], timeout: .seconds(5)) }
        defer { task.cancel() }
        for _ in 0..<100 where !FileManager.default.fileExists(atPath: pidFile.path) {
            try await Task.sleep(for: .milliseconds(10))
        }
        let pid = try #require(Int32(try String(contentsOf: pidFile, encoding: .utf8)))
        #expect(kill(pid, 0) == 0)
        task.cancel()
        await #expect(throws: CancellationError.self) { try await task.value }
        errno = 0
        #expect(kill(pid, 0) == -1 && errno == ESRCH)
        await #expect(throws: (any Error).self) {
            try await Shell.run("/usr/bin/yes", [], timeout: .seconds(2), outputLimit: 1024)
        }
    }

    private func repository() async throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("inspect-\(UUID().uuidString)")
        try await Shell.check("git", ["init", "-b", "main", root.path])
        try await git(["config", "user.name", "Test"], root)
        try await git(["config", "user.email", "test@example.com"], root)
        return root
    }

    private func git(_ args: [String], _ root: URL) async throws {
        _ = try await Shell.check("git", args, cwd: root.path)
    }

    private func commit(_ root: URL, _ message: String) async throws {
        try await git(["-c", "commit.gpgsign=false", "commit", "-m", message], root)
    }
}
