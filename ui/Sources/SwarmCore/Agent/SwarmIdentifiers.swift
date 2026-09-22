import Foundation

public protocol Identifier: Hashable, Sendable, Codable, RawRepresentable, CustomStringConvertible, Comparable where RawValue == String {
    init(_ rawValue: String)
}

extension Identifier {
    public init(rawValue: String) { self.init(rawValue) }
    public var description: String { rawValue }
    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
}

public struct SwarmSessionID: Identifier {
    public let rawValue: String
    public init(_ rawValue: String) { self.rawValue = rawValue }
}

public struct SwarmAgentID: Identifier {
    public let rawValue: String
    public init(_ rawValue: String) { self.rawValue = rawValue }
}

public struct SwarmChairID: Identifier {
    public let rawValue: String
    public init(_ rawValue: String) { self.rawValue = rawValue }
}
