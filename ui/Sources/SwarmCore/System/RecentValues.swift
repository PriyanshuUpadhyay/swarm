import Foundation

/// The values of the most recently stored keys, at most `capacity` of them.
public struct RecentValues<Key: Hashable, Value> {
    public let capacity: Int
    private var values: [Key: Value] = [:]
    private var order: [Key] = []

    public init(capacity: Int) { self.capacity = capacity }

    public subscript(key: Key) -> Value? { values[key] }

    /// Stores the value as the newest and drops the oldest past the capacity.
    public mutating func set(_ value: Value, for key: Key) {
        values[key] = value
        order.removeAll { $0 == key }
        order.append(key)
        while order.count > capacity { values[order.removeFirst()] = nil }
    }
}
