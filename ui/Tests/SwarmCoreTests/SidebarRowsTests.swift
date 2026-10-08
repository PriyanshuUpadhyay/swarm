import Foundation
import Testing
@testable import SwarmCore

@Suite("Sidebar chat rows")
struct SidebarRowsTests {
    private func session(_ id: String, time: Int) -> SwarmSession {
        SwarmSession(id: .init(id), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo",
                     createdAt: time, chairLog: nil, agents: 2, messages: 0, lastMessageAt: nil)
    }

    private func project(_ chats: [SwarmProjectSession]) -> ProjectNode {
        ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo",
                    workspaces: [WorkspaceNode(path: "/repo", name: "repo", sessions: chats)])
    }

    private func rows(
        _ project: ProjectNode, navigation: WorkspaceNavigation = .init(),
        agents: [SwarmSessionID: [SwarmAgent]] = [:], expanded: Set<String> = [], archive: Bool = false
    ) -> [SidebarRow] {
        SidebarRows.sections(projects: [project], workspaces: WorkspaceEntry.list(in: SessionsTree(projects: [project])),
                             navigation: navigation, search: "", showingArchive: archive, now: 100,
                             agentsBySession: agents, expandedLists: expanded).flatMap(\.rows)
    }

    @Test("Chats sort newest first, cut after five, and expand for one workspace")
    func cutAndOrder() {
        let project = project((1...7).map {
            SwarmProjectSession(sessions: [session("s\($0)", time: $0)], title: "Chat \($0)")
        })
        let cut = rows(project)
        #expect(cut.map(\.id) == ["/repo", "chat:s7", "chat:s6", "chat:s5", "chat:s4", "chat:s3", "more:/repo"])
        #expect(cut.map(\.depth) == [0, 1, 1, 1, 1, 1, 1])
        #expect(cut.last?.title == "… 2 more")
        #expect(cut.dropFirst().allSatisfy { $0.parentID == "/repo" })
        #expect(rows(project, expanded: ["/repo"]).count == 8)
        #expect(rows(project, expanded: ["/other"]).count == 7)
    }

    @Test("Children default folded, exclude the chair, and show hidden urgency")
    func childFolds() throws {
        let chat = SwarmProjectSession(sessions: [session("chat", time: 1)], title: "Work", status: .waiting)
        let project = project([chat])
        let agents: [SwarmSessionID: [SwarmAgent]] = [.init("chat"): [
            SwarmAgent(id: .init("orchestrator"), role: "chair", pane: "%0", alive: true, state: "working"),
            SwarmAgent(id: .init("review"), role: "review", pane: "%1", alive: true, state: "waiting"),
            SwarmAgent(id: .init("code"), role: "code", pane: nil, alive: false, state: "done"),
        ]]
        let folded = rows(project, agents: agents)
        #expect(folded.map(\.kind) == [.workspace, .chat])
        #expect(folded[1].expanded == false)
        #expect(folded[1].childrenSummary == "2 agents · 1 waiting")
        #expect(folded[1].status == .waiting)
        var navigation = WorkspaceNavigation()
        navigation.toggleCollapsed("chat:chat")
        let opened = rows(project, navigation: navigation, agents: agents)
        #expect(opened.map(\.id) == ["/repo", "chat:chat", "child:chat/code", "child:chat/review"])
        #expect(opened[1].status == .working)
        #expect(opened[2].depth == 2)
        #expect(opened[2].parentID == "chat:chat")
        #expect(opened[2].status == .ended)
        #expect(opened[2].dimmed)
        #expect(!opened[3].dimmed)
        #expect(!opened[1].dimmed)
        #expect(opened[2].detail == "· code")
        navigation.toggleCollapsed("/repo")
        let workspaceFold = rows(project, navigation: navigation, agents: agents)
        #expect(workspaceFold.count == 1)
        #expect(workspaceFold[0].expanded == false)
        #expect(workspaceFold[0].status == .waiting)
        let restored = try JSONDecoder().decode(WorkspaceNavigation.self, from: JSONEncoder().encode(navigation))
        #expect(restored == navigation)
    }

    @Test("Chat ids stay stable while child rows and selection use only the current session")
    func stableChainIDs() throws {
        let root = session("root", time: 1)
        let next = session("next", time: 2)
        let chat = SwarmProjectSession(sessions: [next, root], title: "Continued")
        var navigation = WorkspaceNavigation()
        navigation.toggleCollapsed("chat:root")
        let agents: [SwarmSessionID: [SwarmAgent]] = [
            root.id: [
                SwarmAgent(id: .init("worker"), role: "old", pane: nil, alive: false),
                SwarmAgent(id: .init("previous-only"), role: "old", pane: nil, alive: false),
            ],
            next.id: [
                SwarmAgent(id: .init("worker"), role: "new", pane: "%1", alive: true),
                SwarmAgent(id: .init("finished"), role: "code", pane: nil, alive: false),
            ],
        ]
        let project = project([chat])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        let result = rows(project, navigation: navigation, agents: agents)
        #expect(result.map(\.id) == ["/repo", "chat:root", "child:next/finished", "child:next/worker"])
        #expect(result[1].childrenSummary == "2 agents")
        #expect(result[2].status == .ended)
        #expect(result[2].dimmed)
        #expect(result[3].status == .done)
        #expect(result[3].dimmed)
        let current = try #require(SidebarRows.selection(for: "child:next/worker", in: entries, agentsBySession: agents))
        #expect(current.agentSessionID == next.id)
        #expect(SidebarRows.selection(for: "child:root/worker", in: entries, agentsBySession: agents) == nil)
        #expect(SidebarRows.selection(for: "child:root/previous-only", in: entries, agentsBySession: agents) == nil)
        let previousAgents = try #require(agents[root.id])
        let previousChildrenOnly = rows(project, navigation: navigation, agents: [root.id: previousAgents])
        #expect(previousChildrenOnly.map(\.id) == ["/repo", "chat:root"])
        #expect(!previousChildrenOnly[1].hasChildren)
        #expect(previousChildrenOnly[1].fields.first { $0.field == .children } == nil)
    }

    @Test("A folded workspace ignores waiting and failed children from older chain members",
          arguments: ["waiting", "failed"])
    func olderChildUrgency(previousState: String) throws {
        let chat = SwarmProjectSession(sessions: [session("next", time: 2), session("root", time: 1)],
                                       title: "Continued", status: .working)
        var navigation = WorkspaceNavigation()
        navigation.toggleCollapsed("/repo")
        let folded = rows(project([chat]), navigation: navigation, agents: [
            .init("root"): [SwarmAgent(id: .init("worker"), role: "review", pane: "%1", alive: true, state: previousState)],
        ])
        let workspace = try #require(folded.first)
        #expect(workspace.status == .working)
    }

    @Test("Pinned and archived workspaces keep their chat rows; empty workspaces stay")
    func pinnedAndArchive() {
        let project = project([SwarmProjectSession(sessions: [session("one", time: 1)], title: "One")])
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/repo"]
        #expect(rows(project, navigation: navigation).map(\.kind) == [.workspace, .chat])
        navigation.archived = ["/repo"]
        #expect(rows(project, navigation: navigation).isEmpty)
        #expect(rows(project, navigation: navigation, archive: true).map(\.kind) == [.workspace, .chat])
        #expect(rows(project, navigation: navigation, archive: true).allSatisfy { $0.archived })
        #expect(rows(self.project([])).map(\.kind) == [.workspace])
    }

    @Test("Children without a wait show only their count")
    func childrenWithoutWaiting() {
        let chat = SwarmProjectSession(sessions: [session("chat", time: 1)], title: "Work")
        let result = rows(project([chat]), agents: [chat.id: [
            SwarmAgent(id: .init("worker"), role: "code", pane: "%1", alive: true, state: "working"),
        ]])
        #expect(result[1].childrenSummary == "1 agent")
        let two = rows(project([chat]), agents: [chat.id: [
            SwarmAgent(id: .init("one"), role: "code", pane: "%1", alive: true),
            SwarmAgent(id: .init("two"), role: "review", pane: "%2", alive: true),
        ]])
        #expect(two[1].childrenSummary == "2 agents")
        let waiting = rows(project([chat]), agents: [chat.id: [
            SwarmAgent(id: .init("worker"), role: "review", pane: "%1", alive: true, state: "waiting"),
        ]])
        #expect(waiting[1].childrenSummary == "1 agent · 1 waiting")
    }

    @Test("Workspace, chat, and child ids map to the right selection; more and stale ids do not")
    func selectionMapping() throws {
        let next = session("next", time: 2)
        let chat = SwarmProjectSession(sessions: [next, session("root", time: 1)], title: "Continued")
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project([chat])]))
        let agents: [SwarmSessionID: [SwarmAgent]] = [next.id: [
            SwarmAgent(id: .init("worker"), role: "code", pane: "%1", alive: true),
        ]]
        let workspace = try #require(SidebarRows.selection(for: "/repo", in: entries, agentsBySession: agents))
        #expect(workspace.workspaceID == "/repo")
        #expect(workspace.chatID == nil)
        let chatTarget = try #require(SidebarRows.selection(for: "chat:root", in: entries, agentsBySession: agents))
        #expect(chatTarget.chatID == next.id)
        #expect(chatTarget.agentID == nil)
        let child = try #require(SidebarRows.selection(for: "child:next/worker", in: entries, agentsBySession: agents))
        #expect(child.workspaceID == "/repo")
        #expect(child.chatID == next.id)
        #expect(child.agentID == .init("worker"))
        #expect(child.agentSessionID == next.id)
        #expect(child.rowID == "child:next/worker")
        #expect(SidebarRows.selection(for: "more:/repo", in: entries, agentsBySession: agents) == nil)
        #expect(SidebarRows.selection(for: "chat:missing", in: entries, agentsBySession: agents) == nil)
        #expect(SidebarRows.selection(for: "child:next/missing", in: entries, agentsBySession: agents) == nil)
        var navigation = WorkspaceNavigation()
        navigation.select(entries[0], chat: child.chatID)
        #expect(navigation.selectedWorkspace == child.workspaceID)
        #expect(navigation.selectedChats[child.workspaceID] == "next")
    }

    @Test("A hidden child highlights its chat and a hidden chat highlights its workspace")
    func selectionFolds() throws {
        let chat = SwarmProjectSession(sessions: [session("chat", time: 1)], title: "Work")
        let project = project([chat])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        let agents: [SwarmSessionID: [SwarmAgent]] = [chat.id: [
            SwarmAgent(id: .init("worker"), role: "code", pane: "%1", alive: true),
        ]]
        let child = try #require(SidebarRows.selection(for: "child:chat/worker", in: entries, agentsBySession: agents))
        var navigation = WorkspaceNavigation()
        #expect(SidebarRows.selectedID(in: rows(project, agents: agents), workspace: "/repo", chat: chat, child: child) == "chat:chat")
        navigation.toggleCollapsed("chat:chat")
        #expect(SidebarRows.selectedID(in: rows(project, navigation: navigation, agents: agents), workspace: "/repo",
                                       chat: chat, child: child) == "child:chat/worker")
        navigation.toggleCollapsed("/repo")
        #expect(SidebarRows.selectedID(in: rows(project, navigation: navigation, agents: agents), workspace: "/repo",
                                       chat: chat, child: child) == "/repo")
    }
}
