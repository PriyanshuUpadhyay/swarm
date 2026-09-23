import Foundation

/// Reads the bounded start of a chair log to name its session.
public enum ChairLogTitle {
    public static func firstUserPrompt(path: String) -> String? {
        guard let file = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return nil }
        defer { try? file.close() }
        guard let data = try? file.read(upToCount: 256 * 1024) else { return nil }
        for line in data.split(separator: 0x0a) {
            guard let object = (try? JSONSerialization.jsonObject(with: Data(line))) as? [String: Any]
            else { continue }
            let content: Any?
            switch object["type"] as? String {
            case "user":
                content = (object["message"] as? [String: Any])?["content"]
            case "response_item":
                let payload = object["payload"] as? [String: Any]
                content = payload?["role"] as? String == "user" ? payload?["content"] : nil
            default:
                content = nil
            }
            let text: String?
            if let string = content as? String {
                text = string
            } else if let blocks = content as? [[String: Any]] {
                text = blocks.compactMap { $0["text"] as? String }.joined(separator: "\n")
            } else {
                text = nil
            }
            guard var text else { continue }
            text = text.replacingOccurrences(
                of: #"(?s)<command-name>.*?</command-name>"#,
                with: "", options: .regularExpression
            )
            text = text.replacingOccurrences(
                of: #"<[^>]+>"#, with: "", options: .regularExpression
            ).trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty, !text.hasPrefix("# AGENTS.md") else { continue }
            if let firstLine = text.components(separatedBy: .newlines).first(where: {
                !$0.trimmingCharacters(in: .whitespaces).isEmpty
            }) {
                return firstLine.trimmingCharacters(in: .whitespaces)
            }
        }
        return nil
    }
}
