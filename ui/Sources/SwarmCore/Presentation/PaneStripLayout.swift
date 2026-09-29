import Foundation

/// The chat page and agent columns of the horizontal pane strip (ADR 0022).
public enum PaneStripLayout {
    /// About 60 terminal columns at 12 pt.
    public static let minimumColumnWidth: CGFloat = 440

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

    /// The chat page leaves the last 10% of the main area to the first agent column.
    public static func widths(main: CGFloat) -> (chat: CGFloat, column: CGFloat) {
        (main * 0.9, max(main / 3, minimumColumnWidth))
    }
}
