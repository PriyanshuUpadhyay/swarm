import Foundation

/// One key press: the key without modifiers, and the modifiers held.
public struct KeyChord: Sendable, Hashable {
    public enum Key: Sendable, Hashable {
        case character(Character)
        case returnKey, escape, left, right, up, down
    }

    public struct Modifiers: OptionSet, Sendable, Hashable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let command = Modifiers(rawValue: 1)
        public static let shift = Modifiers(rawValue: 2)
        public static let option = Modifiers(rawValue: 4)
        public static let control = Modifiers(rawValue: 8)
    }

    public var key: Key
    public var modifiers: Modifiers

    public init(_ key: Key, _ modifiers: Modifiers = []) {
        self.key = key
        self.modifiers = modifiers
    }

    public init(_ character: Character, _ modifiers: Modifiers = []) {
        self.init(.character(character), modifiers)
    }
}

extension KeyChord {
    /// A test chord from text such as "opt+cmd+right" or "cmd+1"; nil for a word it does not know.
    public init?(script: String) {
        var modifiers: Modifiers = []
        var key: Key?
        for part in script.lowercased().split(separator: "+") {
            switch part {
            case "cmd": modifiers.insert(.command)
            case "shift": modifiers.insert(.shift)
            case "opt": modifiers.insert(.option)
            case "ctrl": modifiers.insert(.control)
            case "return": key = .returnKey
            case "esc": key = .escape
            case "left": key = .left
            case "right": key = .right
            case "up": key = .up
            case "down": key = .down
            case _ where part.count == 1: key = .character(part.first!)
            default: return nil
            }
        }
        guard let key else { return nil }
        self.init(key, modifiers)
    }
}

public enum FocusDirection: Sendable, Hashable, CaseIterable {
    case left, right, up, down
}

/// Every app action that has a key (docs/decisions/0023). The menu takes its keys from `chord`.
public enum AppKey: Sendable, Hashable {
    case newWorkspace, newChat, newProject, nextWorkspace, previousWorkspace
    case selectTab(Int), nextTab, previousTab
    case moveFocus(FocusDirection), zoom, focusComposer
    case toggleSidebar, moveSidebar, sidebarView(Int), showChanges
    case search, find, findNext, findPrevious, stop

    public static let table: [(AppKey, KeyChord)] = [
        (.newWorkspace, KeyChord("n", .command)),
        (.newChat, KeyChord("t", .command)),
        (.newProject, KeyChord("n", [.command, .shift])),
        (.nextWorkspace, KeyChord(.down, [.control, .command])),
        (.previousWorkspace, KeyChord(.up, [.control, .command])),
        (.nextTab, KeyChord("]", [.command, .shift])),
        (.previousTab, KeyChord("[", [.command, .shift])),
        (.moveFocus(.left), KeyChord(.left, [.option, .command])),
        (.moveFocus(.right), KeyChord(.right, [.option, .command])),
        (.moveFocus(.up), KeyChord(.up, [.option, .command])),
        (.moveFocus(.down), KeyChord(.down, [.option, .command])),
        (.zoom, KeyChord(.returnKey, .command)),
        (.focusComposer, KeyChord("l", .command)),
        (.toggleSidebar, KeyChord("b", .command)),
        (.moveSidebar, KeyChord("b", [.command, .shift])),
        (.showChanges, KeyChord("i", [.option, .command])),
        (.search, KeyChord("k", .command)),
        (.find, KeyChord("f", .command)),
        (.findNext, KeyChord("g", .command)),
        (.findPrevious, KeyChord("g", [.command, .shift])),
        (.stop, KeyChord(".", .command)),
    ] + (1...9).map { (.selectTab($0), KeyChord(Character(String($0)), .command)) }
      + (1...5).map { (.sidebarView($0), KeyChord(Character(String($0)), [.option, .command])) }

    public var chord: KeyChord { Self.table.first { $0.0 == self }!.1 }

    public static func action(for chord: KeyChord) -> AppKey? {
        table.first { $0.1 == chord }?.0
    }
}

public struct PaneSearchItem: Sendable, Equatable, Identifiable {
    public var id: String
    public var text: String

    public init(id: String, text: String) {
        self.id = id
        self.text = text
    }
}

public enum PaneSearch {
    public static func matches(query: String, in items: some Sequence<PaneSearchItem>) -> [String] {
        guard !query.isEmpty else { return [] }
        return items.compactMap {
            $0.text.localizedCaseInsensitiveContains(query) ? $0.id : nil
        }
    }

    public static func step(current: Int?, count: Int, delta: Int) -> Int? {
        guard count > 0 else { return nil }
        guard let current else { return delta < 0 ? count - 1 : 0 }
        return (current + delta % count + count) % count
    }

    public static func reconcile(
        current: Int?, previousMatches: [String], newMatches: [String]
    ) -> Int? {
        guard !newMatches.isEmpty else { return nil }
        guard let current else { return 0 }
        if previousMatches.indices.contains(current),
           let preserved = newMatches.firstIndex(of: previousMatches[current]) {
            return preserved
        }
        return min(current, newMatches.count - 1)
    }
}
