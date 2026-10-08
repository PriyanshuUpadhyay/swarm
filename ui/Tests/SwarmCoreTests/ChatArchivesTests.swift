import Testing
@testable import SwarmCore

@Suite("Optimistic chat archive")
struct ChatArchivesTests {
    private let first = SwarmSessionID("first")
    private let second = SwarmSessionID("second")
    private let third = SwarmSessionID("third")

    @Test("Archiving keeps missing and locked workspace flags")
    func workspaceFlags() throws {
        let workspace = WorkspaceNode(
            path: "/repo/missing", name: "missing", sessions: tree([first]).projects[0].workspaces[0].sessions,
            branch: "feature", missing: true, mark: .locked
        )
        let source = SessionsTree(projects: [ProjectNode(
            id: .repository(commonDirectory: "/repo/.git"), path: "/repo", launchDirectory: "/repo",
            workspaces: [workspace]
        )])
        var archives = ChatArchives()
        _ = archives.begin(first, in: source)
        let filtered = try #require(archives.applying(to: source).projects.first?.workspaces.first)
        #expect(filtered.sessions.isEmpty)
        #expect(filtered.missing)
        #expect(filtered.mark == .locked)
        #expect(filtered.branch == workspace.branch)
        #expect(!filtered.canStartChat)
    }

    @Test("Archive filtering keeps the removed worktree row unable to start a chat")
    func removedWorkspace() throws {
        let source = SessionsTree(projects: [ProjectNode(
            id: .repository(commonDirectory: "/repo/.git"), path: "/repo", launchDirectory: "/repo",
            workspaces: [WorkspaceNode(
                path: "/repo#removed", name: "Removed worktrees",
                sessions: tree([first]).projects[0].workspaces[0].sessions, isRemoved: true
            )]
        )])
        let filtered = try #require(ChatArchives().applying(to: source).projects.first?.workspaces.first)
        #expect(filtered.isRemoved)
        #expect(!filtered.canStartChat)
        #expect(!filtered.missing)
    }

    @Test("Archive selects the previous open chat and preserves inactive selection")
    func selection() {
        let source = tree([first, second, third])
        let open = [first, second, third].map(\.rawValue)
        let history = [third, first, second].map(\.rawValue)
        #expect(ChatArchives.selection(afterArchiving: second, selected: second, in: source,
                                      history: history, open: open) == first)
        #expect(ChatArchives.selection(afterArchiving: second, selected: second, in: source,
                                      history: history, open: [second.rawValue, third.rawValue]) == third)
        #expect(ChatArchives.selection(afterArchiving: first, selected: third, in: source,
                                      history: history, open: open) == third)
        #expect(ChatArchives.selection(afterArchiving: first, selected: nil, in: source,
                                      history: history, open: open) == nil)
        #expect(ChatArchives.selection(afterArchiving: first, selected: first, in: self.tree([first]),
                                      history: [first.rawValue], open: [first.rawValue]) == nil)
    }

    @Test("Pending archive removes the chat immediately and retains its workspace")
    func pending() {
        var archives = ChatArchives()
        let source = tree([first])
        #expect(archives.begin(first, in: source) == [first])
        let visible = archives.applying(to: source)
        #expect(visible.projects.count == 1)
        #expect(visible.projects[0].workspaces.count == 1)
        #expect(visible.session(first) == nil)
        archives.reconcile(source)
        #expect(archives.applying(to: source).session(first) == nil)
        #expect(archives.begin(first, in: source).isEmpty)
    }

    @Test("Failure restores only that operation and preserves new source data")
    func rollback() {
        var archives = ChatArchives()
        let source = tree([first, second])
        _ = archives.begin(first, in: source)
        _ = archives.begin(second, in: source)
        archives.finish(first, succeeded: false)
        let updated = tree([first, second, third])
        #expect(archives.applying(to: updated).projects[0].chats.map(\.id) == [first, third])
        archives.finish(second, succeeded: true)
        #expect(archives.applying(to: updated).session(second) == nil)
    }

    @Test("Success stays hidden until fresh discovery confirms absence")
    func confirmation() {
        var archives = ChatArchives()
        let source = tree([first, second])
        _ = archives.begin(first, in: source)
        archives.finish(first, succeeded: true)
        archives.reconcile(source)
        #expect(archives.applying(to: source).session(first) == nil)
        archives.reconcile(tree([second]))
        // A later explicit unarchive must be allowed back into the tree.
        #expect(archives.applying(to: source).session(first) != nil)
    }

    @Test("Linked sessions archive together, with selection from an older link")
    func linked() {
        let source = tree([first, third], linked: second)
        var archives = ChatArchives()
        #expect(Set(archives.begin(second, in: source)) == [first, second])
        #expect(archives.applying(to: source).session(first) == nil)
        #expect(archives.applying(to: source).session(second) == nil)
        #expect(archives.applying(to: source).session(third) != nil)
        #expect(ChatArchives.selection(afterArchiving: second, selected: second, in: source,
                                      history: [third.rawValue, second.rawValue],
                                      open: [second.rawValue, third.rawValue]) == third)
        archives.finish(second, succeeded: false)
        #expect(archives.applying(to: source).session(second) != nil)
    }

    @Test("A failed background archive restores tab order, groups and history after refresh")
    func tabRollback() throws {
        let source = tree([first, second, third])
        let entry = try #require(WorkspaceEntry.list(in: source).first)
        var navigation = WorkspaceNavigation()
        navigation.recordTabFirstSight([entry])
        navigation.tabs[entry.id] = TabStrip(open: [third.rawValue, first.rawValue, second.rawValue])
            .grouping(.new(id: "review", name: "Review", color: .blue, tab: first.rawValue))
            .grouping(.add(second.rawValue, to: "review"))
        navigation.tabHistory[entry.id] = [first.rawValue, third.rawValue, second.rawValue]
        navigation.selectedChats[entry.id] = third.rawValue
        let strip = navigation.tabs[entry.id]
        let history = navigation.tabHistory[entry.id]
        let snapshot = ChatArchives.TabSnapshot(workspace: entry.id, navigation: navigation)
        var archives = ChatArchives()
        _ = archives.begin(first, in: source)
        navigation.closeTab(first.rawValue, in: entry)
        #expect(navigation.tabHistory[entry.id]?.contains(first.rawValue) == false)
        navigation.recordTabFirstSight(WorkspaceEntry.list(in: archives.applying(to: source)))
        archives.finish(first, succeeded: false)
        snapshot.restore(in: &navigation)
        #expect(navigation.tabs[entry.id] == strip)
        #expect(navigation.tabHistory[entry.id] == history)
        #expect(navigation.selectedChats[entry.id] == third.rawValue)
    }

    private func tree(_ ids: [SwarmSessionID], linked: SwarmSessionID? = nil) -> SessionsTree {
        let rows = ids.enumerated().map { index, id in
            SwarmProjectSession(
                sessions: [session(id, age: index)] + (index == 0 ? linked.map { [session($0, age: 9)] } ?? [] : []),
                title: id.rawValue
            )
        }
        return SessionsTree(projects: [ProjectNode(
            id: .folder("/test"), path: "/test", launchDirectory: "/test",
            workspaces: [WorkspaceNode(path: "/test", name: "test", sessions: rows)]
        )])
    }

    private func session(_ id: SwarmSessionID, age: Int) -> SwarmSession {
        SwarmSession(
            id: id, talkMode: "lane", adapter: nil, cwd: "/test", createdAt: 100 - age,
            chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil
        )
    }
}
