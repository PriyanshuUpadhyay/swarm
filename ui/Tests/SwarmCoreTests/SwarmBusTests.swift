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
        ])

        let agents = try await bus.agents(in: workspaceSession)
        let messages = try await bus.messages(in: workspaceSession, after: 7)
        let seq = try await bus.send("Fix the parser", to: coderAgent, in: workspaceSession)
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
            call(["ack", "9"], environment: sessionEnvironment),
            call(["sweep"], environment: sessionEnvironment),
            call(["close", "coder-1"], environment: sessionEnvironment),
        ])
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

    private func expectFailure(
        _ outcome: ScriptedRunner.Outcome, _ expected: SwarmProfileError
    ) async {
        let (bus, _) = makeBus([outcome])
        await #expect(throws: expected) {
            try await bus.agents(in: workspaceSession)
        }
    }
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
