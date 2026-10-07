import Foundation
import Testing
@testable import SwarmCore

@Suite("CLI chat names")
struct CLITitleTests {
    // Claude Code 2.1.292 embedded source, ~/.local/share/claude/versions/2.1.292,
    // byte 195642693 (ZW) and 195643051 (oie) append these title records.
    @Test("Claude rename wins over generated names and the latest rename wins")
    func claudeNames() throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let log = root.appendingPathComponent("chat.jsonl")
        try write([
            #"{"type":"custom-title","sessionId":"chat","customTitle":"Old name"}"#,
            #"{"type":"custom-title","sessionId":"other","customTitle":"Other chat"}"#,
            #"{"type":"ai-title","sessionId":"chat","aiTitle":"Generated name"}"#,
            #"{"type":"custom-title","sessionId":"chat","customTitle":"Fix login"}"#,
            "broken record", "{\"type\":\"custom-title\""
        ], to: log)
        #expect(ChairLogTitle.claudeName(path: log.path) == "Fix login")
        try write([#"{"type":"ai-title","sessionId":"chat","aiTitle":"Generated name"}"#], to: log)
        #expect(ChairLogTitle.claudeName(path: log.path) == "Generated name")
        try write([
            #"{"type":"ai-title","sessionId":"chat","aiTitle":"Generated name"}"#,
            #"{"type":"custom-title","sessionId":"chat","customTitle":""}"#
        ], to: log)
        #expect(ChairLogTitle.claudeName(path: log.path) == "Generated name")
    }

    // Claude's dEn/w4o at bytes 191011383-191011418 and ZAn at 195530658
    // read/write <log parent>/<sessionId>/custom-title.json.
    @Test("Claude reads its rename sidecar when the transcript has no rename")
    func claudeSidecarAndLongLog() throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let log = root.appendingPathComponent("chat.jsonl")
        let sidecar = root.appendingPathComponent("chat/custom-title.json")
        try FileManager.default.createDirectory(at: sidecar.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(#"{"customTitle":"Sidecar name"}"#.utf8).write(to: sidecar)
        try write([#"{"type":"ai-title","sessionId":"chat","aiTitle":"Generated name"}"#, String(repeating: "x", count: 300_000)], to: log)
        #expect(ChairLogTitle.claudeName(path: log.path) == "Sidecar name")
        try FileManager.default.removeItem(at: sidecar)
        #expect(ChairLogTitle.claudeName(path: log.path) == "Generated name")
        try write([#"{"type":"custom-title","sessionId":"chat","customTitle":"Old name"}"#, String(repeating: "x", count: 300_000)], to: log)
        #expect(ChairLogTitle.claudeName(path: log.path) == "Old name")
    }

    // https://github.com/openai/codex/blob/main/codex-rs/rollout/src/session_index.rs
    // SessionIndexEntry, append_thread_name and find_thread_names_by_ids define this format.
    @Test("Codex uses the last usable indexed name for each thread")
    func codexNames() throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let index = root.appendingPathComponent("session_index.jsonl")
        try write([
            #"{"id":"first","thread_name":"Old name","updated_at":"2026-10-07T00:00:00Z"}"#,
            #"{"id":"second","thread_name":"Other name","updated_at":"2026-10-07T00:01:00Z"}"#,
            #"{"id":"first","thread_name":"New name","updated_at":"2026-10-07T00:02:00Z"}"#,
            #"{"id":"first","thread_name":" ","updated_at":"2026-10-07T00:03:00Z"}"#,
            "malformed", #"{"id":"first","thread_name":12}"#
        ], to: index)
        #expect(ChairLogTitle.codexNames(home: root) == ["first": "New name", "second": "Other name"])
        #expect(ChairLogTitle.codexNames(home: root.appendingPathComponent("missing")).isEmpty)
    }

    @Test("Claude scans only appended bytes and keeps a cut last record for the next read")
    func incrementalClaudeName() throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let log = root.appendingPathComponent("chat.jsonl")
        try write([#"{"type":"ai-title","sessionId":"chat","aiTitle":"Generated name"}"#, String(repeating: "x", count: 300_000)], to: log)
        var reader = ClaudeNameReader()
        #expect(reader.name(path: log.path) == "Generated name")
        let firstRead = reader.bytesRead
        #expect(firstRead == UInt64(try Data(contentsOf: log).count))
        #expect(reader.name(path: log.path) == "Generated name")
        #expect(reader.bytesRead == firstRead)
        let partial = Data(#"{"type":"custom-title","sessionId":"chat","customTitle":"Later"#.utf8)
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: partial)
        #expect(reader.name(path: log.path) == "Generated name")
        #expect(reader.bytesRead - firstRead == UInt64(partial.count))
        let end = Data(" name\"}\n".utf8)
        try handle.write(contentsOf: end)
        #expect(reader.name(path: log.path) == "Later name")
        #expect(reader.bytesRead - firstRead == UInt64(partial.count + end.count))
        #expect(reader.offset == UInt64(try Data(contentsOf: log).count))
        try write([#"{"type":"ai-title","sessionId":"chat","aiTitle":"After truncate"}"#], to: log)
        #expect(reader.name(path: log.path) == "After truncate")
        try write([#"{"type":"ai-title","sessionId":"chat","aiTitle":"After replaced"}"#], to: log)
        #expect(reader.name(path: log.path) == "After replaced")
    }

    @Test("Claude keeps older names in a long log and still follows later appends")
    func fullClaudeHistory() throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let log = root.appendingPathComponent("chat.jsonl")
        try write([
            #"{"type":"custom-title","sessionId":"chat","customTitle":"Older rename"}"#,
            String(repeating: "x", count: 2 * 1024 * 1024),
            #"{"type":"ai-title","sessionId":"chat","aiTitle":"Recent name"}"#
        ], to: log)
        var reader = ClaudeNameReader()
        #expect(reader.name(path: log.path) == "Older rename")
        let firstRead = reader.bytesRead
        #expect(reader.offset == UInt64(try Data(contentsOf: log).count))
        #expect(reader.name(path: log.path) == "Older rename")
        #expect(reader.bytesRead == firstRead)
        let rename = Data(#"{"type":"custom-title","sessionId":"chat","customTitle":"Later name"}"#.utf8)
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: rename)
        #expect(reader.name(path: log.path) == "Older rename")
        try handle.write(contentsOf: Data([10]))
        #expect(reader.name(path: log.path) == "Later name")
        #expect(reader.bytesRead - firstRead == UInt64(rename.count + 1))
        try write([String(repeating: "x", count: 2 * 1024 * 1024),
                   #"{"type":"ai-title","sessionId":"chat","aiTitle":"Replaced name"}"#], to: log)
        #expect(reader.name(path: log.path) == "Replaced name")
    }

    @Test("Measures the first Claude name read on a synthetic 50 MiB log")
    func claudeFirstReadCost() throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let log = root.appendingPathComponent("chat.jsonl")
        let name = #"{"type":"custom-title","sessionId":"chat","customTitle":"Older rename"}"# + "\n"
        let record = #"{"type":"assistant","message":{"content":""# + String(repeating: "x", count: 950) + #""}}"# + "\n"
        let targetBytes = 50 * 1024 * 1024
        var data = Data(name.utf8)
        let padding = Data(record.utf8)
        while data.count + padding.count <= targetBytes { data.append(padding) }
        data.append(Data((String(repeating: "x", count: targetBytes - data.count - 1) + "\n").utf8))
        try data.write(to: log)
        var elapsed: [Double] = []
        let clock = ContinuousClock()
        for _ in 0..<3 {
            var reader = ClaudeNameReader()
            let started = clock.now
            #expect(reader.name(path: log.path) == "Older rename")
            let duration = started.duration(to: clock.now).components
            elapsed.append(Double(duration.seconds) + Double(duration.attoseconds) / 1e18)
            #expect(reader.bytesRead == UInt64(targetBytes))
        }
        print("Claude 50 MiB first read seconds \(elapsed); p50 \(elapsed.sorted()[1])")
    }

    @Test("Discovery refreshes CLI renames while the first prompt stays cached")
    func refreshNames() async throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let claudeLog = root.appendingPathComponent("chat.jsonl")
        try write([#"{"type":"user","message":{"content":"Original prompt"}}"#], to: claudeLog)
        let claude = session("claude", provider: "claude", log: claudeLog.path, chair: "chat")
        let codexHome = root.appendingPathComponent(".codex")
        let codexLog = codexHome.appendingPathComponent("sessions/rollout.jsonl")
        try FileManager.default.createDirectory(at: codexLog.deletingLastPathComponent(), withIntermediateDirectories: true)
        try write([
            #"{"type":"session_meta","payload":{"id":"thread"}}"#,
            #"{"type":"response_item","payload":{"role":"user","content":[{"text":"Codex prompt"}]}}"#
        ], to: codexLog)
        let codex = session("codex", provider: "codex", log: codexLog.path, chair: nil)
        let discovery = SwarmSessionDiscovery(profiles: EmptyProfiles(), home: root)
        let sessions = [claude, codex]
        _ = await discovery.resolvedTitles(sessions: sessions, agentsBySession: [:])
        #expect(await discovery.resolvedCLINames(sessions: sessions, agentsBySession: [:]).isEmpty)
        try write([
            #"{"type":"user","message":{"content":"Original prompt"}}"#,
            #"{"type":"custom-title","sessionId":"chat","customTitle":"Renamed chat"}"#
        ], to: claudeLog)
        let index = codexHome.appendingPathComponent("session_index.jsonl")
        try write([#"{"id":"thread","thread_name":"Codex name","updated_at":"2026-10-07T00:00:00Z"}"#], to: index)
        #expect(await discovery.resolvedCLINames(sessions: sessions, agentsBySession: [:]) == [claude.id: "Renamed chat", codex.id: "Codex name"])
        try write([#"{"id":"thread","thread_name":"New Codex name","updated_at":"2026-10-07T00:01:00Z"}"#], to: index)
        #expect(await discovery.resolvedCLINames(sessions: sessions, agentsBySession: [:])[codex.id] == "New Codex name")
        try FileManager.default.removeItem(at: index)
        #expect(await discovery.resolvedCLINames(sessions: sessions, agentsBySession: [:])[codex.id] == nil)
        #expect(await discovery.resolvedTitles(sessions: sessions, agentsBySession: [:])[claude.id] == "Original prompt")
    }

    @Test("Discovery drops Claude readers when their chats leave the listing", arguments: [false, true])
    func evictsClaudeReaders(archive: Bool) async throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let log = root.appendingPathComponent("chat.jsonl")
        try write([#"{"type":"custom-title","sessionId":"chat","customTitle":"Saved name"}"#], to: log)
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: 100)], ofItemAtPath: log.path)
        let chat = session("claude", provider: "claude", log: log.path, chair: "chat")
        let discovery = SwarmSessionDiscovery(profiles: EmptyProfiles(), home: root)
        #expect(await discovery.resolvedCLINames(sessions: [chat], agentsBySession: [:])[chat.id] == "Saved name")
        let stamp = try #require(CLINameFileStamp(path: log.path))
        var archived = chat
        archived.archivedAt = 2
        #expect(await discovery.resolvedCLINames(sessions: archive ? [archived] : [], agentsBySession: [:]).isEmpty)
        // Keep the file stamp unchanged so only discarding the old reader can expose the new title.
        try write([#"{"type":"custom-title","sessionId":"chat","customTitle":"Fresh name"}"#], to: log)
        try FileManager.default.setAttributes([.modificationDate: stamp.modified], ofItemAtPath: log.path)
        #expect(CLINameFileStamp(path: log.path) == stamp)
        #expect(await discovery.resolvedCLINames(sessions: [chat], agentsBySession: [:])[chat.id] == "Fresh name")
    }

    @Test("A CLI name on the newest model link beats the oldest prompt without clipping")
    func chainPrecedence() throws {
        let old = session("old", provider: "claude", log: nil, chair: "chat")
        var new = session("new", provider: "codex", log: nil, chair: "thread")
        new.createdAt = 2
        new.continuationOf = old.id
        let title = String(repeating: "n", count: 120)
        let tree = SessionsTree.build(sessions: [old, new], titles: [old.id: "Old prompt"], cliNames: [new.id: title], repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] })
        let chat = try #require(tree.projects.first?.chats.first)
        #expect(ChatTitle.title(chat.session) == title)
        #expect(ChatTab.tabs([chat], closing: [], now: 3).first?.title == title)
        #expect(ChatTitle.title(chat.session, appName: "App name") == "App name")
    }

    @Test("The rollout's account supplies its name and another account cannot fill a miss")
    func accountOwnership() async throws {
        let root = try fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let first = root.appendingPathComponent("first")
        let second = root.appendingPathComponent("second")
        for home in [first, second] {
            try FileManager.default.createDirectory(at: home.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        }
        try write([#"{"id":"thread","thread_name":"Wrong account","updated_at":"2026-10-07T00:00:00Z"}"#], to: first.appendingPathComponent("session_index.jsonl"))
        let index = second.appendingPathComponent("session_index.jsonl")
        try write([#"{"id":"thread","thread_name":"Right account","updated_at":"2026-10-07T00:00:00Z"}"#], to: index)
        let log = second.appendingPathComponent("sessions/rollout.jsonl")
        try write([#"{"type":"session_meta","payload":{"id":"thread"}}"#], to: log)
        let chat = session("swarm-id", provider: "codex", log: log.path, chair: "thread")
        let discovery = SwarmSessionDiscovery(profiles: EmptyProfiles(homes: [first, second]), home: root)
        #expect(await discovery.resolvedCLINames(sessions: [chat], agentsBySession: [:])[chat.id] == "Right account")
        try FileManager.default.removeItem(at: index)
        #expect(await discovery.resolvedCLINames(sessions: [chat], agentsBySession: [:])[chat.id] == nil)
    }

    private func fixture() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func write(_ lines: [String], to file: URL) throws {
        try Data((lines.joined(separator: "\n") + "\n").utf8).write(to: file)
    }

    private func session(_ id: String, provider: String, log: String?, chair: String?) -> SwarmSession {
        SwarmSession(id: .init(id), talkMode: "lane", adapter: "tmux-solo", cwd: "/fixture", createdAt: 1, chairProvider: provider, chairID: chair.map(SwarmChairID.init), chairLog: log, agents: 1, messages: 0, lastMessageAt: nil)
    }
}

private struct EmptyProfiles: SwarmProfileSource {
    var homes: [URL] = []
    func accounts(provider: String) async throws -> SwarmAccountList {
        SwarmAccountList(provider: provider, source: "fixture", accounts: homes.map {
            SwarmAccount(name: $0.lastPathComponent, email: nil, home: $0.path, env: [:], signedIn: true, remainingPct: nil, summary: nil)
        }, auto: nil)
    }
    func usage() async throws -> [SwarmUsageMeter] { [] }
}
