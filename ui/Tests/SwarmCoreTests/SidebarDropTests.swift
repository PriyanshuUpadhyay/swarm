import Foundation
import Testing
@testable import SwarmCore

@Suite("Sidebar folder drops")
struct SidebarDropTests {
    @Test("Only an existing local folder reaches Import")
    func folderFilter() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let folder = root.appendingPathComponent("folder with spaces", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = root.appendingPathComponent("file", isDirectory: true)
        try Data("a regular file".utf8).write(to: file)
        let missing = root.appendingPathComponent("missing", isDirectory: true)
        let encodedPath = try #require(folder.path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed))
        let remote = try #require(URL(string: "https://example.com" + encodedPath))
        #expect(SidebarDrop.folder(in: []) == nil)
        #expect(SidebarDrop.folder(in: [file, missing, remote]) == nil)
        #expect(SidebarDrop.folder(in: [file, missing, remote, folder]) == folder)
        #expect(SidebarDrop.folder(in: [folder, root]) == folder)
    }

    @Test("A local folder link uses the same import path as a chosen folder")
    func folderLink() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appendingPathComponent("target")
        let link = root.appendingPathComponent("link")
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)
        #expect(SidebarDrop.folder(in: [link]) == link)
    }
}
