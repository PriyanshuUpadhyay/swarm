public enum NewChatMenu {
    public struct Row: Sendable, Equatable, Identifiable {
        public var id: String { name }
        public let name: String
        public let caption: String?
        public let isDefault: Bool
    }

    public static func rows(profiles: [SwarmProfile], defaultProfile: String = "chat") -> [Row] {
        let ordered = profiles.filter { $0.name == defaultProfile }
            + profiles.filter { $0.name != defaultProfile }
        return ordered.map { profile in
            Row(
                name: profile.name,
                caption: profile.runners.first.map { "\($0.provider) · \($0.model)" },
                isDefault: profile.name == defaultProfile
            )
        }
    }
}
