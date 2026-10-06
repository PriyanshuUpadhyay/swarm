import Foundation

/// The `swarm` on the login PATH when it is another build than the one inside this app (ADR 0048).
/// The app itself always runs its own copy; this names the copy that Terminal and agents started
/// outside Swarm run.
public struct PathSwarmDrift: Equatable, Sendable {
    /// The PATH entry, as `command -v swarm` prints it.
    public let path: String
    /// `path` with every link resolved, which shows the Homebrew keg behind a `bin` link.
    public let resolved: String
    public let pathLine: String
    public let helperLine: String

    /// What a "Not Now" stores, so the same pair never asks again and a new release or path does.
    public var key: String { [path, pathLine, helperLine].joined(separator: "|") }

    public var fixCommand: String {
        let reinstall = "brew reinstall --cask priyanshuupadhyay/tap/swarm-app"
        return resolved.contains("/Cellar/swarm/")
            ? "brew uninstall priyanshuupadhyay/tap/swarm && \(reinstall)"
            : "Remove \(path), then run \(reinstall)"
    }
}

public enum PathSwarmCheck {
    /// Nil unless a release build (empty `branch`) finds another file on PATH whose
    /// `swarm --version` line differs from the helper's and the pair was not dismissed. A version
    /// that cannot be read in 2 s is nil too, because a guess would nag about a working CLI.
    public static func drift(
        branch: String, pathSwarm: String?, helper: String, dismissed: [String]
    ) async -> PathSwarmDrift? {
        guard branch.isEmpty, let pathSwarm else { return nil }
        let resolved = URL(fileURLWithPath: pathSwarm).resolvingSymlinksInPath().path
        guard resolved != URL(fileURLWithPath: helper).resolvingSymlinksInPath().path,
              let helperLine = await versionLine(helper),
              let pathLine = await versionLine(pathSwarm),
              pathLine != helperLine
        else { return nil }
        let drift = PathSwarmDrift(path: pathSwarm, resolved: resolved, pathLine: pathLine, helperLine: helperLine)
        return dismissed.contains(drift.key) ? nil : drift
    }

    /// This app's check: the login PATH's `swarm` against `Contents/Helpers/swarm`, gated by the
    /// branch that `Tools/build.sh` wrote into Info.plist. Call it after `LoginShellPath.ready()`.
    public static func current(dismissed: [String]) async -> PathSwarmDrift? {
        await drift(
            branch: Bundle.main.object(forInfoDictionaryKey: "SwarmBuildBranch") as? String ?? "",
            pathSwarm: pathSwarm(loginPath: LoginShellPath.discovered),
            helper: Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/swarm").path,
            dismissed: dismissed
        )
    }

    /// The `swarm` that Terminal runs, from the login shell's PATH only. `Shell.which` would also
    /// walk the launch PATH and guessed dirs, and name a stale copy that Terminal never runs.
    static func pathSwarm(loginPath: [String]) -> String? {
        loginPath.lazy.map { ($0 as NSString).appendingPathComponent("swarm") }
            .first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    /// The full `swarm <version> <commit> <branch>` line, so one version from two commits differs.
    static func versionLine(_ executable: String) async -> String? {
        guard FileManager.default.isExecutableFile(atPath: executable),
              let result = try? await Shell.run(executable, ["--version"], timeout: .seconds(2)),
              result.ok,
              let line = result.lines.first?.trimmingCharacters(in: .whitespaces),
              !line.isEmpty
        else { return nil }
        return line
    }
}
