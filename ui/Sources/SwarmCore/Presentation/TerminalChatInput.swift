public enum TerminalChatInput {
    public enum Write: Sendable, Equatable {
        case text(String)
        case key(TerminalKey)
    }

    /// A CLI reads text and Return in one burst as one paste, so Return must arrive separately.
    public static let interWriteDelay: Duration = .milliseconds(150)

    public static func submission(_ text: String) -> [Write] {
        [.text(text), .key(.enter)]
    }
}
