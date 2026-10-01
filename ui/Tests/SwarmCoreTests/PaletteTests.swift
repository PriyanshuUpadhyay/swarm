import Testing
@testable import SwarmCore

@Suite("Command palette")
struct PaletteTests {
    private let items = PaletteItems.build(
        sidebarViews: ["Workspaces", "Files", "Changes", "PR", "Usage"],
        workspaces: [
            PaletteSource.Workspace(id: "/work/atlas", title: "atlas-api", detail: "main", status: .waiting, lastActivity: 300),
            PaletteSource.Workspace(id: "/work/docs", title: "docs-site", detail: "", status: nil, lastActivity: nil),
        ],
        chats: [
            PaletteSource.Chat(id: "chat-billing", title: "Fix billing export", workspace: "atlas-api", status: .working, lastActivity: 500),
        ],
        agents: [PaletteSource.Agent(id: "reviewer", name: "reviewer", role: "review", status: .failed)]
    )

    @Test("Every query word must appear in the title or subtitle, in any case")
    func words() {
        #expect(PaletteSearch.rank(items: items, query: "BILL export").map(\.id) == ["chat:chat-billing"])
        #expect(PaletteSearch.rank(items: items, query: "atlas").map(\.id) == ["workspace:/work/atlas", "chat:chat-billing"])
        #expect(PaletteSearch.rank(items: items, query: "atlas main").map(\.id) == ["workspace:/work/atlas"])
        #expect(PaletteSearch.rank(items: items, query: "review").map(\.id) == ["agent:reviewer"])
        #expect(PaletteSearch.rank(items: items, query: "nothing like this").isEmpty)
    }

    @Test("An empty query lists recent items, newest first, then the actions")
    func emptyQuery() {
        let ranked = PaletteSearch.rank(items: items, query: "  ")
        #expect(ranked.prefix(2).map(\.id) == ["chat:chat-billing", "workspace:/work/atlas"])
        #expect(ranked.dropFirst(2).allSatisfy { $0.group == .action })
        #expect(ranked.count == 2 + PaletteItems.actions.count)
        #expect(!ranked.contains { $0.id == "workspace:/work/docs" })
    }

    @Test("Actions carry their menu key; sidebar views take the app's names")
    func actions() {
        let newWorkspace = items.first { $0.id == "action:newWorkspace" }
        #expect(newWorkspace?.title == "New Workspace")
        #expect(newWorkspace?.shortcut == "⌘N")
        #expect(items.first { $0.id == "action:newChat" }?.shortcut == "⌘T")
        let newProject = items.first { $0.id == "action:newProject" }
        #expect(newProject?.title == "New Project…")
        #expect(newProject?.shortcut == "⇧⌘N")
        #expect(items.first { $0.id == "action:sidebarView(2)" }?.title == "Show Files")
        #expect(items.first { $0.id == "action:sidebarView(2)" }?.shortcut == "⌥⌘2")
        #expect(KeyChord(.right, [.option, .command]).displayText == "⌥⌘→")
        #expect(KeyChord(.down, [.control, .command]).displayText == "⌃⌘↓")
        #expect(!items.contains { $0.id == "action:search" })
        #expect(items.first { $0.group == .agent }?.status == .failed)
    }

    @Test("A recently run action is listed with the recents")
    func recentAction() {
        let withRecent = PaletteItems.build(
            sidebarViews: [], workspaces: [], chats: [], agents: [], recentActions: [.zoom: 900]
        )
        #expect(PaletteSearch.rank(items: withRecent, query: "").first?.id == "action:zoom")
    }
}

@Suite("Open timing summary")
struct OpenTimingTests {
    @Test("p50 and p95 use the nearest rank")
    func summary() {
        let samples = (1...20).map(Double.init)
        let result = SwarmOpenScript.summary(samples)
        #expect(result?.p50 == 10)
        #expect(result?.p95 == 19)
        #expect(result?.max == 20)
        #expect(SwarmOpenScript.summary([]) == nil)
    }
}
