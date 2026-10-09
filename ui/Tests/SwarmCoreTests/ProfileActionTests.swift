import Foundation
import Testing
@testable import SwarmCore

@Suite("Profile actions")
struct ProfileActionTests {
    @Test("Each action sends separate arguments and the revision, and reads the new revision")
    func actionArguments() async throws {
        let actions: [(ProfileAction, [String])] = [
            (.new("custom"), ["new", "custom"]),
            (.rename(from: "custom", to: "renamed"), ["rename", "custom", "renamed"]),
            (.copy(from: "renamed", to: "copied"), ["copy", "renamed", "copied"]),
            (.delete("copied"), ["delete", "copied"]),
            (.reset, ["reset"]),
            (.setMinUsage(37), ["set-min-usage", "37"]),
        ]
        for (action, arguments) in actions {
            let recorder = ProfileActionCalls()
            let bus: any SwarmBus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
                executable, argv, cwd, environment, stdin, timeout in
                #expect(executable == "swarm")
                #expect(cwd == "/fixture")
                #expect(environment["SWARM_SESSION_ID"] == nil)
                #expect(stdin == nil)
                #expect(timeout == .seconds(20))
                return await recorder.reply(argv)
            }
            #expect(try await bus.profileAction(action, revision: "read-revision") == "saved-revision")
            #expect(await recorder.calls == [["roles"] + arguments + ["--revision", "read-revision"]])
        }
    }

    @Test("An action keeps the CLI error text and rejects a malformed revision response")
    func failedAction() async {
        let refused = SwarmCLIBus(environment: [:], cwd: "/fixture") { _, _, _, _, _, _ in
            ShellResult(status: 1, stdout: "", stderr: "swarm: profiles changed on disk; reload and try again\n")
        }
        await #expect(throws: SwarmProfileError.failed("swarm: profiles changed on disk; reload and try again")) {
            try await refused.profileAction(.reset, revision: "stale")
        }
        let malformed = SwarmCLIBus(environment: [:], cwd: "/fixture") { _, _, _, _, _, _ in
            ShellResult(status: 0, stdout: "{}", stderr: "")
        }
        await #expect(throws: SwarmProfileError.failed("swarm returned invalid JSON")) {
            try await malformed.profileAction(.reset, revision: "current")
        }
    }

    @Test("The disconnected stand-in reports an unavailable action")
    func unavailableAction() async {
        let bus: any SwarmBus = UnavailableSwarmBus()
        await #expect(throws: SwarmProfileError.unavailable("swarm is not connected")) {
            try await bus.profileAction(.new("custom"), revision: "current")
        }
    }

    @Test("The name field rejects empty, slash, whitespace and taken names before submission")
    func nameRule() {
        let existing: Set<String> = ["chat", "code.simple"]
        for name in ["", "path/name", "two words", "line\nbreak", "tab\tname", "\u{00A0}", "chat", "code.simple"] {
            #expect(ProfileNameRule.check(name, existing: existing) != nil)
        }
        for name in ["custom", "review.new", "code-simple_2"] {
            #expect(ProfileNameRule.check(name, existing: existing) == nil)
        }
    }
}

private actor ProfileActionCalls {
    private(set) var calls: [[String]] = []

    func reply(_ arguments: [String]) -> ShellResult {
        calls.append(arguments)
        return ShellResult(status: 0, stdout: #"{"revision":"saved-revision"}"#, stderr: "")
    }
}
