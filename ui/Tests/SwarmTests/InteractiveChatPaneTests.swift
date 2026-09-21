import Foundation
import Testing
@testable import Swarm
import SwarmCore

@Suite("Interactive chat pane")
struct InteractiveChatPaneTests {
    @Test func chatIsTheDefaultSurface() {
        let suiteName = "chat-default-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        let chairSession = SessionID("chair")
        defer { defaults.removePersistentDomain(forName: suiteName) }
        #expect(InteractiveChatPanePreferences.surface(for: chairSession, in: defaults) == .chat)
    }

    @Test func terminalToggleIsPerSession() {
        let suiteName = "chat-toggle-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let chairSession = SessionID("chair")
        let workerSession = SessionID("worker")

        InteractiveChatPanePreferences.setShowsTerminal(true, for: chairSession, in: defaults)

        #expect(InteractiveChatPanePreferences.showsTerminal(for: chairSession, in: defaults))
        #expect(!InteractiveChatPanePreferences.showsTerminal(for: workerSession, in: defaults))
        #expect(InteractiveChatPanePreferences.surface(for: chairSession, in: defaults) == .terminal)
        #expect(InteractiveChatPanePreferences.surface(for: workerSession, in: defaults) == .chat)
    }
}
