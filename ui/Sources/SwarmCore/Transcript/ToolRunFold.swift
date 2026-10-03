import Foundation

/// Folds each run of 3 or more adjacent finished tool rows in an ended turn into one item.
public enum ToolRunFold {
    public enum Item: Hashable, Sendable, Identifiable {
        case row(TranscriptRow)
        /// The folded tool rows, in transcript order.
        case fold([TranscriptRow])

        /// A fold takes its first row's id.
        public var id: String {
            switch self {
            case .row(let row): row.eventID
            case .fold(let rows): rows[0].eventID
            }
        }
    }

    /// The rows in transcript order, each fold group replaced by one `.fold` item. Any other row,
    /// a tool that did not finish, or a row id in `pinned` ends a run and stays its own row.
    public static func items(in rows: [TranscriptRow], pinned: Set<String>) -> [Item] {
        let ended = endedTurnFlags(rows)
        var items: [Item] = []
        var run: [TranscriptRow] = []
        func flush() {
            if run.count >= 3 { items.append(.fold(run)) } else { items += run.map(Item.row) }
            run = []
        }
        for (index, row) in rows.enumerated() {
            if ended[index], row.tool?.state == .finished, !pinned.contains(row.eventID) {
                run.append(row)
            } else {
                flush()
                items.append(.row(row))
            }
        }
        flush()
        return items
    }

    /// The ids of tool rows whose turn has no ending row yet. A view pins them, so a row it showed
    /// while its turn ran never folds by itself.
    public static func openTurnToolIDs(in rows: [TranscriptRow]) -> Set<String> {
        let ended = endedTurnFlags(rows)
        return Set(rows.indices.filter { !ended[$0] && rows[$0].tool != nil }.map { rows[$0].eventID })
    }

    /// Tool names with their counts, in first-seen order.
    public static func nameCounts(_ rows: [TranscriptRow]) -> [(name: String, count: Int)] {
        var counts: [(name: String, count: Int)] = []
        for name in rows.compactMap(\.tool?.name) {
            if let index = counts.firstIndex(where: { $0.name == name }) {
                counts[index].count += 1
            } else {
                counts.append((name, 1))
            }
        }
        return counts
    }

    /// "Read ×2 · Grep · Edit".
    public static func summary(of rows: [TranscriptRow]) -> String {
        nameCounts(rows).map { $0.count > 1 ? "\($0.name) ×\($0.count)" : $0.name }.joined(separator: " · ")
    }

    /// The sum of the tool durations; nil when any duration is unknown.
    public static func totalDuration(of rows: [TranscriptRow]) -> Double? {
        var total = 0.0
        for row in rows {
            guard let duration = row.tool?.duration else { return nil }
            total += duration
        }
        return total
    }

    /// Whether each row sits in a turn whose ending row follows it before the next turn starts.
    /// Turn starts and endings are the ones `ChairTurn.isActive` reads.
    private static func endedTurnFlags(_ rows: [TranscriptRow]) -> [Bool] {
        var flags = Array(repeating: false, count: rows.count)
        var endingAhead = false
        for index in rows.indices.reversed() {
            if rows[index].endsTurn { endingAhead = true }
            flags[index] = endingAhead
            if rows[index].kind == .user || rows[index].startsTurn { endingAhead = false }
        }
        return flags
    }
}
