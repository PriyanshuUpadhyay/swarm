import Foundation

/// The parent of the `.swarm` data directory that this app and every `swarm` it starts use.
///
/// The same rule as the CLI's `paths::resolve_home` (ADR 0027). An explicit SWARM_HOME wins. Else
/// a `main` build, a detached build, or a build with no branch uses HOME, and any other branch uses
/// `HOME/.swarm-<branch>`, with every character outside `[A-Za-z0-9._-]` made a `-`.
public enum SwarmHome {
    public static func resolve(swarmHome: String?, home: String, branch: String?) -> String {
        if let swarmHome { return swarmHome }
        let branch = branch ?? ""
        if ["", "main", "HEAD", "unknown"].contains(branch) { return home }
        let safe = String(branch.map { character in
            character.isASCII && (character.isLetter || character.isNumber || "._-".contains(character))
                ? character : "-"
        })
        return URL(fileURLWithPath: home).appendingPathComponent(".swarm-\(safe)").path
    }

    /// This app's home, from its environment and the branch `Tools/build.sh` wrote into Info.plist.
    static func current(environment: [String: String]) -> String {
        resolve(
            swarmHome: environment["SWARM_HOME"],
            home: environment["HOME"] ?? NSHomeDirectory(),
            branch: Bundle.main.object(forInfoDictionaryKey: "SwarmBuildBranch") as? String
        )
    }
}
