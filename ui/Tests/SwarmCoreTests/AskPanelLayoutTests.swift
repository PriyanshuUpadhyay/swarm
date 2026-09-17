import CoreGraphics
import Testing
@testable import SwarmCore

@Suite("Ask Swarm panel")
struct AskPanelLayoutTests {
    @Test func widthIsAProportionClampedAtBothEnds() {
        #expect(AskPanelLayout.width(inWindow: 900) == AskPanelLayout.minimumWidth)
        #expect(AskPanelLayout.width(inWindow: 1_440) == 1_440 * AskPanelLayout.widthProportion)
        #expect(AskPanelLayout.width(inWindow: 3_440) == AskPanelLayout.maximumWidth)
    }

    @Test func aWindowTooSmallForTheFloorStillGetsACardThatFits() {
        #expect(AskPanelLayout.width(inWindow: 600) == 600 - AskPanelLayout.margin * 2)
        #expect(AskPanelLayout.height(inWindow: 400) == 400 - AskPanelLayout.margin * 2)
    }

    @Test func heightIsAProportionClampedAtBothEnds() {
        #expect(AskPanelLayout.height(inWindow: 900) == 900 * AskPanelLayout.heightProportion)
        #expect(AskPanelLayout.height(inWindow: 2_000) == AskPanelLayout.maximumHeight)
    }

    @Test func theRailListsTheNewestConversationFirst() {
        #expect(AskPanelLayout.railOrder([1, 2, 3]) == [3, 2, 1])
        #expect(AskPanelLayout.railOrder([Int]()).isEmpty)
    }

    @Test func waitingOnAPersonOutranksRunning() {
        #expect(AskPanelLayout.status(isRunning: true, isAwaitingPermission: true) == .awaitingPermission)
        #expect(AskPanelLayout.status(isRunning: true, isAwaitingPermission: false) == .running)
        #expect(AskPanelLayout.status(isRunning: false, isAwaitingPermission: false) == nil)
    }
}
