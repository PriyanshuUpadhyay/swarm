import Foundation
import Testing
@testable import SwarmCore

/// `git worktree list --porcelain`, and who a branch of this repository is held by.
///
/// The fixtures are output this Mac actually produced, not output anybody imagined. The
/// there-there project lists twenty-two worktrees over three applications, several locked by an
/// agent, with the main checkout among them looking exactly like the rest; the Swarm repository
/// adds the `locked <reason>` line. Both are trimmed to the records that make a point, and not one
/// character of a record is changed.
@Suite("Worktree listing")
struct WorktreeListingTests {
    /// `git worktree list --porcelain` in /Users/freek/dev/code/there-there, trimmed to six of its
    /// twenty-two records: the main checkout, two of Swarm's, and three of Conductor's, including
    /// the one that held pull request #362's branch and made this whole thing necessary.
    static let thereThere = """
        worktree /Users/freek/dev/code/there-there
        HEAD 67bfc360dad6fea4dcecb3964c027b0800bb0801
        branch refs/heads/main

        worktree /Users/freek/swarm/workspaces/there-there/freekmurze-mawson-sea
        HEAD ba5d665bd1d21692e8bd4c24e59bd74889d58338
        branch refs/heads/freekmurze/review-changes

        worktree /Users/freek/swarm/workspaces/there-there/freekmurze-molucca-sea
        HEAD ba5d665bd1d21692e8bd4c24e59bd74889d58338
        branch refs/heads/freekmurze/review-repo-changes

        worktree /Users/freek/conductor/workspaces/there-there/adelaide
        HEAD ba5d665bd1d21692e8bd4c24e59bd74889d58338
        branch refs/heads/freekmurze/figma-mcp-check

        worktree /Users/freek/conductor/workspaces/there-there/port-louis
        HEAD 9a5419ea7120db9a74ae0486c9b19f808d9cdc90
        branch refs/heads/freekmurze/investigate-ticket-issues

        worktree /Users/freek/orca/workspaces/there-there/anglerfish
        HEAD a870d24b0287a3f9c7c3273d54a9a970c4d0bc82
        branch refs/heads/freekmurze/delete-ticket-workflow-crash

        """

    /// The same command in the Swarm repository, whose agents lock the worktrees they are working
    /// in. `locked <reason>` is the line that arrives with them.
    static let swarm = """
        worktree /Users/freek/dev/code/swarm
        HEAD 7c28676194979a756ff1ec3987b75bf9e6eb1e04
        branch refs/heads/feat/quick-prompt-icon-picker

        worktree /Users/freek/dev/code/swarm/.claude/worktrees/agent-a027934bc60df6d12
        HEAD 97c58990967b1f091feac9de1d019b28edabcbca
        branch refs/heads/docs/audit-fixes

        worktree /Users/freek/dev/code/swarm/.claude/worktrees/agent-a2f34e223cc574ba6
        HEAD 4d885856a5ee18427ed0d771bbf6b76c116dfcc2
        branch refs/heads/swiftlint
        locked claude agent agent-a2f34e223cc574ba6 (pid 2545 start Mon Aug 24 13:40:21 2026)

        """

    // MARK: - Parsing what git prints

    @Test("every record of a real listing is read, in the order git printed them")
    func readsARealListing() {
        let entries = WorktreeListing.parse(Self.thereThere)
        #expect(entries.count == 6)
        #expect(entries.first?.path == "/Users/freek/dev/code/there-there")
        #expect(entries.first?.branch == "main")
        #expect(entries.last?.path == "/Users/freek/orca/workspaces/there-there/anglerfish")
    }

    /// The failure that started this. Only the `refs/heads/` prefix comes off: a branch name may
    /// itself carry slashes, and the name git refused to check out twice is the whole of
    /// `freekmurze/figma-mcp-check`, so anything shorter would not match the branch being asked for.
    @Test("a branch is the full ref with refs/heads/ taken off and nothing else")
    func keepsSlashesInsideABranchName() {
        let entries = WorktreeListing.parse(Self.thereThere)
        let adelaide = entries.first { $0.path.hasSuffix("/adelaide") }
        #expect(adelaide?.branch == "freekmurze/figma-mcp-check")
        #expect(adelaide?.head == "ba5d665bd1d21692e8bd4c24e59bd74889d58338")
    }

    @Test("a locked worktree carries its reason and still holds its branch")
    func readsALockedWorktree() {
        let entries = WorktreeListing.parse(Self.swarm)
        let locked = entries.first { $0.branch == "swiftlint" }
        #expect(locked?.isLocked == true)
        #expect(locked?.lockReason?.hasPrefix("claude agent agent-a2f34e223cc574ba6") == true)
        #expect(entries.first { $0.branch == "docs/audit-fixes" }?.isLocked == false)
    }

    /// A bare main worktree has no HEAD and no branch, a detached one has a HEAD and no branch, and
    /// a prunable one is a folder git has noticed has gone. None of the three is invented: this is
    /// the shape `git worktree list --porcelain` is documented to print.
    @Test("bare, detached and prunable records are read for what they are")
    func readsTheAwkwardRecords() {
        let entries = WorktreeListing.parse("""
            worktree /Users/freek/mirrors/swarm.git
            bare

            worktree /Users/freek/looking/at/a/tag
            HEAD 623188465f198b813d32b4520e43b4e8f84aa2ab
            detached

            worktree /Users/freek/gone
            HEAD 623188465f198b813d32b4520e43b4e8f84aa2ab
            branch refs/heads/left-behind
            prunable gitdir file points to non-existent location

            worktree /Users/freek/locked/without/a/reason
            HEAD 623188465f198b813d32b4520e43b4e8f84aa2ab
            branch refs/heads/parked
            locked
            """)
        #expect(entries.count == 4)
        #expect(entries[0].isBare)
        #expect(entries[0].branch == nil)
        #expect(entries[1].isDetached)
        #expect(entries[1].branch == nil)
        #expect(entries[2].isPrunable)
        #expect(entries[2].pruneReason == "gitdir file points to non-existent location")
        #expect(entries[3].lockReason == "")
        #expect(entries[3].isLocked)
    }

    @Test("a path with spaces in it survives, and the last record needs no blank line after it")
    func readsAPathWithSpaces() {
        let entries = WorktreeListing.parse("""
            worktree /Users/freek/My Projects/there there
            HEAD 67bfc360dad6fea4dcecb3964c027b0800bb0801
            branch refs/heads/main
            """)
        #expect(entries.map(\.path) == ["/Users/freek/My Projects/there there"])
    }

    /// Records are flushed on the next `worktree` line as well as on the blank one. Without that,
    /// one missing separator merges two worktrees into a record naming the first path and the
    /// second branch, which is exactly how Swarm would come to name the wrong folder to close.
    @Test("a missing blank line does not merge two worktrees into one")
    func survivesAMissingSeparator() {
        let entries = WorktreeListing.parse("""
            worktree /a
            HEAD 1111111111111111111111111111111111111111
            branch refs/heads/one
            worktree /b
            HEAD 2222222222222222222222222222222222222222
            branch refs/heads/two
            """)
        #expect(entries.map(\.path) == ["/a", "/b"])
        #expect(entries.map(\.branch) == ["one", "two"])
    }

    @Test("nothing at all is no worktrees rather than one empty one")
    func readsNothing() {
        #expect(WorktreeListing.parse("").isEmpty)
        #expect(WorktreeListing.parse("\n\n").isEmpty)
        #expect(WorktreeListing.parse("HEAD 1111111111111111111111111111111111111111").isEmpty)
    }
}