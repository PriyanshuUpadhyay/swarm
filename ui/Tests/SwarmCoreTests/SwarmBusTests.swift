import Foundation
import Testing
@testable import SwarmCore

@Suite("Swarm bus", .tags(.agentProtocol))
struct SwarmBusTests {
    private let coderAgent = SwarmAgentID("coder-1")
    private let workspaceSession = SwarmSessionID("42")
    private let scratchDirectory = "/tmp/swarm-bus-tests"

    @Test("starts a session with three ordered calls")
    func startsSession() async throws {
        let (bus, runner) = makeBus([
            .result(),
            .result(stdout: "42\n"),
            .result(),
        ])

        let session = try await bus.startSession()

        #expect(session == workspaceSession)
        #expect(await runner.recordedCalls() == [
            call(["init"], environment: adapterEnvironment),
            call(["session", "new", "lane"], environment: adapterEnvironment),
            call(
                ["agent", "add", "orchestrator", "orchestrator"],
                environment: sessionEnvironment
            ),
        ])
    }

    @Test("starts and updates an interactive chair session")
    func startsChairSession() async throws {
        let (bus, runner) = makeBus([
            .result(), .result(stdout: "42\n"), .result(),
        ])
        let chair = try #require(SwarmChair(agent: .claudeCode, id: "chat-id"))

        let session = try await bus.startChairSession(
            chair: chair, directory: "/workspace/project"
        )
        try await bus.setChair(
            try #require(SwarmChair(agent: .codex, id: "thread-id")), in: session
        )

        #expect(session == workspaceSession)
        #expect(await runner.recordedCalls() == [
            call(
                ["init"], cwd: "/workspace/project",
                environment: ["SWARM_ADAPTER": "tmux-solo", "PWD": "/workspace/project"]
            ),
            call(
                ["session", "new", "lane", "--chair", "claude:chat-id"],
                cwd: "/workspace/project",
                environment: ["SWARM_ADAPTER": "tmux-solo", "PWD": "/workspace/project"]
            ),
            call(
                ["session", "chair", "codex:thread-id"],
                environment: [
                    "SWARM_ADAPTER": "tmux-solo",
                    "SWARM_SESSION_ID": "42",
                    "SWARM_AGENT_ID": "orchestrator",
                ]
            ),
        ])
    }

    @Test("builds the chair pane environment")
    func chairEnvironment() throws {
        #expect(SwarmChairLaunch.environment(
            session: workspaceSession, home: "/swarm-home"
        ) == [
            "SWARM_ADAPTER": "tmux-solo",
            "SWARM_SESSION_ID": "42",
            "SWARM_AGENT_ID": "orchestrator",
            "SWARM_HOME": "/swarm-home",
        ])
        #expect(SwarmChairLaunch.registrationCommand
            == "swarm agent add orchestrator orchestrator")
    }

    @Test("builds and hands off a detached chair plan with the first prompt")
    func detachedChairPlan() async throws {
        let workspace = WorkspaceID("workspace")
        let session = Session(
            id: SessionID("chat"), workspaceID: workspace,
            model: "gpt-5", effort: "high", agentKind: .codex
        )
        let plan = try #require(SwarmChairLaunch.plan(
            workspaceID: workspace,
            session: session,
            paneID: TerminalTabID("pane"),
            swarmSession: workspaceSession,
            directory: "/workspace/project",
            prompt: "Fix the launch",
            home: "/swarm-home",
            workspaceEnvironment: ["SWARM_UI_PORT": "3000"],
            statusURL: URL(fileURLWithPath: "/tmp/chat-status.json")
        ))
        let recorder = ChairLaunchRecorder()

        await SwarmChairLaunch.start(plan) { await recorder.record($0) }

        #expect(await recorder.plan == plan)
        #expect(plan.tmuxSession == TmuxSessions.sessionName(
            workspaceID: workspace, paneID: "pane"
        ))
        #expect(plan.arguments.suffix(2) == ["--", "Fix the launch"])
        #expect(plan.environment["SWARM_SESSION_ID"] == "42")
        #expect(plan.environment["SWARM_UI_PORT"] == "3000")
    }

    @Test("rejects a non-integer session id")
    func rejectsInvalidSessionID() async {
        let (bus, _) = makeBus([.result(), .result(stdout: "not-an-id\n")])

        await #expect(throws: SwarmProfileError.failed("swarm returned an invalid session id")) {
            try await bus.startSession()
        }
    }

    @Test("launches with and without an account")
    func launchesAgent() async throws {
        let (bus, runner) = makeBus([
            .result(stdout: "%7\nignored\n", stderr: "notice\naccount work\n"),
            .result(stdout: "%8\n"),
        ])

        let selected = try await bus.launch(
            coderAgent, role: "code.complex", account: "auto",
            in: workspaceSession, directory: "/workspace/project"
        )
        let defaultAccount = try await bus.launch(
            SwarmAgentID("reviewer-1"), role: "review", account: nil,
            in: workspaceSession, directory: "/workspace/project"
        )

        #expect(selected == SwarmLaunch(pane: "%7", account: "work"))
        #expect(defaultAccount == SwarmLaunch(pane: "%8", account: nil))
        #expect(await runner.recordedCalls() == [
            call(
                ["launch", "coder-1", "code.complex", "--account", "auto"],
                cwd: "/workspace/project", environment: launchEnvironment,
                timeout: .seconds(60)
            ),
            call(
                ["launch", "reviewer-1", "review"], cwd: "/workspace/project",
                environment: launchEnvironment, timeout: .seconds(60)
            ),
        ])
    }

    @Test("rejects a launch without a pane")
    func rejectsLaunchWithoutPane() async {
        let (bus, _) = makeBus([.result()])

        await #expect(throws: SwarmProfileError.failed("swarm returned no pane")) {
            try await bus.launch(
                coderAgent, role: "code.complex", account: nil,
                in: workspaceSession, directory: "/workspace/project"
            )
        }
    }

    @Test("rejects a zero message sequence")
    func rejectsZeroMessageSequence() async {
        let (bus, _) = makeBus([.result(stdout: "0\n")])

        await #expect(throws: SwarmProfileError.failed("swarm returned an invalid message sequence")) {
            try await bus.send("Fix the parser", to: coderAgent, in: workspaceSession)
        }
    }

    @Test("runs every session command with its contract inputs")
    func runsSessionCommands() async throws {
        let agentsJSON = #"{"agents":[{"id":"orchestrator","role":"orchestrator","pane":null,"alive":null}]}"#
        let messagesJSON = #"{"messages":[{"seq":8,"sender":"orchestrator","recipient":"coder-1","kind":"ask","body":null,"created_at":1789576942,"read":false}]}"#
        let (bus, runner) = makeBus([
            .result(stdout: agentsJSON, stderr: "swarm: cannot list panes\n"),
            .result(stdout: messagesJSON),
            .result(stdout: "9\n"),
            .result(),
            .result(),
            .result(),
            .result(),
            .result(),
        ])

        let agents = try await bus.agents(in: workspaceSession)
        let messages = try await bus.messages(in: workspaceSession, after: 7)
        let seq = try await bus.send("Fix the parser", to: coderAgent, in: workspaceSession)
        try await bus.type(
            "Use the session adapter", to: coderAgent, in: workspaceSession, adapter: "herdr"
        )
        try await bus.interrupt(coderAgent, in: workspaceSession, adapter: "herdr")
        try await bus.ack(seq, in: workspaceSession)
        try await bus.sweep(in: workspaceSession)
        try await bus.close(coderAgent, in: workspaceSession)

        #expect(agents == [SwarmAgent(
            id: SwarmAgentID("orchestrator"), role: "orchestrator", pane: nil, alive: nil
        )])
        #expect(messages == [SwarmMessage(
            seq: 8, sender: SwarmAgentID("orchestrator"), recipient: coderAgent,
            kind: "ask", body: nil, createdAt: 1_789_576_942, read: false
        )])
        #expect(seq == 9)
        #expect(await runner.recordedCalls() == [
            call(["agents", "--json"], environment: sessionEnvironment),
            call(
                ["messages", "--json", "--after", "7"],
                environment: sessionEnvironment
            ),
            call(
                ["send", "coder-1", "ask"], environment: sessionEnvironment,
                stdin: "Fix the parser"
            ),
            call(
                ["type", "coder-1"], environment: herdrSessionEnvironment,
                stdin: "Use the session adapter"
            ),
            call(["interrupt", "coder-1"], environment: herdrSessionEnvironment),
            call(["ack", "9"], environment: sessionEnvironment),
            call(["sweep"], environment: sessionEnvironment),
            call(["close", "coder-1"], environment: sessionEnvironment),
        ])
    }

    @Test("lists sessions without selecting one in the environment")
    func listsSessions() async throws {
        let json = """
        {"sessions":[{"id":10,"talk_mode":"lane","adapter":"herdr",\
        "cwd":"/workspace/project",\
        "created_at":1789600000,"chair_provider":"claude","chair_id":"chat-id",\
        "chair_log":"/tmp/chair.jsonl","agents":3,\
        "messages":9,"last_message_at":1789610000}]}
        """
        let (bus, runner) = makeBus([.result(stdout: json)])

        let sessions = try await bus.sessions()

        #expect(sessions == [SwarmSession(
            id: SwarmSessionID("10"), talkMode: "lane", adapter: "herdr",
            cwd: "/workspace/project",
            createdAt: 1_789_600_000, chairProvider: "claude",
            chairID: SwarmChairID("chat-id"),
            chairLog: "/tmp/chair.jsonl",
            agents: 3, messages: 9, lastMessageAt: 1_789_610_000
        )])
        #expect(await runner.recordedCalls() == [
            call(["sessions", "--json"], environment: adapterEnvironment),
        ])
    }

    @Test("archives sessions without selecting one in the environment")
    func archivesSessions() async throws {
        let (bus, runner) = makeBus([.result()])

        try await bus.archive([SwarmSessionID("8"), SwarmSessionID("13")])
        try await bus.archive([])

        #expect(await runner.recordedCalls() == [
            call(
                ["session", "archive", "8", "13"],
                environment: adapterEnvironment
            ),
        ])
    }

    @Test("discovered session calls use its adapter and stop when it has none")
    func routesDiscoveredSessionCalls() async throws {
        let bus = AdapterRecordingSwarmBus()
        let session = discoveredSession(adapter: "herdr")

        _ = try await bus.agents(in: session)
        _ = try await bus.messages(in: session, after: 4)
        try await bus.type("Continue", to: coderAgent, in: session)
        try await bus.interrupt(coderAgent, in: session)

        #expect(await bus.recordedCalls() == [
            .agents(session.id, "herdr"),
            .messages(session.id, 4, "herdr"),
            .type(session.id, coderAgent, "Continue", "herdr"),
            .interrupt(session.id, coderAgent, "herdr"),
        ])

        let oldSession = discoveredSession(adapter: nil)
        await #expect(throws: SwarmProfileError.failed(
            SwarmSessionInteraction.missingAdapterSentence
        )) {
            try await bus.agents(in: oldSession)
        }
        #expect(await bus.recordedCalls().count == 4)
    }

    @Test("maps runner and output errors")
    func mapsErrors() async {
        await expectFailure(
            .shellError(status: 127, stderr: "swarm not found on PATH"),
            .unavailable("swarm not found on PATH")
        )
        await expectFailure(
            .result(status: 2, stderr: "routing config is missing\nmore detail\n"),
            .failed("routing config is missing")
        )
        await expectFailure(
            .result(status: 137, stdout: "process was killed\nmore detail\n"),
            .failed("process was killed")
        )
        await expectFailure(.result(status: 1), .failed("swarm exited 1"))
        await expectFailure(.lost, .failed("lost"))
        await expectFailure(
            .result(stdout: "not json"),
            .failed("swarm returned invalid JSON")
        )

        let (cancelledBus, _) = makeBus([.cancelled])
        await #expect(throws: CancellationError.self) {
            try await cancelledBus.agents(in: workspaceSession)
        }
    }

    @Test("builds attach commands without running swarm")
    func buildsAttachCommand() async {
        let (foundBus, foundRunner) = makeBus([], resolvedExecutable: "/usr/local/bin/swarm")
        let found = foundBus.attachCommand(for: coderAgent, in: workspaceSession)
        let (missingBus, missingRunner) = makeBus([], resolvedExecutable: nil)
        let missing = missingBus.attachCommand(for: coderAgent, in: workspaceSession)

        #expect(found == SwarmAttachCommand(
            executable: "/usr/local/bin/swarm", arguments: ["attach", "coder-1"],
            environment: sessionEnvironment
        ))
        #expect(missing == SwarmAttachCommand(
            executable: "/opt/swarm", arguments: ["attach", "coder-1"],
            environment: sessionEnvironment
        ))
        #expect(await foundRunner.recordedCalls().isEmpty)
        #expect(await missingRunner.recordedCalls().isEmpty)
    }

    private var adapterEnvironment: [String: String] {
        ["SWARM_ADAPTER": "tmux-solo"]
    }

    private var sessionEnvironment: [String: String] {
        [
            "SWARM_ADAPTER": "tmux-solo",
            "SWARM_SESSION_ID": workspaceSession.rawValue,
            "SWARM_AGENT_ID": "orchestrator",
        ]
    }

    private var launchEnvironment: [String: String] {
        sessionEnvironment.merging(["PWD": "/workspace/project"]) { _, requested in requested }
    }

    private var herdrSessionEnvironment: [String: String] {
        sessionEnvironment.merging(["SWARM_ADAPTER": "herdr"]) { _, requested in requested }
    }

    private func discoveredSession(adapter: String?) -> SwarmSession {
        SwarmSession(
            id: SwarmSessionID("10"), talkMode: "lane", adapter: adapter,
            cwd: "/workspace/project", createdAt: 1,
            chairLog: nil, agents: 2, messages: 4, lastMessageAt: nil
        )
    }

    private func makeBus(
        _ outcomes: [ScriptedRunner.Outcome], resolvedExecutable: String? = "/opt/swarm"
    ) -> (SwarmCLIBus, ScriptedRunner) {
        let runner = ScriptedRunner(outcomes)
        let bus = SwarmCLIBus(
            environment: ["SWARM_BIN": " /opt/swarm "], cwd: scratchDirectory,
            resolveExecutable: { _ in resolvedExecutable },
            run: { executable, arguments, cwd, environment, stdin, timeout in
                try await runner.run(
                    executable: executable, arguments: arguments, cwd: cwd,
                    environment: environment, stdin: stdin, timeout: timeout
                )
            }
        )
        return (bus, runner)
    }

    private func call(
        _ arguments: [String], cwd: String? = nil,
        environment: [String: String], stdin: String? = nil,
        timeout: Duration = .seconds(20)
    ) -> RecordedCall {
        RecordedCall(
            executable: "/opt/swarm", arguments: arguments,
            cwd: cwd ?? scratchDirectory, environment: environment,
            stdin: stdin, timeout: timeout
        )
    }

    @Test("a Codex chair's new folder is marked trusted once, after the existing config")
    func codexTrustIsAddedOnce() throws {
        let workspace = "/Users/owner/swarm/workspaces.noindex/thine/new-chat"
        let existing = "model = \"gpt-5.5\"\n\n[projects.\"/Users/owner/other\"]\ntrust_level = \"trusted\""

        let trusted = try #require(CodexProjectTrust.config(existing, trusting: workspace))

        #expect(trusted.hasPrefix(existing + "\n"))
        #expect(trusted.hasSuffix("[projects.\"\(workspace)\"]\ntrust_level = \"trusted\"\n"))
        #expect(CodexProjectTrust.config(trusted, trusting: workspace) == nil)
    }

    private func expectFailure(
        _ outcome: ScriptedRunner.Outcome, _ expected: SwarmProfileError
    ) async {
        let (bus, _) = makeBus([outcome])
        await #expect(throws: expected) {
            try await bus.agents(in: workspaceSession)
        }
    }
}

