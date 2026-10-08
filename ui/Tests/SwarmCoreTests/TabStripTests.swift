import Foundation
import Testing
@testable import SwarmCore

@Suite("Stored chat tabs")
struct TabStripTests {
    @Test("Opening inserts after the current tab and never moves an open tab")
    func opening() {
        let strip = TabStrip(open: ["design", "review"])
        #expect(strip.opening("fix", after: "design").open == ["design", "fix", "review"])
        #expect(strip.opening("design", after: "review") == strip)
        #expect(strip.opening("fix", after: "missing").open == ["design", "review", "fix"])
        #expect(TabStrip().opening("design", after: nil).open == ["design"])
    }

    @Test("Closing and pruning keep order and remove empty groups")
    func closingAndPruning() {
        let strip = TabStrip(open: ["design", "review", "fix", "design"], groups: [
            TabGroup(id: "planning", name: "Planning", color: .blue, members: ["review", "design"]),
            TabGroup(id: "repair", name: "Repair", color: .red, members: ["fix"]),
        ])
        let closed = strip.closing("fix")
        #expect(closed.open == ["design", "review"])
        #expect(closed.groups.count == 1)
        #expect(closed.groups.first?.members == ["design", "review"])
        #expect(strip.pruned(to: ["fix"]).open == ["fix"])
        #expect(strip.pruned(to: []).groups.isEmpty)
        #expect(closed.closing("absent") == closed)
    }

    @Test("Moving works in both directions and ignores unknown keys and invalid indexes")
    func moving() {
        let strip = TabStrip(open: ["design", "review", "fix"])
        #expect(strip.moving("design", to: 2).open == ["review", "fix", "design"])
        #expect(strip.moving("fix", to: 0).open == ["fix", "design", "review"])
        #expect(strip.moving("review", to: 1) == strip)
        #expect(strip.moving("missing", to: 0) == strip)
        #expect(strip.moving("design", to: -1) == strip)
        #expect(strip.moving("design", to: 3) == strip)
    }

    @Test("Tabs round-trip in owner choices; absent tabs start empty and unknown colors are grey")
    func persistence() throws {
        var choices = OwnerChoices()
        choices.tabs = ["/repo": TabStrip(open: ["design", "review"], groups: [
            TabGroup(id: "planning", name: "Planning", color: .blue, members: ["design"], folded: true),
        ])]
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: JSONEncoder().encode(choices)) == choices)
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: Data("{}".utf8)).tabs.isEmpty)
        #expect(try JSONDecoder().decode(TabGroupColor.self, from: Data("\"future\"".utf8)) == .grey)
    }

    @Test("Absent strip fields start empty and an absent folded flag is false")
    func absentStripFields() throws {
        #expect(try JSONDecoder().decode(TabStrip.self, from: Data(#"{"open":["design"]}"#.utf8)) == TabStrip(open: ["design"]))
        #expect(try JSONDecoder().decode(TabStrip.self, from: Data("{}".utf8)) == TabStrip())
        #expect(try JSONDecoder().decode(TabStrip.self, from: Data(#"{"groups":[]}"#.utf8)) == TabStrip())
        let group = try JSONDecoder().decode(TabGroup.self, from: Data(
            #"{"id":"planning","name":"Planning","color":"blue","members":["design"]}"#.utf8
        ))
        #expect(!group.folded)
    }

    @Test("A stale workspace save keeps another workspace's tabs and project removal prunes only its tabs")
    func workspaceChanges() {
        var previous = OwnerChoices()
        previous.tabs = ["/repo/main": TabStrip(open: ["design"]), "/other": TabStrip(open: ["notes"])]
        var changed = previous
        changed.tabs["/repo/main"] = TabStrip(open: ["review", "design"])
        var current = previous
        current.tabs["/other"] = TabStrip(open: ["notes", "draft"])
        current.applyWorkspaceChanges(from: previous, to: changed)
        #expect(current.tabs["/repo/main"] == changed.tabs["/repo/main"])
        #expect(current.tabs["/other"]?.open == ["notes", "draft"])
        current.tabs["/repo#removed"] = TabStrip(open: ["offline"])
        current.removeProject("/repo", workspacePaths: ["/repo/main"])
        #expect(Set(current.tabs.keys) == ["/other"])
        var removed = changed
        removed.tabs.removeValue(forKey: "/other")
        changed.applyWorkspaceChanges(from: previous, to: removed)
        #expect(changed.tabs["/other"] == nil)
    }

    @Test("First sight seeds live chats in tree order only once and later drops missing keys")
    func firstSight() throws {
        let entries = WorkspaceEntry.list(in: tree())
        let entry = try #require(entries.first)
        let expected = entry.project.chats.filter { $0.session.isRunning != false }.map { ChatTitle.key($0.session) }
        var navigation = WorkspaceNavigation()
        navigation.recordTabFirstSight(entries)
        #expect(navigation.tabs[entry.id]?.open == expected)
        navigation.tabs[entry.id] = TabStrip()
        navigation.recordTabFirstSight(entries)
        #expect(navigation.tabs[entry.id]?.open == [])
        navigation.tabs[entry.id] = TabStrip(open: ["missing", "review", "design"])
        navigation.recordTabFirstSight(entries)
        #expect(navigation.tabs[entry.id]?.open == ["review", "design"])
    }

    @Test("History keeps the latest twenty distinct selections per workspace in view state")
    func history() throws {
        var navigation = WorkspaceNavigation()
        for number in 0..<25 { navigation.recordTabSelection("chat-\(number)", in: "/repo") }
        navigation.recordTabSelection("chat-5", in: "/repo")
        navigation.recordTabSelection("notes", in: "/other")
        let expected = (6..<25).map { "chat-\($0)" } + ["chat-5"]
        #expect(navigation.tabHistory["/repo"] == expected)
        #expect(navigation.tabHistory["/other"] == ["notes"])
        navigation.tabs["/repo"] = TabStrip(open: ["chat-5"])
        let data = try JSONEncoder().encode(navigation)
        let restored = try JSONDecoder().decode(WorkspaceNavigation.self, from: data)
        #expect(restored.tabHistory == navigation.tabHistory)
        #expect(restored.tabs.isEmpty)
        #expect(try JSONDecoder().decode(WorkspaceNavigation.self, from: Data("{}".utf8)).tabHistory.isEmpty)
    }

    @Test("After a close the last open history entry wins; empty history falls back to strip order")
    func selection() {
        #expect(TabStrip.selectionAfterClose(history: ["design", "review", "fix"], open: ["review", "design"]) == "review")
        #expect(TabStrip.selectionAfterClose(history: ["missing"], open: ["design", "review"]) == "design")
        #expect(TabStrip.selectionAfterClose(history: ["design"], open: []) == nil)
    }

    private func tree() -> SessionsTree {
        let chats = [("design", true), ("review", true), ("ended", false)].map { key, running in
            SwarmProjectSession(
                sessions: [SwarmSession(id: .init(key), talkMode: "lane", adapter: nil, cwd: "/repo",
                                      createdAt: 1, chairProvider: "codex", chairLog: nil, agents: 1,
                                      messages: 0, lastMessageAt: nil)], title: key,
                isRunning: running, provider: "codex"
            )
        }
        return SessionsTree(projects: [ProjectNode(
            id: .folder("/repo"), path: "/repo", launchDirectory: "/repo",
            workspaces: [WorkspaceNode(path: "/repo", name: "repo", sessions: chats)]
        )])
    }
}
