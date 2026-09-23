import Foundation

public enum FocusedSurface: Sendable {
    case terminal, transcript, composer, sidebar
}

public enum RoutedKey: Sendable, Equatable {
    case escape, `return`, shiftReturn, commandN, commandF, commandG, shiftCommandG, other
}

public enum KeyRoute: Sendable, Equatable {
    case terminal, clearComposer, sendComposer, insertNewline, openNewChat
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
        case (.composer, .escape): .clearComposer
        case (.composer, .return): .sendComposer
        case (.composer, .shiftReturn): .insertNewline
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

public enum Composer {
    /// The text that leaves the box, or nil when nothing should be sent.
    public static func outgoing(_ draft: String) -> String? {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }
}

public struct ComposerSendState: Sendable, Equatable {
    private var submittedDraft: String?

    public init() {}

    public var isSending: Bool { submittedDraft != nil }

    public mutating func begin(_ draft: String) -> String? {
        guard submittedDraft == nil, let message = Composer.outgoing(draft) else { return nil }
        submittedDraft = draft
        return message
    }

    public mutating func finish(currentDraft: String, succeeded: Bool) -> String {
        defer { submittedDraft = nil }
        guard succeeded, currentDraft == submittedDraft else { return currentDraft }
        return ""
    }
}
