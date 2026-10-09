import Foundation

public struct GuardListError: LocalizedError, Sendable, Equatable {
    public let reason: String
    public var errorDescription: String? { reason }
}

public struct GuardRules: Codable, Sendable, Equatable {
    public struct Rule: Codable, Sendable, Equatable {
        public var name: String
        public var event: String
        public var tools: [String]?
        public var command: [String]
        public var timeout: UInt64?

        public init(name: String, event: String = "PreToolUse", tools: [String]? = nil,
                    command: [String], timeout: UInt64? = nil) {
            self.name = name
            self.event = event
            self.tools = tools
            self.command = command
            self.timeout = timeout
        }

        public init(from decoder: Decoder) throws {
            try GuardRules.rejectUnknownKeys(in: decoder, allowed: ["name", "event", "tools", "command", "timeout"])
            let container = try decoder.container(keyedBy: CodingKeys.self)
            name = try container.decode(String.self, forKey: .name)
            event = try container.decode(String.self, forKey: .event)
            tools = try container.decodeIfPresent([String].self, forKey: .tools)
            command = try container.decode([String].self, forKey: .command)
            timeout = try container.decodeIfPresent(UInt64.self, forKey: .timeout)
        }
    }

    public var rules: [Rule]
    public init(rules: [Rule] = []) { self.rules = rules }

    public init(from decoder: Decoder) throws {
        try Self.rejectUnknownKeys(in: decoder, allowed: ["rules"])
        let container = try decoder.container(keyedBy: CodingKeys.self)
        rules = try container.decode([Rule].self, forKey: .rules)
    }

    public static func load(url: URL) -> Result<Self, GuardListError> {
        do {
            let data: Data
            do { data = try Data(contentsOf: url) }
            catch CocoaError.fileReadNoSuchFile { return .success(Self()) }
            return .success(try JSONDecoder().decode(Self.self, from: data))
        } catch let error as DecodingError {
            let reason: String
            switch error {
            case .dataCorrupted(let context), .typeMismatch(_, let context), .valueNotFound(_, let context):
                reason = context.debugDescription
            case .keyNotFound(let key, _): reason = "Missing key '\(key.stringValue)'"
            @unknown default: reason = String(describing: error)
            }
            return .failure(GuardListError(reason: reason))
        } catch {
            return .failure(GuardListError(reason: error.localizedDescription))
        }
    }

    public func save(to url: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(self).write(to: url, options: .atomic)
    }

    private struct FieldKey: CodingKey {
        let stringValue: String
        let intValue: Int? = nil
        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { return nil }
    }

    private static func rejectUnknownKeys(in decoder: Decoder, allowed: Set<String>) throws {
        let container = try decoder.container(keyedBy: FieldKey.self)
        if let key = container.allKeys.sorted(by: { $0.stringValue < $1.stringValue })
            .first(where: { !allowed.contains($0.stringValue) }) {
            throw DecodingError.dataCorruptedError(forKey: key, in: container,
                                                   debugDescription: "Unknown key '\(key.stringValue)'")
        }
    }
}
