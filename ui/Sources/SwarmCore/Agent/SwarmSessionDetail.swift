import Foundation
import Synchronization
import TranscriptTool

public enum ChairTranscriptSource: Sendable, Equatable {
    case waiting
    case ready(log: URL, format: String)
    case unsupported

    public static func resolve(
        session: SwarmSession, chairProvider: String? = nil,
        logExists: (String) -> Bool
    ) -> ChairTranscriptSource {
        guard let provider = session.chairProvider ?? chairProvider,
              ["claude", "codex", "agy"].contains(provider) else { return .unsupported }
        guard let path = session.chairLog, !path.isEmpty, logExists(path) else { return .waiting }
        return .ready(log: URL(fileURLWithPath: path), format: provider)
    }
}

public enum ChairTranscriptSnapshot: Sendable, Equatable {
    case loading
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
        case .loading: "notice Loading chat…"
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
    /// When the last search ran. A miss, or a log found with no chair id, waits 10 s for the next one.
    private var discoveredAt: Date?
    /// Account homes per provider with the time they were read. They expire after
    /// `accountHomesTTL`, so the same reader finds a log in an account added later.
    private var homesByProvider: [String: (homes: [URL], at: Date)] = [:]
    private let accountHomesTTL: TimeInterval
    /// Shared by every reader of the swarm CLI's profiles, keyed by source, home, and provider.
    /// Each chat open made a new reader that ran `swarm profiles` again, about 130 ms of a cold
    /// open. Any other source, such as a test's, keeps its own list.
    private static let homesCache = Mutex<[String: (homes: [URL], at: Date)]>([:])

    private func homes(provider: String) async -> [URL] {
        let ttl = accountHomesTTL
        let fresh = { (read: Date) in Date.now.timeIntervalSince(read) < ttl }
        if let own = homesByProvider[provider], fresh(own.at) { return own.homes }
        let key = profiles is SwarmCLIProfileSource
            ? "\(type(of: profiles))|\(home.path)|\(provider)" : nil
        if let key, let shared = Self.homesCache.withLock({ $0[key] }), fresh(shared.at) {
            homesByProvider[provider] = shared
            return shared.homes
        }
        let timing = SwarmPerformance.begin("AccountHomes")
        let accounts = try? await profiles.accounts(provider: provider)
        timing.end(count: accounts?.accounts.count)
        let homes = ChairLogDiscovery.homes(
            provider: provider, accountHomes: accounts?.accounts.map(\.home) ?? [], userHome: home
        )
        let entry = (homes: homes, at: Date.now)
        homesByProvider[provider] = entry
        if let key { Self.homesCache.withLock { $0[key] = entry } }
        return homes
    }

    /// Fills the shared account homes before any chat opens, so a first open skips the profile
    /// lookup. Cheap while the cache is fresh; the app calls it on each refresh.
    public func prefetchHomes() async {
        for provider in ["claude", "codex"] { _ = await homes(provider: provider) }
    }

    private var log: URL?
    private var reader: ToolTranscriptReader?
    public private(set) var currentModel: String?
    public private(set) var usage = ChatUsage()
    /// Claude's queued messages in the current log; a clear's new log has its own queue.
    public private(set) var queuedMessages: [String] = []
    private var rows: [TranscriptRow] = []
    /// The path of the last log that was read. `log` also clears on a gap, so it cannot tell a new
    /// log from the same one that came back.
    private var lastLogPath: URL?
    /// Rows of the logs before a `/clear`, each clear closed by its divider.
    private var frozenRows: [TranscriptRow] = []
    private var rawEntries: [RawTranscriptEntry] = []
    public private(set) var hasOlder = false

    public init(
        binary: URL? = nil,
        profiles: any SwarmProfileSource = SwarmCLIProfileSource(),
        home: URL = FileManager.default.homeDirectoryForCurrentUser,
        // ponytail: a 60 s expiry, not an invalidation; an account added within it waits.
        accountHomesTTL: TimeInterval = 60
    ) {
        self.binary = binary
        self.profiles = profiles
        self.home = home
        self.accountHomesTTL = accountHomesTTL
    }

