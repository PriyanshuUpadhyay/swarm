import Foundation

public enum FocusedSurface: Sendable {
    case terminal, transcript, composer, sidebar
}

public enum RoutedKey: Sendable, Equatable {
    case escape, `return`, shiftReturn, commandN, other
}

public enum KeyRoute: Sendable, Equatable {
    case terminal, clearComposer, sendComposer, insertNewline, openNewChat, ignore
}

public enum KeyRouting {
    public static func route(focus: FocusedSurface, key: RoutedKey) -> KeyRoute {
        if key == .commandN { return .openNewChat }
        return switch (focus, key) {
        case (.terminal, _): .terminal
        case (.composer, .escape): .clearComposer
        case (.composer, .return): .sendComposer
        case (.composer, .shiftReturn): .insertNewline
        default: .ignore
        }
    }
}

public enum Composer {
    /// The text that leaves the box, or nil when nothing should be sent.
    public static func outgoing(_ draft: String) -> String? {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }
}
