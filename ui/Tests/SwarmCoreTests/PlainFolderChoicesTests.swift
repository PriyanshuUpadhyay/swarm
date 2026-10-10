import Foundation
import Testing
@testable import SwarmCore

@Suite("Plain folder choices")
@MainActor
struct PlainFolderChoicesTests {
    @Test("Plain folder choices decode with absent, null, and unknown keys")
    func decodesChoices() throws {
        for json in ["{}", "{\"plainFolders\":null}", "{\"futureKey\":true}"] {
            #expect(try JSONDecoder().decode(OwnerChoices.self, from: Data(json.utf8)).plainFolders.isEmpty)
        }
        let data = Data("{\"plainFolders\":[\"/notes\"],\"futureKey\":true}".utf8)
        let choices = try JSONDecoder().decode(OwnerChoices.self, from: data)
        #expect(choices.plainFolders == ["/notes"])
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: JSONEncoder().encode(choices)) == choices)
    }

    @Test("A stale view saves only changed plain folder answers")
    func mergesChoices() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "PlainFolderChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        let choices = OwnerChoicesStore(folder: folder)
        try choices.update { $0.plainFolders = ["/keep", "/initialize"] }
        let navigationStore = WorkspaceNavigationStore(defaults: defaults, choices: choices)
        var navigation = navigationStore.load()
        try choices.update { $0.plainFolders.insert("/other-process") }
        navigation.ownerChoices.plainFolders.remove("/initialize")
        navigation.ownerChoices.plainFolders.insert("/new-folder")

        #expect(navigationStore.save(navigation) == nil)
        #expect(try choices.load().plainFolders == ["/keep", "/other-process", "/new-folder"])
        #expect(WorkspaceNavigationStore(defaults: defaults, choices: choices).load().ownerChoices.plainFolders
                == ["/keep", "/other-process", "/new-folder"])
        let viewData = try #require(defaults.data(forKey: "workspaces.navigation"))
        let viewState = try #require(JSONSerialization.jsonObject(with: viewData) as? [String: Any])
        #expect(viewState["plainFolders"] == nil)
    }

    @Test("Remove Project clears its plain folder answer and keeps nested projects")
    func prunesChoices() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: folder)
        try choices.update {
            $0.projectPaths = ["/notes", "/notes/nested", "/other"]
            $0.plainFolders = ["/notes", "/notes/nested", "/other"]
        }

        let saved = try SwarmProjectStore(choices: choices).remove("/notes", workspacePaths: ["/notes/main"])
        #expect(saved.plainFolders == ["/notes/nested", "/other"])
        #expect(try choices.load().plainFolders == saved.plainFolders)
    }

    @Test("Import and New Workspace ask only for an unanswered folder")
    func asksOnce() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: folder)
        #expect(GitInitPolicy.shouldAsk(path: "/notes", choices: try choices.load()))
        try choices.update { $0.plainFolders.insert("/notes") }
        let remembered = try OwnerChoicesStore(folder: folder).load()
        #expect(!GitInitPolicy.shouldAsk(path: "/notes", choices: remembered))
        #expect(GitInitPolicy.shouldAsk(path: "/other", choices: remembered))
        try choices.update { $0.plainFolders.remove("/notes") }
        #expect(GitInitPolicy.shouldAsk(path: "/notes", choices: try choices.load()))
    }
}
