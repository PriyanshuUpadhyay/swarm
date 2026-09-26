import Foundation
import Testing
@testable import SwarmCore

@Suite("Workspace navigation")
@MainActor
struct WorkspaceNavigationTests {
    private func tree() -> SessionsTree {
        SessionsTree.build(
            sessions: [
                session("one", path: "/repo/main"),
                session("two", path: "/repo/main/src"),
                session("three", path: "/repo/feature"),
            ], projectPaths: ["/repo/main"],
            titles: [.init("one"): "Fix login", .init("two"): "Review login"],
            repositoryPathsResolver: { _ in
                GitRepositoryPaths(gitDirectory: "/repo/main/.git", commonDirectory: "/repo/.git")
            },
            worktreeLister: { _ in [
                WorktreeEntry(path: "/repo/main", branch: "main"),
                WorktreeEntry(path: "/repo/feature", branch: "feature"),
                WorktreeEntry(path: "/repo/empty", branch: "empty"),
            ] }
        )
    }

    @Test("Multiple chats share one workspace and empty workspaces remain selectable")
    func grouping() throws {
        let entries = WorkspaceEntry.list(in: tree())
        #expect(entries.count == 3)
        let main = try #require(entries.first { $0.id == "/repo/main" })
        #expect(Set(main.chats.map(\.id)) == [.init("one"), .init("two")])
        let empty = try #require(entries.first { $0.id == "/repo/empty" })
        var navigation = WorkspaceNavigation()
        navigation.select(empty)
        #expect(navigation.selectedWorkspace == empty.id)
        #expect(navigation.selectedChat(in: empty) == nil)
    }

    @Test("Switching workspaces remembers each chat and never selects a different workspace's chat")
    func selection() throws {
        let entries = WorkspaceEntry.list(in: tree())
        let main = try #require(entries.first { $0.id == "/repo/main" })
        let feature = try #require(entries.first { $0.id == "/repo/feature" })
        var navigation = WorkspaceNavigation()
        navigation.select(main, chat: .init("one"))
        navigation.select(feature)
        navigation.select(main)
        #expect(navigation.selectedChat(in: main)?.id == .init("one"))
        navigation.selectedChats[main.id] = "three"
        #expect(main.chats.contains { $0.id == navigation.selectedChat(in: main)?.id })
    }

    @Test("Pin, name, archive and selection survive a new store; restore preserves chats")
    func persistence() throws {
        let suite = "WorkspaceNavigationTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let entry = try #require(WorkspaceEntry.list(in: tree()).first { $0.id == "/repo/main" })
        var navigation = WorkspaceNavigation()
        navigation.select(entry, chat: .init("two"))
        navigation.pinned.insert(entry.id)
        navigation.names[entry.id] = "Landing page copy"
        let store = WorkspaceNavigationStore(defaults: defaults)
        store.save(navigation)
        #expect(WorkspaceNavigationStore(defaults: defaults).load() == navigation)
        navigation.archive(entry.id)
        #expect(navigation.selectedWorkspace == nil)
        store.save(navigation)
        var restored = WorkspaceNavigationStore(defaults: defaults).load()
        #expect(restored.archived.contains(entry.id))
        restored.archived.remove(entry.id)
        restored.select(entry)
        #expect(restored.selectedChat(in: entry)?.id == .init("two"))
        #expect(restored.title(for: entry) == "Landing page copy")
        #expect(restored.pinned.contains(entry.id))
        #expect(entry.chats.count == 2)
    }

