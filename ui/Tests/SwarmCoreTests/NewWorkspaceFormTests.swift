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
        #expect(NewWorkspaceForm.preview(request(.existingBranch("origin/fix/remote"))) == "fix/remote (tracks origin/fix/remote)")
        #expect(NewWorkspaceForm.preview(request(.existingBranch(""))) == nil)
        #expect(NewWorkspaceForm.preview(WorkspaceRequest(name: "Fix sidebar", start: .newBranch(base: "main"), prefix: "")) == "fix-sidebar")
        #expect(NewWorkspaceForm.preview(WorkspaceRequest(name: "Fix sidebar", start: .pullRequest(7), prefix: "bad//")) == nil)
    }

    @Test("Create requires a selected known base or branch or a positive PR number")
    func validatesStarts() {
        for start in [WorkspaceStart.newBranch(base: "main"), .newBranch(base: "origin/fix/remote"), .existingBranch("fix/local"), .existingBranch("origin/fix/remote"), .pullRequest(7)] {
            #expect(NewWorkspaceForm.canCreate(request(start), references: references, worktreeFolder: "../tasks"))
        }
        for start in [WorkspaceStart.newBranch(base: ""), .newBranch(base: "gone"), .existingBranch(""), .existingBranch("gone"), .pullRequest(0), .pullRequest(-7)] {
            #expect(!NewWorkspaceForm.canCreate(request(start), references: references, worktreeFolder: "../tasks"))
        }
        for start in [WorkspaceStart.newBranch(base: "main"), .existingBranch("fix/local"), .pullRequest(7)] {
            #expect(!NewWorkspaceForm.canCreate(WorkspaceRequest(name: " ", start: start, prefix: "swarm/"), references: references, worktreeFolder: "../tasks"))
            #expect(!NewWorkspaceForm.canCreate(WorkspaceRequest(name: "Fix sidebar", start: start, prefix: "bad//"), references: references, worktreeFolder: "../tasks"))
        }
    }

    @Test("An unborn repository permits only a new branch without a base")
    func unbornRepository() {
        let unborn = WorkspaceReferences(defaultBranch: nil, local: [], remote: [])
        #expect(NewWorkspaceForm.canCreate(request(.newBranch(base: "")), references: unborn, worktreeFolder: "../tasks"))
        for start in [WorkspaceStart.newBranch(base: "main"), .existingBranch("main"), .pullRequest(7)] {
            #expect(!NewWorkspaceForm.canCreate(request(start), references: unborn, worktreeFolder: "../tasks"))
        }
    }

    @Test("Held local branches and their origin branches cannot be selected")
    func heldBranches() {
        let held = WorkspaceReferences(defaultBranch: "main", local: ["main", "fix/local"], remote: ["origin/main"], held: ["main"])
        #expect(held.availableBranches == ["fix/local"])
        #expect(!NewWorkspaceForm.canCreate(request(.existingBranch("main")), references: held, worktreeFolder: "../tasks"))
        #expect(!NewWorkspaceForm.canCreate(request(.existingBranch("origin/main")), references: held, worktreeFolder: "../tasks"))
        #expect(NewWorkspaceForm.canCreate(request(.newBranch(base: "main")), references: held, worktreeFolder: "../tasks"))
    }

    @Test("An empty folder or name refuses Create")
    func emptyFields() {
        for folder in ["", " ", "\n\t"] {
            #expect(!NewWorkspaceForm.canCreate(request(.newBranch(base: "main")), references: references, worktreeFolder: folder))
        }
        for name in ["", " ", "\n\t"] {
            #expect(!NewWorkspaceForm.canCreate(
                WorkspaceRequest(name: name, start: .existingBranch("fix/local"), prefix: "swarm/"),
                references: references, worktreeFolder: "../tasks"
            ))
        }
    }

    @Test("Only values that differ from the built-in seed are saved")
    func savesOverrides() {
        #expect(NewWorkspaceForm.defaults(
            worktreeFolder: "../project-worktrees", branchPrefix: "swarm/", seedFolder: "../project-worktrees", seedPrefix: "swarm/"
        ) == ProjectDefaults())
        #expect(NewWorkspaceForm.defaults(
            worktreeFolder: "../custom", branchPrefix: "swarm/", seedFolder: "../project-worktrees", seedPrefix: "swarm/"
        ) == ProjectDefaults(worktreeFolder: "../custom"))
        #expect(NewWorkspaceForm.defaults(
            worktreeFolder: "../project-worktrees", branchPrefix: "", seedFolder: "../project-worktrees", seedPrefix: "swarm/"
        ) == ProjectDefaults(branchPrefix: ""))
        #expect(NewWorkspaceForm.defaults(
            worktreeFolder: "../custom", branchPrefix: "fix/", seedFolder: "../project-worktrees", seedPrefix: "swarm/"
        ) == ProjectDefaults(worktreeFolder: "../custom", branchPrefix: "fix/"))
    }

    private func request(_ start: WorkspaceStart) -> WorkspaceRequest {
        WorkspaceRequest(name: "Fix sidebar", start: start, prefix: "swarm/")
    }
}
