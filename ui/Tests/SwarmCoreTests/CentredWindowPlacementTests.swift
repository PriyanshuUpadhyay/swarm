import CoreGraphics
import Testing
@testable import SwarmCore

@Suite("CentredWindowPlacement")
struct CentredWindowPlacementTests {
    @Test("Centres on an offset owner rather than the display")
    func owner() {
        let owner = CGRect(x: 400, y: 120, width: 1000, height: 800)
        let frame = CentredWindowPlacement.frame(size: CGSize(width: 520, height: 424), around: owner,
                                                visible: CGRect(x: 0, y: 0, width: 2000, height: 1200))
        #expect(frame.midX == owner.midX)
        #expect(frame.midY == owner.midY)
    }

    @Test("Changing steps keeps the top edge still when growing and shrinking", arguments: [300, 424, 534, 747] as [CGFloat])
    func resize(height: CGFloat) {
        let before = CGRect(x: 400, y: 400, width: 520, height: 424)
        let frame = CentredWindowPlacement.frame(size: CGSize(width: 520, height: height), keepingTopOf: before,
                                                visible: CGRect(x: 0, y: 0, width: 2000, height: 1400))
        #expect(frame.maxY == before.maxY)
        #expect(frame.midX == before.midX)
        #expect(frame.height == height)
    }

    @Test("A step too tall for the space below is lifted rather than cut off")
    func resizeNearBottom() {
        let visible = CGRect(x: 0, y: 30, width: 1440, height: 870)
        let before = CGRect(x: 400, y: 60, width: 520, height: 424)
        let frame = CentredWindowPlacement.frame(size: CGSize(width: 520, height: 747), keepingTopOf: before,
                                                visible: visible)
        #expect(visible.contains(frame))
        #expect(frame.minY == visible.minY)
    }

    @Test("Keeps every edge on the owner's display", arguments: [-1800.0, 0.0, 1800.0])
    func screenEdges(origin: Double) {
        let visible = CGRect(x: origin, y: 30, width: 1440, height: 870)
        for anchor in [CGRect(x: origin - 100, y: -100, width: 200, height: 200),
                       CGRect(x: visible.maxX - 10, y: visible.maxY - 10, width: 200, height: 200)] {
            let frame = CentredWindowPlacement.frame(size: CGSize(width: 520, height: 424), around: anchor, visible: visible)
            #expect(visible.contains(frame))
        }
    }
}
