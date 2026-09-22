import Foundation
import TranscriptTool

public enum ChairTranscriptSource: Sendable, Equatable {
    case waiting
    case ready(log: URL, format: String)
    case unsupported

    public static func resolve(
        session: SwarmSession, chairProvider: String? = nil,
        logExists: (String) -> Bool
    ) -> ChairTranscriptSource {
        let provider = session.chairProvider ?? chairProvider
        if provider == "agy" { return .unsupported }
        guard let path = session.chairLog, !path.isEmpty, logExists(path) else { return .waiting }
        switch provider {
        case "claude": return .ready(log: URL(fileURLWithPath: path), format: "claude")
        case "codex": return .ready(log: URL(fileURLWithPath: path), format: "codex")
        default: return .unsupported
        }
    }
}

public enum ChairTranscriptSnapshot: Sendable, Equatable {
    case waiting
    case rows([TranscriptRow])
    case notice(String)
    case unavailable(String)

    public var printText: String {
        switch self {
        case .waiting: "notice The chair has not written its log yet"
        case .rows(let rows): rows.map(\.printLine).joined(separator: "\n")
        case .notice(let message): "notice \(message)"
        case .unavailable(let message): "error \(message)"
        }
    }
}

/// Owns one live reader and resolves the log again on every poll until it appears.
public actor SwarmChairTranscript {
    private let binary: URL?
    private let profiles: any SwarmProfileSource
    private let home: URL
    private var discoveredSession: SwarmSessionID?
    private var discoveredLog: URL?
    private var log: URL?
    private var reader: ToolTranscriptReader?
    private var rows: [TranscriptRow] = []

    public init(
        binary: URL? = nil,
        profiles: any SwarmProfileSource = SwarmCLIProfileSource(),
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) {
        self.binary = binary
        self.profiles = profiles
        self.home = home
    }

    public func poll(
        session: SwarmSession, chairProvider: String? = nil
    ) async -> ChairTranscriptSnapshot {
        var resolved = session
        let provider = session.chairProvider ?? chairProvider
        if resolved.chairLog == nil, let provider, provider == "claude" || provider == "codex" {
            if discoveredSession != session.id || discoveredLog == nil {
                let accounts = try? await profiles.accounts(provider: provider)
                let homes = (accounts?.accounts.map { URL(fileURLWithPath: $0.home) } ?? [])
                    + [home.appendingPathComponent(provider == "codex" ? ".codex" : ".claude")]
                discoveredLog = ChairLogDiscovery.path(
                    provider: provider, cwd: session.cwd, createdAt: session.createdAt,
                    homes: homes
                )
                discoveredSession = session.id
            }
            resolved.chairLog = discoveredLog?.path
        }
        switch ChairTranscriptSource.resolve(
            session: resolved, chairProvider: chairProvider,
            logExists: FileManager.default.fileExists(atPath:)
        ) {
        case .waiting:
            reader = nil
            log = nil
            rows = []
            return .waiting
        case .unsupported:
            return .notice("No transcript reader for this provider yet")
        case .ready(let path, let format):
            guard let binary = binary ?? TranscriptToolProcess.bundled else {
                return .unavailable("The transcript tool is not available")
            }
            if path != log {
                log = path
                reader = ToolTranscriptReader(binary: binary, format: format, log: path)
                rows = []
            }
            do {
                if let events = try await reader?.readIfChanged() {
                    rows = TranscriptRowBuilder.rows(from: events)
                }
                return .rows(rows)
            } catch {
                return .unavailable(String(describing: error))
            }
        }
    }
}

public enum SwarmPanePolicy {
    public static let chair = SwarmAgentID("orchestrator")

    public static func selectedAgent(
        in agents: [SwarmAgent], preferred: SwarmAgentID
    ) -> SwarmAgent? {
        let live = agents.filter { $0.alive != false }
        return live.first { $0.id == preferred }
            ?? live.first { $0.id == chair }
            ?? live.first
    }

    public static func unavailableReason(session: SwarmSession, agent: SwarmAgent) -> String? {
        switch session.adapter {
        case "tmux-solo":
            return agent.pane == nil ? "This agent has no pane" : nil
        case "tmux", "herdr": return "This session's host has no attach"
        default: return "This session's host has no attach"
        }
    }

    public static func attachCommand(
        bus: any SwarmBus, session: SwarmSession, agent: SwarmAgentID
    ) -> SwarmAttachCommand {
        var command = bus.attachCommand(for: agent, in: session.id)
        command.environment["SWARM_ADAPTER"] = session.adapter ?? ""
        return command
    }

    public static func typedText(_ text: String) -> String? {
        text.contains { !$0.isWhitespace } ? text : nil
    }
}

public struct SwarmAttachLaunch: Sendable, Hashable {
    public let executable: String
    public let arguments: [String]
    public let environment: [String: String]
    public let directory: String

    public init(command: SwarmAttachCommand, directory: String) {
        let inherited = ChildProcessEnvironment.removingInheritedAgentIdentity(from: Shell.environment())
        environment = Shell.terminalEnvironment(
            inheriting: inherited, extra: command.environment
        )
        executable = Shell.which(command.executable) ?? command.executable
        arguments = command.arguments
        self.directory = directory
    }
}
