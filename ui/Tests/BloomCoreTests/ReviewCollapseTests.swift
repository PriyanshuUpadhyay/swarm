import Testing
@testable import BloomCore

@Suite("Folding a reviewed file")
struct ReviewCollapseTests {
    @Test("A tick folds the file it arrived on")
    func ticking() {
        let folded = ReviewCollapse.collapsed([], viewed: ["a.swift"], wasViewed: [])
        #expect(folded == ["a.swift"])
    }

    @Test("Taking a tick off opens the file again")
    func unticking() {
        let folded = ReviewCollapse.collapsed(["a.swift"], viewed: [], wasViewed: ["a.swift"])
        #expect(folded.isEmpty)
    }

    @Test("A tick leaves every other file where it was")
    func neighbours() {
        let folded = ReviewCollapse.collapsed(
            ["b.swift"], viewed: ["a.swift", "c.swift"], wasViewed: ["c.swift"]
        )
        #expect(folded == ["a.swift", "b.swift"])
    }

    /// The re-render case: a diff stat poll, a neighbouring section loading, a resize. Nothing
    /// about the ticks has moved, so nothing about the folding may either.
    @Test("An unchanged set moves nothing, so a hand unfold survives")
    func reopenedByHand() {
        let ticks: Set<String> = ["a.swift"]
        var folded = ReviewCollapse.collapsed([], viewed: ticks, wasViewed: [])
        #expect(folded == ["a.swift"])

        folded.remove("a.swift")
        for _ in 0..<5 {
            folded = ReviewCollapse.collapsed(folded, viewed: ticks, wasViewed: ticks)
        }
        #expect(folded.isEmpty)
    }

    @Test("A file folded by hand stays folded when its tick comes off the file beside it")
    func foldedByHand() {
        let folded = ReviewCollapse.collapsed(
            ["a.swift"], viewed: [], wasViewed: ["b.swift"]
        )
        #expect(folded == ["a.swift"])
    }

    /// Before the store has answered there is no previous set, and an empty one would read as
    /// "nothing was ticked", folding every file marked in an earlier pass the moment the pane
    /// opened.
    @Test("Marks arriving from the store fold nothing")
    func firstRead() {
        let folded = ReviewCollapse.collapsed(
            [], viewed: ["a.swift", "b.swift"], wasViewed: nil
        )
        #expect(folded.isEmpty)
    }
}
