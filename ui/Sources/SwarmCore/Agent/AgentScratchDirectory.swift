import Foundation

/// A temporary directory for CLI reads that have no selected project.
/// This keeps a child process from treating the user's home as a project.
public enum AgentScratchDirectory {
    /// The folder's name. Prefixed, because the temporary directory is shared with everything else
    /// on this Mac and an unqualified `agent-scratch` in there says nothing about whose it is.
    public static let folderName = "swarm-agent-scratch"

    /// The path this folder has inside a given directory. Pure, so the suite can ask about a
    /// machine that is not the one running it.
    public static func path(in base: String) -> String {
        (base as NSString).appendingPathComponent(folderName)
    }

    /// The folder inside `base`, made if it is not there yet.
    ///
    /// Made on every ask rather than once at launch, because macOS reaps the per-user temporary
    /// directory on its own schedule and a folder that was there when the app started is not
    /// necessarily there ten minutes later. Creating it is one syscall against a spawn.
    ///
    /// A base that cannot be written into falls back to `base` itself, and never to the home
    /// directory: a Mac where a folder cannot be made in the temporary directory is broken in ways
    /// this is not the place to report, and standing one level higher is still not standing in `~`.
    public static func make(in base: String) -> String {
        let directory = path(in: base)
        do {
            try FileManager.default.createDirectory(
                atPath: directory, withIntermediateDirectories: true
            )
            return directory
        } catch {
            return base
        }
    }

    /// The one on this machine.
    public static func current() -> String {
        make(in: NSTemporaryDirectory())
    }
}
