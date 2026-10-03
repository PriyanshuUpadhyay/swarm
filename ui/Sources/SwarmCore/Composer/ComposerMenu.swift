import Foundation

/// A completion token measured in UTF-16, the same unit NSString uses for edits.
public struct ComposerToken: Equatable, Sendable {
    public var start: Int
    public var length: Int
    public var query: String
    /// The token ends where the draft ends.
    public var reachesDraftEnd: Bool

    public init(start: Int, length: Int, query: String, reachesDraftEnd: Bool = false) {
        self.start = start
        self.length = length
        self.query = query
        self.reachesDraftEnd = reachesDraftEnd
    }

    public var end: Int { start + length }
}

public enum ComposerMenuScope: Sendable, Equatable {
    case all
    /// Claude runs a built-in command only at the start of a message.
    case skillsAndCommands
}

/// Adapted from Bloom's ComposerMenu. It keeps menu state derived from text, not view state.
public enum ComposerMenu: Equatable, Sendable {
    case none
    case slash(ComposerToken, ComposerMenuScope)
    case mention(ComposerToken)
    /// A Codex `$skill` reference.
    case skill(ComposerToken)

    public var token: ComposerToken? {
        switch self {
        case .none: nil
        case .slash(let token, _), .mention(let token), .skill(let token): token
        }
    }

    /// Return sends a slash command that is the whole draft, as the CLIs run it.
    public var isWholeDraft: Bool {
        guard case .slash(let token, _) = self else { return false }
        return token.start == 0 && token.reachesDraftEnd
    }

    /// Mid-draft a `/` or `$` can be a path or a price, so that menu shows only with rows.
    public var showsWhenEmpty: Bool {
        switch self {
        case .slash(_, .all), .mention: true
        case .none, .slash(_, .skillsAndCommands), .skill: false
        }
    }

    /// The text a pick puts in for a command.
    public func insertion(for command: ComposerCommand) -> String {
        guard case .skill = self else { return "/" + command.name }
        return "$" + command.name
    }

    /// The commands this menu can offer.
    public func offered(_ commands: [ComposerCommand]) -> [ComposerCommand] {
        switch self {
        case .slash(_, .all): commands
        case .slash(_, .skillsAndCommands): commands.filter { $0.kind != .builtIn }
        case .skill: commands.filter(\.isSkill)
        case .none, .mention: []
        }
    }

    public static func resolve(draft: String, caret: Int, provider: String?) -> ComposerMenu {
        let isCodex = provider?.lowercased() == "codex"
        if let token = token(in: draft, caret: caret, opener: "/") {
            if token.start == 0 { return .slash(token, .all) }
            return isCodex ? .none : .slash(token, .skillsAndCommands)
        }
        if let token = token(in: draft, caret: caret, opener: "@") {
            return .mention(token)
        }
        if isCodex, let token = token(in: draft, caret: caret, opener: "$") {
            return .skill(token)
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
            start: found.location, length: location - found.location, query: query,
            reachesDraftEnd: location == text.length
        )
    }

    private static func beginsWord(in text: NSString, at location: Int) -> Bool {
        let previous = text.substring(with: NSRange(location: location - 1, length: 1))
        return [" ", "\n", "\t", "(", "["].contains(previous)
    }
}

private extension ComposerCommand {
    /// Plugin skills and plugin commands share a kind, so a skill is known by its SKILL.md.
    var isSkill: Bool {
        switch kind {
        case .skill: true
        case .plugin: path?.hasSuffix("/SKILL.md") == true
        case .builtIn, .command: false
        }
    }
}

public enum ComposerInputKey: Sendable, Equatable {
    case up, down, `return`, tab, escape, shiftReturn
}

public enum ComposerKeyAction: Sendable, Equatable {
    case move(Int), pick, pickAndSend, dismissMenu, clear, send, insertNewline, pullBack
}

public enum ComposerKeyRouter {
    public static func route(
        _ key: ComposerInputKey, menu: ComposerMenu, menuOpen: Bool, hasRows: Bool,
        draftIsEmpty: Bool, canPullBack: Bool
    ) -> ComposerKeyAction {
        if menuOpen {
            return switch key {
            case .up: .move(hasRows ? -1 : 0)
            case .down: .move(hasRows ? 1 : 0)
            case .return: hasRows ? (menu.isWholeDraft ? .pickAndSend : .pick) : .send
            case .tab: hasRows ? .pick : .move(0)
            case .escape: .dismissMenu
            case .shiftReturn: .insertNewline
            }
        }
        return switch key {
        case .return: .send
        case .shiftReturn: .insertNewline
        case .escape: .clear
        case .up: draftIsEmpty && canPullBack ? .pullBack : .move(0)
        case .down, .tab: .move(0)
        }
    }

    public static func movedSelection(current: Int, count: Int, delta: Int) -> Int {
        guard count > 0 else { return 0 }
        return (current + delta % count + count) % count
    }
}
