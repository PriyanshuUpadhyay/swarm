import Foundation
import Testing
@testable import SwarmCore

@Suite("Setup settings")
struct SetupSettingsTests {
    @Test("Changing trust plans and applies only trust, even after setup is complete")
    func trustChoice() async throws {
        actor RecordedCalls {
            var arguments: [[String]] = []
            func record(_ value: [String]) { arguments.append(value) }
        }
        let recorder = RecordedCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/temporary-project", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            await recorder.record(arguments)
            return ShellResult(status: 0,
                               stdout: #"{"digest":"ready","consent":"ask","files":[],"conflicts":[]}"#,
                               stderr: "")
        }
        for standing in [false, true] {
            var choice = SwarmSetupChoice.trustOnly(standing: standing)
            let plan = try await bus.setupPlan(choice)
            #expect(plan.isSetUp)
            choice.take(plan)
            #expect(choice.standing == standing)
            #expect(choice.checked == ["trust"])
            try await bus.setUp(digest: plan.digest, choice: choice)
        }
        #expect(await recorder.arguments == [
            ["setup", "--plan", "--json", "--only", "trust", "--consent", "ask"],
            ["setup", "--digest", "ready", "--only", "trust", "--consent", "ask"],
            ["setup", "--plan", "--json", "--only", "trust", "--consent", "standing"],
            ["setup", "--digest", "ready", "--only", "trust", "--consent", "standing"],
        ])
    }

    @Test("Each dependency reports its own present and missing state")
    func dependencies() {
        let names = ["tmux", "claude", "codex", "agy", "gh", "yelo"]
        for foundName in names {
            let rows = Dependencies.check { $0 == foundName ? "/tools/\($0)" : nil }
            #expect(rows.map(\.name) == names)
            for row in rows {
                #expect(row.path == (row.name == foundName ? "/tools/\(row.name)" : nil))
                #expect(!row.installHint.isEmpty)
            }
        }
        #expect(Dependencies.check { _ in nil }.allSatisfy { $0.path == nil })
        #expect(Dependencies.check { "/tools/\($0)" }.allSatisfy { $0.path != nil })
    }

    @Test("Guard lists decode required and optional fields and round trip through the real file")
    func guardRoundTrip() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("guards.json")
        let fixture = #"{"rules":[{"name":"policy","event":"PreToolUse","tools":["Bash"],"command":["/bin/sh","~/policy.sh"],"timeout":3},{"name":"all-tools","event":"PreToolUse","command":["/bin/true"]}]}"#
        try Data(fixture.utf8).write(to: file)
        let list = try GuardRules.load(url: file).get()
        #expect(list.rules == [
            .init(name: "policy", tools: ["Bash"], command: ["/bin/sh", "~/policy.sh"], timeout: 3),
            .init(name: "all-tools", command: ["/bin/true"]),
        ])
        try list.save(to: file)
        #expect(try GuardRules.load(url: file).get() == list)
        try GuardRules().save(to: file)
        #expect(try GuardRules.load(url: file).get().rules.isEmpty)
    }

    @Test("Missing guard files give an empty editor and broken files retain a reason")
    func guardFailures() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("guards.json")
        #expect(try GuardRules.load(url: file).get() == GuardRules())
        for fixture in ["broken", "{}", #"{"rules":"wrong"}"#,
                        #"{"rules":[{"name":"policy","event":"PreToolUse","command":["/bin/true"],"timeout":-1}]}"#] {
            try Data(fixture.utf8).write(to: file)
            guard case .failure(let error) = GuardRules.load(url: file) else {
                Issue.record("Expected a broken guard list"); continue
            }
            #expect(!error.reason.isEmpty)
            #expect(try String(contentsOf: file, encoding: .utf8) == fixture)
        }
        for fixture in [#"{"rules":[],"unexpected":true}"#,
                        #"{"rules":[{"name":"policy","event":"PreToolUse","command":[],"unexpected":true}]}"#] {
            try Data(fixture.utf8).write(to: file)
            guard case .failure(let error) = GuardRules.load(url: file) else {
                Issue.record("Expected rejection of the unknown key"); continue
            }
            #expect(error.reason.contains("unexpected"))
        }
    }

    @Test("Diff preferences default on old choices and preserve the saved layout")
    func diffPreference() throws {
        let decoder = JSONDecoder()
        #expect(try decoder.decode(Prefs.self, from: Data("{}".utf8)).splitDiff == false)
        #expect(try decoder.decode(Prefs.self, from: Data(#"{"splitDiff":true}"#.utf8)).splitDiff)
        let prefs = Prefs(settingsPage: "setup", splitDiff: true)
        #expect(try decoder.decode(Prefs.self, from: JSONEncoder().encode(prefs)) == prefs)
    }

    private func temporaryFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }
}
