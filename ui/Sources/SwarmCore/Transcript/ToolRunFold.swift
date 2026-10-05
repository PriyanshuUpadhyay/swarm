import Foundation

/// Folds each run of steps between two prose rows into one item (ADR 0047). A step is a tool row in
/// any state, a ring that arrived mid-turn, a thought, or a row hidden by default; every other row is
/// prose. A run folds when it holds 2 or more shown steps and at least 1 tool row. The turn state
/// does not matter, so the trailing run of a running turn is a live fold: a new step changes the
/// fold's text, not its height, and no group of rows on screen ever collapses (ADR 0028): a run that
/// drew 2 or more rows before its first tool landed opens when it folds. The one other change of
/// height is a step that fails in a fold the owner has not toggled: the fold opens, because
/// ADR 0047 puts a visible failure above a fixed height.
public enum ToolRunFold {
    public enum Item: Hashable, Sendable, Identifiable {
        case row(TranscriptRow)
        /// The folded steps, in transcript order.
        case fold([TranscriptRow])

        /// A fold's id is its first shown step's id with a "fold:" prefix, because an open fold also
        /// draws that row with its own id. A hidden row is skipped, so Show hidden rows keeps the id.
        public var id: String {
            switch self {
            case .row(let row): row.eventID
            case .fold(let rows): "fold:" + (rows.first { !$0.isHiddenByDefault } ?? rows[0]).eventID
            }
        }
    }

    /// One item of the transcript's lazy list. An open fold's steps follow its fold line as items of
    /// their own, so a long open fold builds only the steps on screen, as every other row does.
    public enum Line: Hashable, Sendable, Identifiable {
        case item(Item)
        case step(TranscriptRow)

        public var id: String {
            switch self {
            case .item(let item): item.id
            case .step(let row): row.eventID
            }
        }
    }

    /// The list items in transcript order; `isExpanded` gets a fold's id and its rows.
    public static func lines(_ items: [Item], isExpanded: (String, [TranscriptRow]) -> Bool) -> [Line] {
        items.flatMap { item -> [Line] in
            guard case .fold(let group) = item, isExpanded(item.id, group) else { return [.item(item)] }
            return [.item(item)] + group.map(Line.step)
        }
    }

    public static func isStep(_ row: TranscriptRow) -> Bool {
        row.tool != nil
            || (row.systemKind == TranscriptSystemKind.swarmRing && !row.startsTurn)
            || row.kind == .thought
            || row.isHiddenByDefault
    }

    /// The rows in transcript order, each folding run replaced by one `.fold` item. O(rows).
    public static func items(in rows: [TranscriptRow]) -> [Item] {
        var items: [Item] = []
        var run: [TranscriptRow] = []
        func flush() {
            if run.count(where: { !$0.isHiddenByDefault }) >= 2, run.contains(where: { $0.tool != nil }) {
                items.append(.fold(run))
            } else {
                items += run.map(Item.row)
            }
            run = []
        }
        for row in rows {
            if isStep(row) {
                run.append(row)
            } else {
                flush()
                items.append(.row(row))
            }
        }
        flush()
        return items
    }

    /// Whether a fold draws its steps: the owner's choice, stored under the fold's id at the time,
    /// else `defaultExpanded`. Load earlier can prepend steps to the window's first fold and so move
    /// its id to an earlier step, so a choice stored under any step's fold id counts, and the
    /// earliest one is the newest. The id cannot be the last step's: the live fold gains steps. O(steps).
    public static func isExpanded(_ rows: [TranscriptRow], overrides: [String: Bool]) -> Bool {
        rows.lazy.compactMap { overrides["fold:" + $0.eventID] }.first ?? defaultExpanded(rows)
    }

    /// A fold with a failed step opens by default, so folding never hides a failure. So does a fold
    /// with 2 or more shown steps before its first tool, because they were rows on screen before the
    /// tool landed.
    public static func defaultExpanded(_ rows: [TranscriptRow]) -> Bool {
        rows.contains { $0.tool?.state == .failed }
            || rows.prefix { $0.tool == nil }.count(where: { !$0.isHiddenByDefault }) >= 2
    }

    /// The id of the trailing fold when a failed step opens it: the live run of a running turn,
    /// so the view can say it opened. An earlier fold with a failure was opened before.
    public static func liveFailedFoldID(in items: [Item]) -> String? {
        guard let last = items.last, case .fold(let rows) = last, rows.contains(where: { $0.tool?.state == .failed })
        else { return nil }
        return last.id
    }

