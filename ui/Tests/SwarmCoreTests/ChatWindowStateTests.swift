import Testing
@testable import SwarmCore

@Suite("Chat window state")
struct ChatWindowStateTests {
    @Test("Each chat window has its own selection and widths")
    func stateIsolation() {
        let mainSelection = SwarmSessionID("main-chat")
        let mainWidths = PaneWidths(column: 320, chat: 500, splits: "main-split")
        var detached = ChatWindowState(selection: .init("detached-chat"))
        var secondWindow = detached
        detached.paneWidths.column = 420
        detached.paneWidths.chat = 650
        detached.paneWidths.splits = "detached-split"
        secondWindow.selection = .init("another-chat")
        #expect(detached.selection == .init("detached-chat"))
        #expect(secondWindow.paneWidths == PaneWidths())
        #expect(mainSelection == .init("main-chat"))
        #expect(mainWidths == PaneWidths(column: 320, chat: 500, splits: "main-split"))
        #expect(ChatWindowState(selection: .init("detached-chat")).paneWidths == PaneWidths())
    }

    @Test("A chat keeps one window identity through a model switch")
    func chatIdentity() {
        let original = session("chat-root", at: 1)
        var continued = session("chat-latest", at: 2)
        continued.continuationOf = original.id
        let chat = SwarmProjectSession(sessions: [original], title: "Chat")
        let switched = SwarmProjectSession(sessions: [continued, original], title: "Chat")
        #expect(ChatWindowState.id(for: chat) == .init("chat-root"))
        #expect(ChatWindowState.id(for: switched) == ChatWindowState.id(for: chat))
    }

    private func session(_ id: String, at time: Int) -> SwarmSession {
        SwarmSession(id: .init(id), talkMode: "lane", adapter: nil, cwd: "/fixture",
                     createdAt: time, chairProvider: "codex", chairLog: nil, agents: 0,
                     messages: 0, lastMessageAt: nil)
    }
}
