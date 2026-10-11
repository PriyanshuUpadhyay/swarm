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

    @Test("A dragged chat leaves 440 pt for the first column and stays at least 400 pt")
    func draggedChatWidth() {
        func width(main: CGFloat, chat: CGFloat?) -> CGFloat {
            PaneStripLayout.widths(main: main, chat: chat).chat
        }
        #expect(width(main: 1500, chat: 700) == 700)
        #expect(width(main: 1500, chat: 100) == 400)
        #expect(width(main: 1500, chat: 2000) == 1060)
        #expect(width(main: 900, chat: 600) == 460)
        #expect(width(main: 840, chat: 600) == 400)
        #expect(width(main: 680, chat: 600) == 400)
        #expect(width(main: 400, chat: 100) == 400)
        // Reset drops the preferred width and restores the existing default.
        #expect(width(main: 1500, chat: nil) == 1350)
        #expect(width(main: 900, chat: nil) == 810)
        #expect(PaneStripLayout.widths(main: 1500, chat: 700).column == 500)
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
        let text = PaneStripLayout.text(saving: ["reviewer": 0.3, "gone": 0.7], scope: "chat-a",
                                        keeping: ["reviewer"], in: "")
        #expect(PaneStripLayout.splits(from: text, scope: "chat-a") == ["reviewer": 0.3])
        #expect(PaneStripLayout.splits(from: "", scope: "chat-a") == [:])
        #expect(PaneStripLayout.splits(from: "not json", scope: "chat-a") == [:])
        #expect(PaneStripLayout.splits(from: #"{"reviewer":0.3}"#, scope: "chat-a") == [:])
    }

    @Test("Two chats with the same agent id keep separate splits")
    func splitsPerChat() {
        var text = PaneStripLayout.text(saving: ["reviewer": 0.3], scope: "chat-a", keeping: ["reviewer"], in: "")
        text = PaneStripLayout.text(saving: ["reviewer": 0.7], scope: "chat-b", keeping: ["reviewer"], in: text)
        #expect(PaneStripLayout.splits(from: text, scope: "chat-a") == ["reviewer": 0.3])
        #expect(PaneStripLayout.splits(from: text, scope: "chat-b") == ["reviewer": 0.7])
    }

    @Test("Saving one chat prunes only that chat's gone columns")
    func pruneOneChat() {
        var text = PaneStripLayout.text(saving: ["reviewer-a": 0.3, "closed-a": 0.6], scope: "chat-a",
                                        keeping: ["reviewer-a", "closed-a"], in: "")
        text = PaneStripLayout.text(saving: ["reviewer-b": 0.7], scope: "chat-b", keeping: ["reviewer-b"], in: text)
        #expect(PaneStripLayout.splits(from: text, scope: "chat-a") == ["reviewer-a": 0.3, "closed-a": 0.6])
        text = PaneStripLayout.text(saving: ["reviewer-a": 0.4, "closed-a": 0.6], scope: "chat-a",
                                    keeping: ["reviewer-a"], in: text)
        #expect(PaneStripLayout.splits(from: text, scope: "chat-a") == ["reviewer-a": 0.4])
        #expect(PaneStripLayout.splits(from: text, scope: "chat-b") == ["reviewer-b": 0.7])
    }

    @Test("Past the cap, the chats saved longest ago are dropped first")
    func splitScopeCap() {
        let cap = PaneStripLayout.maximumSplitScopes
        var text = ""
        for chat in 0...cap {
            text = PaneStripLayout.text(saving: ["reviewer": 0.3], scope: "chat-\(chat)", keeping: ["reviewer"],
                                        in: text)
        }
        // Saving chat-1 again makes it the newest, so chat-2 is next to go.
        text = PaneStripLayout.text(saving: ["reviewer": 0.4], scope: "chat-1", keeping: ["reviewer"], in: text)
        text = PaneStripLayout.text(saving: ["reviewer": 0.3], scope: "chat-new", keeping: ["reviewer"], in: text)
        #expect(PaneStripLayout.splits(from: text, scope: "chat-0") == [:])
        #expect(PaneStripLayout.splits(from: text, scope: "chat-2") == [:])
        #expect(PaneStripLayout.splits(from: text, scope: "chat-1") == ["reviewer": 0.4])
        #expect(PaneStripLayout.splits(from: text, scope: "chat-3") == ["reviewer": 0.3])
        #expect(PaneStripLayout.splits(from: text, scope: "chat-new") == ["reviewer": 0.3])
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
