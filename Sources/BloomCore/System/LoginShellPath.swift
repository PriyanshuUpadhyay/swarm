import Foundation
import Synchronization

/// The PATH the user's own login shell has, asked for once per launch.
///
/// `ExecutableSearchPath` guesses. It names the directories the package managers people actually
/// use put binaries in, and that list is right often enough that Bloom shipped on it. What it
/// cannot do is know about the line somebody wrote in their own `.zshrc` years ago, which is where
/// a surprising amount of a working developer's PATH comes from: a company toolchain under
/// `~/work/bin`, a Herd or a Valet shim, a language manager nobody here has heard of.
///
/// **The report that forced this was one machine disagreeing with itself.** A terminal pane in
/// Bloom runs the real login shell (`TerminalLaunch.loginShell`), so `./.bloom/setup.sh` typed
/// into it found every binary the user had. The same script started by Run Setup got the guessed
/// PATH and could not find them, and the user's fix was to edit `$PATH` inside the setup script,
/// which is a thing nobody should have to work out. `ScriptLaunch`'s own header says what the
/// promise is: the script run from a terminal and the script started by Bloom should be the same
/// program. It cannot be if the two do not agree on where the programs are.
///
/// # Why this is safe to do, when the comment on `ExecutableSearchPath` says it is not
///
/// That comment is right about the danger and wrong about the conclusion. Startup files are
/// arbitrary user code: they may prompt, print, or never return. So this never lets any of that
/// reach a caller.
///
///   - It runs **once** per launch, started at `applicationWillFinishLaunching`, so the usual cost
///     to anything asking for it later is zero.
///   - It runs under a **hard timeout**. `CapturedProcess` signals the process group, so a
///     `.zshrc` that blocks forever costs five seconds once and then Bloom carries on with the
///     guess list exactly as before.
///   - stdin is a pipe that is closed immediately, so a startup file that reads gets EOF rather
///     than a wait nobody can see.
///   - It asks `/usr/bin/env` rather than the shell's own `printf "$PATH"`, because `$PATH` is a
///     list in fish and a string everywhere else and the quoting that is correct in one is wrong
///     in the other. `env -0` is the same program with the same output under every shell.
///   - The output is parsed for a NUL separated `PATH=` record, so a shell that prints a banner,
///     a version notice or a powerlevel10k instant prompt into stdout cannot corrupt the answer.
///     See `directories(inEnvironmentDump:)`.
///
/// Anything unexpected, a non-zero exit, a timeout, no `PATH=` record at all, means an empty
/// answer, and an empty answer changes nothing.
///
/// # The escape hatch
///
/// `BLOOM_LOGIN_SHELL_PATH=0` skips the probe. It is what `Tools/test-core.sh` sets, so the suite
/// never spawns a login shell for a `runSetup` test, and it is the answer for a machine whose
/// startup files genuinely cannot be run headless.
public enum LoginShellPath {
    /// How long a startup file gets before it is killed and the guess list is kept.
    ///
    /// Long enough for a slow `.zshrc`, measured at about 300ms on the owner's machine with a
    /// full oh-my-zsh, and short enough that a hostile one is a pause rather than a hang. It is
    /// paid at most once, and only by the first thing that needs a script's PATH before the probe
    /// started at launch has come back.
    public static let timeout: Duration = .seconds(5)

    /// The one probe, whoever asks for it first.
    private static let probe = Mutex<Task<Void, Never>?>(nil)

    /// Start the probe without waiting for it. Called at launch; calling it twice does nothing.
    public static func begin() {
        _ = task()
    }

    /// Wait for the probe to have finished, starting it if nothing has.
    ///
    /// Called before a setup or archive script is spawned, which is the one place where a PATH
    /// that arrives a moment late is a script that fails rather than a lookup that is slightly
    /// worse. Everywhere else takes whatever has landed.
    public static func ready() async {
        await task().value
    }

    private static func task() -> Task<Void, Never> {
        probe.withLock { existing in
            if let existing { return existing }
            let started = Task<Void, Never> {
                Shell.adoptLoginShellPath(await discover())
            }
            existing = started
            return started
        }
    }

    /// Ask the login shell what its PATH is. Empty means "nothing learned", never an error.
    static func discover(
        shell: String = LoginShell.path(),
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) async -> [String] {
        guard environment["BLOOM_LOGIN_SHELL_PATH"] != "0" else { return [] }
        // Separate flags rather than `-ilc`, because fish does not parse a combined short option
        // group and would take the whole thing as one unknown flag. A shell that rejects any of
        // the three prints its complaint and exits, which lands in the same empty answer as any
        // other failure.
        let result = try? await Shell.runBytes(
            shell, ["-i", "-l", "-c", "/usr/bin/env -0"], timeout: timeout
        )
        guard let result, result.status == 0 else { return [] }
        return directories(inEnvironmentDump: result.stdout)
    }

    /// The PATH entries out of `env -0` output.
    ///
    /// NUL separated, so a value containing a newline cannot be read as the start of the next
    /// record, which is the whole reason for `-0` over plain `env`. Each record is then trimmed
    /// back to whatever follows its last newline: an interactive shell that printed a banner with
    /// no trailing newline leaves that banner glued to the front of the first record, and
    /// `PATH=` would not be found at the front of it.
    ///
    /// Only absolute entries survive. A relative entry on PATH is a real thing and a bad one, and
    /// resolving it against Bloom's working directory rather than the user's would mean the same
    /// string naming two different directories in the two places this exists to reconcile.
    static func directories(inEnvironmentDump data: Data) -> [String] {
        for record in data.split(separator: 0) {
            let text = String(decoding: record, as: UTF8.self)
            let last = text.components(separatedBy: "\n").last ?? text
            guard last.hasPrefix("PATH=") else { continue }
            return unique(String(last.dropFirst("PATH=".count)).components(separatedBy: ":"))
                .filter { $0.hasPrefix("/") }
        }
        return []
    }

    /// The PATH a spawned process gets, most trusted first.
    ///
    /// The login shell's answer leads, because it is the only one of the three that is a fact
    /// rather than a guess, and because a user who put `~/.rbenv/shims` in front of
    /// `/usr/bin/ruby` meant it. What Bloom itself was launched with comes next, and the guessed
    /// directories last: they are there to rescue a machine the probe could not read, and a
    /// duplicate of something already named earlier is dropped rather than moved.
    static func merge(discovered: [String], inherited: [String], guessed: [String]) -> [String] {
        unique(discovered + inherited + guessed).filter { !$0.isEmpty }
    }

    private static func unique(_ entries: [String]) -> [String] {
        var seen = Set<String>()
        return entries.filter { seen.insert($0).inserted }
    }
}
