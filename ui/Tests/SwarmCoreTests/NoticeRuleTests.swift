import Foundation
import Testing
@testable import SwarmCore

@Suite("Owner notices")
struct NoticeRuleTests {
    @Test("Missing and null notice fields keep defaults and saved fields round trip")
    func preferences() throws {
        let decoder = JSONDecoder()
        for json in ["{}", #"{"notices":null}"#, #"{"notices":{}}"#] {
            #expect(try decoder.decode(Prefs.self, from: Data(json.utf8)).notices == NoticePrefs())
        }
        var notices = NoticePrefs()
        notices.post = false
        notices.sound = false
        notices.done = false
        notices.badge = false
        notices.mutedProjects = ["/project", "/other"]
        let prefs = Prefs(settingsPage: "notifications", splitDiff: true, notices: notices)
        #expect(try decoder.decode(Prefs.self, from: JSONEncoder().encode(prefs)) == prefs)
        let partial = try decoder.decode(NoticePrefs.self, from: Data(#"{"sound":false,"done":null,"future":1}"#.utf8))
        #expect(partial.post && !partial.sound && partial.done && partial.badge && partial.mutedProjects.isEmpty)
    }

    @Test("Each notice carries the chat title, fixed body, session id, and sound choice")
    func contents() throws {
        let waiting = chat("waiting", status: .waiting)
        let project = project(chats: [waiting])
        let event = NoticeEvent(kind: .needsInput, chat: waiting, projectPath: project.path)
        let notice = try #require(NoticeRule.shouldPost(event: event, prefs: NoticePrefs(), project: project, title: "Owner renamed chat"))
        #expect(notice.title == "Swarm — Owner renamed chat")
        #expect(notice.body == "project: waiting on a permission or a question")
        #expect(notice.sessionID == waiting.id && notice.sound)
        var silent = NoticePrefs()
        silent.sound = false
        let done = NoticeEvent(kind: .done, chat: waiting, projectPath: project.path)
        let completion = try #require(NoticeRule.shouldPost(event: done, prefs: silent, project: project, title: "Owner renamed chat"))
        #expect(completion.title == "Swarm — Owner renamed chat")
        #expect(completion.body == "project: chat is done")
        #expect(!completion.sound)
    }

    @Test("Post and project mute suppress all notices; done off still permits needs-input")
    func suppression() {
        let waiting = chat("waiting", status: .waiting)
        let project = project(chats: [waiting])
        let input = NoticeEvent(kind: .needsInput, chat: waiting, projectPath: project.path)
        let done = NoticeEvent(kind: .done, chat: waiting, projectPath: project.path)
        var prefs = NoticePrefs()
        prefs.post = false
        #expect(NoticeRule.shouldPost(event: input, prefs: prefs, project: project, title: "Owner renamed chat") == nil)
        #expect(NoticeRule.shouldPost(event: done, prefs: prefs, project: project, title: "Owner renamed chat") == nil)
        prefs.post = true
        prefs.mutedProjects = [project.path]
        #expect(NoticeRule.shouldPost(event: input, prefs: prefs, project: project, title: "Owner renamed chat") == nil)
        #expect(NoticeRule.shouldPost(event: done, prefs: prefs, project: project, title: "Owner renamed chat") == nil)
        prefs.mutedProjects = ["/other"]
        prefs.done = false
        #expect(NoticeRule.shouldPost(event: done, prefs: prefs, project: project, title: "Owner renamed chat") == nil)
        #expect(NoticeRule.shouldPost(event: input, prefs: prefs, project: project, title: "Owner renamed chat") != nil)
    }

    @Test("Snapshots post once per change and can post again after work resumes")
    func changes() {
        let working = tree(chats: [chat("input", status: .working), chat("finished", status: .working)])
        let changed = tree(chats: [chat("input", status: .waiting), chat("finished", status: .done)])
        let events = NoticeRule.transitions(previous: working, current: changed)
        #expect(events.map(\.kind) == [.needsInput, .done])
        #expect(events.map(\.chat.id.rawValue) == ["input", "finished"])
        #expect(events.allSatisfy { $0.projectPath == "/project" })
        #expect(NoticeRule.transitions(previous: changed, current: changed).isEmpty)
        #expect(NoticeRule.transitions(previous: changed, current: working).isEmpty)
        #expect(NoticeRule.transitions(previous: working, current: changed) == events)
    }

    @Test("First-seen and unknown states cannot create completion or recovery notices")
    func firstSightAndUnknown() {
        let empty = SessionsTree(projects: [])
        let first = tree(chats: [chat("new-input", status: .waiting), chat("new-done", status: .done), chat("unknown", status: nil)])
        #expect(NoticeRule.transitions(previous: empty, current: first).map(\.kind) == [.needsInput])
        let unknown = tree(chats: [chat("known", status: nil)])
        for state in [AgentStatus.waiting, .done, .failed, .ended] {
            let known = tree(chats: [chat("known", status: state)])
            #expect(NoticeRule.transitions(previous: unknown, current: known).isEmpty)
            #expect(NoticeRule.transitions(previous: known, current: unknown).isEmpty)
        }
        #expect(NoticeRule.transitions(previous: first, current: empty).isEmpty)
    }

    @Test("A handoff retains the state of its chat chain and uses the new session id")
    func handoff() {
        let previous = chat("original", status: .waiting)
        var next = chat("handoff", status: .waiting)
        next.sessions.append(previous.session)
        #expect(NoticeRule.transitions(previous: tree(chats: [previous]), current: tree(chats: [next])).isEmpty)
        next.status = .done
        let events = NoticeRule.transitions(previous: tree(chats: [previous]), current: tree(chats: [next]))
        #expect(events.count == 1 && events.first?.chat.id == next.id && events.first?.kind == .done)
    }

    @Test("The dock counts chats, including muted projects, and only badge off clears it")
    func badge() {
        let waiting = chat("input", status: .waiting)
        var chain = chat("chain", status: .waiting)
        chain.sessions.append(chat("older", status: .waiting).session)
        let snapshot = tree(chats: [waiting, chain, chat("done", status: .done), chat("failed", status: .failed), chat("unknown", status: nil)])
        var prefs = NoticePrefs()
        #expect(DockBadge.count(tree: snapshot, prefs: prefs) == 2)
        prefs.post = false
        prefs.mutedProjects = ["/project"]
        #expect(DockBadge.count(tree: snapshot, prefs: prefs) == 2)
        prefs.badge = false
        #expect(DockBadge.count(tree: snapshot, prefs: prefs) == 0)
        #expect(DockBadge.count(tree: SessionsTree(projects: []), prefs: NoticePrefs()) == 0)
    }

    private func chat(_ name: String, status: AgentStatus?) -> SwarmProjectSession {
        let session = SwarmSession(id: SwarmSessionID(name), talkMode: "lane", adapter: "herdr",
                                   cwd: "/project", createdAt: 10, chairLog: nil,
                                   agents: 1, messages: 0, lastMessageAt: nil)
        return SwarmProjectSession(sessions: [session], title: "Chat \(name)", status: status)
    }

    private func project(chats: [SwarmProjectSession]) -> ProjectNode {
        ProjectNode(id: .folder("/project"), path: "/project", launchDirectory: "/project",
                    workspaces: [WorkspaceNode(path: "/project", name: "project", sessions: chats)])
    }

    private func tree(chats: [SwarmProjectSession]) -> SessionsTree {
        SessionsTree(projects: [project(chats: chats)])
    }
}
