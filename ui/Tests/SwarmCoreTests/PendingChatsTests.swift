import Foundation
import Testing
@testable import SwarmCore

@Suite("Pending chats")
struct PendingChatsTests {
    // The text `swarm launch` prints when no runner can run (`resolve_role` in `src/main.rs`).
    private let noCLI = """
        swarm: chat: no runner can run
          1 claude · opus: claude CLI not found on PATH
          2 codex · gpt-5.5: codex CLI not found on PATH
        """

    @Test("No provider CLI on the Mac asks for an install; any other failure does not")
    func missingCLI() {
        #expect(LaunchFailure(message: noCLI).missingCLI)
        #expect(LaunchFailure(SwarmProfileError.failed(noCLI)).missingCLI)
        #expect(LaunchFailure(message: noCLI).message == noCLI)
        let signedOut = """
            swarm: chat: no runner can run
              1 claude · opus: claude CLI not found on PATH
              2 codex · gpt-5.5: no signed-in account
            """
        #expect(!LaunchFailure(message: signedOut).missingCLI)
        #expect(!LaunchFailure(message: "swarm: chat: no runner can run").missingCLI)
        #expect(!LaunchFailure(message: "not signed in").missingCLI)
        #expect(!LaunchFailure(SwarmProfileError.unavailable("swarm: not found")).missingCLI)
    }

    @Test("Two starts in one workspace are two pending chats, each with its own tab")
    func twoStarts() {
        var pending = PendingChats()
        let first = pending.add(directory: "/api", previous: SwarmSessionID("old-chat"))
        let second = pending.add(directory: "/api", previous: nil)
        #expect(first != second)
        #expect(pending.inWorkspace("/api").map(\.id) == [first, second])
        #expect(pending.inWorkspace("/docs").isEmpty)
        #expect(pending[first]?.previous == SwarmSessionID("old-chat"))
        let tabs = ChatTab.tabs([], pending: pending.items, closing: [], now: 0)
        #expect(tabs.map(\.title) == ["New chat", "New chat"])
        #expect(Set(tabs.map(\.id)).count == 2)
        #expect(tabs.allSatisfy { $0.pending == .starting && !$0.canClose })
    }

    @Test("A chat leaves the pending list only when launched and listed in the tree")
    func settle() {
        var pending = PendingChats()
        let launched = pending.add(directory: "/api", previous: nil)
        let created = pending.add(directory: "/api", previous: nil)
        let failed = pending.add(directory: "/api", previous: nil)
        pending.update(launched) { $0.session = SwarmSessionID("launched-session"); $0.state = .launched }
        pending.update(created) { $0.session = SwarmSessionID("created-session") }
        pending.update(failed) {
            $0.session = SwarmSessionID("failed-session")
            $0.state = .failed(LaunchFailure(message: "not signed in"))
        }
        #expect(pending.settle { _ in false }.isEmpty)
        let done = pending.settle { _ in true }
        #expect(done.map(\.id) == [launched])
        #expect(pending.items.map(\.id) == [created, failed])
        #expect(pending.sessions == [SwarmSessionID("created-session"), SwarmSessionID("failed-session")])
        #expect(pending.remove(failed)?.session == SwarmSessionID("failed-session"))
        #expect(pending.remove(failed) == nil)
    }

    @Test("A session that a pending tab stands for shows once, as the pending tab")
    func oneTabPerChat() {
        let made = SwarmSession(
            id: SwarmSessionID("made-session"), talkMode: "lane", adapter: "tmux-solo", cwd: "/api",
            createdAt: 1, chairProvider: "codex", chairID: SwarmChairID("chair-1"), chairLog: nil,
            agents: 1, messages: 0, lastMessageAt: nil, archivedAt: nil
        )
        let tree = SessionsTree.build(
            sessions: [made], projectPaths: ["/api"], agentsBySession: [:], titles: [:],
            repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] }
        )
        let chats = tree.workspaceChats(for: made.id)
        #expect(chats.map(\.id) == [made.id])
        var pending = PendingChats()
        let id = pending.add(directory: "/api", previous: nil)
        pending.update(id) {
            $0.session = made.id
            $0.state = .failed(LaunchFailure(message: "not signed in"))
        }
        let tabs = ChatTab.tabs(chats, pending: pending.items, closing: [], now: 0)
        #expect(tabs.map(\.id) == [pending.items[0].tabID])
        #expect(tabs[0].pending == .failed)
    }

    @Test("Two starts at once create their sessions one at a time")
    func serialCreate() async throws {
        let probe = OverlapProbe()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            await probe.enter()
            try? await Task.sleep(for: .milliseconds(20))
            await probe.leave()
            return ShellResult(status: 0, stdout: arguments == ["init"] ? "" : "session-id\n", stderr: "")
        }
        let plan = try #require(SwarmChatLaunchPlan(profileIn: "/work"))
        async let first = SwarmChatLauncher.create(plan, bus: bus)
        async let second = SwarmChatLauncher.create(plan, bus: bus)
        _ = try await (first, second)
        #expect(await probe.most == 1)
        #expect(await probe.calls == 4)
    }
}

private actor OverlapProbe {
    private var running = 0
    private(set) var most = 0
    private(set) var calls = 0

    func enter() {
        running += 1
        calls += 1
        most = max(most, running)
    }

    func leave() { running -= 1 }
}
