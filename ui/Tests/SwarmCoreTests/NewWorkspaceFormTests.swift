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

    @Test("A free local branch hides its matching origin branch")
    func localBranchHidesRemote() {
        let branches = WorkspaceReferences(defaultBranch: "main", local: ["main", "fix/free"], remote: ["origin/main", "origin/fix/free", "origin/fix/remote"], held: ["main"])
        #expect(branches.availableBranches == ["fix/free", "origin/fix/remote"])
        #expect(!NewWorkspaceForm.canCreate(request(.existingBranch("origin/fix/free")), references: branches, worktreeFolder: "../tasks"))
    }

    @Test("Defaults trim the folder and prefix before saving or comparing the seed")
    func trimsDefaults() {
        #expect(NewWorkspaceForm.defaults(
            worktreeFolder: " ../x \n", branchPrefix: " fix/ \t", seedFolder: "../project-worktrees", seedPrefix: "swarm/"
        ) == ProjectDefaults(worktreeFolder: "../x", branchPrefix: "fix/"))
        #expect(NewWorkspaceForm.defaults(
            worktreeFolder: " ../project-worktrees \n", branchPrefix: " swarm/ \t", seedFolder: "../project-worktrees", seedPrefix: "swarm/"
        ) == ProjectDefaults())
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

    @Test("A request trims its branch prefix")
    func requestTrimsPrefix() {
        let spaced = WorkspaceRequest(name: "Fix sidebar", start: .newBranch(base: "main"), prefix: " fix/ ")
        #expect(spaced.prefix == "fix/")
        #expect(NewWorkspaceForm.preview(spaced) == "fix/fix-sidebar")
    }

    private func request(_ start: WorkspaceStart) -> WorkspaceRequest {
        WorkspaceRequest(name: "Fix sidebar", start: start, prefix: "swarm/")
    }
}
