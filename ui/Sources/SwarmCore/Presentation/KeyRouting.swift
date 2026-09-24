import Foundation

public enum FocusedSurface: Sendable {
    case terminal, transcript, sidebar
}

public enum RoutedKey: Sendable, Equatable {
    case escape, commandN, commandF, commandG, shiftCommandG, other
}

public enum KeyRoute: Sendable, Equatable {
    case terminal, openNewChat
    case openFind, findNext, findPrevious, ignore
}

public enum KeyRouting {
    public static func route(focus: FocusedSurface, key: RoutedKey) -> KeyRoute {
        if key == .commandN { return .openNewChat }
        return switch (focus, key) {
        case (.terminal, _): .terminal
        case (.transcript, .commandF): .openFind
        case (.transcript, .commandG): .findNext
        case (.transcript, .shiftCommandG): .findPrevious
        default: .ignore
        }
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
