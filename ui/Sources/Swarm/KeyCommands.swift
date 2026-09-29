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
    var newWorkspace: () -> Void
    var stepWorkspace: (Int) -> Void
    var selectTab: (Int) -> Void
    var stepTab: (Int) -> Void
    var toggleSidebar: () -> Void
    var moveSidebar: () -> Void
    var sidebarView: (Int) -> Void
    var showChanges: () -> Void
    var search: () -> Void
}

/// Actions of the visible chat page and its panes.
struct ChatKeyActions {
    var terminalFocused: Bool
    var open: () -> Void
    var next: () -> Void
    var previous: () -> Void
    var focusComposer: () -> Void
    var moveFocus: (FocusDirection) -> Void
    var zoom: () -> Void
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

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            item("New Chat", .newChat, enabled: window != nil) { window?.newChat?() }
            item("New Workspace", .newWorkspace, enabled: window != nil) { window?.newWorkspace() }
        }
        CommandGroup(after: .textEditing) {
            Divider()
            item("Find…", .find, enabled: chat != nil) { route(.find, action: .showFindPanel) }
            item("Find Next", .findNext, enabled: chat != nil) { route(.findNext, action: .next) }
            item("Find Previous", .findPrevious, enabled: chat != nil) {
                route(.findPrevious, action: .previous)
            }
        }
        CommandGroup(before: .sidebar) {
            item("Toggle Sidebar", .toggleSidebar, enabled: window != nil) { window?.toggleSidebar() }
            item("Move Sidebar to Other Side", .moveSidebar, enabled: window != nil) { window?.moveSidebar() }
            ForEach(Array(WorkspaceSidebarMode.allCases.enumerated()), id: \.offset) { index, mode in
                item("Show \(mode.rawValue)", .sidebarView(index + 1), enabled: window != nil) {
                    window?.sidebarView(index + 1)
                }
            }
            item("Show Changes", .showChanges, enabled: window != nil) { window?.showChanges() }
            Divider()
            item("Zoom Pane", .zoom, enabled: chat != nil) { chat?.zoom() }
            Divider()
        }
        CommandMenu("Navigate") {
            item("Search Workspaces", .search, enabled: window != nil) { window?.search() }
            Divider()
            item("Next Workspace", .nextWorkspace, enabled: window != nil) { window?.stepWorkspace(1) }
            item("Previous Workspace", .previousWorkspace, enabled: window != nil) { window?.stepWorkspace(-1) }
            Divider()
            item("Next Chat", .nextTab, enabled: window != nil) { window?.stepTab(1) }
            item("Previous Chat", .previousTab, enabled: window != nil) { window?.stepTab(-1) }
            ForEach(1...9, id: \.self) { index in
                item("Chat \(index)", .selectTab(index), enabled: window != nil) { window?.selectTab(index) }
            }
            Divider()
            item("Focus Composer", .focusComposer, enabled: chat != nil) { chat?.focusComposer() }
            item("Focus Left", .moveFocus(.left), enabled: chat != nil) { chat?.moveFocus(.left) }
            item("Focus Right", .moveFocus(.right), enabled: chat != nil) { chat?.moveFocus(.right) }
            item("Focus Up", .moveFocus(.up), enabled: chat != nil) { chat?.moveFocus(.up) }
            item("Focus Down", .moveFocus(.down), enabled: chat != nil) { chat?.moveFocus(.down) }
        }
    }

    private func item(
        _ title: String, _ key: AppKey, enabled: Bool, action: @escaping () -> Void
    ) -> some View {
        Button(title, action: action)
            .keyboardShortcut(key.chord.shortcut)
            .disabled(!enabled)
    }

    private func route(_ key: AppKey, action: NSFindPanelAction) {
        let focus: FocusedSurface = chat?.terminalFocused == false ? .transcript : .terminal
        switch KeyRouting.route(focus: focus, key: key.chord) {
        case .app(.find): chat?.open()
        case .app(.findNext): chat?.next()
        case .app(.findPrevious): chat?.previous()
        case .terminal:
            let item = NSMenuItem()
            item.tag = Int(action.rawValue)
            NSApp.sendAction(
                #selector(NSTextView.performFindPanelAction(_:)), to: nil, from: item
            )
        default: break
        }
    }
}
