import Testing
@testable import SwarmCore

@Suite("Workspace start hold")
struct WorkspaceStartHoldTests {
    @Test("a running agent takes over from the hold, and nothing else is let go")
    func settlesOnRunning() {
        let first = WorkspaceID.new()
        let second = WorkspaceID.new()
        var hold = WorkspaceStartHold()
        hold.begin(first)
        hold.begin(second)

        hold.settle(running: [first, WorkspaceID.new()])

        #expect(!hold.contains(first))
        #expect(hold.contains(second))
    }

    @Test("a start that ends without an agent lets go by hand")
    func releases() {
        let id = WorkspaceID.new()
        var hold = WorkspaceStartHold()
        hold.begin(id)

        hold.release(id)

        #expect(hold.ids.isEmpty)
    }
}