private actor ChairLaunchRecorder {
    private(set) var plan: SwarmChairLaunchPlan?

    func record(_ plan: SwarmChairLaunchPlan) {
        self.plan = plan
    }
}

private actor AdapterRecordingSwarmBus: SwarmBus {
    enum Call: Sendable, Equatable {
        case agents(SwarmSessionID, String)
        case messages(SwarmSessionID, Int, String)
        case type(SwarmSessionID, SwarmAgentID, String, String)
        case interrupt(SwarmSessionID, SwarmAgentID, String)
    }

    private var calls: [Call] = []
    private var unused: SwarmProfileError { .failed("unused fake bus call") }

    func startSession() async throws -> SwarmSessionID { throw unused }

    func launch(
        _ agent: SwarmAgentID, role: String, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch { throw unused }

    func agents(in session: SwarmSessionID, adapter: String) async throws -> [SwarmAgent] {
        calls.append(.agents(session, adapter))
        return []
    }

    func messages(
        in session: SwarmSessionID, after seq: Int, adapter: String
    ) async throws -> [SwarmMessage] {
        calls.append(.messages(session, seq, adapter))
        return []
    }

    func sessions() async throws -> [SwarmSession] { throw unused }

    func send(
        _ body: String, to agent: SwarmAgentID, in session: SwarmSessionID
    ) async throws -> Int { throw unused }

    func type(
        _ text: String, to agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        calls.append(.type(session, agent, text, adapter))
    }

    func interrupt(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        calls.append(.interrupt(session, agent, adapter))
    }

    func ack(_ seq: Int, in session: SwarmSessionID) async throws { throw unused }
    func sweep(in session: SwarmSessionID) async throws { throw unused }
    func close(_ agent: SwarmAgentID, in session: SwarmSessionID) async throws { throw unused }

    nonisolated func attachCommand(
        for agent: SwarmAgentID, in session: SwarmSessionID
    ) -> SwarmAttachCommand {
        SwarmAttachCommand(executable: "swarm", arguments: [], environment: [:])
    }

    func recordedCalls() -> [Call] { calls }
}

private struct RecordedCall: Sendable, Equatable {
    var executable: String
    var arguments: [String]
    var cwd: String
    var environment: [String: String]
    var stdin: String?
    var timeout: Duration
}

private actor ScriptedRunner {
    enum Outcome: Sendable {
        case result(status: Int32 = 0, stdout: String = "", stderr: String = "")
        case shellError(status: Int32, stderr: String)
        case cancelled
        case lost
    }

    private enum RunnerFailure: Error { case exhausted, lost }

    private var outcomes: [Outcome]
    private var calls: [RecordedCall] = []

    init(_ outcomes: [Outcome]) {
        self.outcomes = outcomes
    }

    func run(
        executable: String, arguments: [String], cwd: String,
        environment: [String: String], stdin: String?, timeout: Duration
    ) throws -> ShellResult {
        calls.append(RecordedCall(
            executable: executable, arguments: arguments, cwd: cwd,
            environment: environment, stdin: stdin, timeout: timeout
        ))
        guard !outcomes.isEmpty else { throw RunnerFailure.exhausted }
        switch outcomes.removeFirst() {
        case .result(let status, let stdout, let stderr):
            return ShellResult(status: status, stdout: stdout, stderr: stderr)
        case .shellError(let status, let stderr):
            throw ShellError(command: executable, status: status, stderr: stderr)
        case .cancelled:
            throw CancellationError()
        case .lost:
            throw RunnerFailure.lost
        }
    }

    func recordedCalls() -> [RecordedCall] {
        calls
    }
}
