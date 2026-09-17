import Foundation
import Testing
@testable import SwarmCore

/// What the pane looks like when it is sorted by what each workspace needs.
@Suite("Sidebar status grouping")
struct SidebarStatusGroupTests {
    private func workspace(_ id: String, touched: Date = .distantPast) -> Workspace {
        Workspace(
            id: WorkspaceID(id),
            repoID: RepoID("r1"),
            name: id,
            branch: "swarm/\(id)",
            path: "/tmp/\(id)",
            baseBranch: "main",
            lastActivityAt: touched
        )
    }

    // MARK: - Which section a state lands in

    @Test func aPermissionQuestionAndAFailedSetupBothWantAPerson() {
        #expect(SidebarStatusGroup.of(.awaitingPermission) == .needsYou)
        #expect(SidebarStatusGroup.of(.setupFailed) == .needsYou)
    }

    @Test func aFinishedTurnNobodyHasReadRanksAboveOneStillRunning() throws {
        #expect(SidebarStatusGroup.of(.unread) == .readyToRead)
        #expect(SidebarStatusGroup.of(.running) == .working)

        let order = SidebarStatusGroup.allCases
        let read = try #require(order.firstIndex(of: .readyToRead))
        let working = try #require(order.firstIndex(of: .working))
        #expect(read < working)
    }

    @Test func cuttingAWorktreeCountsAsWorking() {
        #expect(SidebarStatusGroup.of(.settingUp) == .working)
    }

    /// Every state GitHub can report is one thing to the queue: not now.
    @Test func everyPullRequestStateIsIdle() {
        let states: [WorkspaceStatus] = [
            .merged, .closed, .conflicted, .checksFailing, .checksRunning, .checksPassed, .draft,
            .pullRequestOpen, .changed, .clean,
        ]
        for state in states {
            #expect(SidebarStatusGroup.of(state) == .idle, "\(state) should be idle")
        }
    }

    /// The mark on the row says the pull request, and the section still says there is a turn to
    /// read. Grouping by the mark alone put these in Idle as soon as `gh` had answered.
    @Test func anUnreadTurnOutranksThePullRequestForTheSection() {
        for state: WorkspaceStatus in [.merged, .checksPassed, .pullRequestOpen, .changed] {
            #expect(SidebarStatusGroup.of(state, unread: true) == .readyToRead, "\(state)")
            #expect(SidebarStatusGroup.of(state, unread: false) == .idle, "\(state)")
        }
    }

    @Test func aQuestionOrARunningTurnStillOutranksUnread() {
        #expect(SidebarStatusGroup.of(.awaitingPermission, unread: true) == .needsYou)
        #expect(SidebarStatusGroup.of(.running, unread: true) == .working)
    }

    @Test func anUnreadWorkspaceWithAMergedPullRequestIsListedAsReadyToRead() {
        var merged = workspace("merged")
        merged.unread = true
        let listing = SidebarStatusListing.build(
            workspaces: [merged, workspace("read")],
            status: { _ in .merged }
        )
        #expect(listing.sections.map(\.group) == [.readyToRead, .idle])
        #expect(listing.sections.first?.workspaces.map(\.id.rawValue) == ["merged"])
    }

    @Test func everyStateHasASection() {
        for status in WorkspaceStatus.allCases {
            _ = SidebarStatusGroup.of(status)
        }
    }

    // MARK: - The listing

    @Test func emptySectionsAreNotDrawn() {
        let listing = SidebarStatusListing.build(
            workspaces: [workspace("a")], status: { _ in .clean }
        )
        #expect(listing.sections.map(\.group) == [.idle])
    }

    @Test func sectionsComeInTheOrderTheCasesAreDeclaredIn() {
        let rows = [workspace("idle"), workspace("run"), workspace("ask"), workspace("unread")]
        let listing = SidebarStatusListing.build(workspaces: rows) { workspace in
            switch workspace.id.rawValue {
            case "run": .running
            case "ask": .awaitingPermission
            case "unread": .unread
            default: .clean
            }
        }
        #expect(listing.sections.map(\.group) == [.needsYou, .readyToRead, .working, .idle])
    }

    /// The order the pane already draws projects and their rows in is kept inside each section, so
    /// a section of six reads project by project rather than in an order nobody chose. Idle is the
    /// one exception, and has a suite of its own below.
    @Test func rowsKeepTheOrderTheyWereHandedIn() {
        let rows = [workspace("a"), workspace("b"), workspace("c")]
        let listing = SidebarStatusListing.build(workspaces: rows, status: { _ in .unread })
        #expect(listing.sections.first?.workspaces.map(\.id.rawValue) == ["a", "b", "c"])
    }

    // MARK: - Idle ranks by recency

    @Test func theIdleSectionPutsWhatWasTouchedLastOnTop() {
        let now = Date()
        let rows = [
            workspace("old", touched: now.addingTimeInterval(-3_600)),
            workspace("older", touched: now.addingTimeInterval(-86_400)),
            workspace("justRead", touched: now),
        ]
        let listing = SidebarStatusListing.build(workspaces: rows, status: { _ in .clean })
        #expect(listing.sections.first?.workspaces.map(\.id.rawValue) == ["justRead", "old", "older"])
    }

