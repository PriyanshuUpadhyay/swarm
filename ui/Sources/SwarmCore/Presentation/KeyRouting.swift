public enum FocusedSurface: Sendable {
    case terminal, transcript, composer, sidebar
}

public enum RoutedKey: Sendable, Equatable {
    case escape, `return`, commandN, other
}

public enum KeyRoute: Sendable, Equatable {
    case terminal, clearComposer, sendComposer, openNewChat, ignore
}

public enum KeyRouting {
    public static func route(focus: FocusedSurface, key: RoutedKey) -> KeyRoute {
        if key == .commandN { return .openNewChat }
        return switch (focus, key) {
        case (.terminal, _): .terminal
        case (.composer, .escape): .clearComposer
        case (.composer, .return): .sendComposer
        default: .ignore
        }
    }
}
