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
                let inherited = ChildProcessEnvironment.removingInheritedAgentIdentity(
                    from: Shell.environment()
                )
                return try await Shell.run(
                    executable, arguments, cwd: cwd,
                    replacingEnvironment: inherited.merging(environment) { _, requested in requested },
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

    public func startChairSession(
        chair: SwarmChair?, directory: String
    ) async throws -> SwarmSessionID {
        _ = try await call(["init"], adapter: "tmux-solo", directory: directory)
        var arguments = ["session", "new", "lane"]
        if let chair { arguments += ["--chair", chair.argument] }
        let created = try await call(arguments, adapter: "tmux-solo", directory: directory)
        let value = created.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else {
            throw SwarmProfileError.failed("swarm returned an invalid session id")
        }
        return SwarmSessionID(value)
    }

    public func setChair(_ chair: SwarmChair, in session: SwarmSessionID) async throws {
        _ = try await call(
            ["session", "chair", chair.argument], in: session, adapter: "tmux-solo"
        )
    }

    public func launch(
        _ agent: SwarmAgentID, role: String, provider: String, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch {
        var arguments = ["launch", agent.rawValue, role, "--provider", provider]
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

    public func agents(
        in session: SwarmSessionID, adapter: String
    ) async throws -> [SwarmAgent] {
        try await listAgents(in: session, adapter: adapter).agents
    }

    public func agentListing(
        in session: SwarmSessionID, adapter: String
    ) async throws -> SwarmAgentList {
        try await listAgents(in: session, adapter: adapter)
    }

    public func messages(
        in session: SwarmSessionID, after seq: Int, adapter: String
    ) async throws -> [SwarmMessage] {
        return try await read(
            ["messages", "--json", "--after", String(seq)],
            in: session, adapter: adapter, as: SwarmMessageList.self
        ).messages
    }

    private func listAgents(
        in session: SwarmSessionID, adapter: String
    ) async throws -> SwarmAgentList {
        try await read(
            ["agents", "--json"], in: session, adapter: adapter, as: SwarmAgentList.self
        )
    }

    /// The CLI resolves chair logs as well as reading bus rows, so it owns this query.
    public func sessions() async throws -> [SwarmSession] {
        try await read(["sessions", "--json"], as: SwarmSessionList.self).sessions
    }

    public func archive(_ sessions: [SwarmSessionID]) async throws {
        guard !sessions.isEmpty else { return }
        _ = try await call(["session", "archive"] + sessions.map(\.rawValue))
    }

    public func linkChat(_ newSession: SwarmSessionID, after oldSession: SwarmSessionID) async throws {
        _ = try await call(["session", "continue", newSession.rawValue, oldSession.rawValue])
    }

    public func type(
        _ text: String, to agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        _ = try await call(
            ["type", agent.rawValue], in: session, adapter: adapter, stdin: text
        )
    }

    public func interrupt(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        _ = try await call(["interrupt", agent.rawValue], in: session, adapter: adapter)
    }

    public func close(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        _ = try await call(["close", agent.rawValue], in: session, adapter: adapter)
    }

    public func attachCommand(for agent: SwarmAgentID, in session: SwarmSessionID) -> SwarmAttachCommand {
        SwarmAttachCommand(
            executable: resolveExecutable(executable) ?? executable,
            arguments: ["attach", agent.rawValue],
            environment: environment(for: session)
        )
    }

    private func read<Value: Decodable>(
        _ arguments: [String], in session: SwarmSessionID? = nil,
        adapter: String? = nil, as type: Value.Type
    ) async throws -> Value {
        let result = try await call(arguments, in: session, adapter: adapter)
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
        adapter: String? = nil, directory: String? = nil, stdin: String? = nil,
        timeout: Duration = .seconds(20)
    ) async throws -> ShellResult {
        let result: ShellResult
        do {
            var environment = environment(for: session, adapter: adapter)
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

    private func environment(
        for session: SwarmSessionID?, adapter: String? = nil
    ) -> [String: String] {
        var environment = ["SWARM_ADAPTER": adapter ?? SwarmSessionInteraction.defaultAdapter]
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
