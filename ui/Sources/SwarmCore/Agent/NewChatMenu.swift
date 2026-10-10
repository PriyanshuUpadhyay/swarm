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
        private static let minimumReadInterval: Duration = .seconds(5)
        private var isReading = false
        private var lastReadAt: ContinuousClock.Instant?

        public init() {}

        public mutating func apply(profiles: [SwarmProfile]) {
            self.profiles = profiles
            error = nil
        }

        public mutating func record(error: String) { self.error = error }

        /// Reserves the read when no read is running and the minimum interval has passed.
        public mutating func reserveRead(at now: ContinuousClock.Instant) -> Bool {
            guard !isReading else { return false }
            if let lastReadAt, lastReadAt.duration(to: now) < Self.minimumReadInterval { return false }
            isReading = true
            return true
        }

        /// Nil releases a cancelled read without setting a completion stamp.
        public mutating func finishRead(at now: ContinuousClock.Instant?) {
            isReading = false
            if let now { lastReadAt = now }
        }

        /// O(n) in the number of profiles.
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
