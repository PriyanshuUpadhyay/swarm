import Foundation

/// Reads the roles, accounts and usage that the `swarm` CLI owns.
public struct SwarmCLIProfileSource: SwarmProfileSource {
    typealias Runner = @Sendable (String, [String], String) async throws -> ShellResult

    private let executable: String
    private let cwd: String
    private let run: Runner

    public init() {
        self.init(
            environment: ProcessInfo.processInfo.environment,
            cwd: AgentScratchDirectory.current(),
            run: { executable, arguments, cwd in
                try await Shell.run(executable, arguments, cwd: cwd)
            }
        )
    }

    init(environment: [String: String], cwd: String, run: @escaping Runner) {
        let configured = environment["SWARM_BIN"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        executable = configured.flatMap { $0.isEmpty ? nil : $0 } ?? "swarm"
        self.cwd = cwd
        self.run = run
    }

    public func roles() async throws -> [SwarmRole] {
        try await read(["roles", "--json"], as: SwarmRoleList.self).roles
    }

    public func accounts(provider: String) async throws -> SwarmAccountList {
        try await read(["accounts", "--provider", provider, "--json"], as: SwarmAccountList.self)
    }

    public func usage() async throws -> [SwarmUsageMeter] {
        try await read(["usage", "--json"], as: SwarmUsage.self).meters
    }

    private func read<Value: Decodable>(_ arguments: [String], as type: Value.Type) async throws -> Value {
        let result: ShellResult
        do {
            result = try await run(executable, arguments, cwd)
        } catch let error as ShellError where error.status == 127 {
            throw SwarmProfileError.unavailable(error.stderr)
        } catch {
            throw SwarmProfileError.failed(String(describing: error))
        }

        guard result.ok else {
            let firstLine = result.stderr.components(separatedBy: .newlines).first ?? ""
            throw SwarmProfileError.failed(firstLine)
        }

        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(type, from: Data(result.stdout.utf8))
        } catch {
            throw SwarmProfileError.failed("swarm returned invalid JSON")
        }
    }
}
