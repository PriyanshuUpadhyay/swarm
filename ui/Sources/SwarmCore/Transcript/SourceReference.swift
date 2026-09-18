import Foundation

/// Internal source links never pass through NSWorkspace, even when the reference names an app bundle.
public enum SourceReference {
    public static func url(_ reference: String) -> URL? {
        var reference = reference
        if reference.hasPrefix("file://"), let file = URL(string: reference), file.isFileURL {
            reference = file.path + (file.fragment.map { "#\($0)" } ?? "")
        }
        guard !reference.contains("://"), !reference.hasPrefix("mailto:"), !reference.contains("\n") else { return nil }
        let location = CodeLocation.parse(reference)
        guard location.path.range(of: #"^[\p{L}\p{N}_./ ~-]+$"#, options: .regularExpression) != nil else { return nil }
        let filename = (location.path as NSString).lastPathComponent
        guard filename.contains("."), filename.contains(where: { $0.isLetter }),
              !location.path.contains(":"), !location.path.contains("#") else { return nil }
        var components = URLComponents()
        components.scheme = "swarm-source"
        components.host = "file"
        components.queryItems = [URLQueryItem(name: "path", value: location.path),
                                URLQueryItem(name: "line", value: String(location.line)),
                                URLQueryItem(name: "column", value: String(location.column))]
        return components.url
    }

    public static func location(_ url: URL) -> CodeLocation? {
        guard url.scheme == "swarm-source", url.host == "file",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let path = items.first(where: { $0.name == "path" })?.value, !path.isEmpty else { return nil }
        return CodeLocation(path: path, line: Int(items.first(where: { $0.name == "line" })?.value ?? "") ?? 1,
                              column: Int(items.first(where: { $0.name == "column" })?.value ?? "") ?? 1)
    }

    /// Two shapes, and the line suffix is only required of one of them.
    ///
    /// A relative path has to carry `:42` or `#L12` to be a link, because the prose these scan is
    /// full of `Package.swift` and `README.md` written as words in a sentence, and a detector that
    /// took those would underline half the transcript. An absolute path carries its own evidence:
    /// `/private/tmp/councils/depth-19d43a07/claude.md` is never a turn of phrase, so it is a link
    /// with no suffix at all. That is what makes a bus summary naming a report openable, and it is
    /// why a version number like `2.1.275` is still left alone — it does not start at `/` or `~/`.
    public static func links(in text: String) -> [(NSRange, URL)] {
        let pattern = #"(?<![\w@:/])(?:[\w./-]+\.[\w]+(?::\d+(?::\d+)?|#L\d+(?:-L?\d+)?)|~?/[\w./-]*[\w-]\.[\w]+)"#
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return [] }
        return regex.matches(in: text, range: NSRange(text.startIndex..., in: text)).compactMap { match in
            guard let url = url((text as NSString).substring(with: match.range)) else { return nil }
            return (match.range, url)
        }
    }
}
