import Foundation
import Testing
@testable import SwarmCore

@Suite("Saved chat and project names")
@MainActor
struct RenameTitleTests {
    @Test("Chat and project renames persist in choices and reach every title source")
    func savedNames() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "RenameTitleTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        let tree = tree()
        let project = try #require(tree.projects.first)
        let chat = try #require(project.chats.first)
        var navigation = WorkspaceNavigation()
        navigation.renameChat(chat.session, to: " Fix login ")
        navigation.renameProject(project, to: " Login app ")
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: folder)
        store.save(navigation)
        let stored = try OwnerChoicesStore(folder: folder).load()
        #expect(stored.chatNames == ["oldest": "Fix login"])
        #expect(stored.projectNames == [project.path: "Login app"])
        let loaded = store.load()
        #expect(loaded == navigation)
        let state = try #require(defaults.data(forKey: "workspaces.navigation"))
        let viewState = try #require(JSONSerialization.jsonObject(with: state) as? [String: Any])
        #expect(viewState["chatNames"] == nil)
        #expect(viewState["projectNames"] == nil)
        let entries = WorkspaceEntry.list(in: tree)
        let sections = SidebarRows.sections(projects: tree.projects, workspaces: entries, navigation: loaded, search: "", showingArchive: false, now: 3)
        #expect(sections.first?.title == "Login app")
        #expect(sections.first?.rows.first { $0.kind == .chat }?.title == "Fix login")
        #expect(ChatTab.tabs([chat], closing: [], now: 3, chatNames: loaded.chatNames).first?.title == "Fix login")
        #expect(PaletteSource.workspaces(entries, navigation: loaded, now: 3).chats.first?.title == "Fix login")
        #expect(loaded.matches("Fix login", entry: entries[0]))
        #expect(loaded.matches("Login app", entry: entries[0]))
        navigation.renameChat(chat.session, to: "\n ")
        navigation.renameProject(project, to: " ")
        store.save(navigation)
        let cleared = try OwnerChoicesStore(folder: folder).load()
        #expect(cleared.chatNames.isEmpty)
        #expect(cleared.projectNames.isEmpty)
        #expect(ChatTitle.title(chat.session, appName: store.load().chatNames["oldest"]) == "CLI name")
        #expect(store.load().projectTitle(for: project) == "app")
    }

    @Test("Custom project headers disambiguate duplicate shown names")
    func duplicateProjectNames() {
        let first = ProjectNode(id: .folder("/work/app"), path: "/work/app", launchDirectory: "/work/app", workspaces: [])
        let second = ProjectNode(id: .folder("/tmp/app"), path: "/tmp/app", launchDirectory: "/tmp/app", workspaces: [])
        var navigation = WorkspaceNavigation()
        navigation.renameProject(first, to: "Service")
        navigation.renameProject(second, to: "Service")
        let sections = SidebarRows.sections(projects: [first, second], workspaces: [], navigation: navigation, search: "", showingArchive: false, now: 1)
        #expect(sections.map(\.title) == ["Service — work", "Service — tmp"])
        navigation.renameProject(second, to: "Website")
        #expect(SidebarRows.sections(projects: [first, second], workspaces: [], navigation: navigation, search: "", showingArchive: false, now: 1).map(\.title) == ["Service", "Website"])
    }

    @Test("Removing a project keeps chat names, whose keys are session ids")
    func removeProjectNames() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        var choices = OwnerChoices()
        choices.projectNames = [root.path: "Existing", root.appendingPathComponent("missing").path: "Gone"]
        choices.chatNames = ["oldest": "Chat name"]
        choices.removeProject(root.appendingPathComponent("missing").path)
        #expect(choices.projectNames == [root.path: "Existing"])
        #expect(choices.chatNames == ["oldest": "Chat name"])
    }

    private func tree() -> SessionsTree {
        let old = SwarmSession(id: .init("oldest"), talkMode: "lane", adapter: "tmux-solo", cwd: "/work/app", createdAt: 1, chairProvider: "claude", chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
        var new = old
        new.id = .init("newest")
        new.createdAt = 2
        new.chairProvider = "codex"
        new.continuationOf = old.id
        return SessionsTree.build(sessions: [new, old], titles: [old.id: "First prompt"], cliNames: [new.id: "CLI name"], repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] })
    }
}
