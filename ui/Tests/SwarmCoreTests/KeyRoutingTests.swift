import Testing
@testable import SwarmCore

@Suite("Key routing")
struct KeyRoutingTests {
    @Test("Keys go only to their focused surface")
    func table() {
        #expect(KeyRouting.route(focus: .terminal, key: .escape) == .terminal)
        #expect(KeyRouting.route(focus: .terminal, key: .return) == .terminal)
        #expect(KeyRouting.route(focus: .terminal, key: .other) == .terminal)
        #expect(KeyRouting.route(focus: .composer, key: .escape) == .clearComposer)
        #expect(KeyRouting.route(focus: .composer, key: .return) == .sendComposer)
        #expect(KeyRouting.route(focus: .composer, key: .shiftReturn) == .insertNewline)
        #expect(KeyRouting.route(focus: .transcript, key: .escape) == .ignore)
        #expect(KeyRouting.route(focus: .transcript, key: .commandF) == .openFind)
        #expect(KeyRouting.route(focus: .transcript, key: .commandG) == .findNext)
        #expect(KeyRouting.route(focus: .transcript, key: .shiftCommandG) == .findPrevious)
        #expect(KeyRouting.route(focus: .terminal, key: .commandF) == .terminal)
        #expect(KeyRouting.route(focus: .sidebar, key: .escape) == .ignore)
        for focus in [FocusedSurface.terminal, .transcript, .composer, .sidebar] {
            #expect(KeyRouting.route(focus: focus, key: .commandN) == .openNewChat)
        }
    }

    @Test("Find matches visible text and wraps in both directions")
    func paneSearch() {
        let items = [
            PaneSearchItem(id: "one", text: "Alpha beta"),
            PaneSearchItem(id: "two", text: "BETA gamma"),
            PaneSearchItem(id: "three", text: "delta"),
        ]
        #expect(PaneSearch.matches(query: "beta", in: items) == ["one", "two"])
        #expect(PaneSearch.matches(query: "", in: items).isEmpty)
        #expect(PaneSearch.step(current: nil, count: 2, delta: 1) == 0)
        #expect(PaneSearch.step(current: 1, count: 2, delta: 1) == 0)
        #expect(PaneSearch.step(current: 0, count: 2, delta: -1) == 1)
        #expect(PaneSearch.step(current: 0, count: 0, delta: 1) == nil)
    }

    @Test("Find selection stays on its row when live matches change")
    func paneSearchReconcile() {
        #expect(PaneSearch.reconcile(
            current: 1,
            previousMatches: ["one", "two", "three"],
            newMatches: ["two", "four"]
        ) == 0)
        #expect(PaneSearch.reconcile(
            current: 2,
            previousMatches: ["one", "two", "three"],
            newMatches: ["one"]
        ) == 0)
        #expect(PaneSearch.reconcile(
            current: nil,
            previousMatches: [],
            newMatches: ["one"]
        ) == 0)
        #expect(PaneSearch.reconcile(
            current: 0,
            previousMatches: ["one"],
            newMatches: []
        ) == nil)
    }

    @Test("Composer sends trimmed text only")
    func outgoing() {
        #expect(Composer.outgoing("  Hello, chair  \n") == "Hello, chair")
        #expect(Composer.outgoing(" \t\n") == nil)
    }

    @Test("Composer keeps new text and blocks a second send while the first is running")
    func sendState() {
        var state = ComposerSendState()
        #expect(state.begin("  First message  ") == "First message")
        #expect(state.isSending)
        #expect(state.begin("First message") == nil)
        #expect(state.finish(currentDraft: "Second message", succeeded: true) == "Second message")
        #expect(!state.isSending)

        #expect(state.begin("Third message") == "Third message")
        #expect(state.finish(currentDraft: "Third message", succeeded: true).isEmpty)
    }
}
