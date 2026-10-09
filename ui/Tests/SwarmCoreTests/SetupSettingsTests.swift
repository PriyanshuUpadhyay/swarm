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
        #expect(settings.storeError == nil)
        #expect(try choices.load().prefs == expected)
        #expect(try choices.load().pinned == ["/project"])
        #expect(SettingsSelection(choices: choices).prefs == expected)
        try choices.update { $0.prefs.splitDiff = false }
        settings.setTheme(.system)
        #expect(!settings.prefs.splitDiff)
        #expect(settings.prefs.sendKey == .commandReturn)
        let unavailable = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        unavailable.setTheme(.light)
        #expect(unavailable.prefs.theme == .system)
        #expect(unavailable.storeError?.contains("Could not save") == true)
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
        #expect(!unavailable.prefs.splitDiff)
        #expect(unavailable.storeError?.contains("Could not save") == true)
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
        editor.drafts[0].command = ["/tools/path with spaces", "argument with spaces", "$(literal)"].map { GuardField(text: $0) }
        editor.drafts[0].allTools = false
        editor.drafts[0].tools = [GuardField(text: "Bash")]
        editor.drafts[0].timeout = "5"
        #expect(editor.canSave)
        try editor.save(to: file)
        let saved = try GuardRules.load(url: file).get().rules[0]
        #expect(saved.command == ["/tools/path with spaces", "argument with spaces", "$(literal)"])
        #expect(saved.tools == ["Bash"])
        #expect(saved.timeout == 5)
        editor.drafts[0].allTools = true
        editor.drafts[0].timeout = ""
        try editor.save(to: file)
        #expect(try GuardRules.load(url: file).get().rules[0].tools == nil)
        #expect(try GuardRules.load(url: file).get().rules[0].timeout == nil)
        for timeout in ["-1", "1.5", "letters", "18446744073709551616"] {
            editor.drafts[0].timeout = timeout
            #expect(!editor.canSave)
            #expect(editor.canEdit)
        }
        editor.drafts[0].timeout = "0"
        #expect(editor.canSave)
        editor.drafts[0].event = "PostToolUse"
        #expect(!editor.canSave)
        editor.drafts[0].event = "PreToolUse"
        editor.drafts[0].allTools = false
        editor.drafts[0].tools = []
        #expect(!editor.canSave)
        editor.delete(id: editor.drafts[0].id)
        #expect(editor.canSave)
        try editor.save(to: file)
        #expect(try GuardRules.load(url: file).get().rules.isEmpty)
    }

    @Test("Guard fields convert optional tools and preserve row identities after removal")
    func guardFieldIdentity() throws {
        let rule = GuardRules.Rule(name: "Policy", tools: ["Bash", "Bash"],
                                   command: ["/tools/path with spaces", "same", "same"], timeout: 5)
        var draft = GuardRuleFields(rule: rule)
        let draftID = draft.id
        #expect(try draft.rule() == rule)
        #expect(Set(draft.tools.map(\.id)).count == 2)
        #expect(Set(draft.command.map(\.id)).count == 3)
        let remainingToolID = draft.tools[1].id
        let remainingArgumentID = draft.command[2].id
        draft.tools.removeFirst()
        draft.command.remove(at: 1)
        #expect(draft.id == draftID)
        #expect(draft.tools.first?.id == remainingToolID)
        #expect(draft.command.last?.id == remainingArgumentID)
        #expect(try draft.rule().command == ["/tools/path with spaces", "same"])
        #expect(try draft.rule().tools == ["Bash"])
        draft.allTools = true
        #expect(try draft.rule().tools == nil)
        draft.allTools = false
        #expect(try draft.rule().tools == ["Bash"])
        let allTools = GuardRuleFields(rule: .init(name: "All", command: ["/bin/true"]))
        #expect(allTools.allTools)
        #expect(try allTools.rule().tools == nil)
        let noTools = GuardRuleFields(rule: .init(name: "Empty", tools: [], command: ["/bin/true"]))
        #expect(!noTools.allTools)
        #expect(try noTools.rule().tools == [])
        var editor = GuardRulesEditor()
        editor.load(.success(.init(rules: [rule, .init(name: "Next", command: ["/bin/true"])])))
        let remainingDraftID = editor.drafts[1].id
        editor.delete(id: editor.drafts[0].id)
        #expect(editor.drafts.map(\.id) == [remainingDraftID])
        editor.delete(id: UUID())
        #expect(editor.drafts.map(\.id) == [remainingDraftID])
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
            #expect(editor.error?.contains("Every tool call is blocked until it is fixed.") == true)
            #expect(!editor.canSave)
            #expect(!editor.canEdit)
            editor.add()
            #expect(editor.drafts.isEmpty)
            #expect(throws: GuardListError.self) { try editor.save(to: file) }
            #expect(try String(contentsOf: file, encoding: .utf8) == fixture)
            try GuardRules().save(to: file)
            editor.load(GuardRules.load(url: file))
            #expect(editor.loadError == nil)
            #expect(editor.canSave)
            #expect(editor.canEdit)
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

    @Test("Saving a linked guard list updates dotfiles and keeps the link")
    func guardSymlinkSave() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let target = folder.appendingPathComponent("dotfiles/guards.json")
        try GuardRules().save(to: target)
        let link = folder.appendingPathComponent("guards.json")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)
        let updated = GuardRules(rules: [.init(name: "policy", command: ["/bin/true"])])
        try updated.save(to: link)
        #expect(try GuardRules.load(url: target).get() == updated)
        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: link.path) == target.path)
    }

    @Test("Saving a dangling guard link creates its target and keeps the link", arguments: ["absolute", "relative"])
    func danglingGuardSymlinkSave(linkKind: String) throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let target = folder.appendingPathComponent("dotfiles/guards.json")
        let link = folder.appendingPathComponent("guards.json")
        let destination = linkKind == "absolute" ? target.path : "dotfiles/guards.json"
        try FileManager.default.createSymbolicLink(atPath: link.path, withDestinationPath: destination)
        let updated = GuardRules(rules: [.init(name: "policy", command: ["/bin/true"])])
        try updated.save(to: link)
        #expect(try GuardRules.load(url: target).get() == updated)
        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: link.path) == destination)
    }

    @Test("Saving through a linked parent updates the file that load reads", arguments: [true, false])
    func linkedParentGuardSave(targetExists: Bool) throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let parent = folder.appendingPathComponent("dotfiles/swarm")
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        let homeLink = folder.appendingPathComponent(".swarm")
        try FileManager.default.createSymbolicLink(at: homeLink, withDestinationURL: parent)
        let target = folder.appendingPathComponent("dotfiles/shared/guards.json")
        if targetExists { try GuardRules().save(to: target) }
        let guardLink = parent.appendingPathComponent("guards.json")
        try FileManager.default.createSymbolicLink(atPath: guardLink.path, withDestinationPath: "../shared/guards.json")
        let loadedPath = homeLink.appendingPathComponent("guards.json")
        let updated = GuardRules(rules: [.init(name: "policy", command: ["/bin/true"])])
        try updated.save(to: loadedPath)
        #expect(try GuardRules.load(url: loadedPath).get() == updated)
        #expect(try GuardRules.load(url: target).get() == updated)
        #expect(!FileManager.default.fileExists(atPath: folder.appendingPathComponent("shared").path))
        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: guardLink.path) == "../shared/guards.json")
    }

    @Test("Saving follows a linked component before a target's parent traversal", arguments: ["relative", "absolute"])
    func linkedGuardTargetSave(linkKind: String) throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let config = folder.appendingPathComponent("config")
        let dotfiles = folder.appendingPathComponent("dotfiles/swarm")
        try FileManager.default.createDirectory(at: config, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: dotfiles, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: config.appendingPathComponent("linked"), withDestinationURL: dotfiles)
        let target = folder.appendingPathComponent("dotfiles/guards.json")
        try GuardRules().save(to: target)
        let link = config.appendingPathComponent("guards.json")
        let destination = linkKind == "absolute" ? config.path + "/linked/../guards.json" : "linked/../guards.json"
        try FileManager.default.createSymbolicLink(atPath: link.path, withDestinationPath: destination)
        let updated = GuardRules(rules: [.init(name: "policy", command: ["/bin/true"])])
        try updated.save(to: link)
        #expect(try GuardRules.load(url: link).get() == updated)
        #expect(try GuardRules.load(url: target).get() == updated)
        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: link.path) == destination)
    }

    @Test("A guard link loop fails without replacing either link")
    func guardSymlinkLoop() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let first = folder.appendingPathComponent("guards.json")
        let second = folder.appendingPathComponent("policy.json")
        try FileManager.default.createSymbolicLink(atPath: first.path, withDestinationPath: "policy.json")
        try FileManager.default.createSymbolicLink(atPath: second.path, withDestinationPath: "guards.json")
        #expect(throws: GuardListError.self) { try GuardRules().save(to: first) }
        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: first.path) == "policy.json")
        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: second.path) == "guards.json")
    }

    @Test("Inline guard errors belong to their draft and exclude file load errors")
    func perDraftGuardErrors() {
        var editor = GuardRulesEditor()
        editor.load(.failure(.init(reason: "Missing", isMissing: true)))
        #expect(editor.drafts.isEmpty)
        editor.add()
        #expect(editor.drafts[0].error == "Rule 'New guard' needs a command")
        editor.drafts[0].command[0].text = "/bin/true"
        #expect(editor.drafts[0].error == nil)
        editor.drafts[0].event = "OtherEvent"
        #expect(editor.drafts[0].error?.contains("needs event PreToolUse") == true)
        editor.drafts[0].event = "PreToolUse"
        editor.drafts[0].timeout = "invalid"
        #expect(editor.drafts[0].error?.contains("needs a whole timeout") == true)
        editor.drafts[0].timeout = ""
        editor.add()
        #expect(editor.drafts[0].error == nil)
        #expect(editor.drafts[1].error == "Rule 'New guard' needs a command")
        editor.delete(id: editor.drafts[1].id)
        #expect(editor.error == nil)
        editor.load(.failure(.init(reason: "Invalid JSON")))
        #expect(editor.drafts.isEmpty && !editor.canEdit)
        #expect(editor.error?.contains("could not be read") == true)
    }

    @Test("The banner includes each invalid draft so each changed line can be announced")
    func joinedDraftValidation() {
        var editor = GuardRulesEditor()
        editor.add()
        editor.drafts[0].name = "First rule"
        editor.add()
        editor.drafts[1].name = "Second rule"
        #expect(editor.error == "Rule 'First rule' needs a command. Rule 'Second rule' needs a command")
        editor.drafts[1].timeout = "invalid"
        #expect(editor.error == "Rule 'First rule' needs a command. Rule 'Second rule' needs a whole timeout in seconds, or an empty field")
        editor.drafts[0].command[0].text = "/bin/true"
        #expect(editor.error == editor.drafts[1].error)
    }

    @Test("A failed guard write reaches the editor error and clears after a good save")
    func guardSaveFailureIsInEditor() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let blocked = folder.appendingPathComponent("blocked-folder")
        try Data("occupied".utf8).write(to: blocked)
        var editor = GuardRulesEditor()
        editor.load(.failure(.init(reason: "Missing", isMissing: true)))
        #expect(throws: (any Error).self) { try editor.save(to: blocked.appendingPathComponent("guards.json")) }
        #expect(editor.error != nil)
        #expect(editor.error?.contains("press Save to create an empty list") == false)
        let valid = folder.appendingPathComponent("guards.json")
        try editor.save(to: valid)
        #expect(editor.error == nil)
        editor.recordSaveFailure(GuardListError(reason: "SWARM_GUARDS is set but empty"))
        #expect(editor.error == "SWARM_GUARDS is set but empty")
        editor.load(.success(GuardRules()))
        #expect(editor.error == nil)
    }

    @Test("A missing guard file shows draft validation when Save cannot run")
    func missingGuardDraftValidation() {
        var editor = GuardRulesEditor()
        editor.load(.failure(.init(reason: "guards.json is missing", isMissing: true)))
        #expect(editor.canEdit)
        editor.add()
        #expect(!editor.canSave)
        #expect(editor.error == "Rule 'New guard' needs a command")
        editor.drafts[0].command[0].text = "/bin/true"
        #expect(editor.canSave)
        #expect(editor.error == nil)
        editor.delete(id: editor.drafts[0].id)
        #expect(editor.error?.contains("press Save to create an empty list") == true)
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

    @Test("Missing guard files report blocked calls and broken files retain a reason")
    func guardFailures() throws {
        let folder = try temporaryFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("guards.json")
        guard case .failure(let missing) = GuardRules.load(url: file) else {
            Issue.record("Expected a missing-file failure"); return
        }
        #expect(missing.reason == "guards.json is missing")
        #expect(missing.isMissing)
        var editor = GuardRulesEditor()
        editor.load(.failure(missing))
        #expect(editor.drafts.isEmpty)
        #expect(editor.canSave)
        #expect(editor.error == "guards.json is missing. Every tool call is blocked until it exists; press Save to create an empty list.")
        try editor.save(to: file)
        #expect(editor.error == nil)
        #expect(editor.loadError == nil)
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