    @Test("Search finds project, branch, saved name and chat text")
    func search() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first { $0.id == "/repo/main" })
        var navigation = WorkspaceNavigation()
        navigation.names[entry.id] = "Landing page copy"
        for query in ["repo", "MAIN", "landing", "login", ""] {
            #expect(navigation.matches(query, entry: entry))
        }
        #expect(!navigation.matches("missing", entry: entry))
    }

    @Test("Model handoff keeps the remembered chat through its previous session id")
    func continuation() throws {
        let old = session("old", path: "/folder")
        var new = session("new", path: "/folder")
        new.continuationOf = old.id
        new.createdAt = old.createdAt + 1
        let tree = SessionsTree.build(
            sessions: [old, new], repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] }
        )
        let entry = try #require(WorkspaceEntry.list(in: tree).first)
        var navigation = WorkspaceNavigation()
        navigation.selectedChats[entry.id] = old.id.rawValue
        #expect(navigation.selectedChat(in: entry)?.id == new.id)
        #expect(entry.chats.count == 1)
    }

    @Test("Default names distinguish projects and stay stable when the branch changes")
    func folderTitles() {
        let swarm = entry(project: "/work/swarm", path: "/work/swarm/wt/main", branch: "main")
        let thine = entry(project: "/work/thine", path: "/work/thine/wt/main", branch: "main")
        let switched = entry(project: "/work/swarm", path: swarm.id, branch: "fix/login")
        let navigation = WorkspaceNavigation()
        #expect(navigation.title(for: swarm) == "swarm / main")
        #expect(navigation.title(for: thine) == "thine / main")
        #expect(navigation.title(for: switched) == navigation.title(for: swarm))
        #expect(navigation.detail(for: switched, among: [switched, thine]) == "fix/login · 0 chats")
        #expect(navigation.detail(for: swarm, among: [swarm, thine]) == "0 chats")
    }

    @Test("Matching project and folder names get the shortest distinct path even across sections")
    func duplicatePaths() {
        let first = entry(project: "/work/client-a/swarm", path: "/work/client-a/swarm/wt/main", branch: "main")
        let second = entry(project: "/work/client-b/swarm", path: "/work/client-b/swarm/wt/main", branch: "main")
        var navigation = WorkspaceNavigation()
        navigation.pinned.insert(first.id)
        navigation.archived.insert(second.id)
        let entries = [first, second]
        #expect(navigation.title(for: first) == navigation.title(for: second))
        #expect(navigation.detail(for: first, among: entries) == "client-a/swarm/wt/main · 0 chats")
        #expect(navigation.detail(for: second, among: entries) == "client-b/swarm/wt/main · 0 chats")
        let root = entry(project: "/swarm", path: "/swarm/wt/main", branch: "main")
        #expect(navigation.detail(for: root, among: [root, first]) == "/swarm/wt/main · 0 chats")
    }

    @Test("Saved names keep project and branch context, chat count, and duplicate path details")
    func savedTitleDetails() throws {
        let main = try #require(WorkspaceEntry.list(in: tree()).first { $0.id == "/repo/main" })
        let second = entry(project: "/repo", path: "/repo/review", branch: "main")
        var navigation = WorkspaceNavigation()
        navigation.names[main.id] = "Fix login"
        navigation.names[second.id] = "fix login"
        #expect(navigation.title(for: main) == "Fix login")
        #expect(navigation.detail(for: main, among: [main]) == "repo · main · 2 chats")
        #expect(navigation.detail(for: main, among: [main, second]) == "main · repo · 2 chats")
        #expect(navigation.detail(for: second, among: [main, second]) == "review · repo · main · 0 chats")
        navigation.select(main, chat: .init("two"))
        #expect(navigation.title(for: main) == "Fix login")
    }

    @Test("Plain folders and blank saved names have readable defaults without fake branches")
    func folderWithoutGit() throws {
        let tree = SessionsTree.build(
            sessions: [session("plain", path: "/work/notes")],
            repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] }
        )
        let folder = try #require(WorkspaceEntry.list(in: tree).first)
        var navigation = WorkspaceNavigation()
        navigation.names[folder.id] = "  "
        #expect(navigation.title(for: folder) == "notes")
        #expect(navigation.detail(for: folder, among: [folder]) == "1 chat")
        navigation.names[folder.id] = "Research"
        #expect(navigation.detail(for: folder, among: [folder]) == "notes · 1 chat")
    }

    private func entry(project path: String, path workspacePath: String, branch: String) -> WorkspaceEntry {
        let workspace = WorkspaceNode(path: workspacePath, name: branch, sessions: [], branch: branch)
        let project = ProjectNode(id: .repository(commonDirectory: path + "/.git"), path: path, launchDirectory: workspacePath, workspaces: [workspace])
        return WorkspaceEntry(project: project, workspace: workspace)
    }

    private func session(_ id: String, path: String) -> SwarmSession {
        SwarmSession(id: .init(id), talkMode: "lane", adapter: "tmux-solo", cwd: path, createdAt: 1, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
    }
}
