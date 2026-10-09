import Foundation
import Testing
@testable import SwarmCore

@Suite("Setup groups")
struct SetupGroupTests {
    @Test("Each known group owns its wire name, title, and decline key")
    func mapping() {
        #expect(SetupGroup.allCases.map(\.rawValue) == ["hooks", "trust", "herdr", "skills"])
        #expect(SetupGroup.allCases.map(\.title) == ["Agent hooks", "Folder trust", "Herdr", "Skills"])
        #expect(SetupGroup.allCases.map(\.declineFlagKey) == [
            "hooksSetupDeclined", "trustSetupDeclined", nil, "skillsSetupDeclined",
        ])
        #expect(SetupGroup(rawValue: "future") == nil)
        #expect(SwarmHooksPlan.groupTitle("future") == "future")
    }

    @Test("A decline affects only its group and Herdr still has no flag")
    func declineAndStatus() {
        let flags = ["skillsSetupDeclined": true, "trustSetupDeclined": false]
        let declined = SetupGroup.declined { flags[$0] ?? false }
        #expect(declined == [.skills])
        #expect(!SwarmSetupStatus(hooks: true, trust: true, herdr: true, skills: false).needsSheet(declined: declined))
        #expect(SwarmSetupStatus(hooks: false, trust: true, herdr: true, skills: false).needsSheet(declined: declined))
        #expect(SwarmSetupStatus(hooks: true, trust: true, herdr: false, skills: true).needsSheet(declined: [.herdr]))
    }

    @Test("Undo and restore use the same writer and kind mapping")
    func writerAndKind() {
        #expect(SetupGroup.restoreGroup(writer: "hooks.state", kinds: ["toml_key"]) == .hooks)
        #expect(SetupGroup.restoreGroup(writer: "hooks.guard", kinds: ["json_key"]) == .hooks)
        #expect(SetupGroup.restoreGroup(writer: "skills", kinds: ["symlink"]) == .skills)
        #expect(SetupGroup.restoreGroup(writer: "skills", kinds: ["future_kind"]) == nil)
        #expect(SetupGroup.restoreGroup(writer: "skills", kinds: ["symlink", "future_kind"]) == nil)
        #expect(SetupGroup.restoreGroup(writer: "launch.trust", kinds: ["json_key"]) == nil)
        #expect(SetupGroup.restoreGroup(writer: "herdr", kinds: ["json_key"]) == nil)
        #expect(SetupGroup.restoreGroup(writer: "future.writer", kinds: ["symlink"]) == nil)
    }

    @Test("Known restore routes retain their exact CLI calls")
    func restoreCalls() async throws {
        actor Calls {
            var arguments: [[String]] = []
            func add(_ value: [String]) { arguments.append(value) }
        }
        let calls = Calls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            await calls.add(arguments)
            return ShellResult(status: 0, stdout: #"{"digest":"group-proof","files":[],"conflicts":[]}"#, stderr: "")
        }
        for group in [SetupGroup.hooks, .skills] {
            let plan = try await group.restorePlan(using: bus)
            try await group.restore(using: bus, digest: plan.digest)
        }
        #expect(await calls.arguments == [
            ["hooks", "setup", "--plan", "--json"],
            ["hooks", "setup", "--digest", "group-proof"],
            ["setup", "--plan", "--json", "--only", "skills"],
            ["setup", "--digest", "group-proof", "--only", "skills"],
        ])
    }
}
