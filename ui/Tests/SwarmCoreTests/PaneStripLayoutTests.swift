import Foundation
import Testing
@testable import SwarmCore

@Suite("Pane strip layout")
struct PaneStripLayoutTests {
    @Test("An odd count starts with one full-height pane, then pairs")
    func columns() {
        #expect(PaneStripLayout.columns(count: 0) == [])
        #expect(PaneStripLayout.columns(count: 1) == [[0]])
        #expect(PaneStripLayout.columns(count: 2) == [[0, 1]])
        #expect(PaneStripLayout.columns(count: 3) == [[0], [1, 2]])
        #expect(PaneStripLayout.columns(count: 4) == [[0, 1], [2, 3]])
        #expect(PaneStripLayout.columns(count: 5) == [[0], [1, 2], [3, 4]])
        #expect(PaneStripLayout.columns(count: 12) == [
            [0, 1], [2, 3], [4, 5], [6, 7], [8, 9], [10, 11],
        ])
    }

    @Test("The chat takes 90% and a column a third, but at least 440 pt")
    func widths() {
        let wide = PaneStripLayout.widths(main: 1500)
        #expect(wide.chat == 1350)
        #expect(wide.column == 500)
        let narrow = PaneStripLayout.widths(main: 900)
        #expect(narrow.chat == 810)
        #expect(narrow.column == 440)
    }

    @Test("A dragged column width stays between 440 pt and 90% of the main area")
    func columnWidth() {
        func width(_ main: CGFloat, _ preferred: CGFloat?) -> CGFloat {
            PaneStripLayout.columnWidth(main: main, preferred: preferred)
        }
        #expect(width(1500, nil) == 500)
        #expect(width(900, nil) == 440)
        #expect(width(1500, 700) == 700)
        #expect(width(1500, 300) == 440)
        #expect(width(1500, 1400) == 1350)
        // A window too narrow for 440 pt at 90% keeps the minimum.
        #expect(width(400, 1000) == 440)
        #expect(width(400, 100) == 440)
    }

    @Test("A split stays between 25% and 75% and starts at half")
    func split() {
        #expect(PaneStripLayout.split(preferred: nil) == 0.5)
        #expect(PaneStripLayout.split(preferred: 0.6) == 0.6)
        #expect(PaneStripLayout.split(preferred: 0.1) == 0.25)
        #expect(PaneStripLayout.split(preferred: 0.9) == 0.75)
    }

    @Test("Saved splits keep only shown columns and survive unreadable text")
    func savedSplits() {
        let text = PaneStripLayout.text(splits: ["reviewer": 0.3, "gone": 0.7], keeping: ["reviewer"])
        #expect(PaneStripLayout.splits(from: text) == ["reviewer": 0.3])
        #expect(PaneStripLayout.splits(from: "") == [:])
        #expect(PaneStripLayout.splits(from: "not json") == [:])
    }

    @Test("Focus moves across columns and the chat page, and within a column")
    func move() {
        typealias F = PaneStripLayout.Focus
        func move(_ from: F, _ count: Int, _ direction: FocusDirection) -> F {
            PaneStripLayout.move(from: from, count: count, direction: direction)
        }
        // Five panes: columns [0], [1, 2], [3, 4].
        #expect(move(.chat, 5, .right) == .pane(0))
        #expect(move(.pane(0), 5, .left) == .chat)
        #expect(move(.pane(0), 5, .right) == .pane(1))
        #expect(move(.pane(2), 5, .right) == .pane(4))
        #expect(move(.pane(2), 5, .left) == .pane(0))
        #expect(move(.pane(1), 5, .down) == .pane(2))
        #expect(move(.pane(2), 5, .down) == .pane(2))
        #expect(move(.pane(2), 5, .up) == .pane(1))
        #expect(move(.pane(4), 5, .right) == .pane(4))
        #expect(move(.pane(0), 5, .up) == .pane(0))
        // No panes: the chat keeps focus.
        #expect(move(.chat, 0, .right) == .chat)
        #expect(move(.chat, 4, .up) == .chat)
    }
}
