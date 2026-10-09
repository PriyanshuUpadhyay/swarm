import Testing
@testable import SwarmCore

@Suite("Key routing")
struct KeyRoutingTests {
    @Test("Choice seven opens Skills with Command-Option-7")
    func skillsChoice() {
        #expect(AppKey.action(for: KeyChord("7", [.option, .command])) == .sidebarView(7))
    }

    @Test("Each app key has one chord")
    func table() {
        let appKeys: [(KeyChord, AppKey)] = [
            (KeyChord("n", .command), .newWorkspace),
            (KeyChord("t", .command), .newChat),
            (KeyChord("t", [.shift, .command]), .recentlyClosed),
            (KeyChord("n", [.command, .shift]), .newProject),
            (KeyChord(.down, [.control, .command]), .nextWorkspace),
            (KeyChord(.up, [.control, .command]), .previousWorkspace),
            (KeyChord("1", .command), .selectTab(1)),
            (KeyChord("9", .command), .lastTab),
            (KeyChord("w", .command), .closeTab),
            (KeyChord("w", [.shift, .command]), .closeWindow),
            (KeyChord(.tab, .control), .previousRecentChat),
            (KeyChord(.tab, [.control, .shift]), .nextRecentChat),
            (KeyChord("]", [.command, .shift]), .nextTab),
            (KeyChord("[", [.command, .shift]), .previousTab),
            (KeyChord(.left, [.option, .command]), .moveFocus(.left)),
            (KeyChord(.right, [.option, .command]), .moveFocus(.right)),
            (KeyChord(.up, [.option, .command]), .moveFocus(.up)),
            (KeyChord(.down, [.option, .command]), .moveFocus(.down)),
            (KeyChord(.returnKey, [.shift, .command]), .zoom),
            (KeyChord("l", .command), .focusComposer),
            (KeyChord("b", .command), .toggleSidebar),
            (KeyChord("b", [.command, .shift]), .moveSidebar),
            (KeyChord("1", [.option, .command]), .sidebarView(1)),
            (KeyChord("5", [.option, .command]), .sidebarView(5)),
            (KeyChord("6", [.option, .command]), .sidebarView(6)),
            (KeyChord("i", [.option, .command]), .showChanges),
            (KeyChord("k", .command), .search),
            (KeyChord("f", .command), .find),
            (KeyChord("g", .command), .findNext),
            (KeyChord("g", [.command, .shift]), .findPrevious),
            (KeyChord(".", .command), .stop),
        ]
        for (chord, action) in appKeys {
            #expect(AppKey.action(for: chord) == action)
            #expect(action.chord == chord)
        }
        #expect(AppKey.action(for: KeyChord("8", [.option, .command])) == nil)
        #expect(AppKey.action(for: KeyChord(.returnKey, .command)) == nil)
        #expect(Set(AppKey.table.map(\.1)).count == AppKey.table.count)
    }

    @Test("Key script words read as chords")
    func script() {
        #expect(KeyChord(script: "opt+cmd+right") == KeyChord(.right, [.option, .command]))
        #expect(KeyChord(script: "cmd+1") == KeyChord("1", .command))
        #expect(KeyChord(script: "ctrl+tab") == KeyChord(.tab, .control))
        #expect(KeyChord(script: "ctrl+shift+tab") == KeyChord(.tab, [.control, .shift]))
        #expect(KeyChord(script: "cmd+return") == KeyChord(.returnKey, .command))
        #expect(KeyChord(script: "cmd") == nil)
        #expect(KeyChord(script: "hyper+x") == nil)
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
        #expect(state.begin(sessionID: "one", draft: "  First message  ") == "First message")
        #expect(state.isSending(sessionID: "one"))
        #expect(state.begin(sessionID: "one", draft: "First message") == nil)
        #expect(state.finish(
            sessionID: "one", currentDraft: "Second message", succeeded: true
        ) == "Second message")
        #expect(!state.isSending(sessionID: "one"))

        #expect(state.begin(sessionID: "two", draft: "Third message") == "Third message")
        #expect(state.finish(
            sessionID: "two", currentDraft: "Third message", succeeded: true
        ).isEmpty)
    }

    @Test("A picked command clears after send, but a later edit stays")
    func pickedCommandSendState() {
        var state = ComposerSendState()
        #expect(state.begin(sessionID: "one", draft: "/review ") == "/review")
        #expect(state.finish(sessionID: "one", currentDraft: "/review ", succeeded: true) == "")
        #expect(state.begin(sessionID: "one", draft: "/review ") == "/review")
        #expect(state.finish(
            sessionID: "one", currentDraft: "/review \n", succeeded: true
        ) == "/review \n")
    }
}
