import Foundation
import Testing
@testable import SwarmCore

@Suite("Owner choices")
@MainActor
struct OwnerChoicesTests {
    @Test("An empty swarm home neither reads nor writes choices")
    func emptyHome() throws {
        let folder = SwarmHome.dataFolder(home: "")
        #expect(folder == nil)
        let store = OwnerChoicesStore(folder: folder)
        #expect(try store.load() == OwnerChoices())
        #expect(throws: OwnerChoicesError.self) {
            try store.update { $0.pinned.insert("/project/main") }
        }
    }

    @Test("Choices round-trip while selection and folds stay in defaults")
    func roundTrip() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/project/main"]
        navigation.archived = ["/project/old"]
        navigation.names = ["/project/main": "Main work"]
        navigation.selectedWorkspace = "/project/main"
        navigation.selectedChats = ["/project/main": "chat"]
        navigation.collapsed = ["/project"]
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: folder)
        store.save(navigation)
        #expect(WorkspaceNavigationStore(defaults: defaults, choicesFolder: folder).load() == navigation)
        let choices = try OwnerChoicesStore(folder: folder).load()
        #expect(choices.pinned == navigation.pinned)
        #expect(choices.archived == navigation.archived)
        #expect(choices.names == navigation.names)
        let data = try #require(defaults.data(forKey: "workspaces.navigation"))
        let viewState = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        #expect(viewState["pinned"] == nil)
        #expect(viewState["archived"] == nil)
        #expect(viewState["names"] == nil)
        #expect(viewState["selectedWorkspace"] as? String == "/project/main")
        #expect(viewState["collapsed"] as? [String] == ["/project"])
    }

    @Test("Absent choices keys start empty")
    func absentKeys() throws {
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: Data("{}".utf8)) == OwnerChoices())
    }

    @Test("A bad file is kept before empty choices are saved")
    func badFile() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let bad = Data("{broken".utf8)
        try bad.write(to: folder.appendingPathComponent("choices.json"))
        let store = OwnerChoicesStore(folder: folder)
        #expect(try store.load() == OwnerChoices())
        let backup = try #require(FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
            .first { $0.lastPathComponent.hasPrefix("choices.json.bad-") })
        #expect(try Data(contentsOf: backup) == bad)
        try store.update { $0.pinned.insert("/project/main") }
        #expect(try Data(contentsOf: backup) == bad)
        #expect(try store.load().pinned == ["/project/main"])
    }

    @Test("Pins in two data homes stay apart with shared view defaults")
    func separateHomes() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: root)
        }
        let first = WorkspaceNavigationStore(defaults: defaults, choicesFolder: root.appendingPathComponent("first"))
        let second = WorkspaceNavigationStore(defaults: defaults, choicesFolder: root.appendingPathComponent("second"))
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/first"]
        first.save(navigation)
        #expect(second.load().pinned.isEmpty)
        navigation.pinned = ["/second"]
        second.save(navigation)
        #expect(first.load().pinned == ["/first"])
        #expect(second.load().pinned == ["/second"])
    }
}
