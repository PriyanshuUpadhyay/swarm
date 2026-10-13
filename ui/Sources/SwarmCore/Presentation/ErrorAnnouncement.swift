public enum ErrorText {
    public static func sentence(_ text: String) -> String {
        guard let last = text.last else { return "" }
        return ".?!…".contains(last) ? text : text + "."
    }
}

public enum ErrorAnnouncement {
    public static func joined(_ messages: [String?]) -> String? {
        let sentences = messages.compactMap { $0 }.filter { !$0.isEmpty }.map(ErrorText.sentence)
        return sentences.isEmpty ? nil : sentences.joined(separator: " ")
    }
}
