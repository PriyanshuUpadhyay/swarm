import Darwin
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
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(folder))
        store.save(navigation)
        #expect(WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(folder)).load() == navigation)
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
        let store = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
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
        let first = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(root.appendingPathComponent("first")))
        let second = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(root.appendingPathComponent("second")))
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/first"]
        first.save(navigation)
        #expect(second.load().pinned.isEmpty)
        navigation.pinned = ["/second"]
        second.save(navigation)
        #expect(first.load().pinned == ["/first"])
        #expect(second.load().pinned == ["/second"])
    }
    @Test("An unclaimed home keeps all files unchanged and reports the failed save")
    func unclaimedHome() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = OwnerChoicesStore(folder: folder)
        #expect(throws: (any Error).self) {
            try store.update { $0.pinned.insert("/project/main") }
        }
        #expect(!FileManager.default.fileExists(atPath: folder.path))
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appendingPathComponent("choices.json")
        let bad = Data("{broken".utf8)
        try bad.write(to: file)
        #expect(try store.load() == OwnerChoices())
        #expect(try Data(contentsOf: file) == bad)
        #expect(try FileManager.default.contentsOfDirectory(atPath: folder.path) == ["choices.json"])
    }

    @Test("An update waits for another process and loads its completed choices")
    func processLock() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data("swarm\n".utf8).write(to: folder.appendingPathComponent("swarm-home"))
        let ready = folder.appendingPathComponent("ready")
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        child.arguments = ["-c", """
        import fcntl, json, pathlib, sys, time
        folder = pathlib.Path(sys.argv[1])
        with (folder / 'choices.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            (folder / 'ready').write_text('locked')
            time.sleep(0.25)
            (folder / 'choices.json').write_text(json.dumps({'pinned': ['/other/process']}))
        """, folder.path]
        try child.run()
        for _ in 0..<500 where !FileManager.default.fileExists(atPath: ready.path) { usleep(10_000) }
        #expect(FileManager.default.fileExists(atPath: ready.path))
        let store = OwnerChoicesStore(folder: folder)
        try store.update { $0.pinned.insert("/this/process") }
        child.waitUntilExit()
        #expect(child.terminationStatus == 0)
        #expect(try store.load().pinned == ["/other/process", "/this/process"])
    }

    @Test("A failed choices write still saves selection and folds in view defaults")
    func failedChoicesKeepsViewState() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var navigation = WorkspaceNavigation()
        navigation.selectedWorkspace = "/project/main"
        navigation.selectedChats = ["/project/main": "chat"]
        navigation.collapsed = ["pinned"]
        navigation.pinned = ["/project/main"]
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: folder)
        #expect(store.save(navigation)?.contains("Run swarm init") == true)
        let loaded = store.load()
        #expect(loaded.selectedWorkspace == navigation.selectedWorkspace)
        #expect(loaded.selectedChats == navigation.selectedChats)
        #expect(loaded.collapsed == navigation.collapsed)
        #expect(!FileManager.default.fileExists(atPath: folder.path))
    }

}
