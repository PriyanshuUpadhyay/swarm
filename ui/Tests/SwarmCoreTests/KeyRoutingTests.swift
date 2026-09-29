import Testing
@testable import SwarmCore

@Suite("Key routing")
struct KeyRoutingTests {
    @Test("App keys win in every focus; the terminal gets every other key")
    func table() {
        let surfaces: [FocusedSurface] = [.terminal, .transcript, .sidebar]
        let appKeys: [(KeyChord, AppKey)] = [
            (KeyChord("n", .command), .newChat),
            (KeyChord("n", [.command, .shift]), .newWorkspace),
            (KeyChord(.down, [.control, .command]), .nextWorkspace),
            (KeyChord(.up, [.control, .command]), .previousWorkspace),
            (KeyChord("1", .command), .selectTab(1)),
            (KeyChord("9", .command), .selectTab(9)),
            (KeyChord("]", [.command, .shift]), .nextTab),
            (KeyChord("[", [.command, .shift]), .previousTab),
            (KeyChord(.left, [.option, .command]), .moveFocus(.left)),
            (KeyChord(.right, [.option, .command]), .moveFocus(.right)),
            (KeyChord(.up, [.option, .command]), .moveFocus(.up)),
            (KeyChord(.down, [.option, .command]), .moveFocus(.down)),
            (KeyChord(.returnKey, .command), .zoom),
            (KeyChord("l", .command), .focusComposer),
            (KeyChord("b", .command), .toggleSidebar),
            (KeyChord("b", [.command, .shift]), .moveSidebar),
            (KeyChord("1", [.option, .command]), .sidebarView(1)),
            (KeyChord("5", [.option, .command]), .sidebarView(5)),
            (KeyChord("i", [.option, .command]), .showChanges),
            (KeyChord("k", .command), .search),
            (KeyChord(".", .command), .stop),
        ]
        for (chord, action) in appKeys {
            #expect(AppKey.action(for: chord) == action)
            #expect(action.chord == chord)
            for focus in surfaces { #expect(KeyRouting.route(focus: focus, key: chord) == .app(action)) }
        }
        #expect(AppKey.action(for: KeyChord("6", [.option, .command])) == nil)
        #expect(Set(AppKey.table.map(\.1)).count == AppKey.table.count)
    }

    @Test("A terminal gets every key without ⌘; other ⌘ keys go nowhere, except copy, paste, select all")
    func terminalPolicy() {
        let toTerminal = [
            KeyChord("c", .control), KeyChord("r", .control), KeyChord(.escape), KeyChord("x"),
            KeyChord(.returnKey), KeyChord(.left, .option), KeyChord("b", .option),
            KeyChord("d", [.control, .shift]), KeyChord(.up),
        ]
        for chord in toTerminal {
            #expect(KeyRouting.route(focus: .terminal, key: chord) == .terminal)
            #expect(KeyRouting.route(focus: .transcript, key: chord) == .ignore)
        }
        for chord in [KeyChord(.left, .command), KeyChord(.right, .command), KeyChord("e", .command),
                      KeyChord("j", .command), KeyChord("c", [.command, .shift])] {
            #expect(KeyRouting.route(focus: .terminal, key: chord) == .blocked)
        }
        for character in ["c", "v", "a"] as [Character] {
            #expect(KeyRouting.route(focus: .terminal, key: KeyChord(character, .command)) == .edit)
            #expect(KeyRouting.route(focus: .transcript, key: KeyChord(character, .command)) == .ignore)
        }
        let optionMeta = KeyChord("o", [.option, .command])
        for focus in [FocusedSurface.terminal, .transcript, .sidebar] {
            #expect(KeyRouting.route(focus: focus, key: optionMeta) == .blocked)
        }
    }

    @Test("Find keys open the chat's find, and the terminal's own find in a pane")
    func findKeys() {
        let find = KeyChord("f", .command)
        #expect(KeyRouting.route(focus: .transcript, key: find) == .app(.find))
        #expect(KeyRouting.route(focus: .terminal, key: find) == .terminal)
        #expect(KeyRouting.route(focus: .terminal, key: KeyChord("g", .command)) == .terminal)
        #expect(KeyRouting.route(focus: .transcript, key: KeyChord("g", [.command, .shift]))
            == .app(.findPrevious))
    }

    @Test("Key script words read as chords")
    func script() {
        #expect(KeyChord(script: "opt+cmd+right") == KeyChord(.right, [.option, .command]))
        #expect(KeyChord(script: "cmd+1") == KeyChord("1", .command))
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
