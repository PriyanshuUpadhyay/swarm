import Foundation

/// Known Setup groups; unknown wire names stay visible in the plan and managed list.
public enum SetupGroup: String, CaseIterable, Sendable, Hashable {
    case hooks, trust, herdr, skills

    public var title: String {
        switch self {
        case .hooks: "Agent hooks"
        case .trust: "Folder trust"
        case .herdr: "Herdr"
        case .skills: "Skills"
        }
    }

    /// Herdr has no decline flag because this build plans no Herdr write.
    public var declineFlagKey: String? {
        switch self {
        case .hooks: "hooksSetupDeclined"
        case .trust: "trustSetupDeclined"
        case .herdr: nil
        case .skills: "skillsSetupDeclined"
        }
    }

    public static func declined(read: (String) -> Bool) -> Set<Self> {
        Set(allCases.filter { group in group.declineFlagKey.map(read) ?? false })
    }

    /// Only rows whose writer and kinds have a known restore route can decline that group.
    public static func restoreGroup(writer: String, kinds: [String]) -> Self? {
        if writer.hasPrefix("hooks.") { return .hooks }
        if writer == skills.rawValue && kinds.allSatisfy({ $0 == "symlink" }) { return .skills }
        return nil
    }

    public func restorePlan(using bus: SwarmCLIBus) async throws -> SwarmHooksPlan {
        switch self {
        case .hooks: try await bus.hooksPlan()
        case .skills: try await bus.setupPlan(.skillsOnly())
        case .trust, .herdr: throw SwarmProfileError.failed("\(title) has no managed restore route")
        }
    }

    public func restore(using bus: SwarmCLIBus, digest: String) async throws {
        switch self {
        case .hooks: try await bus.setUpHooks(digest: digest)
        case .skills: try await bus.setUp(digest: digest, choice: .skillsOnly())
        case .trust, .herdr: throw SwarmProfileError.failed("\(title) has no managed restore route")
        }
    }
}
