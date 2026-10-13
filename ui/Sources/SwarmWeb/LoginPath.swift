// The login shell's PATH for `swarm serve`, and the PATH swarm alert of ADR 0048.
import Foundation

/// A Finder or Dock launch gets only `/usr/bin:/bin:/usr/sbin:/sbin`, so `swarm serve` and the agents
/// it starts would not find tmux, git, gh or an agent CLI. The login shell knows the user's PATH.
enum LoginPath {
    /// Rescues a machine whose login shell could not be read.
    // ponytail: no walk of nvm or fnm version folders; the login shell's answer covers them.
    static func guesses(home: String) -> [String] {
        ["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
            + [".local/bin", ".npm-packages/bin", ".volta/bin", ".yarn/bin", ".bun/bin", ".cargo/bin",
               ".composer/vendor/bin", "bin", ".local/share/mise/shims", ".asdf/shims", ".nodenv/shims",
               "Library/pnpm", ".local/share/pnpm", ".nix-profile/bin"].map { "\(home)/\($0)" }
            + ["/nix/var/nix/profiles/default/bin"]
    }

    /// The login shell's own PATH entries; empty on any failure. Startup files are user code that may
    /// print, prompt or hang, so stdin is closed, the answer is `env -0` (the same output in fish and
    /// zsh), and the shell gets 5 s.
    static func discover() -> [String] {
        let shell = getpwuid(getuid()).flatMap { String(validatingCString: $0.pointee.pw_shell) } ?? "/bin/zsh"
        // Separate flags, because fish does not parse a combined `-ilc`.
        guard let dump = output(shell, ["-i", "-l", "-c", "/usr/bin/env -0"], timeout: 5) else { return [] }
        return entries(inEnvironmentDump: dump)
    }

    /// The PATH entries of `env -0` output. A banner printed with no newline sticks to the first
    /// record, so each record is cut to its last line. Only absolute entries count.
    static func entries(inEnvironmentDump data: Data) -> [String] {
        for record in data.split(separator: 0) {
            let line = String(decoding: record, as: UTF8.self).components(separatedBy: "\n").last ?? ""
            guard line.hasPrefix("PATH=") else { continue }
            return unique(line.dropFirst(5).components(separatedBy: ":")).filter { $0.hasPrefix("/") }
        }
        return []
    }

    /// The login shell's entries first, then what the app was launched with, then the guesses.
    static func merged(login: [String], inherited: String, home: String) -> String {
        unique(login + inherited.components(separatedBy: ":") + guesses(home: home)).filter { !$0.isEmpty }
            .joined(separator: ":")
    }

    static func unique(_ entries: [String]) -> [String] {
        var seen = Set<String>()
        return entries.filter { seen.insert($0).inserted }
    }

    /// Runs `executable` and returns its stdout, or nil on a non-zero exit or after `timeout`. Stdout
    /// goes to a temp file, so a child of a startup file that keeps a pipe open cannot block the read.
    static func output(_ executable: String, _ arguments: [String], timeout: TimeInterval) -> Data? {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("swarm-\(UUID().uuidString)")
        guard FileManager.default.createFile(atPath: file.path, contents: nil),
              let handle = try? FileHandle(forWritingTo: file) else { return nil }
        defer {
            try? handle.close()
            try? FileManager.default.removeItem(at: file)
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = handle
        process.standardError = FileHandle.nullDevice
        let done = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in done.signal() }
        guard (try? process.run()) != nil else { return nil }
        guard done.wait(timeout: .now() + timeout) == .success else {
            process.terminate()
            return nil
        }
        return process.terminationStatus == 0 ? try? Data(contentsOf: file) : nil
    }
}

/// The `swarm` on the login PATH when it is another build than the one inside this app (ADR 0048).
/// The app always runs its own copy; this names the copy that Terminal and agents started elsewhere run.
struct PathDrift: Equatable {
    /// The PATH entry, and the same path with every link resolved (it shows a Homebrew keg).
    let path: String
    let resolved: String
    let pathLine: String
    let helperLine: String

    /// What an answer stores, so the same pair never asks again and a new release or path does.
    var key: String { [path, pathLine, helperLine].joined(separator: "|") }

    /// One fix from every brew state, the same text as the README and `make install`. The old
    /// formula left the tap, so its keg is removed; on no keg that step fails alone. The cask
    /// reinstall then links `swarm`, and `--force` replaces a Swarm.app dragged from the DMG.
    static let brewFix = "brew uninstall swarm; brew reinstall --cask --force priyanshuupadhyay/tap/swarm-app"

    var fixCommand: String {
        resolved.contains("/Cellar/swarm/") ? Self.brewFix : "Remove \(path), then run \(Self.brewFix)"
    }

    var message: String {
        "Terminal runs \(pathLine) from \(path). This app runs \(helperLine). Agents that Swarm starts use "
            + "the app's copy, but commands in Terminal and agents started elsewhere use the other one.\n\n\(fixCommand)"
    }

    /// Nil unless a release build (empty `branch`) finds another file on the login PATH whose
    /// `swarm --version` line differs from the helper's, and the pair was not dismissed. A version
    /// that cannot be read in 2 s is nil too, because a guess would nag about a working CLI.
    static func check(branch: String, loginPath: [String], helper: String, dismissed: [String]) -> PathDrift? {
        guard branch.isEmpty,
              let path = loginPath.lazy.map({ ($0 as NSString).appendingPathComponent("swarm") })
                  .first(where: { FileManager.default.isExecutableFile(atPath: $0) })
        else { return nil }
        let resolved = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
        guard resolved != URL(fileURLWithPath: helper).resolvingSymlinksInPath().path,
              let helperLine = versionLine(helper),
              let pathLine = versionLine(path),
              pathLine != helperLine
        else { return nil }
        let drift = PathDrift(path: path, resolved: resolved, pathLine: pathLine, helperLine: helperLine)
        return dismissed.contains(drift.key) ? nil : drift
    }

    /// The full `swarm <version> <commit> <branch>` line, so one version from two commits differs.
    static func versionLine(_ executable: String) -> String? {
        guard FileManager.default.isExecutableFile(atPath: executable),
              let data = LoginPath.output(executable, ["--version"], timeout: 2),
              let line = String(decoding: data, as: UTF8.self).split(separator: "\n").first?
                  .trimmingCharacters(in: .whitespaces),
              !line.isEmpty
        else { return nil }
        return line
    }
}