    /// A row that has just been read lands at the top of Idle rather than in the middle of it, which
    /// is the whole reason the section is ranked at all.
    @Test func aRowLeavingReadyToReadLandsAtTheTopOfIdle() {
        let now = Date()
        let read = workspace("read", touched: now)
        let rows = [workspace("a", touched: now.addingTimeInterval(-60)), read]
        let listing = SidebarStatusListing.build(workspaces: rows, status: { _ in .clean })
        #expect(listing.sections.first?.workspaces.first?.id == read.id)
    }

    @Test func onlyIdleRanksByRecency() {
        #expect(SidebarStatusGroup.idle.ranksByRecency)
        for group in SidebarStatusGroup.allCases where group != .idle {
            #expect(!group.ranksByRecency, "\(group) should keep the pane's order")
        }
    }

    // MARK: - Folding

    @Test func onlyIdleFolds() {
        #expect(SidebarStatusGroup.idle.isFoldable)
        for group in SidebarStatusGroup.allCases where group != .idle {
            #expect(!group.isFoldable, "\(group) should not fold")
        }
    }

    // MARK: - Grouping

    @Test func onlyTheProjectShapeCanBeReordered() {
        #expect(SidebarGrouping.projects.allowsReordering)
        #expect(!SidebarGrouping.status.allowsReordering)
    }

    /// The menu lists these in declaration order, and the default has to lead it.
    @Test func statusIsTheFirstShapeOffered() {
        #expect(SidebarGrouping.allCases.first == .status)
    }
}

/// Which states keep a mark at rest, and which lost one.
@Suite("Sidebar mark policy")
struct SidebarMarkPolicyTests {
    @Test func aRestingRowOnlyMarksWorkAndAnswers() {
        for status in [WorkspaceStatus.settingUp, .awaitingPermission, .running, .setupFailed, .unread] {
            #expect(SidebarMarkPolicy.drawsMark(status), "\(status) should keep its mark")
        }
    }

    /// The two branch states a person has to clear by hand, which nothing else in the pane says.
    @Test func brokenBranchesKeepTheirMark() {
        #expect(SidebarMarkPolicy.drawsMark(.checksFailing))
        #expect(SidebarMarkPolicy.drawsMark(.conflicted))
    }

    @Test func aStateThatWantsNothingDrawsNothing() {
        let quiet: [WorkspaceStatus] = [
            .merged, .closed, .checksRunning, .checksPassed, .draft, .pullRequestOpen, .changed,
            .clean,
        ]
        for status in quiet {
            #expect(!SidebarMarkPolicy.drawsMark(status), "\(status) should rest")
        }
    }
}

/// The row the owner clicked in Ready to read stays there until they leave it.
@Suite("Sidebar reading hold")
struct SidebarReadingHoldTests {
    private func workspace(_ id: String, unread: Bool) -> Workspace {
        var workspace = Workspace(
            id: WorkspaceID(id),
            repoID: RepoID("r1"),
            name: id,
            branch: "swarm/\(id)",
            path: "/tmp/\(id)",
            baseBranch: "main"
        )
        workspace.unread = unread
        return workspace
    }

    @Test func selectingAnUnreadWorkspaceHoldsIt() {
        let rows = [workspace("a", unread: true)]
        let hold = SidebarReadingHold.next(selection: .workspace(WorkspaceID("a")), current: nil, workspaces: rows)
        #expect(hold == WorkspaceID("a"))
    }

    @Test func selectingAReadWorkspaceHoldsNothing() {
        let rows = [workspace("a", unread: false)]
        let hold = SidebarReadingHold.next(selection: .workspace(WorkspaceID("a")), current: nil, workspaces: rows)
        #expect(hold == nil)
    }

    /// The flag clears a moment after arrival, and the selection re-firing must not drop the hold.
    @Test func theHoldSurvivesTheFlagClearingWhileSelected() {
        let rows = [workspace("a", unread: false)]
        let id = WorkspaceID("a")
        let hold = SidebarReadingHold.next(selection: .workspace(id), current: id, workspaces: rows)
        #expect(hold == id)
    }

    @Test func leavingTheWorkspaceReleasesIt() {
        let rows = [workspace("a", unread: false), workspace("b", unread: false)]
        let id = WorkspaceID("a")
        #expect(SidebarReadingHold.next(selection: .workspace(WorkspaceID("b")), current: id, workspaces: rows) == nil)
        #expect(SidebarReadingHold.next(selection: .home, current: id, workspaces: rows) == nil)
    }

    @Test func aHeldWorkspaceIsListedInReadyToRead() {
        let rows = [workspace("a", unread: false), workspace("b", unread: false)]
        let listing = SidebarStatusListing.build(workspaces: rows, holding: WorkspaceID("a"), status: { _ in .clean })
        #expect(listing.sections.map(\.group) == [.readyToRead, .idle])
        #expect(listing.sections.first?.workspaces.map(\.id.rawValue) == ["a"])
    }

    @Test func aHoldDoesNotPullAWorkingRowOutOfWorking() {
        let rows = [workspace("a", unread: false)]
        let listing = SidebarStatusListing.build(workspaces: rows, holding: WorkspaceID("a"), status: { _ in .running })
        #expect(listing.sections.map(\.group) == [.working])
    }
}
