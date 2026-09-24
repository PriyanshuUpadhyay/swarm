import Foundation

/// Reads the roles, accounts and usage that the `swarm` CLI owns.
public struct SwarmCLIProfileSource: SwarmProfileSource {
    typealias Runner = @Sendable (String, [String], String) async throws -> ShellResult

    private let executable: String
    private let cwd: String?
    private let run: Runner

    public init() {
        self.init(
            environment: ProcessInfo.processInfo.environment,
            run: { executable, arguments, cwd in
                // Usage can wait on a network quota read, so it gets the same bounded wait as
                // sibling CLI reads instead of leaving a menu bar refresh alive forever.
                try await Shell.run(executable, arguments, cwd: cwd, timeout: .seconds(20))
            }
        )
    }

    init(environment: [String: String], cwd: String? = nil, run: @escaping Runner) {
        let configured = environment["SWARM_BIN"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        executable = configured.flatMap { $0.isEmpty ? nil : $0 } ?? "swarm"
        self.cwd = cwd
        self.run = run
    }

    public func roles() async throws -> [SwarmRole] {
        try await read(["roles", "--json"], as: SwarmRoleList.self).roles
    }

    public func models(provider: String) async throws -> [SwarmModel] {
        try await read(
            ["models", "--provider", provider, "--json"], as: SwarmModelList.self
        ).models
    }

    /// Saves to the shared router config, so every role using this runner changes.
    public func setModel(_ model: String, for runner: String) async throws {
        let name = model.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else {
            throw SwarmProfileError.failed("Enter a model name")
        }
        _ = try await call(["roles", "set-model", runner, name])
    }

    public func accounts(provider: String) async throws -> SwarmAccountList {
        try await read(["accounts", "--provider", provider, "--json"], as: SwarmAccountList.self)
    }

    public func usage() async throws -> [SwarmUsageMeter] {
        try await read(["usage", "--json"], as: SwarmUsage.self).meters
    }

    private func read<Value: Decodable>(_ arguments: [String], as type: Value.Type) async throws -> Value {
        let result = try await call(arguments)
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(type, from: Data(result.stdout.utf8))
        } catch {
            throw SwarmProfileError.failed("swarm returned invalid JSON")
        }
    }

    private func call(_ arguments: [String]) async throws -> ShellResult {
        let result: ShellResult
        do {
            // swarm needs no project, and remaking the temporary folder per call also survives
            // macOS reaping it while Swarm stays open.
            result = try await run(executable, arguments, cwd ?? AgentScratchDirectory.current())
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
            let firstLine = output.components(separatedBy: .newlines).first { !$0.isEmpty }
                ?? "swarm exited \(result.status)"
            throw SwarmProfileError.failed(firstLine)
        }

        return result
    }
}
