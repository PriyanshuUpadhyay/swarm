public struct ErrorAnnouncement: Equatable, Sendable {
    public let text: String?
    public let revision: Int

    public init(messages: [String?], revision: Int) {
        let text = messages.compactMap { $0 }.filter { !$0.isEmpty }.reduce("") { previous, message in
            guard !previous.isEmpty else { return message }
            return previous + (previous.hasSuffix(".") ? " " : ". ") + message
        }
        self.text = text.isEmpty ? nil : text
        self.revision = revision
    }
}
