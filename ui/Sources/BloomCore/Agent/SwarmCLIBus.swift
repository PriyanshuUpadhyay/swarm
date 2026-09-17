import Foundation

/// Runs the swarm bus contract through the `swarm` CLI.
public struct SwarmCLIBus: SwarmBus {
    typealias Runner = @Sendable (
        _ executable: String, _ arguments: [String], _ cwd: String,
        _ environment: [String: String], _ stdin: String?, _ timeout: Duration
    ) async throws -> ShellResult
    typealias ExecutableResolver = @Sendable (String) -> String?

    private let executable: String
    private let cwd: String?
    private let resolveExecutable: ExecutableResolver
    private let run: Runner

    public init() {
        self.init(
            environment: ProcessInfo.processInfo.environment,
            resolveExecutable: { Shell.which($0) },
            run: { executable, arguments, cwd, environment, stdin, timeout in
                try await Shell.run(
                    executable, arguments, cwd: cwd, env: environment,
                    stdin: stdin, timeout: timeout
                )
            }
        )
    }

    init(
        environment: [String: String], cwd: String? = nil,
        resolveExecutable: @escaping ExecutableResolver = { Shell.which($0) },
        run: @escaping Runner
    ) {
        let configured = environment["SWARM_BIN"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        executable = configured.flatMap { $0.isEmpty ? nil : $0 } ?? "swarm"
        self.cwd = cwd
        self.resolveExecutable = resolveExecutable
        self.run = run
    }

    public func startSession() async throws -> SwarmSessionID {
        _ = try await call(["init"])
        let created = try await call(["session", "new", "lane"])
        let value = created.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let number = Int(value), number > 0 else {
            throw SwarmProfileError.failed("swarm returned an invalid session id")
        }

        let session = SwarmSessionID(value)
        _ = try await call(["agent", "add", "orchestrator", "orchestrator"], in: session)
        return session
    }

    public func launch(
        _ agent: SwarmAgentID, role: String, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch {
        var arguments = ["launch", agent.rawValue, role]
        if let account { arguments += ["--account", account] }
        let result = try await call(
            arguments, in: session, directory: directory, timeout: .seconds(60)
        )
        guard let pane = firstLine(in: result.stdout) else {
            throw SwarmProfileError.failed("swarm returned no pane")
        }
        let reported = result.stderr.components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { $0.hasPrefix("account ") }
            .map { String($0.dropFirst("account ".count)) }
        return SwarmLaunch(pane: pane, account: reported)
    }

    public func agents(in session: SwarmSessionID) async throws -> [SwarmAgent] {
        try await read(["agents", "--json"], in: session, as: SwarmAgentList.self).agents
    }

    public func messages(in session: SwarmSessionID, after seq: Int) async throws -> [SwarmMessage] {
        try await read(
            ["messages", "--json", "--after", String(seq)],
            in: session, as: SwarmMessageList.self
        ).messages
    }

    public func send(
        _ body: String, to agent: SwarmAgentID, in session: SwarmSessionID
    ) async throws -> Int {
        let result = try await call(["send", agent.rawValue, "ask"], in: session, stdin: body)
        let value = result.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let seq = Int(value), seq > 0 else {
            throw SwarmProfileError.failed("swarm returned an invalid message sequence")
        }
        return seq
    }

    public func ack(_ seq: Int, in session: SwarmSessionID) async throws {
        _ = try await call(["ack", String(seq)], in: session)
    }

    public func sweep(in session: SwarmSessionID) async throws {
        _ = try await call(["sweep"], in: session)
    }

    public func close(_ agent: SwarmAgentID, in session: SwarmSessionID) async throws {
        _ = try await call(["close", agent.rawValue], in: session)
    }

    public func attachCommand(for agent: SwarmAgentID, in session: SwarmSessionID) -> SwarmAttachCommand {
        SwarmAttachCommand(
            executable: resolveExecutable(executable) ?? executable,
            arguments: ["attach", agent.rawValue],
            environment: environment(for: session)
        )
    }

    private func read<Value: Decodable>(
        _ arguments: [String], in session: SwarmSessionID, as type: Value.Type
    ) async throws -> Value {
        let result = try await call(arguments, in: session)
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(type, from: Data(result.stdout.utf8))
        } catch {
            throw SwarmProfileError.failed("swarm returned invalid JSON")
        }
    }

    private func call(
        _ arguments: [String], in session: SwarmSessionID? = nil,
        directory: String? = nil, stdin: String? = nil,
        timeout: Duration = .seconds(20)
    ) async throws -> ShellResult {
        let result: ShellResult
        do {
            var environment = environment(for: session)
            if let directory { environment["PWD"] = directory }
            result = try await run(
                executable, arguments, directory ?? cwd ?? AgentScratchDirectory.current(),
                environment, stdin, timeout
            )
        } catch let error as CancellationError {
            throw error
        } catch let error as ShellError where error.status == 127 {
            throw SwarmProfileError.unavailable(error.stderr)
        } catch {
            throw SwarmProfileError.failed(String(describing: error))
        }

        guard result.ok else {
            let stderr = result.stderr.trimmingCharacters(in: .whitespacesAndNewlines)
            let output = stderr.isEmpty
                ? result.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
                : stderr
            let message = firstLine(in: output) ?? "swarm exited \(result.status)"
            throw SwarmProfileError.failed(message)
        }
        return result
    }

    private func environment(for session: SwarmSessionID?) -> [String: String] {
        var environment = ["SWARM_ADAPTER": "tmux-solo"]
        if let session {
            environment["SWARM_SESSION_ID"] = session.rawValue
            environment["SWARM_AGENT_ID"] = "orchestrator"
        }
        return environment
    }

    private func firstLine(in output: String) -> String? {
        output.components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
    }
}
