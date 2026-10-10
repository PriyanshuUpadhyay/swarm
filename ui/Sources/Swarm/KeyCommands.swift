import AppKit
import SwiftUI
import SwarmCore

extension KeyChord {
    /// The chord of a key-down event; nil for keys the app never routes, such as F-keys.
    init?(_ event: NSEvent) {
        var modifiers: Modifiers = []
        let flags = event.modifierFlags
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.control) { modifiers.insert(.control) }
        switch event.keyCode {
        case 36, 76: self.init(.returnKey, modifiers)
        case 48: self.init(.tab, modifiers)
        case 53: self.init(.escape, modifiers)
        case 123: self.init(.left, modifiers)
        case 124: self.init(.right, modifiers)
        case 125: self.init(.down, modifiers)
        case 126: self.init(.up, modifiers)
        default:
            // Without modifiers, so ⇧⌘] reads as "]" and ⌥⌘1 as "1".
            guard let character = event.characters(byApplyingModifiers: [])?.lowercased().first else {
                return nil
            }
            self.init(character, modifiers)
        }
    }

    var shortcut: KeyboardShortcut {
        let equivalent: KeyEquivalent = switch key {
        case .character(let character): KeyEquivalent(character)
        case .returnKey: .return
        case .tab: .tab
        case .escape: .escape
        case .left: .leftArrow
        case .right: .rightArrow
        case .up: .upArrow
        case .down: .downArrow
        }
        var flags: EventModifiers = []
        if modifiers.contains(.command) { flags.insert(.command) }
        if modifiers.contains(.shift) { flags.insert(.shift) }
        if modifiers.contains(.option) { flags.insert(.option) }
        if modifiers.contains(.control) { flags.insert(.control) }
        return KeyboardShortcut(equivalent, modifiers: flags)
    }
}

/// Window actions for the menu. A window publishes them with `focusedSceneValue`.
struct WindowKeyActions {
    var newChat: (() -> Void)?
    var closeTab: () -> Void
    var lastTab: () -> Void
    var stepRecentChat: (Int) -> Void
    var recentlyClosed: () -> Void
    var newWorkspace: () -> Void
    var newProject: () -> Void
    var stepWorkspace: (Int) -> Void
    var selectTab: (Int) -> Void
    var stepTab: (Int) -> Void
    var toggleSidebar: () -> Void
    var moveSidebar: () -> Void
    var sidebarView: (Int) -> Void
    var showChanges: () -> Void
    /// Opens the command palette.
    var search: () -> Void
}

/// Actions of the visible chat page and its panes.
struct ChatKeyActions {
    var focusComposer: () -> Void
    var moveFocus: (FocusDirection) -> Void
    var zoom: () -> Void
    var stop: () -> Void
}

/// The one place an app key becomes an action, for the menu and the command palette alike.
@MainActor
struct AppKeyTarget {
    /// The target the menu last built. The palette runs actions through it. A window must not
    /// observe the focused chat values itself: they change on every chat render, and observing
    /// them re-rendered the whole window in a loop.
    static var current = AppKeyTarget()

    var window: WindowKeyActions?
    var chat: ChatKeyActions?
    var transcript: TranscriptFindActions?

    func canPerform(_ key: AppKey) -> Bool {
        switch key {
        case .closeTab: window != nil || NSApp.keyWindow != nil
        case .closeWindow: NSApp.keyWindow != nil
        case .find, .findNext, .findPrevious, .moveFocus, .zoom, .focusComposer, .stop: chat != nil
        default: window != nil
        }
    }

