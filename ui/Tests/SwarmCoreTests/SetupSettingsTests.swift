import Foundation
import Testing
@testable import SwarmCore

@Suite("Setup settings")
struct SetupSettingsTests {
    @Test("Appearance changes apply now, persist, and preserve other owner choices")
    @MainActor
    func sharedAppearancePreferences() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        try choices.update { $0.pinned = ["/project"]; $0.prefs.splitDiff = true }
        let settings = SettingsSelection(choices: choices)
        settings.select(.appearance)
        settings.setTheme(.dark)
        settings.setTextSize(.large)
        settings.setDensity(.compact)
        settings.setSendKey(.commandReturn)
        let expected = Prefs(settingsPage: "appearance", splitDiff: true, theme: .dark,
                             textSize: .large, density: .compact, sendKey: .commandReturn)
        #expect(settings.prefs == expected)
        #expect(settings.error == nil)
        #expect(try choices.load().prefs == expected)
        #expect(try choices.load().pinned == ["/project"])
        #expect(SettingsSelection(choices: choices).prefs == expected)
        try choices.update { $0.prefs.splitDiff = false }
        settings.setTheme(.system)
        #expect(!settings.prefs.splitDiff)
        #expect(settings.prefs.sendKey == .commandReturn)
        let unavailable = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        unavailable.setTheme(.light)
        #expect(unavailable.prefs.theme == .light)
        #expect(unavailable.error?.contains("Could not save") == true)
    }

    @Test("Reading default preferences does not create choices.json")
    @MainActor
    func readDefaultDiffPreference() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        #expect(try choices.load().prefs.splitDiff == false)
        #expect(!FileManager.default.fileExists(atPath: folder.appendingPathComponent("choices.json").path))
    }

    @Test("The shared diff preference updates now, persists, and preserves other choices")
    @MainActor
    func sharedDiffPreference() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        try choices.update { $0.pinned = ["/project"] }
        let settings = SettingsSelection(choices: choices)
        settings.setSplitDiff(true)
        #expect(settings.prefs.splitDiff)
        #expect(try choices.load().prefs.splitDiff)
        #expect(SettingsSelection(choices: choices).prefs.splitDiff)
        settings.select(.appearance)
        #expect(settings.prefs.splitDiff)
        #expect(try choices.load().pinned == ["/project"])
        settings.setSplitDiff(false)
        #expect(!settings.prefs.splitDiff)
        #expect(try choices.load().prefs.settingsPage == "appearance")
        let unavailable = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        unavailable.setSplitDiff(true)
        #expect(unavailable.prefs.splitDiff)
        #expect(unavailable.error?.contains("Could not save") == true)
    }

    @Test("The guards path is global across builds and SWARM_HOME, with an explicit override")
    func guardPath() throws {
        #expect(try GuardRules.fileURL(environment: ["HOME": "/owner", "SWARM_HOME": "/branch"])
            .path == "/owner/.swarm/guards.json")
        #expect(try GuardRules.fileURL(environment: ["SWARM_GUARDS": "/fixtures/rules.json"])
            .path == "/fixtures/rules.json")
        #expect(throws: GuardListError.self) { try GuardRules.fileURL(environment: [:]) }
        #expect(throws: GuardListError.self) { try GuardRules.fileURL(environment: ["SWARM_GUARDS": ""]) }
    }

    @Test("Guard edits preserve argument boundaries and optional tool and timeout fields")
    func guardEdits() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("new-home/guards.json")
        var editor = GuardRulesEditor()
        editor.load(.success(GuardRules()))
        #expect(editor.canSave)
        editor.add()
        #expect(!editor.canSave)
        editor.drafts[0].name = "Policy"
        editor.drafts[0].command = ["/tools/path with spaces", "argument with spaces", "$(literal)"]
        editor.drafts[0].tools = ["Bash"]
        editor.drafts[0].timeout = "5"
        #expect(editor.canSave)
        try editor.save(to: file)
        let saved = try GuardRules.load(url: file).get().rules[0]
        #expect(saved.command == ["/tools/path with spaces", "argument with spaces", "$(literal)"])
        #expect(saved.tools == ["Bash"])
        #expect(saved.timeout == 5)
        editor.drafts[0].tools = nil
        editor.drafts[0].timeout = ""
        try editor.save(to: file)
        #expect(try GuardRules.load(url: file).get().rules[0].tools == nil)
        #expect(try GuardRules.load(url: file).get().rules[0].timeout == nil)
        for timeout in ["-1", "1.5", "letters", "18446744073709551616"] {
            editor.drafts[0].timeout = timeout
            #expect(!editor.canSave)
        }
        editor.drafts[0].timeout = "0"
        #expect(editor.canSave)
        editor.drafts[0].event = "PostToolUse"
        #expect(!editor.canSave)
        editor.drafts[0].event = "PreToolUse"
        editor.drafts[0].tools = []
        #expect(!editor.canSave)
        editor.delete(at: 0)
        #expect(editor.canSave)
        try editor.save(to: file)
        #expect(try GuardRules.load(url: file).get().rules.isEmpty)
    }

    @Test("A broken guard list disables Save until a valid replacement is loaded")
    func brokenGuardEditing() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("guards.json")
        for fixture in ["broken", #"{"rules":[{"name":"bad-event","event":"PostToolUse","command":["/bin/true"]}]}"#,
                        #"{"rules":[{"name":"no-tools","event":"PreToolUse","tools":[],"command":["/bin/true"]}]}"#] {
            try Data(fixture.utf8).write(to: file)
            var editor = GuardRulesEditor()
            editor.load(GuardRules.load(url: file))
            #expect(editor.loadError != nil)
            #expect(!editor.canSave)
            editor.add()
            #expect(editor.drafts.isEmpty)
            #expect(throws: GuardListError.self) { try editor.save(to: file) }
            #expect(try String(contentsOf: file, encoding: .utf8) == fixture)
            try GuardRules().save(to: file)
            editor.load(GuardRules.load(url: file))
            #expect(editor.loadError == nil)
            #expect(editor.canSave)
            try editor.save(to: file)
        }
    }

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
