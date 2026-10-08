import Foundation
import Testing
@testable import SwarmCore

@Suite("Recently closed chats")
struct RecentlyClosedTests {
    @Test("The list groups archived sessions and sorts archive time, with title, workspace and age")
    func list() async throws {
        let current = session("current", archivedAt: 180, createdAt: 20, continuation: "older")
        let older = session("older", archivedAt: 170, createdAt: 10)
        let newest = session("newest", archivedAt: 200, createdAt: 1)
        let active = session("active", archivedAt: nil, createdAt: 30)
        let discovery = SwarmSessionDiscovery(home: URL(fileURLWithPath: "/missing/fixture-home"))
        let groups = await discovery.archivedChats([older, newest, active, current])
        #expect(groups.count == 2)
        var navigation = WorkspaceNavigation()
        navigation.chatNames["older"] = "Saved title"
        navigation.names["/fixture/workspace"] = "Feature workspace"
        let rows = RecentlyClosed.list(chats: groups, navigation: navigation, workspaces: [], now: 260)
        #expect(rows.map(\.id) == [.init("newest"), .init("current")])
        #expect(rows.map(\.age) == ["1m ago", "1m ago"])
        #expect(rows[1].title == "Saved title")
        #expect(rows[1].workspace == "Feature workspace")
        #expect(rows[1].chat.sessions.map(\.id) == [.init("current"), .init("older")])
        #expect(rows[1].archivedAt == 180)
        #expect(RecentlyClosed.list(chats: groups, navigation: .init(), workspaces: [], now: 0)
            .allSatisfy { $0.age == "0s ago" && $0.workspace == "workspace" })
    }

    @Test("The shared row age keeps seconds, minutes, hours and days")
    func ageText() {
        #expect(SessionRowPresentation.ageText(since: 0, now: 45) == "45s")
        #expect(SessionRowPresentation.ageText(since: 0, now: 180) == "3m")
        #expect(SessionRowPresentation.ageText(since: 0, now: 7_200) == "2h")
        #expect(SessionRowPresentation.ageText(since: 0, now: 345_600) == "4d")
        #expect(SessionRowPresentation.ageText(since: 10, now: 0) == "0s")
    }

    @Test("Restore calls unarchive once with every id, and a failed call reaches the caller")
    func restore() async throws {
        let recorder = ClosedChatCalls()
        let chat = SwarmProjectSession(sessions: [session("current"), session("older")], title: "Chat")
        try await RecentlyClosed.restore(chat, bus: bus(recorder))
        #expect(await recorder.calls == [["session", "unarchive", "current", "older"]])
        let failed = ClosedChatCalls(fail: true)
        await #expect(throws: (any Error).self) {
            try await RecentlyClosed.restore(chat, bus: bus(failed))
        }
        #expect(await failed.calls.count == 1)
    }

    @Test("The CLI listing requests archived rows and filters live rows; empty restores do nothing")
    func cli() async throws {
        let recorder = ClosedChatCalls()
        let value = bus(recorder)
        #expect(try await value.archivedSessions().map(\.id) == [.init("archived")])
        try await value.unarchive([])
        #expect(await recorder.calls == [["sessions", "--json", "--archived"]])
    }

    @Test("An explicit restore clears archive suppression even before absence was discovered")
    func suppression() {
        let chat = SwarmProjectSession(sessions: [session("archived")], title: "Chat")
        let source = SessionsTree(projects: [ProjectNode(
            id: .folder("/fixture"), path: "/fixture", launchDirectory: "/fixture",
            workspaces: [WorkspaceNode(path: "/fixture/workspace", name: "workspace", sessions: [chat])]
        )])
        var archives = ChatArchives()
        _ = archives.begin(chat.id, in: source)
        archives.finish(chat.id, succeeded: true)
        #expect(archives.applying(to: source).session(chat.id) == nil)
        archives.restore(chat.sessions.map(\.id))
        #expect(archives.applying(to: source).session(chat.id) != nil)
    }

    @Test("The CLI cap notice counts archived rows before chat grouping")
    func limitNotice() {
        let limit = RecentlyClosed.Listing.cliArchivedLimit
        #expect(RecentlyClosed.Listing(chats: [], archivedSessionCount: limit - 1).notice == nil)
        #expect(RecentlyClosed.Listing(chats: [], archivedSessionCount: 0).notice == nil)
        #expect(RecentlyClosed.Listing(chats: [], archivedSessionCount: limit).notice ==
                "Showing the newest \(limit) closed sessions.")
    }

    @Test("A superseded restore refresh retries once and then uses the completed list")
    @MainActor
    func refreshSuperseded() async throws {
        var calls = 0
        var listed = false
        let result = try await RecentlyClosed.refreshRestoredChat(refresh: {
            calls += 1
            if calls == 1 { return false }
            listed = true
            return true
        }, isListed: { listed })
        #expect(result)
        #expect(calls == 2)
    }

    @Test("Two superseded refreshes leave restore pending without a false missing-chat error")
    @MainActor
    func refreshPending() async throws {
        var calls = 0
        let result = try await RecentlyClosed.refreshRestoredChat(refresh: {
            calls += 1
            return false
        }, isListed: { false })
        #expect(!result)
        #expect(calls == 2)
    }

    @Test("Only a completed refresh can report a missing restored chat")
    @MainActor
    func refreshMissing() async throws {
        var calls = 0
        await #expect(throws: (any Error).self) {
            try await RecentlyClosed.refreshRestoredChat(refresh: {
                calls += 1
                return true
            }, isListed: { false })
        }
        #expect(calls == 1)
        #expect(try await RecentlyClosed.refreshRestoredChat(refresh: { false }, isListed: { true }))
    }

    @Test("A pending restored selection settles on the next completed list and reports absence once")
    func pendingSelection() {
        let id = SwarmSessionID("restored")
        var pending: PendingChatSelection? = .restored(id)
        #expect(pending?.id == id)
        #expect(pending?.isRestoring == true)
        var notices: [String] = []
        for _ in 0..<2 {
            if let selection = pending {
                pending = selection.afterRefresh(isListed: false)
                if selection.isRestoring { notices.append(RecentlyClosed.restoredButNotListed) }
            }
        }
        #expect(pending == nil)
        #expect(notices == [RecentlyClosed.restoredButNotListed])
        #expect(PendingChatSelection.restored(id).afterRefresh(isListed: true) == nil)
        #expect(PendingChatSelection.handoff(id).afterRefresh(isListed: false) == .handoff(id))
        #expect(PendingChatSelection.handoff(id).afterRefresh(isListed: true) == nil)
    }

    private func session(
        _ id: String, archivedAt: Int? = 100, createdAt: Int = 1, continuation: String? = nil
    ) -> SwarmSession {
        SwarmSession(id: .init(id), talkMode: "lane", adapter: nil, cwd: "/fixture/workspace",
                     createdAt: createdAt, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil,
                     continuationOf: continuation.map(SwarmSessionID.init), archivedAt: archivedAt)
    }

    private func bus(_ recorder: ClosedChatCalls) -> SwarmCLIBus {
        SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            await recorder.reply(arguments)
        }
    }
}

private actor ClosedChatCalls {
    private(set) var calls: [[String]] = []
    let fail: Bool
    init(fail: Bool = false) { self.fail = fail }
    func reply(_ arguments: [String]) -> ShellResult {
        calls.append(arguments)
        if fail { return ShellResult(status: 1, stdout: "", stderr: "Restore refused") }
        if arguments.first == "sessions" {
            return ShellResult(status: 0, stdout: """
                {"sessions":[
                  {"id":"active","talk_mode":"lane","cwd":"/fixture","created_at":1,"agents":1,"messages":0},
                  {"id":"archived","talk_mode":"lane","cwd":"/fixture","created_at":1,"agents":1,"messages":0,"archived_at":2}
                ]}
                """, stderr: "")
        }
        return ShellResult(status: 0, stdout: "", stderr: "")
    }
}
