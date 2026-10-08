import Foundation

/// A finished git process whose stdout is kept as bytes.
///
/// Paths are byte strings on macOS and Linux, and git's `-z` output hands them back verbatim.
/// Decoding stdout to a `String` first would silently rewrite anything that is not valid UTF-8,
/// so the parsers work from `Data` and decode one field at a time.
struct GitOutput: Sendable {
    let status: Int32
    let stdout: Data
    let stderr: String

    var ok: Bool { status == 0 }
}

/// Running git, and the facts every other part of `Git` asks it for.
public enum Git {
    static func runRaw(
        _ arguments: [String], in directory: String, env: [String: String] = [:],
        timeout: Duration? = nil, outputLimit: Int = 64 * 1024 * 1024
    ) async throws -> GitOutput {
        let result = try await Shell.runBytes("git", arguments, cwd: directory, env: [
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_OPTIONAL_LOCKS": "0",
        ].merging(env) { $1 }, timeout: timeout, outputLimit: outputLimit)
        return GitOutput(
            status: result.status,
            stdout: result.stdout,
            stderr: String(decoding: result.stderr, as: UTF8.self)
        )
    }

    /// `runRaw` that refuses to hand back output from a failed command.
    ///
    /// A broken repository, a base branch that no longer exists or a contended `index.lock` all
    /// exit non-zero with empty stdout. Treating that as "no changes" shows a clean worktree to
    /// someone who has plenty of work in it, which is the worst possible lie to tell here.
    static func checkRaw(
        _ arguments: [String], in directory: String, env: [String: String] = [:], timeout: Duration? = nil
    ) async throws -> GitOutput {
        let result = try await runRaw(arguments, in: directory, env: env, timeout: timeout)
        guard result.ok else {
            throw error(arguments, result.status, result.stderr, String(decoding: result.stdout, as: UTF8.self))
        }
        return result
    }

    static func error(_ arguments: [String], _ status: Int32, _ stderr: String, _ stdout: String) -> ShellError {
        ShellError(
            command: "git " + arguments.joined(separator: " "),
            status: status,
            stderr: stderr.isEmpty ? stdout : stderr
        )
    }
}

extension Git {
    /// `git init` with the user's own `init.defaultBranch`.
    public static func initialize(at path: String) async throws {
        _ = try await checkRaw(["init", "-q"], in: path)
    }

    public static func initializeProject(at path: String) async throws -> FirstCommit {
        try await initializeProject(at: path, environment: [:])
    }

    static func initializeProject(at path: String, environment: [String: String]) async throws -> FirstCommit {
        _ = try await checkRaw(["init", "-q"], in: path, env: environment)
        let ignore = URL(fileURLWithPath: path).appendingPathComponent(".gitignore")
        let existing: String
        do {
            existing = try String(contentsOf: ignore, encoding: .utf8)
        } catch CocoaError.fileReadNoSuchFile {
            existing = ""
        }
        if !existing.components(separatedBy: .newlines).contains("tmp/") {
            let separator = existing.isEmpty || existing.hasSuffix("\n") ? "" : "\n"
            try (existing + separator + "tmp/\n").write(to: ignore, atomically: true, encoding: .utf8)
        }
        for key in ["user.name", "user.email"] {
            let identity = try await runRaw(["config", key], in: path, env: environment)
            if identity.status == 1 { return .skippedNoIdentity }
            guard identity.ok else {
                throw error(["config", key], identity.status, identity.stderr, String(decoding: identity.stdout, as: UTF8.self))
            }
            if String(decoding: identity.stdout, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return .skippedNoIdentity
            }
        }
        _ = try await checkRaw(["add", ".gitignore"], in: path, env: environment)
        _ = try await checkRaw(["commit", "-q", "-m", "Create project"], in: path, env: environment)
        return .made
    }

    /// Git's own answer, which also knows a bare clone that `repositoryPaths` does not read. It is
    /// false only when git says "not a git repository"; a fault (no git, dubious ownership) counts
    /// as true, so the app never offers `git init` on a doubt.
    public static func isRepository(at path: String) async -> Bool {
        // The C locale keeps git's message in English, which the check below reads.
        guard let result = try? await runRaw(["rev-parse", "--git-dir"], in: path, env: ["LC_ALL": "C"]) else {
            return true
        }
        return result.ok || !result.stderr.contains("not a git repository")
    }

    public static func worktrees(of repo: String) async throws -> [WorktreeEntry] {
        WorktreeListing.parse(try await checkRaw(["worktree", "list", "--porcelain", "-z"], in: repo).stdout)
    }

    public static func pruneWorktrees(in commonDirectory: String) async throws {
        _ = try await checkRaw(["worktree", "prune"], in: commonDirectory)
    }
}

public enum FirstCommit: Sendable, Equatable {
    case made, skippedNoIdentity
}
