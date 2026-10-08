public enum NewChatMenu {
    public struct Row: Sendable, Equatable, Identifiable {
        public enum ID: Sendable, Hashable { case profile(String), readError }
        public var id: ID { isEnabled ? .profile(name) : .readError }
        public let name: String
        public let caption: String?
        public let isDefault: Bool
        public let isEnabled: Bool
    }

    public struct State: Sendable, Equatable {
        public private(set) var profiles: [SwarmProfile] = []
        public private(set) var error: String?

        public init() {}

        public mutating func received(_ profiles: [SwarmProfile]) {
            self.profiles = profiles
            error = nil
        }

        public mutating func failed(_ reason: String) { error = reason }

        public var rows: [Row] { NewChatMenu.rows(profiles: profiles, error: error) }
    }

    public static func rows(profiles: [SwarmProfile], error: String? = nil, defaultProfile: String = "chat") -> [Row] {
        let ordered = profiles.filter { $0.name == defaultProfile }
            + profiles.filter { $0.name != defaultProfile }
        var rows = ordered.map { profile in
            Row(
                name: profile.name,
                caption: profile.runners.first.map { "\($0.provider) · \($0.model)" },
                isDefault: profile.name == defaultProfile,
                isEnabled: true
            )
        }
        if let error {
            rows.append(Row(name: "Could not read profiles. \(error)", caption: nil, isDefault: false, isEnabled: false))
        }
        return rows
    }
}
