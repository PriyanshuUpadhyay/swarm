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
        navigation.toggleCollapsed("/repo")
        let workspaceFold = rows(project, navigation: navigation, agents: agents)
        #expect(workspaceFold.count == 1)
        #expect(workspaceFold[0].expanded == false)
        #expect(workspaceFold[0].status == .waiting)
        let restored = try JSONDecoder().decode(WorkspaceNavigation.self, from: JSONEncoder().encode(navigation))
        #expect(restored == navigation)
    }

    @Test("Chat ids use the oldest chain member and children keep their session id")
    func stableChainIDs() {
        let root = session("root", time: 1)
        let next = session("next", time: 2)
        let chat = SwarmProjectSession(sessions: [next, root], title: "Continued")
        var navigation = WorkspaceNavigation()
        navigation.toggleCollapsed("chat:root")
        let result = rows(project([chat]), navigation: navigation, agents: [
            root.id: [SwarmAgent(id: .init("worker"), role: "old", pane: nil, alive: false)],
            next.id: [SwarmAgent(id: .init("worker"), role: "new", pane: "%1", alive: true)],
        ])
        #expect(result.map(\.id) == ["/repo", "chat:root", "child:next/worker", "child:root/worker"])
    }

    @Test("A waiting child in an older chain member reaches a folded workspace")
    func olderChildUrgency() {
        let chat = SwarmProjectSession(sessions: [session("next", time: 2), session("root", time: 1)],
                                       title: "Continued", status: .working)
        var navigation = WorkspaceNavigation()
        navigation.toggleCollapsed("/repo")
        let folded = rows(project([chat]), navigation: navigation, agents: [
            .init("root"): [SwarmAgent(id: .init("worker"), role: "review", pane: "%1", alive: true, state: "waiting")],
        ])
        #expect(folded[0].status == .waiting)
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
}
