import Foundation
import TranscriptTool

/// `swarm managed list --json`: each item swarm wrote outside its home, recorded or found, with its
/// live state (ADR 0042). The page shows one row per writer and file, because the items of one
/// file work only together, such as the six Codex trust keys of the state hooks.
public struct SwarmManagedList: Sendable, Hashable, Codable {
    public struct Entry: Sendable, Hashable, Codable, Identifiable {
        public var id: String
        /// Open set: `hooks.state`, `hooks.guard`, `launch.trust`, `herdr`, and later ones.
        public var writer: String
        public var file: String
        /// Open set: `toml_key`, `json_key`, `json_array_item`, and later ones.
        public var kind: String
        public var path: [String]
        public var wrote: JSONElement
        /// The live value, only for a `changed` item.
        public var found: JSONElement?
        /// Open set: `present`, `changed`, `gone`, `off`, and later ones.
        public var state: String
        public var recorded: Bool
        public var atS: Int?
        public var with: String?
    }

    /// One row's state. A row is on while each of its items equals what swarm wrote.
    public enum RowState: Sendable, Hashable {
        case on
        case changed(found: String, wrote: String)
        case gone
        case off

        /// The state as the page shows it.
        public var name: String {
            switch self {
            case .on: "On"
            case .changed: "Changed by you"
            case .gone: "Gone"
            case .off: "Off"
            }
        }
    }

    public struct Row: Sendable, Hashable, Identifiable {
        public var writer: String
        public var file: String
        public var entries: [Entry]

        public var id: String { writer + "\u{0}" + file }
        public var ids: [String] { entries.map(\.id) }
        /// The ids that a revert of this row removes now.
        public var presentIDs: [String] { entries.filter { $0.state == "present" }.map(\.id) }

        /// What the row holds, as the owner reads it in the file.
        public var entry: String {
            if entries.allSatisfy({ $0.kind == "toml_key" && $0.path.last == "trusted_hash" }) {
                return entries.count == 1 ? "1 trust key" : "\(entries.count) trust keys"
            }
            guard entries.count == 1, let first = entries.first, let last = first.path.last else {
                return "\(entries.count) entries"
            }
            switch first.kind {
            case "json_key": return "group \"\(last)\""
            case "json_array_item": return "\(last) group"
            default: return first.path.joined(separator: ".")
            }
        }

        public var state: RowState {
            if let changed = entries.first(where: { $0.state == "changed" }) {
                return .changed(found: changed.found?.compactJSON ?? "", wrote: changed.wrote.compactJSON)
            }
            if entries.contains(where: { $0.state == "present" }) { return .on }
            if entries.allSatisfy({ $0.state == "off" }) { return .off }
            return .gone
        }
    }

    public struct Group: Sendable, Hashable, Identifiable {
        public var name: String
        /// The command that makes these items.
        public var source: String?
        public var rows: [Row]

        public var id: String { name }
        public var presentIDs: [String] { rows.flatMap(\.presentIDs) }
        /// What VoiceOver reads for the group's header, such as "Guard group, 1 row".
        public var spoken: String {
            "\(name) group, " + String(AttributedString(localized: "^[\(rows.count) row](inflect: true)").characters)
        }
    }

    public var entries: [Entry]

    public init(entries: [Entry]) {
        self.entries = entries
    }

    /// Decode the CLI's JSON, with its snake_case keys.
    public static func decode(_ data: Data) throws -> SwarmManagedList {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(SwarmManagedList.self, from: data)
    }

    /// Recorded items grouped by writer in a fixed order, a writer this build does not know
    /// after them by its own name, and found items last.
    public var groups: [Group] {
        let known: [(writer: String, name: String, source: String)] = [
            ("hooks.state", "Agent hooks", "hooks setup"),
            ("hooks.guard", "Guard", "hooks setup"),
            ("launch.trust", "Folder trust", "launch"),
            ("herdr", "Herdr notices", "notify"),
        ]
        let recorded = entries.filter(\.recorded)
        var writers = known.map(\.writer)
        for entry in recorded where !writers.contains(entry.writer) { writers.append(entry.writer) }
        var groups: [Group] = writers.compactMap { writer in
            let rows = Self.rows(recorded.filter { $0.writer == writer })
            guard !rows.isEmpty else { return nil }
            let named = known.first { $0.writer == writer }
            return Group(name: named?.name ?? writer, source: named?.source, rows: rows)
        }
        let found = Self.rows(entries.filter { !$0.recorded })
        if !found.isEmpty { groups.append(Group(name: "Found, not recorded", source: nil, rows: found)) }
        return groups
    }

    /// Each present item, for Undo All.
    public var presentIDs: [String] { entries.filter { $0.state == "present" }.map(\.id) }

    /// The counts of rows, such as "2 on · 1 changed by you · 1 found"; a zero count is left out.
    public var summary: String {
        var counts: [String: Int] = [:]
        for group in groups {
            for row in group.rows {
                let key: String
                if row.entries.first?.recorded == false {
                    key = "found"
                } else {
                    switch row.state {
                    case .on: key = "on"
                    case .changed: key = "changed by you"
                    case .gone: key = "gone"
                    case .off: key = "off"
                    }
                }
                counts[key, default: 0] += 1
            }
        }
        return ["on", "changed by you", "gone", "off", "found"]
            .compactMap { key in counts[key].map { "\($0) \(key)" } }
            .joined(separator: " · ")
    }

    /// A line for each row whose state is not what it was in `old`, and for each row that is gone
    /// from the list, such as "Agent hooks, ~/.codex/config.toml, Off", so VoiceOver hears the row
    /// that an undo flipped.
    public func changes(since old: SwarmManagedList) -> [String] {
        func rows(_ list: SwarmManagedList) -> [(key: String, line: String, state: RowState)] {
            list.groups.flatMap { group in
                group.rows.map { row in
                    (group.name + "\u{0}" + row.id,
                     "\(group.name), \((row.file as NSString).abbreviatingWithTildeInPath)", row.state)
                }
            }
        }
        let now = rows(self)
        let was = rows(old)
        let wasState = Dictionary(was.map { ($0.key, $0.state) }, uniquingKeysWith: { first, _ in first })
        let nowKeys = Set(now.map(\.key))
        let flipped = now.compactMap { row in
            wasState[row.key].flatMap { $0 == row.state ? nil : "\(row.line), \(row.state.name)" }
        }
        return flipped + was.filter { !nowKeys.contains($0.key) }.map { "\($0.line), Removed" }
    }

    private static func rows(_ entries: [Entry]) -> [Row] {
        var rows: [Row] = []
        for entry in entries {
            if let index = rows.firstIndex(where: { $0.writer == entry.writer && $0.file == entry.file }) {
                rows[index].entries.append(entry)
            } else {
                rows.append(Row(writer: entry.writer, file: entry.file, entries: [entry]))
            }
        }
        return rows.sorted { $0.file < $1.file }
    }
}
