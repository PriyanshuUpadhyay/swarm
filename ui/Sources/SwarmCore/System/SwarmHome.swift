import Foundation

/// The parent of the `.swarm` data directory that this app and every `swarm` it starts use.
///
/// The same rule as the CLI's `paths::home` (ADR 0027). An explicit SWARM_HOME wins. Else a release
/// build (`""`) uses HOME, and a dev build, `main` and a detached `HEAD` too, uses `HOME/<folder>`.
/// An empty SWARM_HOME is kept, not read as unset: `paths::home` refuses it ("SWARM_HOME is set
/// but empty"), so every `swarm` the app starts fails with that message instead of guessing a home.
public enum SwarmHome {
    public static func resolve(swarmHome: String?, home: () -> String, branch: String) -> String {
        if let swarmHome { return swarmHome }
        let home = home()
        guard let folder = folder(branch: branch) else { return home }
        return URL(fileURLWithPath: home).appendingPathComponent(folder).path
    }

    /// Byte for byte the CLI's `paths::branch_folder`; see its comment for the rule.
    static func folder(branch: String) -> String? {
        if branch.isEmpty { return nil }
        let bytes = Array(branch.utf8)
        func safe(_ byte: UInt8) -> Bool {
            switch byte {
            case UInt8(ascii: "a")...UInt8(ascii: "z"), UInt8(ascii: "0")...UInt8(ascii: "9"),
                 UInt8(ascii: "."), UInt8(ascii: "_"), UInt8(ascii: "-"):
                true
            default: false
            }
        }
        if bytes.count <= 200, bytes.allSatisfy(safe), bytes[0] != UInt8(ascii: "."),
           bytes[0] != UInt8(ascii: "-") {
            return ".swarm-\(branch)"
        }
        let slug = String(
            decoding: bytes.prefix(200).map { byte -> UInt8 in
                let lowered = (UInt8(ascii: "A")...UInt8(ascii: "Z")).contains(byte) ? byte + 32 : byte
                return safe(lowered) ? lowered : UInt8(ascii: "-")
            }, as: UTF8.self)
        let hash = bytes.reduce(UInt64(0xcbf2_9ce4_8422_2325)) { ($0 ^ UInt64($1)) &* 0x0100_0000_01b3 }
        return ".swarm-\(slug)+\(String(format: "%016llx", hash))"
    }

    /// This app's home, from its environment and the branch `Tools/build.sh` wrote into Info.plist.
    /// A missing key is no branch. HOME is read only when SWARM_HOME is unset.
    static func current(environment: [String: String]) -> String {
        resolve(
            swarmHome: environment["SWARM_HOME"],
            home: { environment["HOME"] ?? NSHomeDirectory() },
            branch: Bundle.main.object(forInfoDictionaryKey: "SwarmBuildBranch") as? String ?? ""
        )
    }
}
