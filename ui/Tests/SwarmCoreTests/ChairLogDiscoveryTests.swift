import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Chair log discovery")
struct ChairLogDiscoveryTests {
    @Test("Finds the earliest matching Codex home and retries until a log exists")
    func codex() async throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let cwd = "/work/\(UUID().uuidString)"
        let cutoff = 1_790_079_961
        let first = fixture.root.appendingPathComponent(".codex-first")
        let second = fixture.root.appendingPathComponent(".codex-second")
        let accounts = SwarmAccountList(provider: "codex", source: "fixture", accounts: [
            account("first", home: first), account("second", home: second)
        ], auto: "first")
        let session = SwarmSession(
            id: .init("fixture-session"), talkMode: "lane", adapter: "tmux-solo",
            cwd: cwd, createdAt: cutoff, chairProvider: "codex", chairID: nil,
            chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil
        )
        let binary = try #require(TranscriptToolProcess.bundled)
        let reader = SwarmChairTranscript(
            binary: binary, profiles: FixtureProfiles(accountList: accounts), home: fixture.root
        )
        #expect(await reader.poll(session: session) == .waiting)

        try fixture.codexLog(home: first, name: "old", cwd: cwd, at: "2026-09-22T12:25:00Z")
        try fixture.codexLog(home: first, name: "wrong", cwd: "/other", at: "2026-09-22T12:27:00Z")
        let earlier = try fixture.codexLog(
            home: first, name: "earlier", cwd: cwd, at: "2026-09-22T12:26:02Z"
        )
        _ = try fixture.codexLog(
            home: second, name: "latest", cwd: cwd, at: "2026-09-22T12:27:01.500Z"
        )
        #expect(ChairLogDiscovery.path(
            provider: "codex", cwd: cwd, createdAt: cutoff, homes: [first, second]
        )?.lastPathComponent == earlier.lastPathComponent)
        #expect(ChairLogDiscovery.path(
            provider: "codex", cwd: cwd, createdAt: cutoff, homes: [first]
        )?.lastPathComponent == earlier.lastPathComponent)
        guard case .rows(let rows) = await reader.poll(session: session) else {
            Issue.record("The reader did not retry after the log appeared")
            return
        }
        #expect(rows.contains { $0.kind == .user && $0.text.contains("List files") })
    }

    @Test("Chooses the chair rollout instead of a later rollout in the same folder")
    func codexChairWindow() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let home = fixture.root.appendingPathComponent(".codex")
        let chair = try fixture.codexLog(
            home: home, name: "chair", cwd: "/work", at: "2026-09-22T12:26:06Z"
        )
        _ = try fixture.codexLog(
            home: home, name: "later-agent", cwd: "/work", at: "2026-09-22T14:26:01Z"
        )

        #expect(ChairLogDiscovery.path(
            provider: "codex", cwd: "/work", createdAt: 1_790_079_961, homes: [home]
        )?.lastPathComponent == chair.lastPathComponent)
    }

    @Test("Rejects a rollout that starts twenty minutes after the session")
    func codexTooLate() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let home = fixture.root.appendingPathComponent(".codex")
        try fixture.codexLog(
            home: home, name: "late", cwd: "/work", at: "2026-09-22T12:46:01Z"
        )

        #expect(ChairLogDiscovery.path(
            provider: "codex", cwd: "/work", createdAt: 1_790_079_961, homes: [home]
        ) == nil)
    }

    @Test("Rejects a rollout that starts before the session")
    func codexTooEarly() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let home = fixture.root.appendingPathComponent(".codex")
        try fixture.codexLog(
            home: home, name: "early", cwd: "/work", at: "2026-09-22T12:26:00Z"
        )

        #expect(ChairLogDiscovery.path(
            provider: "codex", cwd: "/work", createdAt: 1_790_079_961, homes: [home]
        ) == nil)
    }

    @Test("Finds a Claude project log for the same cwd")
    func claude() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let home = fixture.root.appendingPathComponent(".claude")
        let project = home.appendingPathComponent("projects/project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let log = project.appendingPathComponent("native-id.jsonl")
        try Data("""
            {"timestamp":"2026-09-22T12:27:00Z","type":"queue-operation"}
            {"timestamp":"2026-09-22T12:27:01Z","cwd":"/work","type":"user"}
            """.utf8).write(to: log)
        #expect(ChairLogDiscovery.path(
            provider: "claude", cwd: "/work", createdAt: 1_790_079_961, homes: [home]
        )?.lastPathComponent == log.lastPathComponent)
    }

    private func account(_ name: String, home: URL) -> SwarmAccount {
        SwarmAccount(
            name: name, email: nil, home: home.path, env: [:], signedIn: true,
            remainingPct: nil, summary: nil
        )
    }
}

private struct FixtureProfiles: SwarmProfileSource {
    let accountList: SwarmAccountList
    func roles() async throws -> [SwarmRole] { [] }
    func accounts(provider: String) async throws -> SwarmAccountList { accountList }
    func usage() async throws -> [SwarmUsageMeter] { [] }
}

private struct Fixture {
    let root: URL

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }

    @discardableResult
    func codexLog(home: URL, name: String, cwd: String, at timestamp: String) throws -> URL {
        let directory = home.appendingPathComponent("sessions/2026/09/22")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let path = directory.appendingPathComponent("rollout-\(name).jsonl")
        let lines = """
            {"type":"session_meta","timestamp":"\(timestamp)","payload":{"cwd":"\(cwd)"}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"List files"}]}}

            """
        try Data(lines.utf8).write(to: path)
        return path
    }
}
