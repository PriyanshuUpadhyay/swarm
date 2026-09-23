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
        #expect(KeyRouting.route(focus: .sidebar, key: .escape) == .ignore)
        for focus in [FocusedSurface.terminal, .transcript, .composer, .sidebar] {
            #expect(KeyRouting.route(focus: focus, key: .commandN) == .openNewChat)
        }
    }

    @Test("Composer sends trimmed text only")
    func outgoing() {
        #expect(Composer.outgoing("  Hello, chair  \n") == "Hello, chair")
        #expect(Composer.outgoing(" \t\n") == nil)
    }
}
