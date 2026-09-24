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
    case rows([TranscriptRow], raw: [RawTranscriptEntry])
    case notice(String)
    case unavailable(String)

    public static func waitingMessage(isRunning: Bool?) -> String {
        isRunning == false
            ? "This chat ended before its log was found"
            : "The chair has not written its log yet"
    }

    public var printText: String {
        switch self {
        case .waiting: "notice The chair has not written its log yet"
        case .rows(let rows, _): rows.map(\.printLine).joined(separator: "\n")
        case .notice(let message): "notice \(message)"
        case .unavailable(let message): "error \(message)"
        }
    }
}

/// Owns one live reader and rechecks time-matched logs while the bus has no chair id.
public actor SwarmChairTranscript {
    private let binary: URL?
    private let profiles: any SwarmProfileSource
    private let home: URL
    private var discoveredSession: SwarmSessionID?
    private var discoveredProvider: String?
    private var discoveredChairID: SwarmChairID?
    private var discoveredLog: URL?
    private var homesByProvider: [String: [URL]] = [:]
    private var log: URL?
    private var reader: ToolTranscriptReader?
    private var rows: [TranscriptRow] = []
    private var rawEntries: [RawTranscriptEntry] = []

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
        let timing = SwarmPerformance.begin("TranscriptPoll")
        defer { timing.end(count: rows.count) }
        var resolved = session
        if resolved.chairLog == nil {
            resolved.chairLog = await discoveredLog(
                for: session, chairProvider: chairProvider
            )?.path
        }
        switch ChairTranscriptSource.resolve(
            session: resolved, chairProvider: chairProvider,
            logExists: FileManager.default.fileExists(atPath:)
        ) {
        case .waiting:
            reader = nil
            log = nil
            rows = []
            rawEntries = []
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
                rawEntries = []
            }
            do {
                let records: [TranscriptRecord]?
                do {
                    let readTiming = SwarmPerformance.begin("TranscriptRead")
                    defer { readTiming.end() }
                    records = try await reader?.readIfChanged()
                }
                if let records {
                    let buildTiming = SwarmPerformance.begin("TranscriptRows")
                    rows = TranscriptRowBuilder.rows(from: records)
                    rawEntries = TranscriptDebugData.entries(from: records)
                    buildTiming.end(count: rows.count)
                }
                return .rows(rows, raw: rawEntries)
            } catch {
                return .unavailable(String(describing: error))
            }
        }
    }

    func discoveredLog(
        for session: SwarmSession, chairProvider: String? = nil
    ) async -> URL? {
        let provider = session.chairProvider ?? chairProvider
        guard session.chairLog == nil, let provider,
              provider == "claude" || provider == "codex" else { return nil }
        if homesByProvider[provider] == nil {
            let accounts = try? await profiles.accounts(provider: provider)
            homesByProvider[provider] = ChairLogDiscovery.homes(
                provider: provider, accountHomes: accounts?.accounts.map(\.home) ?? [],
                userHome: home
            )
        }
        if session.chairID == nil || discoveredSession != session.id
            || discoveredProvider != provider || discoveredChairID != session.chairID
            || discoveredLog == nil {
            discoveredLog = ChairLogDiscovery.path(
                provider: provider, chairID: session.chairID?.rawValue,
                cwd: session.cwd, createdAt: session.createdAt,
                homes: homesByProvider[provider] ?? []
            )
            discoveredSession = session.id
            discoveredProvider = provider
            discoveredChairID = session.chairID
        }
        return discoveredLog
    }
}

public enum SwarmAgentCellKind: Sendable, Equatable {
    case attach
    case notice(String)
}

public struct SwarmAgentCell: Sendable, Equatable, Identifiable {
    public let agent: SwarmAgent
    public let kind: SwarmAgentCellKind
    public var id: SwarmAgentID { agent.id }
}

public enum SwarmPanePolicy {
    public static let chair = SwarmAgentID("orchestrator")

    public static func hasLiveChildAgents(session: SwarmSession, agents: [SwarmAgent]) -> Bool {
        !cells(session: session, agents: agents).isEmpty
    }

    public static func cells(session: SwarmSession, agents: [SwarmAgent]) -> [SwarmAgentCell] {
        agents
            .filter {
                $0.alive == true && $0.id != chair && $0.id.rawValue != session.chairID?.rawValue
            }
            .sorted {
                if $0.createdAt != $1.createdAt { return ($0.createdAt ?? .max) < ($1.createdAt ?? .max) }
                return $0.id.rawValue < $1.id.rawValue
            }
            .map { agent in
                let kind: SwarmAgentCellKind
                if let reason = unavailableReason(session: session, agent: agent) {
                    kind = .notice(reason)
                } else {
                    kind = .attach
                }
                return SwarmAgentCell(agent: agent, kind: kind)
            }
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
}

public enum SwarmSessionCloser {
    public static func close(_ session: SwarmSession, bus: any SwarmBus) async throws {
        let agents = try await bus.agents(in: session)
        let live = agents.filter { $0.alive == true }
        let children = live.filter { $0.id != SwarmPanePolicy.chair }
        let chairs = live.filter { $0.id == SwarmPanePolicy.chair }
        for agent in children + chairs {
            try await bus.close(agent.id, in: session)
        }
        try await bus.archive([session.id])
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
