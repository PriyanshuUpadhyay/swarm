import Foundation
import Testing
@testable import SwarmCore

@Suite("New chat launch")
struct SwarmChatLaunchTests {
    @Test("A choice creates a tmux-solo session then launches the chair")
    func arguments() async throws {
        let id = "01a0c8f8-78e1-7478-a117-24f1e1dff304"
        let calls = LaunchCalls([
            ShellResult(status: 0, stdout: "", stderr: ""),
            ShellResult(status: 0, stdout: id + "\n", stderr: ""),
            ShellResult(status: 0, stdout: "%1\n", stderr: ""),
        ])
        let bus = bus(calls)
        let plan = try #require(SwarmChatLaunchPlan(
            directory: "/work", provider: "claude", model: "sonnet", account: .auto
        ))
        let created = try await SwarmChatLauncher.start(plan, bus: bus)
        #expect(created.rawValue == id)
        #expect(await calls.arguments == [
            ["init"], ["session", "new", "lane"],
            ["launch", "orchestrator", "chat", "--provider", "claude", "--model", "sonnet", "--account", "auto"],
        ])
        #expect(await calls.adapters == ["tmux-solo", "tmux-solo", "tmux-solo"])
    }

    @Test("The chat profile launches with no provider or model, so swarm picks its runner")
    func profileArguments() async throws {
        let calls = LaunchCalls([
            ShellResult(status: 0, stdout: "", stderr: ""),
            ShellResult(status: 0, stdout: "session-id\n", stderr: ""),
            ShellResult(status: 0, stdout: "%1\n", stderr: ""),
        ])
        let plan = try #require(SwarmChatLaunchPlan(profileIn: "/work"))
        _ = try await SwarmChatLauncher.start(plan, bus: bus(calls))
        #expect(await calls.arguments.last == ["launch", "orchestrator", "chat", "--account", "auto"])
        #expect(SwarmChatLaunchPlan(profileIn: "relative") == nil)
    }

    @Test("A named profile reaches launch with automatic account choice and no provider or model override")
    func namedProfile() async throws {
        let calls = LaunchCalls([
            ShellResult(status: 0, stdout: "", stderr: ""),
            ShellResult(status: 0, stdout: "session-id\n", stderr: ""),
            ShellResult(status: 0, stdout: "%1\n", stderr: ""),
        ])
        let plan = try #require(SwarmChatLaunchPlan(profile: "code.complex", in: "/work"))
        #expect(plan.directory == "/work")
        #expect(plan.role == "code.complex")
        #expect(plan.provider == nil)
        #expect(plan.model == nil)
        #expect(plan.account == "auto")
        _ = try await SwarmChatLauncher.start(plan, bus: bus(calls))
        #expect(await calls.arguments.last == ["launch", "orchestrator", "code.complex", "--account", "auto"])
        #expect(SwarmChatLaunchPlan(profile: "", in: "/work") == nil)
        #expect(SwarmChatLaunchPlan(profile: " \n", in: "/work") == nil)
        #expect(SwarmChatLaunchPlan(profile: "chat", in: "relative") == nil)
    }

    @Test("Start returns the CLI stderr when launch fails")
    func failure() async throws {
        let calls = LaunchCalls([
            ShellResult(status: 0, stdout: "", stderr: ""),
            ShellResult(status: 0, stdout: "session-id\n", stderr: ""),
            ShellResult(status: 1, stdout: "", stderr: "not signed in\nmore detail\n"),
        ])
        let plan = try #require(SwarmChatLaunchPlan(
            directory: "/work", provider: "claude", model: "sonnet", account: .auto
        ))
        do {
            _ = try await SwarmChatLauncher.start(plan, bus: bus(calls))
            Issue.record("Launch should fail")
        } catch let error as SwarmProfileError {
            #expect(error.message == "not signed in\nmore detail")
        }
    }

    private func bus(_ calls: LaunchCalls) -> SwarmCLIBus {
        SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await calls.reply(arguments: arguments, adapter: environment["SWARM_ADAPTER"] ?? "")
        }
    }
}

private actor LaunchCalls {
    private var replies: [ShellResult]
    private(set) var arguments: [[String]] = []
    private(set) var adapters: [String] = []

    init(_ replies: [ShellResult]) { self.replies = replies }

    func reply(arguments: [String], adapter: String) -> ShellResult {
        self.arguments.append(arguments)
        adapters.append(adapter)
        return replies.removeFirst()
    }
}
