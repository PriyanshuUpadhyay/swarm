import Foundation
import Testing
@testable import SwarmCore

/// The reader that replaced a `swarm` process per second.
///
/// These build a database with the columns the live `~/.swarm/swarm.db` has, so a migration on
/// swarm's side that drops or renames one of them fails here rather than in front of a user.
@Suite("Swarm bus store", .tags(.agentProtocol))
struct SwarmBusStoreTests {
    private let chair = SwarmAgentID("orchestrator")
    private let coder = SwarmAgentID("coder-1")
    private let session = SwarmSessionID("42")

    @Test("reads the roster in id order, and never claims to know whether a pane is alive")
    func readsRoster() async throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(atPath: root) }
        try write(to: root) { database in
            try database.run(
                "INSERT INTO agent (id, session_id, role, pane_id) VALUES (?, 42, ?, ?);",
                [.text("coder-1"), .text("code.complex"), .text("%3")]
            )
            try database.run(
                "INSERT INTO agent (id, session_id, role, pane_id) VALUES (?, 42, ?, NULL);",
                [.text("orchestrator"), .text("orchestrator")]
            )
            // A second session's agent, which must not appear in the first session's roster.
            try database.run(
                "INSERT INTO agent (id, session_id, role, pane_id) VALUES (?, 7, ?, ?);",
                [.text("stranger"), .text("code.simple"), .text("%9")]
            )
        }

        let agents = try await SwarmBusStore(root: root).agents(in: session)

        #expect(agents.map(\.id) == [coder, chair])
        #expect(agents.map(\.pane) == ["%3", nil])
        // Nil for both, including the one that has a pane. Liveness is the adapter's answer and
        // this type must not guess it. See `SwarmPaneLiveness`.
        #expect(agents.allSatisfy { $0.alive == nil })
    }

    @Test("reads messages after a sequence, with their bodies and their read marks")
    func readsMessages() async throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(atPath: root) }
        try FileManager.default.createDirectory(
            atPath: root + "/runs/42", withIntermediateDirectories: true
        )
        try "Fix the parser".write(
            toFile: root + "/runs/42/7.txt", atomically: true, encoding: .utf8
        )
        try write(to: root) { database in
            try insertMessage(database, seq: 6, sender: "orchestrator", body: "runs/42/6.txt")
            try insertMessage(database, seq: 7, sender: "orchestrator", body: "runs/42/7.txt")
            try database.run(
                "INSERT INTO read_mark (message_seq, agent_id) VALUES (7, ?);",
                [.text("coder-1")]
            )
        }

        let messages = try await SwarmBusStore(root: root).messages(in: session, after: 6)

        #expect(messages.count == 1)
        let only = try #require(messages.first)
        #expect(only.seq == 7)
        #expect(only.sender == chair)
        #expect(only.recipient == coder)
        #expect(only.body == "Fix the parser")
        #expect(only.read)
    }

    @Test("reports a body it cannot read as nil rather than as empty text")
    func missingBodyIsNil() async throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(atPath: root) }
        try write(to: root) { database in
            try insertMessage(database, seq: 1, sender: "coder-1", body: "runs/42/gone.txt")
        }

        let messages = try await SwarmBusStore(root: root).messages(in: session, after: 0)

        #expect(messages.first?.body == nil)
    }

    @Test("is unavailable when swarm has no database, so the caller can fall back to the CLI")
    func missingDatabaseIsUnavailable() async throws {
        let store = SwarmBusStore(root: "/var/empty/no-swarm-here")

        await #expect(throws: SwarmProfileError.self) {
            try await store.agents(in: session)
        }
    }

    /// The failure this nearly shipped with. A WAL database needs a `-shm` file to be read at all,
    /// the `swarm` CLI removes it when its last connection closes, and a `SQLITE_OPEN_READONLY`
    /// connection cannot make one. Every read then failed with SQLITE_CANTOPEN whenever no swarm
    /// process was running, which is nearly always, and the app fell back to the CLI for ever
    /// while looking like it worked.
    @Test("reads a WAL database whose shared-memory file is not there")
    func readsAWalDatabaseWithNoSharedMemoryFile() async throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(atPath: root) }
        try write(to: root) { database in
            try database.execute("PRAGMA journal_mode = WAL;")
            try database.run(
                "INSERT INTO agent (id, session_id, role, pane_id) VALUES (?, 42, ?, NULL);",
                [.text("coder-1"), .text("code.complex")]
            )
        }
        // What the CLI leaves behind between runs, and the state a read has to cope with.
        for suffix in ["-shm", "-wal"] {
            try? FileManager.default.removeItem(atPath: root + "/swarm.db" + suffix)
        }

        let agents = try await SwarmBusStore(root: root).agents(in: session)

        #expect(agents.map(\.id) == [coder])
    }

    @Test("refuses a write through the connection it calls read-only")
    func refusesAWrite() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(atPath: root) }
        try write(to: root) { _ in }

        let reader = try SQLiteDatabase(path: root + "/swarm.db", readOnly: true)

        #expect(throws: SQLiteError.self) {
            try reader.run("INSERT INTO agent (id, session_id, role) VALUES ('x', 1, 'y');")
        }
    }

    @Test("asks the adapter once per interval however many times it is called")
    func livenessIsThrottled() async throws {
        let counter = CallCounter()
        let liveness = SwarmPaneLiveness()
        let listed = [SwarmAgent(id: coder, role: "code.complex", pane: "%3", alive: true)]

        for _ in 0..<5 {
            _ = await liveness.panes(in: session) {
                await counter.record()
                return listed
            }
        }

        #expect(await counter.count == 1)
    }

    @Test("keeps the last answer when the adapter read fails")
    func livenessSurvivesAFailedRead() async throws {
        let liveness = SwarmPaneLiveness()
        let listed = [SwarmAgent(id: coder, role: "code.complex", pane: "%3", alive: true)]
        _ = await liveness.panes(in: session) { listed }

        await liveness.forget()
        _ = await liveness.panes(in: session) { listed }
        let afterFailure = await liveness.panes(in: session) {
            throw SwarmProfileError.unavailable("swarm is not on the path")
        }

        #expect(afterFailure[coder]?.alive == true)
    }

    // MARK: - The database these read

    private func makeRoot() throws -> String {
        let root = NSTemporaryDirectory() + "swarm-bus-store-" + UUID().uuidString
        try FileManager.default.createDirectory(
            atPath: root, withIntermediateDirectories: true
        )
        return root
    }

    /// The columns the live database has, copied from `sqlite3 ~/.swarm/swarm.db .schema` rather
    /// than from swarm's `migrations/`, because what the app opens is the migrated result.
    private func write(to root: String, _ body: (SQLiteDatabase) throws -> Void) throws {
        let database = try SQLiteDatabase(path: root + "/swarm.db")
        try database.execute(
            """
            CREATE TABLE session (
                id INTEGER PRIMARY KEY AUTOINCREMENT, talk_mode TEXT NOT NULL, cwd TEXT,
                created_at INTEGER, chair_log TEXT, adapter TEXT, chair_provider TEXT,
                chair_id TEXT, archived_at INTEGER
            );
            CREATE TABLE agent (
                id TEXT NOT NULL, session_id INTEGER NOT NULL, role TEXT NOT NULL, pane_id TEXT,
                PRIMARY KEY (session_id, id)
            );
            CREATE TABLE message (
                seq INTEGER PRIMARY KEY AUTOINCREMENT, session_id INTEGER NOT NULL,
                sender_id TEXT NOT NULL, recipient_id TEXT NOT NULL, kind TEXT NOT NULL,
                body_path TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                rung_at INTEGER, seen_at INTEGER
            );
            CREATE TABLE read_mark (
                message_seq INTEGER NOT NULL, agent_id TEXT NOT NULL,
                PRIMARY KEY (message_seq, agent_id)
            );
            """
        )
        try body(database)
    }

    private func insertMessage(
        _ database: SQLiteDatabase, seq: Int, sender: String, body: String
    ) throws {
        let recipient = sender == "orchestrator" ? "coder-1" : "orchestrator"
        try database.run(
            """
            INSERT INTO message (seq, session_id, sender_id, recipient_id, kind, body_path, created_at)
            VALUES (?, 42, ?, ?, 'ask', ?, 1789600000);
            """,
            [.int(Int64(seq)), .text(sender), .text(recipient), .text(body)]
        )
    }
}

private actor CallCounter {
    private(set) var count = 0

    func record() { count += 1 }
}