    func perform(_ key: AppKey) {
        switch key {
        // Each press starts a chat at once (ADR 0035), so a held key must not start one per repeat.
        // `isARepeat` raises for an event that is not a key event, such as a menu click.
        case .newChat:
            let repeated = NSApp.currentEvent.map { $0.type == .keyDown && $0.isARepeat } ?? false
            if !repeated { window?.newChat?() }
        case .recentlyClosed: window?.recentlyClosed()
        case .newWorkspace: window?.newWorkspace()
        case .newProject: window?.newProject()
        case .nextWorkspace: window?.stepWorkspace(1)
        case .previousWorkspace: window?.stepWorkspace(-1)
        case .closeTab:
            if let window { window.closeTab() }
            else { NSApp.keyWindow?.performClose(nil) }
        case .closeWindow: NSApp.keyWindow?.performClose(nil)
        case .previousRecentChat: window?.stepRecentChat(-1)
        case .nextRecentChat: window?.stepRecentChat(1)
        case .lastTab: window?.lastTab()
        case .selectTab(let number): window?.selectTab(number)
        case .nextTab: window?.stepTab(1)
        case .previousTab: window?.stepTab(-1)
        case .moveFocus(let direction): chat?.moveFocus(direction)
        case .zoom: chat?.zoom()
        case .focusComposer: chat?.focusComposer()
        case .toggleSidebar: window?.toggleSidebar()
        case .moveSidebar: window?.moveSidebar()
        case .sidebarView(let number): window?.sidebarView(number)
        case .showChanges: window?.showChanges()
        case .search: window?.search()
        // The focused child column's transcript, or else the chair's, publishes these.
        case .find: transcript?.open()
        case .findNext: transcript?.next()
        case .findPrevious: transcript?.previous()
        case .stop: chat?.stop()
        }
    }
}

extension FocusedValues {
    @Entry var windowKeyActions: WindowKeyActions?
    @Entry var chatKeyActions: ChatKeyActions?
}

/// Every app key is a menu command, so AppKit handles it before a terminal (docs/decisions/0023).
/// Keys change focus, tabs, and zoom with no animation. An item is disabled only when its window
/// or chat is missing: a key does not refresh a menu item's state, so an item disabled by finer
/// state stayed disabled after that state changed. An action with nothing to act on does nothing.
struct AppKeyCommands: Commands {
    @FocusedValue(\.windowKeyActions) private var window
    @FocusedValue(\.chatKeyActions) private var chat
    @FocusedValue(\.transcriptFindActions) private var transcript

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            item("New Workspace", .newWorkspace)
            item("New Chat", .newChat)
            item("New Project…", .newProject)
            item("Recently closed…", .recentlyClosed)
        }
        CommandGroup(replacing: .saveItem) {
            item("Close Tab", .closeTab)
            item("Close Window", .closeWindow)
        }
        CommandGroup(after: .textEditing) {
            Divider()
            item("Find…", .find)
            item("Find Next", .findNext)
            item("Find Previous", .findPrevious)
        }
        CommandGroup(before: .sidebar) {
            item("Toggle Sidebar", .toggleSidebar)
            item("Move Sidebar to Other Side", .moveSidebar)
            ForEach(Array(WorkspaceSidebarMode.allCases.enumerated()), id: \.offset) { index, mode in
                item("Show \(mode.rawValue)", .sidebarView(index + 1))
            }
            item("Show Changes", .showChanges)
            Divider()
            item("Zoom Pane", .zoom)
            Divider()
        }
        CommandMenu("Navigate") {
            item("Command Palette…", .search)
            Divider()
            item("Next Workspace", .nextWorkspace)
            item("Previous Workspace", .previousWorkspace)
            Divider()
            item("Next Chat", .nextTab)
            item("Previous Chat", .previousTab)
            item("Previous Recent Chat", .previousRecentChat)
            item("Next Recent Chat", .nextRecentChat)
            ForEach(1...8, id: \.self) { index in
                item("Chat \(index)", .selectTab(index))
            }
            Divider()
            item("Last Chat", .lastTab)
            item("Focus Composer", .focusComposer)
            item("Focus Left", .moveFocus(.left))
            item("Focus Right", .moveFocus(.right))
            item("Focus Up", .moveFocus(.up))
            item("Focus Down", .moveFocus(.down))
        }
    }

    private var target: AppKeyTarget {
        let target = AppKeyTarget(window: window, chat: chat, transcript: transcript)
        AppKeyTarget.current = target
        return target
    }

    private func item(_ title: String, _ key: AppKey) -> some View {
        Button(title) { target.perform(key) }
            .keyboardShortcut(key.chord.shortcut)
            .disabled(!target.canPerform(key))
    }
}