    public func poll(
        session: SwarmSession, chairProvider: String? = nil
    ) async -> ChairTranscriptSnapshot {
        let timing = SwarmPerformance.begin("TranscriptPoll")
        defer { timing.end(count: rows.count) }
        var resolved = session
        if resolved.chairLog.map({ !FileManager.default.fileExists(atPath: $0) }) ?? true {
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
            return rows.isEmpty ? .waiting : .rows(rows, raw: rawEntries)
        case .unsupported:
            return .notice("No transcript reader for this provider yet")
        case .ready(let path, let format):
            return await read(log: path, format: format)
        }
    }

    /// A child agent's chat, from the log its provider's hooks reported (ADR 0029). Before the
    /// first report there is no log, and the chat waits.
    public func poll(childLog: String?, provider: String?) async -> ChairTranscriptSnapshot {
        guard let provider, ["claude", "codex", "agy"].contains(provider) else {
            return .notice("No transcript reader for this provider yet")
        }
        guard let childLog, !childLog.isEmpty, FileManager.default.fileExists(atPath: childLog) else {
            reader = nil
            log = nil
            return rows.isEmpty ? .waiting : .rows(rows, raw: rawEntries)
        }
        return await read(log: URL(fileURLWithPath: childLog), format: provider)
    }

    private func read(log path: URL, format: String) async -> ChairTranscriptSnapshot {
        guard let binary = binary ?? TranscriptToolProcess.bundled else {
            return .unavailable("The transcript tool is not available")
        }
        if path != log {
            log = path
            usage = ChatUsage()
            queuedMessages = []
            reader = ToolTranscriptReader(binary: binary, format: format, log: path)
        }
        do {
            let records: [TranscriptRecord]?
            do {
                let readTiming = SwarmPerformance.begin("TranscriptRead")
                defer { readTiming.end() }
                records = try await reader?.readIfChanged()
            }
            if records != nil, let reader {
                await rebuild(from: reader)
            }
            return .rows(rows, raw: rawEntries)
        } catch {
            reader = nil
            log = nil
            return rows.isEmpty
                ? .unavailable(String(describing: error))
                : .rows(rows, raw: rawEntries)
        }
    }

    public func loadOlder() async throws -> ChairTranscriptSnapshot {
        guard let reader else { return .rows(rows, raw: rawEntries) }
        _ = try await reader.loadOlder()
        await rebuild(from: reader)
        return .rows(rows, raw: rawEntries)
    }

    /// Claude's pull-back of its queued messages: `press("Up")`, then this log's queue records
    /// until each owner message has its own, then `press("C-u")` until the CLI box is empty.
    /// Returns the pulled text, or nil when the CLI took the messages first. With no record by
    /// 1.5 s it presses nothing more and throws, because C-u could wipe text Up pulled. It never
    /// presses Escape or C-c, because both stop the turn.
    public func pullBack(press: @Sendable (String) async throws -> Void) async throws -> String? {
        guard let reader else { return nil }
        let deadline = ContinuousClock.now + .milliseconds(1500)
        // The live reader, not the last poll: it holds messages sent after that poll.
        let start = await reader.window()
        let queue = QueuedMessages.replay(start.records)
        guard let last = queue.last(where: QueuedMessages.isOwners) else { return nil }
        // Up pops the whole queue, so it would take a notification or agent message that the
        // agent still needs out of the queue.
        guard queue.allSatisfy(QueuedMessages.isOwners) else {
            throw SwarmProfileError.failed("The agent has its own message in the queue. Edit after it is delivered.")
        }
        let mark = start.indexOffset + start.records.count
        try await press("Up")
        var decision = QueuePullBack.waiting
        while decision == .waiting {
            let window = await reader.window()
            decision = QueuePullBack.decide(
                queue: queue, after: window.records.dropFirst(mark - window.indexOffset),
                pastDeadline: ContinuousClock.now >= deadline
            )
            if decision == .waiting { try await Task.sleep(for: .milliseconds(50)) }
        }
        let pulled: String?
        switch decision {
        case .pulled(let owner, _):
            pulled = owner.joined(separator: "\n")
        case .unconfirmed:
            throw SwarmProfileError.failed("Could not confirm the pull-back. Check the agent's input box.")
        case .alreadySent, .waiting:
            pulled = nil
        }
        for index in 0..<decision.clearPresses(last: last) {
            if index > 0 { try await Task.sleep(for: .milliseconds(50)) }
            try await press("C-u")
        }
        return pulled
    }

