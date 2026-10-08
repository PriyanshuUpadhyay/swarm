import Testing
@testable import SwarmCore

@Suite("New workspace form")
struct NewWorkspaceFormTests {
    private let references = WorkspaceReferences(defaultBranch: "main", local: ["main", "fix/local"], remote: ["origin/fix/remote"])

    @Test("The preview uses the new slug or the existing branch as given")
    func previewsBranches() {
        #expect(NewWorkspaceForm.preview(request(.newBranch(base: "main"))) == "swarm/fix-sidebar")
        #expect(NewWorkspaceForm.preview(request(.pullRequest(7))) == "swarm/fix-sidebar")
        #expect(NewWorkspaceForm.preview(request(.existingBranch("fix/local"))) == "fix/local")
        #expect(NewWorkspaceForm.preview(request(.existingBranch("origin/fix/remote"))) == "origin/fix/remote")
        #expect(NewWorkspaceForm.preview(request(.existingBranch(""))) == nil)
        #expect(NewWorkspaceForm.preview(WorkspaceRequest(name: "Fix sidebar", start: .newBranch(base: "main"), prefix: "")) == "fix-sidebar")
        #expect(NewWorkspaceForm.preview(WorkspaceRequest(name: "Fix sidebar", start: .pullRequest(7), prefix: "bad//")) == nil)
    }

    @Test("Create requires a selected known base or branch or a positive PR number")
    func validatesStarts() {
        for start in [WorkspaceStart.newBranch(base: "main"), .newBranch(base: "origin/fix/remote"), .existingBranch("fix/local"), .existingBranch("origin/fix/remote"), .pullRequest(7)] {
            #expect(NewWorkspaceForm.canCreate(request(start), references: references))
        }
        for start in [WorkspaceStart.newBranch(base: ""), .newBranch(base: "gone"), .existingBranch(""), .existingBranch("gone"), .pullRequest(0), .pullRequest(-7)] {
            #expect(!NewWorkspaceForm.canCreate(request(start), references: references))
        }
        for start in [WorkspaceStart.newBranch(base: "main"), .existingBranch("fix/local"), .pullRequest(7)] {
            #expect(!NewWorkspaceForm.canCreate(WorkspaceRequest(name: " ", start: start, prefix: "swarm/"), references: references))
            #expect(!NewWorkspaceForm.canCreate(WorkspaceRequest(name: "Fix sidebar", start: start, prefix: "bad//"), references: references))
        }
    }

    @Test("An unborn repository permits only a new branch without a base")
    func unbornRepository() {
        let unborn = WorkspaceReferences(defaultBranch: nil, local: [], remote: [])
        #expect(NewWorkspaceForm.canCreate(request(.newBranch(base: "")), references: unborn))
        for start in [WorkspaceStart.newBranch(base: "main"), .existingBranch("main"), .pullRequest(7)] {
            #expect(!NewWorkspaceForm.canCreate(request(start), references: unborn))
        }
    }

    @Test("Held local branches and their origin branches cannot be selected")
    func heldBranches() {
        let held = WorkspaceReferences(defaultBranch: "main", local: ["main", "fix/local"], remote: ["origin/main"], held: ["main"])
        #expect(held.availableBranches == ["fix/local"])
        #expect(!NewWorkspaceForm.canCreate(request(.existingBranch("main")), references: held))
        #expect(!NewWorkspaceForm.canCreate(request(.existingBranch("origin/main")), references: held))
        #expect(NewWorkspaceForm.canCreate(request(.newBranch(base: "main")), references: held))
    }

    private func request(_ start: WorkspaceStart) -> WorkspaceRequest {
        WorkspaceRequest(name: "Fix sidebar", start: start, prefix: "swarm/")
    }
}
