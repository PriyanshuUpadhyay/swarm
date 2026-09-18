import Foundation

/// Reads the swarm bus straight out of `~/.swarm/swarm.db`, instead of asking the `swarm` CLI.
///
/// # Why this exists
///
/// `SwarmCLIBus` answers every question by launching a process, and the panes ask once a second.
/// The app's own performance log for 2026-09-18 carries the bill: 2,089 `busRead` events over
/// 100ms, a median of 164ms and a p99 of 261ms, to serve **four** sessions, plus 115 `swarm`
/// launches that each took more than 250ms with a median of 910ms. The fastest poll ever recorded
/// was 100ms. None of that work discovers anything most of the time, because on an idle machine
/// the answer is always "the same as last second".
///
/// Every byte of it is already on this disk, in a 135KB SQLite file that this app can open. So it
/// opens it. `Store` proves the app can talk to SQLite, `WorktreeWatcher` proves it can be told
/// when a file changes, and neither needed anything new.
///
/// # What this deliberately does not answer
///
/// `SwarmAgent.alive` is not in the database and cannot be. `docs/bus-contract.md:100-103` defines
/// it as whether the adapter's `list` shows the recorded pane, so answering it means running
/// `tmux list-panes` or `herdr pane list`. A pane dying writes nothing to `~/.swarm`, so no file
/// watcher would see it either. This type reports the roster with `alive` nil, and `SwarmCLIBus`
/// keeps a throttled CLI call for the liveness alone.
///
/// # The coupling, stated plainly
///
/// This reads swarm's tables rather than its JSON, so a migration in swarm's `migrations/` can
/// break it. That is a real cost and it is accepted: both sides are one project, the read is
/// behind `SwarmProfileError.unavailable`, and a failure falls back to the CLI rather than
/// showing a wrong answer. `columnsAreMissing` is what turns a schema change into that fallback.
public actor SwarmBusStore {
    public static let shared = SwarmBusStore()

    /// The contract's page size, so a caller that pages by `seq` sees what the CLI showed it.
    /// `docs/bus-contract.md:107-108`.
    static let messageLimit = 500

    /// `~/.swarm`, and `nonisolated` because a watcher on the main actor needs it without an
    /// await. `SwarmSessionReaderModel` hands it to `WorktreeWatcher`, and it never changes after
    /// `init`, so nothing here has to be serialised.
    public nonisolated let root: String
    private var database: SQLiteDatabase?

    /// Message bodies by `seq`, which is sound because a body file is written once and never
    /// edited: `swarm send` creates `runs/<session>/<seq>.txt` and nothing rewrites it.
    ///
    /// This is here because the readers ask with `after: 0` every pass, so a session sitting at
    /// the 500-row page limit meant 500 file opens per session per second. Bodies are what the
    /// agents panel draws, so they cannot simply be left out.
    ///
    /// ponytail: cleared whole when it passes `bodyLimit`, rather than evicted least-recently-used.
    /// The readers walk a sliding window of recent seqs, so a clear costs one slow pass and the
    /// window is back in the cache. Swap in an LRU only if that pass shows up in `PerfLog`.
    private var bodies: [Int: String] = [:]
    private static let bodyLimit = 4_000

    /// `SWARM_HOME` then `HOME`, which is what `swarm::paths::home()` does. `SWARM_HOME` is not in
    /// `ChildProcessEnvironment.exactNames`, so a CLI child inherits whatever this process has and
    /// the two always read the same file.
    public init(root: String? = nil) {
        if let root {
            self.root = root
        } else {
            let home = ProcessInfo.processInfo.environment["SWARM_HOME"] ?? NSHomeDirectory()
            self.root = (home as NSString).appendingPathComponent(".swarm")
        }
    }

    private var path: String { (root as NSString).appendingPathComponent("swarm.db") }

    /// Opened once and kept. Read-only, so this never creates the file and never writes a journal
    /// setting onto a database the CLI owns.
    private func connection() throws -> SQLiteDatabase {
        if let database { return database }
        guard FileManager.default.fileExists(atPath: path) else {
            throw SwarmProfileError.unavailable("swarm has no bus at \(path)")
        }
        do {
            let opened = try SQLiteDatabase(path: path, readOnly: true)
            database = opened
            return opened
        } catch {
            throw SwarmProfileError.unavailable("swarm's bus could not be opened: \(error)")
        }
    }

    /// Drops the handle so the next call opens again.
    ///
    /// The CLI replaces this file wholesale on `swarm init`, and a handle to the old inode would
    /// then answer from a database nobody is writing to any more. Every failure path comes through
    /// here, so a stale handle costs one failed read rather than a session that never updates.
    private func forget() {
        database = nil
    }

    /// The session's agents, sorted by id, with `alive` nil for every one of them.
    public func agents(in session: SwarmSessionID) throws -> [SwarmAgent] {
        guard let id = Int64(session.rawValue) else {
            throw SwarmProfileError.failed("swarm session id \(session.rawValue) is not a number")
        }
        let rows = try query(
            "SELECT id, role, pane_id FROM agent WHERE session_id = ? ORDER BY id;",
            [.int(id)]
        )
        return try rows.map { row in
            guard let id = row.string("id"), let role = row.string("role") else {
                throw columnsAreMissing("agent")
            }
            return SwarmAgent(
                id: SwarmAgentID(id), role: role, pane: row.string("pane_id"), alive: nil
            )
        }
    }

    /// The session's messages after `seq`, in `seq` order, at most `messageLimit`.
    ///
    /// `read` is the presence of a `read_mark` row for the recipient, which is what `swarm ack`
    /// writes. `body` is the body file's text, or nil when it cannot be read, matching
    /// `docs/bus-contract.md:127-128`.
    public func messages(in session: SwarmSessionID, after seq: Int) throws -> [SwarmMessage] {
        guard let id = Int64(session.rawValue) else {
            throw SwarmProfileError.failed("swarm session id \(session.rawValue) is not a number")
        }
        let rows = try query(
            """
            SELECT m.seq, m.sender_id, m.recipient_id, m.kind, m.body_path, m.created_at,
                   EXISTS (
                       SELECT 1 FROM read_mark r
                       WHERE r.message_seq = m.seq AND r.agent_id = m.recipient_id
                   ) AS was_read
            FROM message m
            WHERE m.session_id = ? AND m.seq > ?
            ORDER BY m.seq
            LIMIT \(Self.messageLimit);
            """,
            [.int(id), .int(Int64(seq))]
        )
        return try rows.map { row in
            guard let seq = row.int("seq").map(Int.init),
                  let sender = row.string("sender_id"),
                  let recipient = row.string("recipient_id"),
                  let kind = row.string("kind"),
                  let bodyPath = row.string("body_path"),
                  let createdAt = row.int("created_at").map(Int.init)
            else { throw columnsAreMissing("message") }
            return SwarmMessage(
                seq: seq, sender: SwarmAgentID(sender), recipient: SwarmAgentID(recipient),
                kind: kind, body: body(seq: seq, at: bodyPath), createdAt: createdAt,
                read: row.bool("was_read")
            )
        }
    }

    /// `body_path` is relative to the swarm root, which is the spelling `swarm inbox` prints and
    /// `ring_text` tells an agent to join with the root (`src/main.rs:8`).
    ///
    /// A file that cannot be read is not cached, so a body still being written is picked up on the
    /// next pass rather than remembered as nil for the life of the app.
    private func body(seq: Int, at bodyPath: String) -> String? {
        if let cached = bodies[seq] { return cached }
        let full = bodyPath.hasPrefix("/")
            ? bodyPath
            : (root as NSString).appendingPathComponent(bodyPath)
        guard let text = try? String(contentsOfFile: full, encoding: .utf8) else { return nil }
        if bodies.count >= Self.bodyLimit { bodies.removeAll(keepingCapacity: true) }
        bodies[seq] = text
        return text
    }

    private func query(_ sql: String, _ bindings: [SQLValue]) throws -> [Row] {
        do {
            return try connection().query(sql, bindings)
        } catch let error as SwarmProfileError {
            throw error
        } catch {
            forget()
            throw SwarmProfileError.unavailable("swarm's bus could not be read: \(error)")
        }
    }

    /// A row that arrived without a column this type needs, which is how a migration on swarm's
    /// side reaches the app. Reported as `unavailable` on purpose, because the caller's answer to
    /// that is to run the CLI, and the CLI is the side that knows the new schema.
    private func columnsAreMissing(_ table: String) -> SwarmProfileError {
        forget()
        return .unavailable("swarm's \(table) table is not the shape this app reads")
    }
}

