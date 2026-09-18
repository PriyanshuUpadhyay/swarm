import Testing
@testable import SwarmCore

/// Hiding a project means two different things on screen depending on one switch, and the whole
/// reason this type exists is that using the wrong animation in either case looks broken.
@Suite struct ProjectVisibilityMotionTests {
    @Test func hidingWithTheHiddenOnesShownIsAContrastChangeAndNothingMoves() {
        let motion = ProjectVisibilityMotion.hideGesture(showingHidden: true, reduceMotion: false)
        #expect(motion == .dim(seconds: ProjectVisibilityMotion.seconds))
        #expect(!motion.fadesArrivals)
    }

    @Test func hidingWithThemNotShownTakesTheRowOutAndClosesTheGap() {
        let motion = ProjectVisibilityMotion.hideGesture(showingHidden: false, reduceMotion: false)
        #expect(motion == .reflow(seconds: ProjectVisibilityMotion.seconds))
        #expect(motion.fadesArrivals)
    }

    @Test func theFilterSwitchIsAlwaysAReflow() {
        // It can insert four project headers at four different depths, each with its workspaces.
        #expect(ProjectVisibilityMotion.filterToggle(reduceMotion: false)
            == .reflow(seconds: ProjectVisibilityMotion.seconds))
    }

    @Test func reduceMotionDropsAllThreeRatherThanSlowingThem() {
        #expect(ProjectVisibilityMotion.hideGesture(showingHidden: true, reduceMotion: true) == .instant)
        #expect(ProjectVisibilityMotion.hideGesture(showingHidden: false, reduceMotion: true) == .instant)
        #expect(ProjectVisibilityMotion.filterToggle(reduceMotion: true) == .instant)
        #expect(ProjectVisibilityMotion.instant.seconds == nil)
        #expect(!ProjectVisibilityMotion.instant.fadesArrivals)
    }

    @Test func everyCaseTakesTheSameLengthAsARowSettling() {
        // Three versions of one confirmation. Three lengths would read as three apps, and the
        // number is the one `TranscriptMotion.arrival` already spends on a row landing.
        #expect(ProjectVisibilityMotion.seconds == TranscriptMotion.arrival(reduceMotion: false)?.seconds)
    }
}

/// Which reloads of the projects `AppModel.reload` publishes inside an animation.
@Suite struct ProjectRowsMovementTests {
    private let first = Repo(name: "swarm", path: "/code/swarm")
    private let second = Repo(name: "site", path: "/code/site")

    @Test func hidingOrUnhidingAProjectMovesRows() {
        var hidden = second
        hidden.hidden = true
        #expect(ProjectVisibilityMotion.movesProjectRows(from: [first, second], to: [first, hidden]))
        #expect(ProjectVisibilityMotion.movesProjectRows(from: [first, hidden], to: [first, second]))
    }

    @Test func addingOrRemovingAProjectMovesRows() {
        #expect(ProjectVisibilityMotion.movesProjectRows(from: [first, second], to: [first]))
        #expect(ProjectVisibilityMotion.movesProjectRows(from: [first], to: [first, second]))
    }

    @Test func renamingFoldingOrReorderingMovesNothing() {
        var renamed = second
        renamed.name = "website"
        renamed.collapsed = true
        #expect(!ProjectVisibilityMotion.movesProjectRows(from: [first, second], to: [first, renamed]))
        #expect(!ProjectVisibilityMotion.movesProjectRows(from: [first, second], to: [second, first]))
    }
}
