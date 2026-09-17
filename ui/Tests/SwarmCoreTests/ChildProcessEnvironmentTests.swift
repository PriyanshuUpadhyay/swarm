import Testing
@testable import SwarmCore

@Suite("Child process environment")
struct ChildProcessEnvironmentTests {
    @Test("A child agent does not inherit its orchestrator identity")
    func removesOrchestratorIdentity() {
        let orchestratorEnvironment = [
            "HERDR_PANE_ID": "pane-1",
            "HERDR_FUTURE_VALUE": "future",
            "CLAUDECODE": "1",
            "CLAUDE_PID": "42",
            "AI_AGENT": "claude",
            "CLAUDE_CODE_ENTRYPOINT": "cli",
            "SWARM_SESSION_ID": "7",
            "SWARM_AGENT_ID": "orchestrator",
            "SWARM_ADAPTER": "herdr",
            "SWARM_PANE": "%1",
            "CLAUDE_CONFIG_DIR": "/tmp/claude-work",
            "PATH": "/usr/bin",
        ]

        let agentEnvironment = ChildProcessEnvironment.removingInheritedAgentIdentity(
            from: orchestratorEnvironment
        )

        #expect(agentEnvironment == [
            "CLAUDE_CONFIG_DIR": "/tmp/claude-work",
            "PATH": "/usr/bin",
        ])
    }

    @Test("The tmux server is told to remove exact names and inherited prefix names")
    func namesForTmuxServer() {
        let orchestratorEnvironment = [
            "HERDR_PANE_ID": "pane-1",
            "CLAUDE_CODE_ENTRYPOINT": "cli",
            "PATH": "/usr/bin",
        ]
        let removed = ChildProcessEnvironment.inheritedAgentIdentityNames(
            in: orchestratorEnvironment
        )

        #expect(removed.contains("HERDR_PANE_ID"))
        #expect(removed.contains("CLAUDE_CODE_ENTRYPOINT"))
        #expect(removed.contains("SWARM_SESSION_ID"))
        #expect(!removed.contains("PATH"))
        #expect(!removed.contains("CLAUDE_CONFIG_DIR"))
    }

    @Test("Every agent launch applies the same environment rule")
    func agentLaunch() {
        let agentLaunch = AgentLaunch(
            executable: "claude",
            arguments: [],
            cwd: "/tmp/work",
            environment: [
                "HERDR_PANE_ID": "pane-1",
                "CLAUDE_CONFIG_DIR": "/tmp/claude-work",
            ]
        )

        #expect(agentLaunch.environment == ["CLAUDE_CONFIG_DIR": "/tmp/claude-work"])
    }
}
