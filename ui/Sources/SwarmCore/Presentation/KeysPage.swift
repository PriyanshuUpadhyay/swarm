import Foundation

public enum KeysPage {
    public struct Row: Sendable, Hashable {
        public let shortcut: String
        public let title: String
    }

    public static let rows: [Row] = [
        Row(shortcut: AppKey.newChat.chord.displayText, title: "New Chat"),
        Row(shortcut: AppKey.closeTab.chord.displayText, title: "Close tab"),
        Row(shortcut: AppKey.closeWindow.chord.displayText, title: "Close Window"),
        Row(shortcut: AppKey.recentlyClosed.chord.displayText, title: "Recently closed…"),
        Row(shortcut: AppKey.previousRecentChat.chord.displayText, title: "Previous Recent Chat"),
        Row(shortcut: AppKey.nextRecentChat.chord.displayText, title: "Next Recent Chat"),
        Row(shortcut: "⌘1–8", title: "Select chat 1–8"),
        Row(shortcut: AppKey.lastTab.chord.displayText, title: "Last Chat"),
        Row(shortcut: AppKey.search.chord.displayText, title: "Command Palette…"),
        Row(shortcut: AppKey.newWorkspace.chord.displayText, title: "New Workspace"),
        Row(shortcut: AppKey.newProject.chord.displayText, title: "New Project…"),
        Row(shortcut: "⌘,", title: "Settings…"),
    ]
}
