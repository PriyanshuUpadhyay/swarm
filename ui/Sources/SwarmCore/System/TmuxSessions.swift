import Foundation

public enum TerminalKey: String, Sendable, Equatable, CaseIterable {
    case enter, controlC, tab, escape, up, down, left, right
}

/// Where a terminal pane's shell is going to run.
///
/// The two tmux cases carry the same argv, because `new-session -A` already means "attach if it is
/// there, create it if it is not". They stay separate anyway: the difference is the whole feature,
/// and a caller that cannot see it has no way to tell a restored `npm run dev` from a fresh shell.
public enum TerminalStartDecision: Sendable, Equatable {
    /// A pty forked as a child of the app, which dies with the app. The historical behaviour, and
    /// the fallback whenever tmux cannot be used.
    case inProcess
    /// A session that outlived the last quit and is being picked back up.
    case attach(session: String)
    /// Persistence is on, but this pane has no session yet.
    case createFresh(session: String)

    public var session: String? {
        switch self {
        case .inProcess: nil
        case .attach(let name), .createFresh(let name): name
        }
    }
}

/// Naming and argument building for the tmux sessions that keep a pane's shell alive across a quit.
///
/// One session per Swarm *pane*, never per window: `SplitLayout` already owns the geometry, so
/// tmux is only ever asked to hold one shell. Its own splitting, its status line and its prefix key
/// are all turned off, which is what keeps the feature invisible.
public enum TmuxSessions {
    /// Session names carry it so a stray `tmux ls` on our socket reads clearly, and so nothing here
    /// could ever act on a session a person created by hand.
    public static let sessionPrefix = "swarmui"

    /// The one character an id may not contain, which is what makes a name splittable back into
    /// its parts. See `sanitized`.
    private static let separator: Character = "_"

    // MARK: - Naming

    /// `swarmui_<session>_<pane>`, stable across launches while the bus session and pane ids stay fixed.
    public static func sessionName(sessionID: SwarmSessionID, paneID: String) -> String {
        [sessionPrefix, sanitized(sessionID.rawValue), sanitized(paneID)].joined(separator: String(separator))
    }

    public static func paneID(ofSessionName name: String) -> String? {
        parts(of: name)?.pane
    }

    public static func sessionID(ofSessionName name: String) -> String? {
        parts(of: name)?.session
    }

    public static func isSwarmSession(_ name: String) -> Bool {
        parts(of: name) != nil
    }

    private static func parts(of name: String) -> (session: String, pane: String)? {
        let fields = name.split(separator: separator, omittingEmptySubsequences: false)
        guard fields.count == 3, fields[0] == sessionPrefix,
              !fields[1].isEmpty, !fields[2].isEmpty else { return nil }
        return (String(fields[1]), String(fields[2]))
    }

    /// tmux rejects `.` and `:` outright and treats the rest of a name as an addressable target, so
    /// anything outside the safe set is folded away rather than trusted. The underscore is folded
    /// too, because it is what separates the fields above: an id that could contain one would make
    /// a name that cannot be read back. Ids are UUIDs today, and this is what keeps that from being
    /// a load-bearing assumption.
    private static func sanitized(_ id: String) -> String {
        let safe = id.unicodeScalars.map { scalar -> Character in
            let isSafe = (scalar >= "a" && scalar <= "z")
                || (scalar >= "A" && scalar <= "Z")
                || (scalar >= "0" && scalar <= "9")
                || scalar == "-"
            return isSafe ? Character(scalar) : "-"
        }
        return String(safe)
    }

    // MARK: - Socket

    /// tmux is run on a private socket rather than the user's default one.
    ///
    /// A project path gives each repository its own stable socket.
    public static func socketName(projectPath: String) -> String {
        "swarmui-" + fingerprint(projectPath)
    }

    /// Swarm agents share this server so one adapter can address every recorded pane.
    public static let swarmSocketName = "swarm"

    /// FNV-1a rather than `Hashable`, whose seed changes every launch. This value names a socket
    /// that has to be found again tomorrow.
    static func fingerprint(_ value: String) -> String {
        var hash: UInt32 = 2_166_136_261
        for byte in value.utf8 {
            hash ^= UInt32(byte)
            hash &*= 16_777_619
        }
        return String(format: "%08x", hash)
    }

    // MARK: - The decision

    /// What a pane should do when it is about to be drawn.
    ///
    /// `existingSessions` is a snapshot, so it may be stale or, on the very first frame after
    /// launch, empty. That is deliberately harmless: `attach` and `createFresh` produce identical
    /// arguments, and tmux itself resolves which one actually happens. A pane whose session was
    /// killed behind our back therefore gets a fresh shell rather than an error, which is the
    /// requirement.
    public static func decide(
        sessionID: SwarmSessionID,
        paneID: String,
        persistenceEnabled: Bool,
        requiresTmux: Bool = false,
        tmuxAvailable: Bool,
        existingSessions: Set<String>
    ) -> TerminalStartDecision {
        guard (persistenceEnabled || requiresTmux), tmuxAvailable else { return .inProcess }
        let name = sessionName(sessionID: sessionID, paneID: paneID)
        return existingSessions.contains(name) ? .attach(session: name) : .createFresh(session: name)
    }

