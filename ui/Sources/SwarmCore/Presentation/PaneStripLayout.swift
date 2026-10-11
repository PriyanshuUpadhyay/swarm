import Foundation

/// The chat page and agent columns of the horizontal pane strip (ADR 0030).
public enum PaneStripLayout {
    /// About 60 terminal columns at 12 pt.
    public static let minimumColumnWidth: CGFloat = 440
    public static let minimumChatWidth: CGFloat = 400

    /// Pane indexes per column. An odd count starts with one full-height pane; the rest pair up.
    public static func columns(count: Int) -> [[Int]] {
        guard count > 0 else { return [] }
        var columns: [[Int]] = count.isMultiple(of: 2) ? [] : [[0]]
        var index = count % 2
        while index < count {
            columns.append([index, index + 1])
            index += 2
        }
        return columns
    }

    public enum Focus: Sendable, Hashable {
        case chat, pane(Int)
    }

    /// Spatial focus: left and right cross columns, the chat page being left of the first;
    /// up and down stay in a column. At an edge the focus stays where it is.
    public static func move(from focus: Focus, count: Int, direction: FocusDirection) -> Focus {
        let columns = columns(count: count)
        guard case .pane(let index) = focus,
              let column = columns.firstIndex(where: { $0.contains(index) }) else {
            return direction == .right && !columns.isEmpty ? .pane(columns[0][0]) : .chat
        }
        let row = columns[column].firstIndex(of: index)!
        func pane(_ target: Int) -> Focus { .pane(columns[target][min(row, columns[target].count - 1)]) }
        switch direction {
        case .left: return column == 0 ? .chat : pane(column - 1)
        case .right: return column + 1 < columns.count ? pane(column + 1) : focus
        case .up: return .pane(columns[column][max(row - 1, 0)])
        case .down: return .pane(columns[column][min(row + 1, columns[column].count - 1)])
        }
    }

    /// With no drag the chat keeps its 90% default. A drag leaves 440 pt for the first column.
    /// Below 840 pt the 400 pt chat minimum wins and the strip scrolls (ADR 0054).
    public static func widths(main: CGFloat, chat: CGFloat? = nil) -> (chat: CGFloat, column: CGFloat) {
        let width = chat.map { min(max($0, minimumChatWidth), max(minimumChatWidth, main - minimumColumnWidth)) } ?? main * 0.9
        return (width, columnWidth(main: main, preferred: nil))
    }

    /// The owner's dragged column width, kept between 440 pt and 90% of the main area (ADR 0026).
    /// With no drag a column is a third of the main area. The minimum wins in a narrow window.
    public static func columnWidth(main: CGFloat, preferred: CGFloat?) -> CGFloat {
        guard let preferred else { return max(main / 3, minimumColumnWidth) }
        return max(min(preferred, main * 0.9), minimumColumnWidth)
    }

    /// The top pane's share of a two-pane column, kept between 25% and 75%.
    public static func split(preferred: Double?) -> Double {
        min(max(preferred ?? 0.5, 0.25), 0.75)
    }

    /// The chats whose splits are kept; saving one more drops the chat saved longest ago.
    public static let maximumSplitScopes = 50

    /// One chat's saved splits, keyed by each two-pane column's first agent id.
    private struct SplitScope: Codable {
        let scope: String
        let splits: [String: Double]
    }

    /// Splits are saved per chat (the scope) as one JSON array, the chat saved last at the end.
    /// Unreadable text, and the older format without a scope, read as no splits.
    public static func splits(from text: String, scope: String) -> [String: Double] {
        scopes(from: text).last { $0.scope == scope }?.splits ?? [:]
    }

    /// Replaces the chat's splits with those of columns still shown and moves the chat to the end.
    /// Other chats keep theirs, up to `maximumSplitScopes` chats, so the text stays small.
    public static func text(saving splits: [String: Double], scope: String, keeping ids: Set<String>,
                            in text: String) -> String {
        var scopes = scopes(from: text).filter { $0.scope != scope }
        let kept = splits.filter { ids.contains($0.key) }
        if !kept.isEmpty { scopes.append(SplitScope(scope: scope, splits: kept)) }
        guard let data = try? JSONEncoder().encode(Array(scopes.suffix(maximumSplitScopes))) else { return "" }
        return String(decoding: data, as: UTF8.self)
    }

    private static func scopes(from text: String) -> [SplitScope] {
        (try? JSONDecoder().decode([SplitScope].self, from: Data(text.utf8))) ?? []
    }
}
