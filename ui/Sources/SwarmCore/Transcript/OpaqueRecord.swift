import Foundation

/// A transcript line no reader has a case for, kept rather than dropped.
///
/// ## Why this type exists
///
/// A CLI's session file grows types faster than any client writes code for them. Across 446 real
/// Claude Code captures there are 22 distinct top-level record types, and two of them,
/// `file-history-delta` and `pr-link`, first appear in a single month. The reader understood two,
/// `user` and `assistant`, and returned nothing for the rest, so a feature the reader could watch
/// happen in the terminal left no trace at all in the chat. `/compact` was the one that was
/// noticed; it was never the only one.
///
/// The rule this type enforces is that no `default:` drops a row. A line that nothing claims
/// becomes one collapsed row carrying its own bytes. Nobody has to guess what it means, and
/// nothing has to ship before it is visible.
///
/// ## What it must never do
///
/// It never renders as speech. An opaque row says a record arrived and shows the record; it does
/// not put unread bytes in the agent's voice or the reader's. It also never replays a control: a
/// stored permission request is history, and answering it belongs to the live hook.
public struct OpaqueRecord: Sendable, Hashable {
    /// What to call the row, in two or three words, such as `pr-link` or `local command`.
    public var title: String
    /// One line of the content, for the collapsed row. Empty when the record has no obvious text.
    public var summary: String
    /// The whole line, laid out for reading when the row is opened.
    public var detail: String

    public init(title: String, summary: String, detail: String) {
        self.title = title
        self.summary = summary
        self.detail = detail
    }

    /// The tags Claude Code wraps around a line the reader never typed, and what to call each.
    ///
    /// These arrive as ordinary `user` records, which is why they used to be hidden by prefix
    /// rather than by type. Naming them here is what turns `/compact` from a bare unexplained
    /// message into a row that says what it was.
    static let scaffoldingTitles: [(tag: String, title: String)] = [
        ("<local-command-caveat>", "local command"),
        ("<local-command-stdout>", "command output"),
        ("<command-name>", "local command"),
        ("<command-message>", "local command"),
        ("<command-args>", "local command"),
        ("<system-reminder>", "system reminder"),
        ("<task-notification>", "task notification"),
        ("<bash-input>", "bash"),
        ("<bash-stdout>", "bash output"),
    ]

    /// Read one stored line into the row a pane can draw.
    ///
    /// Returns nil only when the bytes are not JSON at all, because a row with nothing readable in
    /// it is worse than no row.
    public static func read(_ payload: Data) -> OpaqueRecord? {
        guard let json = JSONValue.parse(payload) else { return nil }
        let text = firstText(in: json)
        return OpaqueRecord(
            title: name(for: json, text: text),
            summary: summarise(text),
            detail: pretty(payload)
        )
    }

    private static func name(for json: JSONValue, text: String?) -> String {
        if let text, let match = scaffoldingTitles.first(where: { text.hasPrefix($0.tag) }) {
            return match.title
        }
        if json["isMeta"]?.boolValue == true { return "meta" }
        // A block that no case claimed keeps the block's own name, which is the only word that
        // says which part of the line was not understood.
        if let block = json["message"]?["content"]?.arrayValue?.first,
           let kind = block["type"]?.stringValue {
            return kind
        }
        return json["type"]?.stringValue ?? "record"
    }

    /// The first readable string in the line, wherever the provider chose to put it.
    private static func firstText(in json: JSONValue) -> String? {
        if let content = json["message"]?["content"] {
            if let direct = content.stringValue { return direct }
            if let blocks = content.arrayValue {
                for block in blocks {
                    if let value = block["text"]?.stringValue, !value.isEmpty { return value }
                }
            }
        }
        for key in ["text", "content", "summary", "message", "title", "url"] {
            if let value = json[key]?.stringValue, !value.isEmpty { return value }
        }
        return nil
    }

    /// One line, with the wrapper tag taken off the front so the row shows the words inside it.
    private static func summarise(_ text: String?) -> String {
        guard var text else { return "" }
        for entry in scaffoldingTitles where text.hasPrefix(entry.tag) {
            text = String(text.dropFirst(entry.tag.count))
            break
        }
        let line = text
            .split(whereSeparator: \.isNewline)
            .first { $0.contains { !$0.isWhitespace } }
            .map(String.init) ?? ""
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.count > summaryLimit
            ? String(trimmed.prefix(summaryLimit)) + "…"
            : trimmed
    }

    private static let summaryLimit = 120
    /// Enough to read what arrived, and not so much that one unknown record is a document. The
    /// row is a receipt, and the file on disk is still the whole truth.
    private static let detailLimit = 8_000

    private static func pretty(_ payload: Data) -> String {
        let text: String
        if let object = try? JSONSerialization.jsonObject(with: payload),
           let laid = try? JSONSerialization.data(
               withJSONObject: object, options: [.prettyPrinted, .sortedKeys]
           ),
           let value = String(data: laid, encoding: .utf8) {
            text = value
        } else {
            text = String(decoding: payload, as: UTF8.self)
        }
        return text.count > detailLimit ? String(text.prefix(detailLimit)) + "\n…" : text
    }
}
