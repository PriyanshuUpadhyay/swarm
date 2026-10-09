public struct ErrorAnnouncement: Equatable, Sendable {
    public let text: String?
    public let revision: Int

    public init(messages: [String?], revision: Int) {
        self.text = Self.joined(messages)
        self.revision = revision
    }

    public static func joined(_ messages: [String?]) -> String? {
        let text = messages.compactMap { $0 }.filter { !$0.isEmpty }.reduce("") { previous, message in
            guard !previous.isEmpty else { return message }
            return previous + (previous.hasSuffix(".") ? " " : ". ") + message
        }
        return text.isEmpty ? nil : text
    }
}
