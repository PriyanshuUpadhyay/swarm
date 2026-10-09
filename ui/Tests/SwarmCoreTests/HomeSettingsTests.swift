import Foundation
import Testing
@testable import SwarmCore

@Suite("Home and settings")
struct HomeSettingsTests {
    @Test("Choices without preferences keep their defaults")
    func preferencesDecode() throws {
        let decoder = JSONDecoder()
        #expect(try decoder.decode(OwnerChoices.self, from: Data("{}".utf8)).prefs == Prefs())
        #expect(try decoder.decode(OwnerChoices.self, from: Data(#"{"prefs":{}}"#.utf8)).prefs == Prefs())
        let saved = try decoder.decode(OwnerChoices.self, from: Data(#"{"prefs":{"settingsPage":"setup"}}"#.utf8))
        #expect(saved.prefs.settingsPage == "setup")
        #expect(try decoder.decode(OwnerChoices.self, from: JSONEncoder().encode(saved)) == saved)
    }

    @Test("A preferences change replaces the complete preferences and preserves other choices")
    func preferencesMerge() {
        let before = OwnerChoices()
        var after = before
        after.prefs.settingsPage = "setup"
        var current = before
        current.pinned = ["/other/project"]
        current.prefs.settingsPage = "advanced"
        current.applyWorkspaceChanges(from: before, to: before)
        #expect(current.prefs.settingsPage == "advanced")
        current.applyWorkspaceChanges(from: before, to: after)
        #expect(current.prefs == after.prefs)
        #expect(current.pinned == ["/other/project"])
    }

    @Test("Settings pages and palette entries keep their specified order")
    func settingsPages() {
        #expect(SettingsPage.allCases.map(\.title) == [
            "Profiles", "Skills", "Accounts", "Setup", "Managed Changes", "Appearance",
            "Notifications", "Keys", "Advanced",
        ])
        #expect(SettingsPage.allCases.allSatisfy { !$0.symbol.isEmpty })
        #expect(SettingsPage.skills.placeholder == "Coming soon.")
        #expect(SettingsPage.accounts.placeholder == nil)
        #expect(SettingsPage.appearance.placeholder == nil)
        #expect(SettingsPage.notifications.placeholder == nil)
        #expect(SettingsPage.keys.placeholder == nil)
        #expect(SettingsPage.profiles.placeholder == nil)
        #expect(SettingsPage.paletteItems.map(\.title) == ["Setup", "Managed Changes", "Profiles"])
        #expect(SettingsPage.paletteItems.compactMap { SettingsPage.openable(from: $0) }
            == [.setup, .managedChanges, .profiles])
        #expect(SettingsPage.openable(from: PaletteItem(id: "other", title: "Setup", group: .action)) == nil)
    }

    @Test("Recent work uses activity across projects, owner titles, and a nonnegative cap")
    func recentWork() {
        let olderLive = chat("older-live", directory: "/project-one", activity: 10, running: true)
        let newerEnded = chat("newer-ended", directory: "/project-two", activity: 50, running: false)
        let middleLive = chat("middle-live", directory: "/project-two", activity: 30, running: true)
        let empty = chat("empty", directory: "/project-two", activity: 60, running: false, agents: 0)
        let firstProject = project("/project-one", chats: [olderLive])
        let secondProject = project("/project-two", chats: [newerEnded, middleLive, empty])
        let tree = SessionsTree(projects: [firstProject, secondProject])
        var navigation = WorkspaceNavigation()
        navigation.renameChat(newerEnded, to: "Saved title")
        navigation.names["/project-two"] = "Saved workspace"
        let rows = HomeModel.recentWork(tree: tree, navigation: navigation, now: 110, limit: 2)
        #expect(rows.map(\.id) == [newerEnded.id, middleLive.id])
        #expect(rows[0].title == "Saved title")
        #expect(rows[0].workspace == "Saved workspace")
        #expect(rows[0].workspacePath == "/project-two")
        #expect(rows[0].age == "1m")
        #expect(rows[0].state == .ended)
        #expect(HomeModel.recentWork(tree: tree, navigation: navigation, now: 110).count == 3)
        #expect(HomeModel.recentWork(tree: tree, navigation: navigation, now: 110, limit: 0).isEmpty)
        #expect(HomeModel.recentWork(tree: tree, navigation: navigation, now: 110, limit: -1).isEmpty)
        #expect(HomeModel.recentWork(tree: SessionsTree(projects: []), navigation: navigation, now: 110).isEmpty)
    }

    @Test("First-run steps show checks for every state and leave once all steps are done")
    func firstRunStates() {
        for hasProject in [false, true] {
            for isSetUp in [false, true] {
                for hasChat in [false, true] {
                    let steps = HomeModel.firstRunSteps(hasProject: hasProject, isSetUp: isSetUp, hasChat: hasChat)
                    if hasProject && isSetUp && hasChat {
                        #expect(steps.isEmpty)
                    } else {
                        #expect(steps.map(\.id) == [.importProject, .runSetup, .startChat])
                        #expect(steps.map(\.title) == ["Import a project", "Run setup", "Start a chat"])
                        #expect(steps.map(\.done) == [hasProject, isSetUp, hasChat])
                    }
                }
            }
        }
    }

    @Test("Recent work defaults to eight rows and gives equal activity a fixed order")
    func recentWorkDefaultCap() {
        let chats = (0..<10).reversed().map {
            chat("chat-\($0)", directory: "/project", activity: 50, running: true)
        }
        let tree = SessionsTree(projects: [project("/project", chats: chats)])
        let rows = HomeModel.recentWork(tree: tree, navigation: WorkspaceNavigation(), now: 110)
        #expect(rows.map(\.id.rawValue) == (0..<8).map { "chat-\($0)" })
    }

    @Test("Home starts a chat in sidebar order, skips unusable workspaces, and disables an empty list")
    func startChatDirectory() {
        let workspaces = [
            WorkspaceNode(path: "/project/archived", name: "archived", sessions: []),
            WorkspaceNode(path: "/project/missing", name: "missing", sessions: [], missing: true),
            WorkspaceNode(path: "/project/removed", name: "removed", sessions: [], isRemoved: true),
            WorkspaceNode(path: "/project/first", name: "first", sessions: []),
            WorkspaceNode(path: "/project/second", name: "second", sessions: []),
        ]
        let firstProject = ProjectNode(id: .folder("/project"), path: "/project",
                                      launchDirectory: "/project", workspaces: workspaces)
        let pinnedProject = project("/other-project", chats: [])
        let tree = SessionsTree(projects: [firstProject, pinnedProject])
        var navigation = WorkspaceNavigation()
        navigation.archived = ["/project/archived"]
        navigation.workspaceOrder["/project"] = workspaces.map(\.path)
        #expect(HomeModel.startChatDirectory(tree: tree, navigation: navigation) == "/project/first")
        navigation.workspaceOrder["/project"] = ["/project/second", "/project/first"]
        #expect(HomeModel.startChatDirectory(tree: tree, navigation: navigation) == "/project/second")
        navigation.pinned = ["/other-project"]
        #expect(HomeModel.startChatDirectory(tree: tree, navigation: navigation) == "/other-project")
        navigation.archived.insert("/other-project")
        #expect(HomeModel.startChatDirectory(tree: tree, navigation: navigation) == "/project/second")
        navigation.archived.formUnion(["/project/first", "/project/second"])
        #expect(HomeModel.startChatDirectory(tree: tree, navigation: navigation) == nil)
        #expect(HomeModel.startChatDirectory(tree: SessionsTree(projects: []), navigation: navigation) == nil)
    }

    private func chat(
        _ name: String, directory: String, activity: Int, running: Bool, agents: Int = 1
    ) -> SwarmProjectSession {
        let session = SwarmSession(
            id: SwarmSessionID(name), talkMode: "lane", adapter: "tmux-solo", cwd: directory,
            createdAt: 1, chairProvider: "codex", chairID: nil, chairLog: nil,
            agents: agents, messages: 0, lastMessageAt: activity, archivedAt: nil
        )
        return SwarmProjectSession(sessions: [session], title: name, isRunning: running)
    }

    private func project(_ path: String, chats: [SwarmProjectSession]) -> ProjectNode {
        ProjectNode(id: .folder(path), path: path, launchDirectory: path,
                    workspaces: [WorkspaceNode(path: path, name: path, sessions: chats)])
    }
}
