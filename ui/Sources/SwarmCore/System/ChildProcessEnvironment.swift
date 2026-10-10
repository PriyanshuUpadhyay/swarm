/// Removes an agent host's identity before Swarm starts a child terminal or agent.
public enum ChildProcessEnvironment {
    private static let exactNames: Set<String> = [
        "CLAUDECODE",
        "CLAUDE_PID",
        "AI_AGENT",
        "SWARM_SESSION_ID",
        "SWARM_AGENT_ID",
        "SWARM_ADAPTER",
        "SWARM_PANE",
        // `swarm answer` refuses a caller inside an agent pane; the app is never in one.
        "TMUX_PANE",
    ]

    static func swarmCall(overrides: [String: String], inherited: [String: String]) -> [String: String] {
        removingInheritedAgentIdentity(from: inherited).merging(overrides) { _, requested in requested }
    }

    public static func removingInheritedAgentIdentity(
        from environment: [String: String]
    ) -> [String: String] {
        environment.filter { !isInheritedAgentIdentity($0.key) }
    }

    public static func inheritedAgentIdentityNames(
        in environment: [String: String]
    ) -> [String] {
        Array(exactNames.union(environment.keys.filter(isInheritedAgentIdentity))).sorted()
    }

    private static func isInheritedAgentIdentity(_ name: String) -> Bool {
        exactNames.contains(name) || name.hasPrefix("HERDR_") || name.hasPrefix("CLAUDE_CODE_")
    }
}
