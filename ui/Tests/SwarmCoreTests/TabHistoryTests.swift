import Testing
@testable import SwarmCore

@Suite("Recent chat keys")
struct TabHistoryTests {
    @Test("Recent chats wrap at both ends and ignore hidden or repeated keys")
    func recentChats() {
        let history = ["oldest", "hidden", "middle", "newest", "middle"]
        let open = ["newest", "middle", "oldest"]
        #expect(TabHistory.flip(history: history, open: open, current: "middle", step: -1) == "newest")
        #expect(TabHistory.flip(history: history, open: open, current: "oldest", step: -1) == "middle")
        #expect(TabHistory.flip(history: history, open: open, current: "middle", step: 1) == "oldest")
        #expect(TabHistory.flip(history: history, open: open, current: "oldest", step: 1) == "newest")
    }

    @Test("Missing current chat starts at the recent end and empty history has no target")
    func missingChats() {
        #expect(TabHistory.flip(history: ["older", "recent"], open: ["older", "recent"], current: nil, step: -1) == "recent")
        #expect(TabHistory.flip(history: ["older", "recent"], open: ["older", "recent"], current: "hidden", step: 1) == "older")
        #expect(TabHistory.flip(history: ["hidden"], open: ["open"], current: "open", step: -1) == nil)
        #expect(TabHistory.flip(history: ["only"], open: [], current: "only", step: 1) == nil)
        #expect(TabHistory.flip(history: ["only"], open: ["only"], current: "only", step: 1) == "only")
    }

    @Test("The Keys page lists every agreed shortcut and its action")
    func keysPage() {
        #expect(KeysPage.rows.map(\.shortcut) == [
            "⌘T", "⌘W", "⇧⌘W", "⇧⌘T", "⌃Tab", "⌃⇧Tab", "⌘1–8", "⌘9", "⌘K", "⌘N", "⇧⌘N", "⌘,",
        ])
        #expect(KeysPage.rows.map(\.title) == [
            "New Chat", "Close Tab", "Close Window", "Recently closed…", "Previous Recent Chat", "Next Recent Chat",
            "Select chat 1–8", "Last Chat", "Command Palette…", "New Workspace", "New Project…", "Settings…",
        ])
    }
}
