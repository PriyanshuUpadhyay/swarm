import Foundation
import Testing
@testable import SwarmCore

@Suite("Session activity")
struct SessionActivityTests {
    private func session(_ id: String = "solo", log: URL? = nil, created: Int = 100, bus: Int? = 200) -> SwarmSession {
        SwarmSession(id: .init(id), talkMode: "lane", adapter: "tmux-solo", cwd: "/repo", createdAt: created,
                     chairLog: log?.path, agents: 1, messages: 0, lastMessageAt: bus)
    }

    private func writeLog(in folder: URL, modified: Int) throws -> URL {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appendingPathComponent("chair.jsonl")
        try Data("This is not a readable transcript".utf8).write(to: file)
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: Double(modified)),
                                               .posixPermissions: 0o000], ofItemAtPath: file.path)
        return file
    }

    @Test("A solo chair log write beats the bus value without reading its contents")
    func chairActivity() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let log = try writeLog(in: folder, modified: 300)
        let chat = SwarmProjectSession(sessions: [session(log: log)], title: "Solo")
        #expect(chat.lastActivity == 300)
    }

    @Test("The newest bus value in a chat chain beats an older log")
    func busActivity() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let log = try writeLog(in: folder, modified: 300)
        let chat = SwarmProjectSession(sessions: [session(log: log), session("older", bus: 400)], title: "Continued")
        #expect(chat.lastActivity == 400)
    }

    @Test("An absent or missing log keeps bus activity and creation as fallbacks")
    func fallbacks() {
        let missing = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        #expect(SwarmProjectSession(sessions: [session(log: missing)], title: "Missing").lastActivity == 200)
        #expect(SwarmProjectSession(sessions: [session()], title: "Absent").lastActivity == 200)
        #expect(SwarmProjectSession(sessions: [session(bus: nil)], title: "New").lastActivity == 100)
    }

    @Test("Log activity stays fixed during sorting and changes in the next built value")
    func refresh() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let log = try writeLog(in: folder, modified: 300)
        let source = session(log: log)
        let before = SwarmProjectSession(sessions: [source], title: "Solo")
        try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: 500)], ofItemAtPath: log.path)
        let after = SwarmProjectSession(sessions: [source], title: "Solo")
        #expect(before.lastActivity == 300)
        #expect(after.lastActivity == 500)
        #expect(before != after)
    }

    @Test("Tree rows use a discovered chair log even when the bus has no log path")
    func resolvedLog() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let log = try writeLog(in: folder, modified: 500)
        let source = session()
        let tree = SessionsTree.build(sessions: [source], chairLogs: [source.id: log.path],
                                      repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] })
        #expect(tree.session(source.id)?.lastActivity == 500)
        #expect(tree.session(source.id)?.session.chairLog == nil)
    }

    @Test("A log write moves its chat to the top while workspace order stays fixed")
    func sidebarOrder() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let log = try writeLog(in: folder, modified: 500)
        let logged = SwarmProjectSession(sessions: [session("logged", log: log, bus: 100)], title: "Logged", isRunning: false)
        let bus = SwarmProjectSession(sessions: [session("bus", bus: 300)], title: "Bus", isRunning: true)
        let project = ProjectNode(id: .repository(commonDirectory: "/repo/.git"), path: "/repo", launchDirectory: "/repo", workspaces: [
            WorkspaceNode(path: "/repo/zeta", name: "zeta", sessions: [
                SwarmProjectSession(sessions: [session("other", bus: 300)], title: "Other"),
            ]),
            WorkspaceNode(path: "/repo/alpha", name: "alpha", sessions: [bus, logged]),
        ])
        let entries = WorkspaceEntry.list(in: SessionsTree(projects: [project]))
        #expect(entries.map(\.id) == ["/repo/alpha", "/repo/zeta"])
        let rows = SidebarRows.sections(projects: [project], workspaces: entries, navigation: .init(), search: "",
                                        showingArchive: false, now: 600).flatMap(\.rows)
        #expect(rows.map(\.id) == ["/repo/alpha", "chat:logged", "chat:bus", "/repo/zeta", "chat:other"])
        #expect(rows[1].age == "1m")
    }
}
