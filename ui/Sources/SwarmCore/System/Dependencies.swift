import Foundation

public struct DependencyRow: Sendable, Hashable, Identifiable {
    public let name: String
    public let path: String?
    public let installHint: String
    public var id: String { name }
}

public enum Dependencies {
    public static func check(lookup: (String) -> String?) -> [DependencyRow] {
        let dependencies = [
            ("tmux", "Install tmux with Homebrew: brew install tmux"),
            ("claude", "Install Claude Code from code.claude.com/docs/en/setup"),
            ("codex", "Install Codex with Homebrew: brew install --cask codex"),
            ("agy", "Install AGY from antigravity.google"),
            ("gh", "Install GitHub CLI with Homebrew: brew install gh"),
            ("yelo", "Install yelo and add it to PATH"),
        ]
        return dependencies.map { DependencyRow(name: $0.0, path: lookup($0.0), installHint: $0.1) }
    }
}
