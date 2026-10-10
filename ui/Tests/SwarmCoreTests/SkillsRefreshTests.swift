import Foundation
import Testing
@testable import SwarmCore

@Suite("Skills refresh")
@MainActor
struct SkillsRefreshTests {
    actor Calls {
        var values: [[String]] = []
        var fail = false
        func setFailure(_ value: Bool) { fail = value }
        func call(_ arguments: [String]) -> ShellResult {
            values.append(arguments)
            return ShellResult(status: fail ? 1 : 0, stdout: "", stderr: fail ? "missing bundle source" : "")
        }
        func plan(_ arguments: [String]) -> ShellResult {
            values.append(arguments)
            return ShellResult(status: 0, stdout: #"{"digest":"plan","files":[],"conflicts":[]}"#, stderr: "")
        }
    }

    @Test("The CLI bus calls refresh without flags")
    func busCommand() async throws {
        let calls = Calls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, timeout in
            #expect(timeout == .seconds(120))
            return await calls.call(arguments)
        }
        try await bus.refreshSkills()
        #expect(await calls.values == [["skills", "refresh"]])
    }

    @Test("Startup waits for its prerequisite, and shared waits make one refresh")
    func startupSuccess() async {
        let calls = Calls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in await calls.call(arguments)
        }
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.beginSkillsRefresh(after: {
            #expect(await calls.values.isEmpty)
            return true
        }, refresh: { try await bus.refreshSkills() })
        #expect(await selection.waitForSkillsRefresh())
        #expect(await selection.waitForSkillsRefresh())
        #expect(selection.skillsReady && !selection.skillsRefreshing)
        #expect(selection.skillsRefreshError == nil && selection.appError == nil)
        #expect(await calls.values == [["skills", "refresh"]])
    }

    @Test("A setup reader before startup registration waits for the refresh")
    func readerBeforeStartup() async throws {
        let calls = Calls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            if arguments.first == "setup" { return await calls.plan(arguments) }
            return await calls.call(arguments)
        }
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        var started = false
        var result: Bool?
        let reader = Task {
            started = true
            let choice = try await selection.setupChoice(.init())
            _ = try await bus.setupPlan(choice)
            result = true
        }
        while !started { await Task.yield() }
        #expect(result == nil)
        #expect(await calls.values.isEmpty)
        selection.beginSkillsRefresh(after: { true }, refresh: { try await bus.refreshSkills() })
        try await reader.value
        #expect(result == true)
        #expect(await calls.values == [["skills", "refresh"], ["setup", "--plan", "--json"]])
    }

    @Test("A failed refresh stays in Setup and plans only the other groups until Retry succeeds")
    func planAfterFailure() async throws {
        let calls = Calls()
        await calls.setFailure(true)
        let bus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            if arguments.first == "setup" { return await calls.plan(arguments) }
            return await calls.call(arguments)
        }
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        selection.beginSkillsRefresh(after: { true }, refresh: { try await bus.refreshSkills() })
        let choice = try await selection.setupChoice(.init(standing: false))
        let plan = try await bus.setupPlan(choice)
        #expect(plan.conflicts.isEmpty)
        #expect(choice.checked == ["hooks", "trust", "herdr"])
        #expect(selection.pageError?.contains("Could not refresh skills.") == true)
        #expect(selection.pageError?.contains("missing bundle source") == true)
        #expect(selection.appError == nil)
        #expect(await calls.values == [["skills", "refresh"],
                                      ["setup", "--plan", "--json", "--only", "hooks,trust,herdr", "--consent", "ask"]])
        await #expect(throws: SwarmProfileError.self) {
            _ = try await selection.setupChoice(.skillsOnly())
        }
        let clearedTrust = SwarmSetupChoice(groups: SetupGroup.allCases.map(\.rawValue), unchecked: ["trust"])
        #expect(try await selection.setupChoice(clearedTrust).checked == ["hooks", "herdr"])
        await calls.setFailure(false)
        #expect(await selection.retrySkillsRefresh())
        #expect(try await selection.setupChoice(.init()).arguments.isEmpty)
        #expect(selection.pageError == nil)
    }

    @Test("A refresh failure and Retry use only the Setup error source")
    func failureAndRetry() async {
        let calls = Calls()
        await calls.setFailure(true)
        let bus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in await calls.call(arguments)
        }
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        selection.setPageError("Guards failed.", on: .setup)
        selection.setAppError("Unrelated app error.")
        let appRevision = selection.appErrorRevision
        selection.beginSkillsRefresh(after: { true }, refresh: { try await bus.refreshSkills() })
        #expect(await selection.waitForSkillsRefresh() == false)
        #expect(selection.skillsRefreshError?.contains("missing bundle source") == true)
        #expect(selection.pageError?.contains("Guards failed.") == true)
        #expect(selection.pageError?.contains("missing bundle source") == true)
        #expect(selection.appError == "Unrelated app error." && selection.appErrorRevision == appRevision)
        selection.select(.profiles)
        #expect(selection.pageError == nil)
        selection.select(.setup)
        #expect(selection.pageError?.contains("missing bundle source") == true)
        selection.setPageError("Guards failed again.", on: .setup)
        await calls.setFailure(false)
        #expect(await selection.retrySkillsRefresh())
        #expect(selection.skillsRefreshError == nil && selection.skillsReady)
        #expect(selection.pageError == "Guards failed again.")
        #expect(selection.appError == "Unrelated app error." && selection.appErrorRevision == appRevision)
        #expect(await calls.values == [["skills", "refresh"], ["skills", "refresh"]])
    }

    @Test("A failed app prerequisite never runs refresh or Retry")
    func failedPrerequisite() async {
        let calls = Calls()
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.beginSkillsRefresh(after: { false }, refresh: { _ = await calls.call(["refresh"]) })
        #expect(await selection.waitForSkillsRefresh() == false)
        #expect(await selection.retrySkillsRefresh() == false)
        #expect(await calls.values.isEmpty)
        #expect(selection.skillsRefreshError == nil && !selection.skillsReady)
    }
}
