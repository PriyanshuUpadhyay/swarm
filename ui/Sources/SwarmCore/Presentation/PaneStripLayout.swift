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

    /// The chat page leaves the last 10% of the main area to the first agent column.
    public static func widths(main: CGFloat) -> (chat: CGFloat, column: CGFloat) {
        (main * 0.9, max(main / 3, minimumColumnWidth))
    }
}
