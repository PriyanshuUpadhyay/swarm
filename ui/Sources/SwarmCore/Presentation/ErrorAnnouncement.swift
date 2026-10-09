public struct ErrorAnnouncement: Equatable, Sendable {
    public let text: String?
    public let revision: Int

    public init(messages: [String?], revision: Int) {
        let text = messages.compactMap { $0 }.joined(separator: " ")
        self.text = text.isEmpty ? nil : text
        self.revision = revision
    }
}