    private func rebuild(from reader: ToolTranscriptReader) async {
        let window = await reader.window()
        let timing = SwarmPerformance.begin("TranscriptRows")
        defer { timing.end(count: rows.count) }
        let built = TranscriptRowBuilder.rows(
            from: window.records, indexOffset: window.indexOffset, hasOlder: window.hasOlder
        )
        if let path = log, path != lastLogPath {
            // Claude writes bookkeeping lines first, so until a real record lands the new log
            // cannot say whether it is a clear. Keep the old rows and decide on a later read.
            let undecided = window.records.allSatisfy {
                if case .ignored = $0.event { true } else { false }
            }
            if lastLogPath != nil, undecided { return }
            if lastLogPath != nil, ConversationBoundary.isClear(window.records), !rows.isEmpty {
                // The new log numbers its raw entries from 0 again, so an old row's sources would
                // point at new events.
                frozenRows = rows.map { var row = $0; row.sourceIDs = []; return row }
                    + [Self.clearDivider(logName: path.lastPathComponent)]
            } else {
                frozenRows = []
                // `/clear` keeps the model, but another log may come from another agent.
                currentModel = nil
            }
            lastLogPath = path
        }
        rows = frozenRows + (frozenRows.isEmpty ? built : ConversationBoundary.withoutClearPreamble(built))
        rawEntries = TranscriptDebugData.entries(from: window.records, indexOffset: window.indexOffset)
        hasOlder = window.hasOlder
        usage = window.usage
        queuedMessages = QueuedMessages.pending(in: window.records)
        if let model = ChatModelChoice.latest(in: window.records) { currentModel = model }
    }

    private static func clearDivider(logName: String) -> TranscriptRow {
        let time = Date.now.formatted(date: .omitted, time: .shortened)
        var row = TranscriptRow(
            kind: .divider, text: "Context cleared · \(time)", eventID: "clear-\(logName)"
        )
        row.detail = time
        return row
    }

    func discoveredLog(
        for session: SwarmSession, chairProvider: String? = nil, now: Date = .now
    ) async -> URL? {
        let provider = session.chairProvider ?? chairProvider
        guard session.chairLog.map({ !FileManager.default.fileExists(atPath: $0) }) ?? true,
              let provider,
              provider == "claude" || provider == "codex" else { return nil }
        if discoveredSession == session.id, discoveredProvider == provider,
           discoveredChairID == session.chairID,
           discoveredLog.map({ FileManager.default.fileExists(atPath: $0.path) }) ?? true,
           let discoveredAt, now.timeIntervalSince(discoveredAt) < 10 {
            return discoveredLog
        }
        let homes = await homes(provider: provider)
        if session.chairID == nil || discoveredSession != session.id
            || discoveredProvider != provider || discoveredChairID != session.chairID
            || discoveredLog.map({ !FileManager.default.fileExists(atPath: $0.path) }) ?? true {
            discoveredLog = ChairLogDiscovery.path(
                provider: provider, chairID: session.chairID?.rawValue,
                cwd: session.cwd, createdAt: session.createdAt,
                homes: homes
            )
            discoveredSession = session.id
            discoveredProvider = provider
            discoveredChairID = session.chairID
            discoveredAt = now
        }
        return discoveredLog
    }
}

public struct SwarmAgentCell: Sendable, Equatable, Identifiable {
    public let agent: SwarmAgent
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
            .map(SwarmAgentCell.init(agent:))
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
