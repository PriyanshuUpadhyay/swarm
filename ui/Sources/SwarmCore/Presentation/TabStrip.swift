import Foundation

public struct TabStrip: Codable, Sendable, Hashable {
    public var open: [String]
    public var groups: [TabGroup]

    public init(open: [String] = [], groups: [TabGroup] = []) {
        self.open = open
        self.groups = groups
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        open = try container.decodeIfPresent([String].self, forKey: .open) ?? []
        groups = try container.decodeIfPresent([TabGroup].self, forKey: .groups) ?? []
    }

    public func opening(_ key: String, after current: String?) -> Self {
        guard !open.contains(key) else { return self }
        var strip = self
        let index = current.flatMap { open.firstIndex(of: $0) }.map { $0 + 1 } ?? open.count
        strip.open.insert(key, at: index)
        return strip
    }

    public func closing(_ key: String) -> Self {
        pruned(to: Set(open).subtracting([key]))
    }

    public func moving(_ key: String, to index: Int) -> Self {
        guard let source = open.firstIndex(of: key), open.indices.contains(index) else { return self }
        var strip = self
        strip.open.remove(at: source)
        strip.open.insert(key, at: index)
        return strip.pruned(to: Set(open))
    }

    public func pruned(to listed: Set<String>) -> Self {
        var strip = self
        var seen = Set<String>()
        strip.open = open.filter { listed.contains($0) && seen.insert($0).inserted }
        strip.groups = groups.compactMap { group in
            var pruned = group
            pruned.members = strip.open.filter { group.members.contains($0) }
            return pruned.members.isEmpty ? nil : pruned
        }
        return strip
    }

    public static func seed(_ chats: [ChatRow]) -> Self {
        Self(open: chats.filter { SessionRowPresentation.make($0, now: 0).state == .live }
            .map { ChatTitle.key($0.session) })
    }

    public static func selectionAfterClose(history: [String], open: [String]) -> String? {
        history.last { open.contains($0) } ?? open.first
    }
}

public struct TabGroup: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    public var color: TabGroupColor
    public var members: [String]
    public var folded: Bool = false

    public init(id: String, name: String, color: TabGroupColor, members: [String], folded: Bool = false) {
        self.id = id
        self.name = name
        self.color = color
        self.members = members
        self.folded = folded
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        color = try container.decode(TabGroupColor.self, forKey: .color)
        members = try container.decodeIfPresent([String].self, forKey: .members) ?? []
        folded = try container.decodeIfPresent(Bool.self, forKey: .folded) ?? false
    }
}

public enum TabGroupColor: String, Codable, Sendable, Hashable {
    case grey, blue, green, yellow, orange, red, purple, pink

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .grey
    }
}
