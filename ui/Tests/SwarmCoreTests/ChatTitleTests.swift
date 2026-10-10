import Foundation
import Testing
@testable import SwarmCore

@Suite("Chat titles")
struct ChatTitleTests {
    @Test("Titles use app name, CLI name, prompt line, then provider and id")
    func precedence() {
        let id = SwarmSessionID("12345678-rest")
        #expect(ChatTitle.resolve(appName: " Fix login ", cliName: "CLI title", firstLine: "Error log", provider: "codex", id: id) == "Fix login")
        #expect(ChatTitle.resolve(appName: " \n", cliName: "CLI title", firstLine: "Error log", provider: "codex", id: id) == "CLI title")
        #expect(ChatTitle.resolve(appName: nil, cliName: nil, firstLine: "\n First prompt \nSecond line", provider: "codex", id: id) == "First prompt")
        #expect(ChatTitle.resolve(appName: nil, cliName: " ", firstLine: "", provider: "agy", id: id) == "agy 12345678")
        #expect(ChatTitle.resolve(appName: nil, cliName: nil, firstLine: nil, provider: nil, id: id) == "Chat 12345678")
        #expect(ChatTitle.resolve(appName: nil, cliName: nil, firstLine: String(repeating: "x", count: 100), provider: "codex", id: id).count == 80)
        #expect(ChatTitle.resolve(appName: String(repeating: "x", count: 100), cliName: nil, firstLine: nil, provider: "codex", id: id).count == 100)
    }

    @Test("Rows, tabs and palette keep an app name across model switches")
    func sharedTitle() throws {
        let old = session("oldest-id", provider: "claude", at: 1)
        var new = session("newest-id", provider: "codex", at: 2)
        new.continuationOf = old.id
        let tree = SessionsTree.build(
            sessions: [new, old], titles: [old.id: "Original prompt", new.id: "Switch prompt"],
            repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] }
        )
        let chat = try #require(tree.projects.first?.chats.first)
        #expect(ChatTitle.key(chat.session) == old.id.rawValue)
        #expect(SidebarRows.chatID(chat.session) == "chat:\(ChatTitle.key(chat.session))")
        var navigation = WorkspaceNavigation()
        navigation.chatNames[old.id.rawValue] = "Fix login"
        let entries = WorkspaceEntry.list(in: tree)
        let rows = SidebarRows.sections(projects: tree.projects, workspaces: entries, navigation: navigation, search: "", showingArchive: false, now: 3).flatMap(\.rows)
        #expect(rows.first { $0.kind == .chat }?.title == "Fix login")
        #expect(ChatTab.tabs([chat], strip: .init(open: [ChatTitle.key(chat.session)]), closing: [], now: 3, navigation: navigation).first?.title == "Fix login")
        #expect(PaletteSource.workspaces(entries, navigation: navigation, now: 3).chats.first?.title == "Fix login")
        #expect(SessionRowPresentation.make(chat, now: 3).title == "Original prompt")
    }

    @Test("A missing title has the same provider and id fallback on every surface")
    func fallback() throws {
        let tree = SessionsTree.build(sessions: [session("abcdefgh-rest", provider: "agy", at: 1)], repositoryPathsResolver: { _ in nil }, worktreeLister: { _ in [] })
        let chat = try #require(tree.projects.first?.chats.first)
        let navigation = WorkspaceNavigation()
        let entries = WorkspaceEntry.list(in: tree)
        #expect(chat.session.title == "agy abcdefgh")
        #expect(SessionRowPresentation.make(chat, now: 3).title == "agy abcdefgh")
        #expect(ChatTab.tabs([chat], strip: .init(open: [ChatTitle.key(chat.session)]), closing: [], now: 3).first?.title == "agy abcdefgh")
        #expect(PaletteSource.workspaces(entries, navigation: navigation, now: 3).chats.first?.title == "agy abcdefgh")
    }

    private func session(_ id: String, provider: String, at: Int) -> SwarmSession {
        SwarmSession(id: .init(id), talkMode: "lane", adapter: "tmux-solo", cwd: "/fixture", createdAt: at, chairProvider: provider, chairLog: nil, agents: 1, messages: 0, lastMessageAt: nil)
    }
}
