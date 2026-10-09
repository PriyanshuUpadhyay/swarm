import Foundation
import Testing
@testable import SwarmCore

@Suite("Skills setup")
struct SkillsSetupTests {
    static let skillsPlan = #"""
        {"digest":"skills-digest","files":[
          {"group":"skills","path":"/owner/.agents/skills/flow","diff":"--- /owner/.agents/skills/flow\n+++ /owner/.agents/skills/flow\n+link -> /build/.swarm/skills/kit/skills/flow\n"}],
         "conflicts":[],"skipped":[]}
        """#
    static let linkRows = #"""
        {"entries":[
          {"id":"link","writer":"skills","file":"/owner/.agents/skills/flow","kind":"symlink","path":[],"wrote":"/build/.swarm/skills/kit/skills/flow","state":"off","recorded":true},
          {"id":"future","writer":"future.writer","file":"/owner/future","kind":"future_kind","path":[],"wrote":"value","state":"future_state","recorded":true}]}
        """#

    @Test("Skills status is required and its decline flag affects only Skills")
    func statusAndDecline() throws {
        let status = try JSONDecoder().decode(SwarmSetupStatus.self, from: Data(
            #"{"hooks":true,"trust":true,"herdr":true,"skills":false}"#.utf8
        ))
        #expect(!status.skills)
        #expect(status.needsSheet(declined: [.hooks, .trust]))
        #expect(!status.needsSheet(declined: [.skills]))
        var hooksPending = status
        hooksPending.hooks = false
        #expect(hooksPending.needsSheet(declined: [.trust, .skills]))
        #expect(!hooksPending.needsSheet(declined: [.hooks, .trust, .skills]))
        for fixture in [#"{"hooks":true,"trust":true,"herdr":true}"#,
                        #"{"hooks":true,"trust":true,"herdr":true,"skills":null}"#] {
            #expect(throws: DecodingError.self) {
                try JSONDecoder().decode(SwarmSetupStatus.self, from: Data(fixture.utf8))
            }
        }
    }

    @Test("Skills plans have a title, checkbox, and independent Not now choice")
    func choiceAndTitle() throws {
        let plan = try JSONDecoder().decode(SwarmHooksPlan.self, from: Data(Self.skillsPlan.utf8))
        #expect(plan.groupIDs == ["skills"])
        #expect(SwarmHooksPlan.groupTitle("skills") == "Skills")
        var choice = SwarmSetupChoice(groups: ["hooks", "trust", "herdr"])
        choice.take(plan)
        #expect(choice.groups == ["hooks", "trust", "herdr", "skills"])
        choice.unchecked = ["skills"]
        #expect(choice.notNowDeclines == ["skills"])
        #expect(choice.arguments == ["--only", "hooks,trust,herdr"])
        #expect(SwarmSetupChoice.trustOnly(standing: false).checked == ["trust"])
    }

    @Test("Skill rows show their link target and unknown wire names stay visible")
    func linksAndUnknownNames() throws {
        let list = try SwarmManagedList.decode(Data(Self.linkRows.utf8))
        #expect(list.groups.map(\.name) == ["Skills", "future.writer"])
        let row = list.groups[0].rows[0]
        #expect(row.file == "/owner/.agents/skills/flow")
        #expect(row.entry == "Link to /build/.swarm/skills/kit/skills/flow")
        #expect(row.state == .off)
        #expect(row.restoreGroup == .skills)
        let unknown = list.groups[1].rows[0]
        #expect(unknown.entry.contains("future_kind"))
        #expect(unknown.state == .unknown(state: "future_state"))
        #expect(unknown.restoreGroup == nil)
    }


    @Test("Skill links keep managed states and a future kind is visible without a restore route")
    func linkStates() throws {
        let original = try SwarmManagedList.decode(Data(Self.linkRows.utf8)).entries[0]
        for (state, expected) in [("present", SwarmManagedList.RowState.on), ("gone", .gone),
                                  ("off", .off), ("changed", .changed(found: "", wrote: original.wrote.compactJSON)),
                                  ("unreadable", .unreadable(reason: "")), ("future_state", .unknown(state: "future_state"))] {
            var entry = original
            entry.state = state
            let row = SwarmManagedList(entries: [entry]).groups[0].rows[0]
            #expect(row.state == expected)
        }
        var future = original
        future.kind = "future_link"
        let row = SwarmManagedList(entries: [future]).groups[0].rows[0]
        #expect(row.entry == "future_link")
        #expect(row.restoreGroup == nil)
    }

    @Test("Missing-copy Skills conflicts keep the refresh fix and permit clearing just Skills")
    func missingCopyConflict() throws {
        let fixture = #"""
            {"digest":"missing-copy","files":[],"conflicts":[
              {"kind":"unreadable","group":"skills","file":"/build/.swarm/skills","entry":"the default skills copy","found":"missing","wanted":"a complete default skills copy","fix":"run `swarm skills refresh`"}],"skipped":[]}
            """#
        let plan = try JSONDecoder().decode(SwarmHooksPlan.self, from: Data(fixture.utf8))
        #expect(plan.groupIDs == ["skills"])
        #expect(plan.conflicts[0].fix == "run `swarm skills refresh`")
        #expect(!plan.canApply)
        var choice = SwarmSetupChoice(groups: ["hooks", "trust", "skills"])
        choice.take(plan)
        choice.unchecked = ["skills"]
        #expect(choice.checked == ["hooks", "trust"])
        #expect(choice.notNowDeclines == ["skills"])
    }

    @Test("Restoring Skills plans and applies only Skills after the choice takes its plan")
    func restoreRouting() async throws {
        actor Calls {
            var arguments: [[String]] = []
            func add(_ value: [String]) { arguments.append(value) }
        }
        let calls = Calls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/fixture", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            await calls.add(arguments)
            return ShellResult(status: 0, stdout: Self.skillsPlan, stderr: "")
        }
        var choice = SwarmSetupChoice.skillsOnly()
        let plan = try await bus.setupPlan(choice)
        choice.take(plan)
        #expect(choice.checked == ["skills"])
        try await bus.setUp(digest: plan.digest, choice: choice)
        #expect(await calls.arguments == [
            ["setup", "--plan", "--json", "--only", "skills"],
            ["setup", "--digest", "skills-digest", "--only", "skills"],
        ])
    }
}
