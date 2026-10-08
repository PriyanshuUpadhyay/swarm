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
        return strip.pruned(to: Set(strip.open))
    }

    public func closing(_ key: String) -> Self {
        pruned(to: Set(open).subtracting([key]))
    }

    public func moving(_ key: String, to index: Int) -> Self {
        guard let source = open.firstIndex(of: key), open.indices.contains(index) else { return self }
        guard source != index else { return self }
        let targetGroup = groups.first { $0.members.contains(open[index]) }?.id
        var strip = self
        for groupIndex in strip.groups.indices {
            strip.groups[groupIndex].members.removeAll { $0 == key }
            if strip.groups[groupIndex].id == targetGroup { strip.groups[groupIndex].members.append(key) }
        }
        strip.open.remove(at: source)
        strip.open.insert(key, at: index)
        return strip.pruned(to: Set(open))
    }

    /// A menu move keeps membership; ungrouped tabs cross a group as one block.
    public func stepTargets(excluding pending: Set<String> = []) -> [String: (left: String?, right: String?)] {
        let keys = open.filter { !pending.contains($0) }
        let groupByKey = Dictionary(uniqueKeysWithValues: groups.flatMap { group in
            group.members.map { ($0, group.id) }
        })
        var bounds: [String: (first: Int, last: Int)] = [:]
        for (index, key) in keys.enumerated() {
            if let group = groupByKey[key] { bounds[group] = (bounds[group]?.first ?? index, index) }
        }
        var targets: [String: (left: String?, right: String?)] = [:]
        for (index, key) in keys.enumerated() {
            var left: String?
            var right: String?
            if let group = groupByKey[key], let range = bounds[group] {
                if index > range.first { left = keys[index - 1] }
                if index < range.last { right = keys[index + 1] }
            } else {
                if index > 0 {
                    let start = groupByKey[keys[index - 1]].flatMap { bounds[$0]?.first } ?? index - 1
                    left = keys[start]
                }
                if index + 1 < keys.count {
                    let end = groupByKey[keys[index + 1]].flatMap { bounds[$0]?.last } ?? index + 1
                    right = keys[end]
                }
            }
            targets[key] = (left, right)
        }
        return targets
    }

    public func stepping(_ key: String, toward target: String) -> Self {
        guard let targets = stepTargets()[key], targets.left == target || targets.right == target,
              let source = open.firstIndex(of: key), let destination = open.firstIndex(of: target) else { return self }
        var strip = self
        strip.open.remove(at: source)
        strip.open.insert(key, at: destination)
        return strip.pruned(to: Set(open))
    }

    public func pruned(to listed: Set<String>) -> Self {
        var strip = self
        var seen = Set<String>()
        strip.open = open.filter { listed.contains($0) && seen.insert($0).inserted }
        var grouped = Set<String>()
        strip.groups = groups.compactMap { group in
            var pruned = group
            pruned.members = strip.open.filter { group.members.contains($0) && grouped.insert($0).inserted }
            return pruned.members.isEmpty ? nil : pruned
        }
        let groupByKey = Dictionary(uniqueKeysWithValues: strip.groups.flatMap { group in
            group.members.map { ($0, group) }
        })
        var emitted = Set<String>()
        strip.open = strip.open.flatMap { key -> [String] in
            guard let group = groupByKey[key] else { return [key] }
            return emitted.insert(group.id).inserted ? group.members : []
        }
        return strip
    }

    public enum Grouping: Sendable {
        case new(id: String, name: String, color: TabGroupColor, tab: String)
        case add(String, to: String)
        case remove(String)
        case rename(String, to: String)
        case recolor(String, to: TabGroupColor)
        case fold(String, Bool)
        case delete(String)
    }

    public func grouping(_ action: Grouping) -> Self {
        var strip = self
        switch action {
        case let .new(id, name, color, key):
            guard open.contains(key), !groups.contains(where: { $0.id == id }),
                  let name = ChatTitle.nonblank(name) else { return self }
            strip = grouping(.remove(key))
            strip.groups.append(TabGroup(id: id, name: name, color: color, members: [key]))
        case let .add(key, id):
            guard open.contains(key), let group = groups.first(where: { $0.id == id }),
                  !group.members.contains(key) else { return self }
            strip = grouping(.remove(key))
            guard let groupIndex = strip.groups.firstIndex(where: { $0.id == id }),
                  let last = group.members.last else { return self }
            strip.open.removeAll { $0 == key }
            let position = strip.open.firstIndex(of: last).map { $0 + 1 } ?? strip.open.count
            strip.open.insert(key, at: position)
            strip.groups[groupIndex].members.append(key)
        case let .remove(key):
            for index in strip.groups.indices { strip.groups[index].members.removeAll { $0 == key } }
        case let .rename(id, name):
            guard let index = groups.firstIndex(where: { $0.id == id }),
                  let name = ChatTitle.nonblank(name) else { return self }
            strip.groups[index].name = name
        case let .recolor(id, color):
            guard let index = groups.firstIndex(where: { $0.id == id }) else { return self }
            strip.groups[index].color = color
        case let .fold(id, folded):
            guard let index = groups.firstIndex(where: { $0.id == id }) else { return self }
            strip.groups[index].folded = folded
        case let .delete(id):
            strip.groups.removeAll { $0.id == id }
        }
        return strip.pruned(to: Set(open))
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

public enum TabGroupColor: String, Codable, Sendable, Hashable, CaseIterable {
    case grey, blue, green, yellow, orange, red, purple, pink

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .grey
    }
}
