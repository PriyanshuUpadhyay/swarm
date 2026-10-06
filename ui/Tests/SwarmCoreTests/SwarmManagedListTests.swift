import Foundation
import Testing
@testable import SwarmCore

@Suite("Managed changes")
struct SwarmManagedListTests {
    /// What `swarm managed list --json` printed after hooks setup with a rule list, with one trust
    /// key changed by the owner, one AGY group gone, and one found item.
    static let listing = #"""
        {"entries": [
          {"id":"a1","writer":"hooks.state","file":"/u/.codex/config.toml","kind":"toml_key","path":["hooks","state","/<session-flags>/config.toml:stop:1:0","trusted_hash"],"wrote":"sha256:1","before":null,"state":"present","found":null,"recorded":true,"at_s":1760000000,"with":null},
          {"id":"a2","writer":"hooks.state","file":"/u/.codex/config.toml","kind":"toml_key","path":["hooks","state","/<session-flags>/config.toml:interrupt:1:0","trusted_hash"],"wrote":"sha256:2","before":null,"state":"present","found":null,"recorded":true,"at_s":1760000000,"with":null},
          {"id":"g1","writer":"hooks.guard","file":"/u/.codex/hooks.json","kind":"json_array_item","path":["hooks","PreToolUse"],"wrote":{"hooks":[]},"before":null,"state":"present","found":null,"recorded":true,"at_s":1760000000,"with":null},
          {"id":"g2","writer":"hooks.guard","file":"/u/.codex/config.toml","kind":"toml_key","path":["hooks","state","/u/.codex/hooks.json:pre_tool_use:0:0","trusted_hash"],"wrote":"sha256:3","before":null,"state":"changed","found":"sha256:edited","recorded":true,"at_s":1760000000,"with":"g1"},
          {"id":"s1","writer":"hooks.state","file":"/u/.gemini/config/hooks.json","kind":"json_key","path":["swarm"],"wrote":{},"before":null,"state":"gone","found":null,"recorded":true,"at_s":1760000000,"with":null},
          {"id":"o1","writer":"hooks.guard","file":"/u/.claude/settings.json","kind":"json_array_item","path":["hooks","PreToolUse"],"wrote":{},"before":null,"state":"off","found":null,"recorded":true,"at_s":1760000000,"with":null},
          {"id":"f1","writer":"hooks.state","file":"/u/.codex-old/config.toml","kind":"toml_key","path":["hooks","state","k","trusted_hash"],"wrote":"sha256:4","before":null,"state":"present","found":null,"recorded":false,"at_s":null,"with":null}
        ]}
        """#

    @Test("Rows are one writer and one file, grouped by writer, with found items apart")
    func groupsAndRows() throws {
        let list = try SwarmManagedList.decode(Data(Self.listing.utf8))
        #expect(list.groups.map(\.name) == ["Agent hooks", "Guard", "Found, not recorded"])
        let hooks = list.groups[0].rows
        #expect(hooks.map(\.file) == ["/u/.codex/config.toml", "/u/.gemini/config/hooks.json"])
        #expect(hooks[0].ids == ["a1", "a2"])
        #expect(hooks[0].entry == "2 trust keys")
        #expect(hooks[0].state == .on)
        #expect(hooks[1].entry == #"group "swarm""#)
        #expect(hooks[1].state == .gone)
        let guardRows = list.groups[1].rows
        #expect(guardRows.map(\.file) == ["/u/.claude/settings.json", "/u/.codex/config.toml", "/u/.codex/hooks.json"])
        #expect(guardRows.map(\.entry) == ["PreToolUse group", "1 trust key", "PreToolUse group"])
        #expect(guardRows[0].state == .off)
        #expect(guardRows[1].state == .changed(found: #""sha256:edited""#, wrote: #""sha256:3""#))
        #expect(guardRows[2].state == .on)
        #expect(list.groups[2].rows.map(\.ids) == [["f1"]])
        #expect(SwarmManagedList.summary(list.groups) == "2 on · 1 changed by you · 1 gone · 1 off · 1 found")
        #expect(list.presentIDs == ["a1", "a2", "g1", "f1"])
        #expect(list.groups[1].presentIDs == ["g1"])
    }

    @Test("A group's spoken name counts its rows with the right plural")
    func spokenGroup() throws {
        let list = try SwarmManagedList.decode(Data(Self.listing.utf8))
        #expect(list.groups[0].spoken == "Agent hooks group, 2 rows")
        #expect(list.groups[2].spoken == "Found, not recorded group, 1 row")
    }

    @Test("A reload names each row whose state flipped, such as the rows an undo turned off")
    func flips() throws {
        let before = try SwarmManagedList.decode(Data(Self.listing.utf8))
        var after = before
        after.entries = before.entries.compactMap { entry in
            var entry = entry
            if ["a1", "a2"].contains(entry.id) { entry.state = "off" }
            return entry.id == "f1" ? nil : entry
        }
        #expect(SwarmManagedList.changes(from: before.groups, to: after.groups) == [
            "Agent hooks, /u/.codex/config.toml, Off",
            "Found, not recorded, /u/.codex-old/config.toml, Removed",
        ])
        #expect(SwarmManagedList.changes(from: before.groups, to: before.groups).isEmpty)
    }

    @Test("Only a hooks writer's items can be set up again")
    func hooksWriter() throws {
        let list = try SwarmManagedList.decode(Data(Self.listing.utf8))
        #expect(list.entries.allSatisfy { $0.isHooks })
        #expect(list.groups.flatMap(\.rows).allSatisfy { $0.isHooks })
        var trust = list.entries[0]
        trust.writer = "launch.trust"
        #expect(!trust.isHooks)
        #expect(!SwarmManagedList.Row(writer: "launch.trust", file: trust.file, entries: [trust]).isHooks)
    }

    @Test("An empty list has no groups and says so")
    func empty() throws {
        let list = try SwarmManagedList.decode(Data(#"{"entries":[]}"#.utf8))
        #expect(list.groups.isEmpty)
        #expect(SwarmManagedList.summary(list.groups) == "")
    }

    @Test("The bus lists, plans a revert, and reverts with the plan's digest")
    func busCalls() async throws {
        let calls = Calls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            await calls.add(arguments)
            let stdout = arguments.contains("list")
                ? Self.listing
                : arguments.contains("--plan") ? #"{"digest":"d2","files":[],"conflicts":[]}"# : ""
            return ShellResult(status: 0, stdout: stdout, stderr: "")
        }
        #expect(try await bus.managedList().entries.count == 7)
        #expect(try await bus.managedRevertPlan(ids: ["a1", "a2"]).digest == "d2")
        try await bus.revertManaged(ids: ["a1", "a2"], digest: "d2")
        #expect(await calls.arguments == [
            ["managed", "list", "--json"],
            ["managed", "revert", "a1", "a2", "--plan", "--json"],
            ["managed", "revert", "a1", "a2", "--digest", "d2"],
        ])
    }
}

private actor Calls {
    private(set) var arguments: [[String]] = []
    func add(_ arguments: [String]) { self.arguments.append(arguments) }
}
