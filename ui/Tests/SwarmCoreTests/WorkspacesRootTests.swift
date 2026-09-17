import Testing
import Foundation
@testable import SwarmCore

@Suite("Where new worktrees are cut")
struct WorkspacesRootTests {
    @Test("a new installation gets the folder Spotlight skips")
    func newInstallation() {
        let home = URL(fileURLWithPath: "/Users/tester")
        #expect(WorkspacesRoot.resolve(home: home).path == "/Users/tester/swarm/workspaces.noindex")
    }

    @Test("the folder name keeps Spotlight out")
    func theNameIsTheMechanism() {
        #expect(WorkspacesRoot.preferredName.hasSuffix(".noindex"))
        #expect(WorkspacesRoot.note.contains(".noindex"))
    }

    @Test("WorkspaceManager uses the shared rule")
    func theManagerUsesTheRule() {
        #expect(WorkspaceManager.workspacesRoot == WorkspacesRoot.resolve())
    }
}
