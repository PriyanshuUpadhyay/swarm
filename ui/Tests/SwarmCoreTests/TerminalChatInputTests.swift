import Testing
@testable import SwarmCore

@Suite("Terminal chat input")
struct TerminalChatInputTests {
    @Test("Submission writes text, waits, and then writes Return")
    func submissionPlan() {
        #expect(TerminalChatInput.submission("Hello") == [.text("Hello"), .key(.enter)])
        #expect(TerminalChatInput.interWriteDelay == .milliseconds(150))
    }
}
