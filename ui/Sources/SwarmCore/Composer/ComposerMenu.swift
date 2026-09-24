import Foundation

/// A completion token measured in UTF-16, the same unit NSString uses for edits.
public struct ComposerToken: Equatable, Sendable {
    public var start: Int
    public var length: Int
    public var query: String

    public init(start: Int, length: Int, query: String) {
        self.start = start
        self.length = length
        self.query = query
    }

    public var end: Int { start + length }
}

/// Adapted from Bloom's ComposerMenu. It keeps menu state derived from text, not view state.
public enum ComposerMenu: Equatable, Sendable {
    case none
    case slash(ComposerToken)
    case mention(ComposerToken)

    public var token: ComposerToken? {
        switch self {
        case .none: nil
        case .slash(let token), .mention(let token): token
        }
    }

    public static func resolve(draft: String, caret: Int) -> ComposerMenu {
        if let token = token(in: draft, caret: caret, opener: "/"), token.start == 0 {
            return .slash(token)
        }
        if let token = token(in: draft, caret: caret, opener: "@") {
            return .mention(token)
        }
        return .none
    }

    public static func inserting(_ value: String, into draft: String, token: ComposerToken) -> String {
        let text = draft as NSString
        let end = min(token.end, text.length)
        let range = NSRange(location: token.start, length: end - token.start)
        let next = end < text.length
            ? text.substring(with: NSRange(location: end, length: 1)) : ""
        let hasSeparator = next.rangeOfCharacter(from: .whitespacesAndNewlines) != nil
        return text.replacingCharacters(in: range, with: value + (hasSeparator ? "" : " "))
    }

    private static func token(in draft: String, caret: Int, opener: String) -> ComposerToken? {
        let text = draft as NSString
        let location = min(max(caret, 0), text.length)
        guard location > 0 else { return nil }
        let before = text.substring(to: location) as NSString
        let found = before.range(of: opener, options: .backwards)
        guard found.location != NSNotFound else { return nil }
        let query = before.substring(from: found.location + 1)
        guard !query.contains(where: { $0.isWhitespace }) else { return nil }
        guard found.location == 0 || beginsWord(in: before, at: found.location) else { return nil }
        return ComposerToken(
            start: found.location, length: location - found.location, query: query
        )
    }

    private static func beginsWord(in text: NSString, at location: Int) -> Bool {
        let previous = text.substring(with: NSRange(location: location - 1, length: 1))
        return [" ", "\n", "\t", "(", "["].contains(previous)
    }
}

public enum ComposerInputKey: Sendable, Equatable {
    case up, down, `return`, tab, escape, shiftReturn
}

public enum ComposerKeyAction: Sendable, Equatable {
    case move(Int), pick, dismissMenu, clear, send, insertNewline
}

public enum ComposerKeyRouter {
    public static func route(
        _ key: ComposerInputKey, menuOpen: Bool, hasRows: Bool
    ) -> ComposerKeyAction {
        if menuOpen {
            return switch key {
            case .up: .move(hasRows ? -1 : 0)
            case .down: .move(hasRows ? 1 : 0)
            case .return: hasRows ? .pick : .send
            case .tab: hasRows ? .pick : .move(0)
            case .escape: .dismissMenu
            case .shiftReturn: .insertNewline
            }
        }
        return switch key {
        case .return: .send
        case .shiftReturn: .insertNewline
        case .escape: .clear
        case .up, .down, .tab: .move(0)
        }
    }

    public static func movedSelection(current: Int, count: Int, delta: Int) -> Int {
        guard count > 0 else { return 0 }
        return (current + delta % count + count) % count
    }
}
