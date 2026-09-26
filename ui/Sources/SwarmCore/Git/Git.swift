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
        _ arguments: [String], in directory: String,
        timeout: Duration? = nil, outputLimit: Int = 64 * 1024 * 1024
    ) async throws -> GitOutput {
        let result = try await Shell.runBytes("git", arguments, cwd: directory, env: [
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_OPTIONAL_LOCKS": "0",
        ], timeout: timeout, outputLimit: outputLimit)
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
    static func checkRaw(_ arguments: [String], in directory: String) async throws -> GitOutput {
        let result = try await runRaw(arguments, in: directory)
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
    public static func worktrees(of repo: String) async throws -> [WorktreeEntry] {
        WorktreeListing.parse(try await checkRaw(["worktree", "list", "--porcelain", "-z"], in: repo).stdout)
    }
}
