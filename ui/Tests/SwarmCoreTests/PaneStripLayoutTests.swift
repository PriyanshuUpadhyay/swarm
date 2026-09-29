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
}
