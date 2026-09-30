import Foundation

/// Reads the profiles, providers, accounts and usage that the `swarm` CLI owns.
public struct SwarmCLIProfileSource: SwarmProfileSource {
    typealias Runner = @Sendable (String, [String], String) async throws -> ShellResult

    private let executable: String
    private let cwd: String?
    private let run: Runner

    public init() {
        self.init(
            environment: SwarmCLIBus.appEnvironment(),
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

    public func profiles() async throws -> SwarmProfileList {
        try await read(["roles", "--json"], as: SwarmProfileList.self)
    }

    /// Which runner each profile would start now. It reads every provider's accounts, so it is
    /// slower than `profiles()`.
    public func check() async throws -> [SwarmProfileCheck] {
        try await read(["roles", "check", "--json"], as: SwarmProfileCheckList.self).profiles
    }

    public func providers() async throws -> [SwarmProvider] {
        try await read(["providers", "--json"], as: SwarmProviderList.self).providers
    }

    /// Saves one whole profile and returns the file's new revision. It fails when the file
    /// changed after `revision` was read.
    public func save(_ profile: SwarmProfile, revision: String) async throws -> String {
        let json = String(decoding: try JSONEncoder().encode(profile), as: UTF8.self)
        return try await read(
            ["roles", "save", "--revision", revision, json], as: SavedRevision.self
        ).revision
    }

    public func models(provider: String) async throws -> [SwarmModel] {
        try await read(
            ["models", "--provider", provider, "--json"], as: SwarmModelList.self
        ).models
    }

    public func accounts(provider: String) async throws -> SwarmAccountList {
        try await read(["accounts", "--provider", provider, "--json"], as: SwarmAccountList.self)
    }

    public func usage() async throws -> [SwarmUsageMeter] {
        try await read(["usage", "--json"], as: SwarmUsage.self).meters
    }

    private func read<Value: Decodable>(_ arguments: [String], as type: Value.Type) async throws -> Value {
        try decode(try await call(arguments), as: type)
    }

    private func decode<Value: Decodable>(_ result: ShellResult, as type: Value.Type) throws -> Value {
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
            // A refused save names every broken rule, one per line.
            throw SwarmProfileError.failed(output.isEmpty ? "swarm exited \(result.status)" : output)
        }

        return result
    }
}

private struct SavedRevision: Decodable {
    var revision: String
}