    public struct NameCount: Hashable, Sendable {
        public var name: String
        public var count: Int
    }

    /// What one fold line says, computed from its rows and never stored.
    public struct Summary: Hashable, Sendable {
        public var commands = 0
        /// Codex `sleep` calls.
        public var waits = 0
        /// Other tools by name, in first-seen order.
        public var otherTools: [NameCount] = []
        public var rings = 0
        public var failed = 0
        public var interrupted = 0
        /// Tools with no result in a turn that completed.
        public var unreported = 0
        /// A tool in the fold still waits for its result.
        public var isRunning = false
        /// The newest shown step's title, for a running fold.
        public var latestTitle = ""
        /// The sum of the tool durations; nil when any tool's duration is unknown.
        public var duration: Double?

        /// The fold line's glyph: the worst step state, so a fold claims finished only when every
        /// tool in it finished.
        public var state: TranscriptToolActivity.State {
            if failed > 0 { return .failed }
            if isRunning { return .waiting }
            if interrupted > 0 { return .interrupted }
            return unreported > 0 ? .unreported : .finished
        }

        /// "9 commands · 2 waits · Read ×2 · 4 rings"; a view draws the failures apart in their color.
        public var stepsText: String { counts(times: " ×").joined(separator: " · ") }

        /// `stepsText` plus " · 1 failed".
        public var text: String {
            (counts(times: " ×") + (failed > 0 ? ["\(failed) failed"] : [])).joined(separator: " · ")
        }

        /// "Steps: 9 commands, 2 waits, Read 2, 4 rings, 1 failed, 51 seconds", plus ", running,
        /// now swarm inbox" while a step runs.
        public var accessibilityLabel: String {
            var parts = counts(times: " ")
            if failed > 0 { parts.append("\(failed) failed") }
            if let duration { parts.append(Self.spoken(duration)) }
            if isRunning {
                parts.append("running")
                if !latestTitle.isEmpty { parts.append("now \(latestTitle)") }
            }
            return "Steps: " + parts.joined(separator: ", ")
        }

        private func counts(times: String) -> [String] {
            // Each phrase is its own localized literal, so every locale inflects its own plural.
            func phrase(_ count: Int, _ text: @autoclosure () -> AttributedString) -> String? {
                count == 0 ? nil : String(text().characters)
            }
            let tools = otherTools.map { $0.count > 1 ? "\($0.name)\(times)\($0.count)" : $0.name }
            return [
                phrase(commands, AttributedString(localized: "^[\(commands) command](inflect: true)")),
                phrase(waits, AttributedString(localized: "^[\(waits) wait](inflect: true)")),
            ].compactMap { $0 }
                + tools + [phrase(rings, AttributedString(localized: "^[\(rings) ring](inflect: true)"))].compactMap { $0 }
                + (interrupted > 0 ? ["\(interrupted) interrupted"] : [])
                + (unreported > 0 ? ["\(unreported) no result"] : [])
        }

        private static func spoken(_ seconds: Double) -> String {
            Duration.seconds(Int(seconds.rounded())).formatted(.units(allowed: [.minutes, .seconds], width: .wide))
        }
    }

    /// O(rows); a view calls it once per fold line.
    public static func summary(of rows: [TranscriptRow]) -> Summary {
        var summary = Summary()
        var total: Double? = 0
        for row in rows where !row.isHiddenByDefault {
            if row.systemKind == TranscriptSystemKind.swarmRing {
                summary.rings += 1
                summary.latestTitle = "swarm message"
            }
            guard let tool = row.tool else { continue }
            if tool.name == "sleep" {
                summary.waits += 1
            } else if tool.command != nil {
                summary.commands += 1
            } else if let index = summary.otherTools.firstIndex(where: { $0.name == tool.name }) {
                summary.otherTools[index].count += 1
            } else {
                summary.otherTools.append(NameCount(name: tool.name, count: 1))
            }
            switch tool.state {
            case .failed: summary.failed += 1
            case .waiting: summary.isRunning = true
            case .interrupted: summary.interrupted += 1
            case .unreported: summary.unreported += 1
            case .finished: break
            }
            total = total.flatMap { sum in tool.duration.map { sum + $0 } }
            let title = tool.headerTitle
            summary.latestTitle = title.isEmpty ? tool.name : title
        }
        summary.duration = total
        return summary
    }
}
