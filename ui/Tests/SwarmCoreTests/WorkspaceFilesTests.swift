import Foundation
import Testing
@testable import SwarmCore

@Suite("Workspace files")
struct WorkspaceFilesTests {
    @Test("Files include hidden entries, nested paths, and link markers but omit Git metadata")
    func listingAndPreview() async throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        for folder in ["one", "two", ".git"] {
            try FileManager.default.createDirectory(at: root.appendingPathComponent(folder), withIntermediateDirectories: true)
        }
        for path in ["one/same.txt", "two/same.txt", ".hidden"] {
            try path.write(to: root.appendingPathComponent(path), atomically: true, encoding: .utf8)
        }
        try FileManager.default.createSymbolicLink(atPath: root.path + "/link", withDestinationPath: "/etc")
        let files = try await WorkspaceFiles.list(in: root.path)
        #expect(files.entries.map(\.name) == ["one", "two", ".hidden", "link"])
        #expect(files.entries.last?.kind == .symbolicLink)
        #expect(!files.truncated)
        #expect(try await WorkspaceFiles.list(in: root.path, path: "one").entries.first?.path == "one/same.txt")
        #expect(try await WorkspaceFiles.preview(in: root.path, path: "two/same.txt") == .text("two/same.txt"))
        for path in ["link", "../", "/etc", ".git", "one/../two", "..\0", ".git\0"] {
            await #expect(throws: WorkspaceReadError.self) { try await WorkspaceFiles.list(in: root.path, path: path) }
        }
        await #expect(throws: WorkspaceReadError.self) { try await WorkspaceFiles.preview(in: root.path, path: "link/passwd") }
    }

    @Test("Directory listing reports its limit without scanning the whole tree")
    func listingLimit() async throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        for index in 0..<2_001 { FileManager.default.createFile(atPath: root.path + "/file-\(index)", contents: Data()) }
        let listing = try await WorkspaceFiles.list(in: root.path)
        #expect(listing.entries.count == 2_000)
        #expect(listing.truncated)
    }

    @Test("Untracked previews are valid patches, including unusual names and missing newlines")
    func addedFilePatch() throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let path = "quote\"\nfile.txt"
        try "<script>window.pwned=true</script>\nlast".write(to: root.appendingPathComponent(path), atomically: true, encoding: .utf8)
        let patch = try UntrackedPreview.read(path, in: root.path)
        #expect(patch.hasPrefix("diff --git "))
        #expect(patch.contains("@@ -0,0 +1,2 @@"))
        #expect(patch.contains("+<script>window.pwned=true</script>"))
        #expect(patch.contains("+last\n\\ No newline at end of file"))
        #expect(patch.contains("quote\\\"\\nfile.txt"))
        try "".write(to: root.appendingPathComponent("empty"), atomically: true, encoding: .utf8)
        #expect(try UntrackedPreview.read("empty", in: root.path).contains("+1,0"))
    }

    private func fixture() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("workspace-files-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root.resolvingSymlinksInPath()
    }
}