/// Whether each recorded pane is still there, asked of the `swarm` CLI no more often than
/// `interval`.
///
/// This is the half of `agents` that `SwarmBusStore` cannot answer, kept apart because it is the
/// expensive half. Nothing in `~/.swarm` changes when a pane dies, so no file watcher replaces it
/// either, and the interval is the whole of the saving.
public actor SwarmPaneLiveness {
    public static let shared = SwarmPaneLiveness()

    /// Five seconds. A pane ends when somebody closes a window or an agent exits, and the reader
    /// shows that as a "Running" badge turning to "Ended". Neither has to land within one frame.
    static let interval: Duration = .seconds(5)

    /// The pane the answer was about, so a stale answer is not applied to a new pane.
    public struct Pane: Sendable, Equatable {
        public var pane: String
        public var alive: Bool?
    }

    private var known: [SwarmSessionID: [SwarmAgentID: Pane]] = [:]
    private var asked: [SwarmSessionID: ContinuousClock.Instant] = [:]

    public init() {}

    public func panes(
        in session: SwarmSessionID, list: () async throws -> [SwarmAgent]
    ) async -> [SwarmAgentID: Pane] {
        let now = ContinuousClock.now
        if let last = asked[session], now - last < Self.interval { return known[session] ?? [:] }
        // Stamped before the call and not after, which is what makes one process per interval
        // true rather than nearly true. An actor suspends at the await, so a second caller
        // arriving while this one waits would otherwise find no stamp and start a second `swarm`.
        asked[session] = now
        guard let agents = try? await list() else {
            // The old answer, kept. A failed read is usually swarm missing for a moment, and
            // dropping the cache would flip every badge to "Status unknown" for five seconds.
            return known[session] ?? [:]
        }
        var panes: [SwarmAgentID: Pane] = [:]
        for agent in agents {
            guard let pane = agent.pane else { continue }
            panes[agent.id] = Pane(pane: pane, alive: agent.alive)
        }
        known[session] = panes
        return panes
    }

    /// Drops what is remembered, so the next read asks again. For a caller that has just changed
    /// the panes itself, and for the suite.
    public func forget() {
        known = [:]
        asked = [:]
    }
}
