import Foundation
import Testing
@testable import SwarmCore

@Suite("Project folders")
@MainActor
struct SwarmProjectStoreTests {
    @Test("Opened and created folders remain available")
    func savedFolders() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "SwarmProjectStoreTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: root)
        }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let store = SwarmProjectStore(defaults: defaults)
        let created = root.appendingPathComponent("New Project")

        #expect(try store.create(at: created) == created.path)
        #expect(FileManager.default.fileExists(atPath: created.path))
        #expect(!FileManager.default.fileExists(atPath: created.appendingPathComponent(".git").path))
        #expect(try store.add(created) == created.path)
        #expect(SwarmProjectStore(defaults: defaults).paths() == [created.path])
        #expect(throws: SwarmProjectError.self) { try store.create(at: created) }
    }
}
