import Foundation
import Testing
@testable import SwarmCore

@Suite("Project defaults")
@MainActor
struct ProjectDefaultsTests {
    @Test("Project defaults survive choices decoding and encoding")
    func retainsDefaults() throws {
        let data = Data("{\"projectDefaults\":{\"/project\":{\"worktreeFolder\":\"wt/\",\"branchPrefix\":\"fix/\"}}}".utf8)
        let choices = try JSONDecoder().decode(OwnerChoices.self, from: data)
        let encoded = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(choices)) as? [String: Any])
        #expect(encoded["projectDefaults"] != nil)
    }

    @Test("Project defaults decode with absent, null, partial, and unknown keys")
    func decodesChoices() throws {
        for json in ["{}", "{\"projectDefaults\":null}", "{\"futureKey\":true}"] {
            #expect(try JSONDecoder().decode(OwnerChoices.self, from: Data(json.utf8)).projectDefaults.isEmpty)
        }
        let data = Data("{\"projectDefaults\":{\"/project\":{\"worktreeFolder\":\"wt/\",\"branchPrefix\":\"\",\"futureKey\":true},\"/empty\":{}},\"futureKey\":true}".utf8)
        let choices = try JSONDecoder().decode(OwnerChoices.self, from: data)
        #expect(choices.projectDefaults["/project"] == ProjectDefaults(worktreeFolder: "wt/", branchPrefix: ""))
        #expect(choices.projectDefaults["/empty"] == ProjectDefaults())
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: JSONEncoder().encode(choices)) == choices)
    }

    @Test("A stale view replaces changed defaults and preserves other projects")
    func mergesChoices() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "ProjectDefaultsTests.\(UUID().uuidString)"
        let viewDefaults = try #require(UserDefaults(suiteName: suite))
        defer {
            viewDefaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        let choices = OwnerChoicesStore(folder: folder)
        let original = ProjectDefaults(worktreeFolder: "wt/", branchPrefix: "old/")
        try choices.update { $0.projectDefaults = ["/project": original, "/remove": original] }
        let navigationStore = WorkspaceNavigationStore(defaults: viewDefaults, choices: choices)
        var navigation = navigationStore.load()
        try choices.update {
            $0.projectDefaults["/other-process"] = ProjectDefaults(branchPrefix: "other/")
            $0.projectDefaults["/project"] = ProjectDefaults(worktreeFolder: "concurrent", branchPrefix: "other/")
        }
        let changed = ProjectDefaults(worktreeFolder: "wt/", branchPrefix: "new/")
        navigation.ownerChoices.projectDefaults["/project"] = changed
        navigation.ownerChoices.projectDefaults.removeValue(forKey: "/remove")
        #expect(navigationStore.save(navigation) == nil)
        let saved = try choices.load()
        #expect(saved.projectDefaults["/project"] == changed)
        #expect(saved.projectDefaults["/other-process"] == ProjectDefaults(branchPrefix: "other/"))
        #expect(saved.projectDefaults["/remove"] == nil)
        #expect(WorkspaceNavigationStore(defaults: viewDefaults, choices: choices).load().ownerChoices.projectDefaults == saved.projectDefaults)
        let viewData = try #require(viewDefaults.data(forKey: "workspaces.navigation"))
        let viewState = try #require(JSONSerialization.jsonObject(with: viewData) as? [String: Any])
        #expect(viewState["projectDefaults"] == nil)
    }

    @Test("Remove Project prunes its defaults and preserves nested projects")
    func prunesChoices() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: folder)
        try choices.update {
            $0.projectPaths = ["/project", "/project/nested", "/other"]
            $0.projectDefaults = ["/project": ProjectDefaults(), "/project/nested": ProjectDefaults(), "/other": ProjectDefaults()]
        }
        let saved = try SwarmProjectStore(choices: choices).remove("/project", workspacePaths: ["/project/main"])
        #expect(Set(saved.projectDefaults.keys) == ["/project/nested", "/other"])
        #expect(try choices.load().projectDefaults == saved.projectDefaults)
    }

    @Test("Unset defaults use the checkout sibling or the bare hub wt folder")
    func resolvesUnsetDefaults() {
        let checkout = project(common: "/work/project/.git")
        let resolved = ProjectDefaults().resolved(for: checkout)
        #expect(resolved.folder == "/work/project-worktrees")
        #expect(resolved.prefix == "swarm/")
        #expect(ProjectDefaults().folderSetting(for: checkout) == "../project-worktrees")
        #expect(ProjectDefaults().folderSetting(for: project(common: "/work/project/.bare")) == "wt/")
        #expect(ProjectDefaults().resolved(for: project(common: "/work/project/.bare")).folder == "/work/project/wt")
        #expect(ProjectDefaults().resolved(for: project(common: "/work/project")).folder == "/work/project-worktrees")
    }

    @Test("Stored folders resolve against the project and prefixes allow empty strings")
    func resolvesStoredDefaults() {
        let checkout = project(common: "/work/project/.git")
        let relative = ProjectDefaults(worktreeFolder: "../tasks", branchPrefix: "fix/").resolved(for: checkout)
        #expect(relative.folder == "/work/tasks")
        #expect(relative.prefix == "fix/")
        let absolute = ProjectDefaults(worktreeFolder: "/custom/tasks", branchPrefix: "").resolved(for: checkout)
        #expect(absolute.folder == "/custom/tasks")
        #expect(absolute.prefix == "")
        #expect(ProjectDefaults(worktreeFolder: "wt/").resolved(for: checkout).folder == "/work/project/wt")
        let tildeFolder = "~/worktrees"
        #expect(ProjectDefaults(worktreeFolder: tildeFolder).resolved(for: checkout).folder
            == URL(fileURLWithPath: (tildeFolder as NSString).expandingTildeInPath).standardizedFileURL.path)
    }

    private func project(common: String) -> ProjectNode {
        ProjectNode(id: .repository(commonDirectory: common), path: "/work/project", launchDirectory: "/work/project", workspaces: [])
    }

}
