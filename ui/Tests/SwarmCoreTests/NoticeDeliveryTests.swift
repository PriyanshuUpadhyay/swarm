import Foundation
import Testing
@testable import SwarmCore

@Suite("Notice delivery")
struct NoticeDeliveryTests {
    @Test("Concurrent posts ask once at the first post and carry every notice")
    func authorizationOnce() async throws {
        let calls = DeliveryCalls()
        let delivery = NoticeDelivery(authorize: { await calls.authorize(granted: true) },
                                      deliver: { await calls.deliver($0) }, onDenied: { await calls.denied() })
        #expect(await calls.authorizationCount == 0)
        let notice = notice()
        try await withThrowingTaskGroup(of: Void.self) { group in
            for _ in 0..<6 { group.addTask { try await delivery.post(notice) } }
            try await group.waitForAll()
        }
        #expect(await calls.authorizationCount == 1)
        #expect(await calls.notices == Array(repeating: notice, count: 6))
        try await delivery.post(notice)
        #expect(await calls.authorizationCount == 1)
        #expect(await calls.notices.count == 7)
    }

    @Test("Denial never posts and is checked again on the next post")
    func denied() async throws {
        let calls = DeliveryCalls()
        let delivery = NoticeDelivery(authorize: { await calls.authorize(granted: false) },
                                      deliver: { await calls.deliver($0) }, onDenied: { await calls.denied() })
        try await delivery.post(notice())
        try await delivery.post(notice())
        #expect(await calls.authorizationCount == 2)
        #expect(await calls.notices.isEmpty)
        #expect(await calls.denialCount == 1)
    }

    @Test("Authorization and post errors reach the caller without retrying")
    func failures() async {
        let authCalls = DeliveryCalls()
        let failedAuth = NoticeDelivery(authorize: {
            _ = await authCalls.authorize(granted: false)
            throw DeliveryFailure.failed
        }, deliver: { await authCalls.deliver($0) }, onDenied: { await authCalls.denied() })
        for _ in 0..<2 {
            await #expect(throws: DeliveryFailure.self) { try await failedAuth.post(notice()) }
        }
        #expect(await authCalls.authorizationCount == 2)
        #expect(await authCalls.notices.isEmpty)
        let postCalls = DeliveryCalls()
        let failedPost = NoticeDelivery(authorize: { await postCalls.authorize(granted: true) }, deliver: {
            await postCalls.deliver($0)
            throw DeliveryFailure.failed
        }, onDenied: { await postCalls.denied() })
        await #expect(throws: DeliveryFailure.self) { try await failedPost.post(notice()) }
        #expect(await postCalls.notices.count == 1)
    }

    @Test("Enabling notifications after denial delivers the next post without another denial banner")
    func enabledAfterDenial() async throws {
        let calls = DeliveryCalls()
        let delivery = NoticeDelivery(authorize: { try await calls.authorizeCurrent() },
                                      deliver: { await calls.deliver($0) }, onDenied: { await calls.denied() })
        try await delivery.post(notice())
        #expect(await calls.notices.isEmpty)
        await calls.setAuthorization(granted: true)
        try await delivery.post(notice())
        try await delivery.post(notice())
        #expect(await calls.authorizationCount == 2)
        #expect(await calls.notices.count == 2)
        #expect(await calls.denialCount == 1)
    }

    @Test("A failed authorization can recover on the next post")
    func recoveredAuthorization() async throws {
        let calls = DeliveryCalls()
        await calls.setAuthorization(granted: false, fails: true)
        let delivery = NoticeDelivery(authorize: { try await calls.authorizeCurrent() },
                                      deliver: { await calls.deliver($0) }, onDenied: { await calls.denied() })
        await #expect(throws: DeliveryFailure.self) { try await delivery.post(notice()) }
        await calls.setAuthorization(granted: true)
        try await delivery.post(notice())
        #expect(await calls.authorizationCount == 2)
        #expect(await calls.notices == [notice()])
        #expect(await calls.denialCount == 0)
    }

    @Test("A click selects the live chat or reopens its own window across handoffs")
    func destinations() {
        let original = session("original", time: 10)
        let current = session("handoff", time: 20)
        let chat = SwarmProjectSession(sessions: [current, original], title: "Chat", status: .waiting)
        let project = ProjectNode(id: .folder("/project"), path: "/project", launchDirectory: "/project",
                                  workspaces: [WorkspaceNode(path: "/project", name: "project", sessions: [chat])])
        let tree = SessionsTree(projects: [project])
        #expect(NoticeDestination.resolve(sessionID: original.id, tree: tree, openChats: []) == .mainChat(current.id))
        #expect(NoticeDestination.resolve(sessionID: current.id, tree: tree, openChats: [original.id]) == .chatWindow(original.id))
        #expect(NoticeDestination.resolve(sessionID: original.id, tree: tree, openChats: [original.id]) == .chatWindow(original.id))
        #expect(NoticeDestination.resolve(sessionID: SwarmSessionID("missing"), tree: tree, openChats: []) == nil)
    }

    private func notice() -> Notice {
        Notice(title: "Swarm — Chat", body: "project: chat is done", sessionID: SwarmSessionID("chat"), sound: false)
    }

    private func session(_ role: String, time: Int) -> SwarmSession {
        SwarmSession(id: SwarmSessionID(role), talkMode: "lane", adapter: "herdr", cwd: "/project",
                     createdAt: time, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
    }
}

private enum DeliveryFailure: Error { case failed }

private actor DeliveryCalls {
    var authorizationCount = 0
    var notices: [Notice] = []
    var denialCount = 0
    private var granted = false
    private var fails = false
    func denied() { denialCount += 1 }
    func setAuthorization(granted: Bool, fails: Bool = false) {
        self.granted = granted
        self.fails = fails
    }
    func authorizeCurrent() async throws -> Bool {
        authorizationCount += 1
        if fails { throw DeliveryFailure.failed }
        return granted
    }
    func authorize(granted: Bool) async -> Bool {
        authorizationCount += 1
        await Task.yield()
        return granted
    }
    func deliver(_ notice: Notice) { notices.append(notice) }
}
