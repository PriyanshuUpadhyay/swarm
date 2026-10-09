import Foundation
import Synchronization
import Testing
@testable import SwarmCore

@Suite("Notice delivery")
struct NoticeDeliveryTests {
    @Test("Each sequential post reads permission again")
    func eachPostReadsPermissionAgain() async throws {
        let calls = DeliveryCalls()
        let delivery = NoticeDelivery(authorize: { await calls.authorize(granted: true) },
                                      deliver: { await calls.deliver($0) }, onDenied: { await calls.denied() })
        let notice = notice()
        let postCount = 3
        for expectedCount in 1...postCount {
            try await delivery.post(notice)
            #expect(await calls.authorizationCount == expectedCount)
        }
        #expect(await calls.notices == Array(repeating: notice, count: postCount))
    }

    @Test("Six posts queued during authorization share exactly one request")
    func inFlightAuthorizationIsShared() async throws {
        let postCount = 6
        let gate = AuthorizationGate(posts: postCount)
        let calls = DeliveryCalls()
        let delivery = NoticeDelivery(authorize: {
            await gate.wait()
            return await calls.authorize(granted: true)
        }, deliver: { await calls.deliver($0) }, onDenied: { await calls.denied() })
        let notice = notice()
        try await withThrowingTaskGroup(of: Void.self) { group in
            for _ in 0..<postCount { group.addTask { try await delivery.post(notice, queuedOn: gate) } }
            try await group.waitForAll()
        }
        #expect(await calls.authorizationCount == 1)
        #expect(await calls.notices == Array(repeating: notice, count: postCount))
        try await delivery.post(notice)
        #expect(await calls.authorizationCount == 2)
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
        #expect(await calls.authorizationCount == 3)
        #expect(await calls.notices.count == 2)
        #expect(await calls.denialCount == 1)
    }

    @Test("Revoking a granted permission is read on the next post and denial is reported once")
    func revokedAuthorization() async throws {
        let calls = DeliveryCalls()
        await calls.setAuthorization(granted: true)
        let delivery = NoticeDelivery(authorize: { try await calls.authorizeCurrent() },
                                      deliver: { await calls.deliver($0) }, onDenied: { await calls.denied() })
        try await delivery.post(notice())
        await calls.setAuthorization(granted: false)
        try await delivery.post(notice())
        try await delivery.post(notice())
        #expect(await calls.authorizationCount == 3)
        #expect(await calls.notices == [notice()])
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

private final class AuthorizationGate: Sendable {
    private struct State {
        var queued = 0
        var waiters: [CheckedContinuation<Void, Never>] = []
    }
    private let state = Mutex(State())
    private let posts: Int

    init(posts: Int) { self.posts = posts }

    func wait() async {
        await withCheckedContinuation { continuation in
            let released = state.withLock {
                if $0.queued == posts { return true }
                $0.waiters.append(continuation)
                return false
            }
            if released { continuation.resume() }
        }
    }

    func queued() {
        let waiters = state.withLock {
            $0.queued += 1
            guard $0.queued == posts else { return [CheckedContinuation<Void, Never>]() }
            let waiters = $0.waiters
            $0.waiters = []
            return waiters
        }
        for waiter in waiters { waiter.resume() }
    }
}

private extension NoticeDelivery {
    func post(_ notice: Notice, queuedOn gate: AuthorizationGate) async throws {
        // Stay on the delivery actor until post registers its waiter, so the last signal cannot race it.
        gate.queued()
        try await post(notice)
    }
}
