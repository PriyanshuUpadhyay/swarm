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
    /// Nil means every read runs the CLI, and that is the default for the initialiser the suite
    /// uses. A test hands in its own `run` and asserts on the arguments, so a store reading this
    /// machine's real `~/.swarm/swarm.db` would make those tests depend on whatever the developer
    /// happened to be running. Only `init()`, the live app's, connects the store.
    private let store: SwarmBusStore?
    private let liveness: SwarmPaneLiveness?

    public init() {
        self.init(
            environment: ProcessInfo.processInfo.environment,
            resolveExecutable: { Shell.which($0) },
            store: .shared, liveness: .shared,
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
        store: SwarmBusStore? = nil, liveness: SwarmPaneLiveness? = nil,
        run: @escaping Runner
    ) {
        let configured = environment["SWARM_BIN"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        executable = configured.flatMap { $0.isEmpty ? nil : $0 } ?? "swarm"
        self.cwd = cwd
        self.resolveExecutable = resolveExecutable
        self.store = store
        self.liveness = liveness
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

    public func startChairSession(
        chair: SwarmChair?, directory: String
    ) async throws -> SwarmSessionID {
        _ = try await call(["init"], adapter: "tmux", directory: directory)
        var arguments = ["session", "new", "lane"]
        if let chair { arguments += ["--chair", chair.argument] }
        let created = try await call(arguments, adapter: "tmux", directory: directory)
        let value = created.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let number = Int(value), number > 0 else {
            throw SwarmProfileError.failed("swarm returned an invalid session id")
        }
        return SwarmSessionID(value)
    }

    public func setChair(_ chair: SwarmChair, in session: SwarmSessionID) async throws {
        _ = try await call(
            ["session", "chair", chair.argument], in: session, adapter: "tmux"
        )
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

    /// The roster from `SwarmBusStore`, and `alive` from the CLI on `SwarmPaneLiveness.interval`.
    ///
    /// Split because the two halves change at completely different rates and cost completely
    /// different amounts. The roster is three columns of a SQLite table. `alive` is the adapter's
    /// `list`, which is a second process inside the first one, and it is why `swarm agents --json`
    /// showed a 910ms median in the performance log.
    public func agents(
        in session: SwarmSessionID, adapter: String
    ) async throws -> [SwarmAgent] {
        guard let store, let liveness else {
            return try await listAgents(in: session, adapter: adapter)
        }
        let roster: [SwarmAgent]
        do {
            roster = try await store.agents(in: session)
        } catch {
            return try await listAgents(in: session, adapter: adapter)
        }
        let alive = await liveness.panes(in: session) {
            try await listAgents(in: session, adapter: adapter)
        }
        return roster.map { agent in
            var agent = agent
            // Only where the recorded pane still matches. An agent relaunched into a new pane
            // since the last liveness pass would otherwise inherit the dead pane's answer.
            if let pane = agent.pane, let known = alive[agent.id], known.pane == pane {
                agent.alive = known.alive
            }
            return agent
        }
    }

    public func messages(
        in session: SwarmSessionID, after seq: Int, adapter: String
    ) async throws -> [SwarmMessage] {
        if let store, let messages = try? await store.messages(in: session, after: seq) {
            return messages
        }
        return try await read(
            ["messages", "--json", "--after", String(seq)],
            in: session, adapter: adapter, as: SwarmMessageList.self
        ).messages
    }

    private func listAgents(
        in session: SwarmSessionID, adapter: String
    ) async throws -> [SwarmAgent] {
        try await read(
            ["agents", "--json"], in: session, adapter: adapter, as: SwarmAgentList.self
        ).agents
    }

    /// Still the CLI, and deliberately so.
    ///
    /// `SwarmBusStore` could run this query, and the `WHERE` and `ORDER BY` were checked against
    /// `store.rs:259-271` and matched. `chair_log` is what stops it. swarm does not report the
    /// column: `resolved_chair_log` (`src/main.rs:181`) ignores a path whose file has gone, and
    /// then searches `$CLAUDE_CONFIG_DIR/projects/*/<id>.jsonl` for a Claude chair, or three days
    /// of `$CODEX_HOME/sessions/<day>/rollout-*-<id>.jsonl` for a Codex one. A diff against the
    /// live database found two of nine sessions where the column was null and the CLI had found
    /// the log anyway. That rule is swarm's to own, and copying it here would leave two spellings
    /// of it to drift apart. This is polled every two seconds rather than every second, so it is
    /// also a twelfth of what the two reads above were costing.
    public func sessions() async throws -> [SwarmSession] {
        try await read(["sessions", "--json"], as: SwarmSessionList.self).sessions
    }

    public func archive(_ sessions: [SwarmSessionID]) async throws {
        guard !sessions.isEmpty else { return }
        _ = try await call(["session", "archive"] + sessions.map(\.rawValue))
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
        var environment = ["SWARM_ADAPTER": adapter ?? SwarmSessionInteraction.workspaceAdapter]
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