    public static func parsePanePIDs(_ output: String) -> [String: Int32] {
        var pids: [String: Int32] = [:]
        for line in output.split(separator: "\n") {
            let fields = line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true)
            guard fields.count == 2, let pid = Int32(fields[0]) else { continue }
            let session = fields[1].trimmingCharacters(in: .whitespaces)
            // The first pane of a session wins. Swarm's own sessions hold exactly one, and a
            // second would be one a person made by hand inside a pane, which is theirs.
            guard !session.isEmpty, pids[session] == nil else { continue }
            pids[session] = pid
        }
        return pids
    }

    /// `list-sessions -F '#{session_name}'` output. An empty result is also what tmux prints to
    /// stderr when no server is running, so the caller can ignore the exit status entirely.
    public static func parseSessionList(_ output: String) -> [String] {
        output
            .split(separator: "\n", omittingEmptySubsequences: true)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }

    // MARK: - Configuration

    /// The server configuration, written to disk and passed with `-f`.
    ///
    /// Everything here exists to stop tmux from being noticed. No status line, no prefix key and no
    /// key bindings mean none of the user's keystrokes are ever swallowed on their way to the
    /// shell. `escape-time 0` is the one that is felt if it is missing: without it tmux waits to
    /// see whether an Escape starts a sequence, and vim feels broken.
    ///
    /// The mouse is the exception, and it is on for a reason. A tmux client puts the outer terminal
    /// into its alternate screen, which freezes SwiftTerm's own scrollback, so with the mouse off a
    /// scroll wheel would do nothing at all. On, the wheel scrolls tmux's history instead and the
    /// user cannot tell the difference. `set-clipboard on` is what keeps a drag-selection landing
    /// on the macOS pasteboard: SwiftTerm implements OSC 52, so tmux's copy reaches it.
    public static func configuration(defaultShell: String) -> String {
        """
        # Written by Swarm on every launch. Edits will be overwritten.
        #
        # This file configures a private tmux server that holds one shell per Swarm terminal pane.
        # It is not your tmux configuration and does not affect your own sessions.

        set -g default-shell "\(defaultShell)"
        set -g default-terminal "xterm-256color"
        set -ga terminal-features ",xterm-256color:RGB"
        set -g escape-time 0
        set -g history-limit 50000
        set -g status off
        set -g mouse on
        set -g set-clipboard on
        set -g focus-events on
        set -g set-titles off
        set -g bell-action none
        set -g destroy-unattached off
        set -g renumber-windows on
        set -g prefix None
        set -g prefix2 None
        unbind-key -a -T prefix

        """
    }
}

/// One resolved tmux binary on one private socket, and every command line Swarm sends it.
///
/// A value rather than a singleton so the tests can build one without a tmux on the machine.
public struct TmuxCommand: Sendable, Equatable {
    public let executable: String
    public let socketName: String
    public let configPath: String

    public init(executable: String, socketName: String, configPath: String) {
        self.executable = executable
        self.socketName = socketName
        self.configPath = configPath
    }

    /// `-u` forces UTF-8 rather than inferring it from a LANG that a GUI-launched app may not have
    /// inherited. `-f` is read only when the server starts, which is why `sourceConfiguration`
    /// exists for the case where it is already up.
    public var globalArguments: [String] {
        ["-L", socketName, "-f", configPath, "-u"]
    }

    public func arguments(_ tail: [String]) -> [String] {
        globalArguments + tail
    }

    /// The command a pane's pty actually execs.
    ///
    /// `-A` attaches to the session when it exists and creates it otherwise, which is the whole
    /// reattach-or-start-fresh rule in one flag. `-D` detaches any client that is somehow still
    /// attached, so a leftover client from a crashed launch cannot shrink the pane to its old size.
    /// `-c` and `-e` are read only when the session is created, so a restored session keeps the
    /// directory and environment it was born with.
    public func attachOrCreate(
        session: String,
        directory: String,
        environment: [String: String],
        removingEnvironment: [String] = ["NO_COLOR"]
    ) -> [String] {
        // Existing servers keep their original environment even after Swarm is rebuilt.
        var tail = removingEnvironment.sorted().flatMap {
            ["set-environment", "-gr", $0, ";"]
        }
        tail += ["new-session", "-A", "-D", "-s", session, "-c", directory]
        for key in environment.keys.sorted() {
            tail.append("-e")
            tail.append("\(key)=\(environment[key]!)")
        }
        return arguments(tail)
    }

    public func pasteBuffer(_ buffer: String, intoAgentPaneOf session: String) -> [String] {
        arguments([
            "load-buffer", "-b", buffer, "-", ";",
            "paste-buffer", "-d", "-p", "-b", buffer, "-t", agentPane(of: session),
        ])
    }

    public func send(_ key: TerminalKey, toAgentPaneOf session: String) -> [String] {
        arguments(["send-keys", "-t", agentPane(of: session), tmuxName(of: key)])
    }

    private func agentPane(of session: String) -> String {
        // Councils add panes to this window, but the CLI stays in the pane the session began with.
        "=\(session):0.0"
    }

    private func tmuxName(of key: TerminalKey) -> String {
        switch key {
        case .enter: "Enter"
        case .controlC: "C-c"
        case .tab: "Tab"
        case .escape: "Escape"
        case .up: "Up"
        case .down: "Down"
        case .left: "Left"
        case .right: "Right"
        }
    }

    public func killSession(_ session: String) -> [String] {
        arguments(["kill-session", "-t", "=" + session])
    }

    public var listSessions: [String] {
        arguments(["list-sessions", "-F", "#{session_name}"])
    }

    /// Every pane on our socket with the pid of the shell inside it. `-a` because the question is
    /// asked of the whole server at once: one spawn answers for every restored pane rather than one
    /// per pane per poll.
    public var listPanes: [String] {
        arguments(["list-panes", "-a", "-F", "#{pane_pid} #{session_name}"])
    }

    /// Re-applies the configuration to a server that was already running when Swarm launched,
    /// which is how a changed option reaches sessions started by yesterday's build.
    public var sourceConfiguration: [String] {
        arguments(["source-file", configPath])
    }
}
