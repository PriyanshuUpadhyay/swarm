import Foundation
import Testing
@testable import SwarmCore

@Suite("Stored chat tabs")
struct TabStripTests {
    @Test("Opening inserts after the current tab and never moves an open tab")
    func opening() {
        let strip = TabStrip(open: ["design", "review"])
        #expect(strip.opening("fix", after: "design").open == ["design", "fix", "review"])
        #expect(strip.opening("design", after: "review") == strip)
        #expect(strip.opening("fix", after: "missing").open == ["design", "review", "fix"])
        #expect(TabStrip().opening("design", after: nil).open == ["design"])
    }

    @Test("Closing and pruning keep order and remove empty groups")
    func closingAndPruning() {
        let strip = TabStrip(open: ["design", "review", "fix", "design"], groups: [
            TabGroup(id: "planning", name: "Planning", color: .blue, members: ["review", "design"]),
            TabGroup(id: "repair", name: "Repair", color: .red, members: ["fix"]),
        ])
        let closed = strip.closing("fix")
        #expect(closed.open == ["design", "review"])
        #expect(closed.groups.count == 1)
        #expect(closed.groups.first?.members == ["design", "review"])
        #expect(strip.pruned(to: ["fix"]).open == ["fix"])
        #expect(strip.pruned(to: []).groups.isEmpty)
        #expect(closed.closing("absent") == closed)
    }

    @Test("Moving works in both directions and ignores unknown keys and invalid indexes")
    func moving() {
        let strip = TabStrip(open: ["design", "review", "fix"])
        #expect(strip.moving("design", to: 2).open == ["review", "fix", "design"])
        #expect(strip.moving("fix", to: 0).open == ["fix", "design", "review"])
        #expect(strip.moving("review", to: 1) == strip)
        #expect(strip.moving("missing", to: 0) == strip)
        #expect(strip.moving("design", to: -1) == strip)
        #expect(strip.moving("design", to: 3) == strip)
    }

    @Test("Tabs round-trip in owner choices; absent tabs start empty and unknown colors are grey")
    func persistence() throws {
        var choices = OwnerChoices()
        choices.tabs = ["/repo": TabStrip(open: ["design", "review"], groups: [
            TabGroup(id: "planning", name: "Planning", color: .blue, members: ["design"], folded: true),
        ])]
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: JSONEncoder().encode(choices)) == choices)
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: Data("{}".utf8)).tabs.isEmpty)
        #expect(try JSONDecoder().decode(TabGroupColor.self, from: Data("\"future\"".utf8)) == .grey)
    }

    @Test("Absent strip fields start empty and an absent folded flag is false")
    func absentStripFields() throws {
        #expect(try JSONDecoder().decode(TabStrip.self, from: Data(#"{"open":["design"]}"#.utf8)) == TabStrip(open: ["design"]))
        #expect(try JSONDecoder().decode(TabStrip.self, from: Data("{}".utf8)) == TabStrip())
        #expect(try JSONDecoder().decode(TabStrip.self, from: Data(#"{"groups":[]}"#.utf8)) == TabStrip())
        let group = try JSONDecoder().decode(TabGroup.self, from: Data(
            #"{"id":"planning","name":"Planning","color":"blue","members":["design"]}"#.utf8
        ))
        #expect(!group.folded)
    }

    @Test("A stale workspace save keeps another workspace's tabs and project removal prunes only its tabs")
    func workspaceChanges() {
        var previous = OwnerChoices()
        previous.tabs = ["/repo/main": TabStrip(open: ["design"]), "/other": TabStrip(open: ["notes"])]
        var changed = previous
        changed.tabs["/repo/main"] = TabStrip(open: ["review", "design"])
        var current = previous
        current.tabs["/other"] = TabStrip(open: ["notes", "draft"])
        current.applyWorkspaceChanges(from: previous, to: changed)
        #expect(current.tabs["/repo/main"] == changed.tabs["/repo/main"])
        #expect(current.tabs["/other"]?.open == ["notes", "draft"])
        current.tabs["/repo#removed"] = TabStrip(open: ["offline"])
        current.removeProject("/repo", workspacePaths: ["/repo/main"])
        #expect(Set(current.tabs.keys) == ["/other"])
        var removed = changed
        removed.tabs.removeValue(forKey: "/other")
        changed.applyWorkspaceChanges(from: previous, to: removed)
        #expect(changed.tabs["/other"] == nil)
    }

    @Test("First sight seeds live chats in tree order only once and later drops missing keys")
    func firstSight() throws {
        let entries = WorkspaceEntry.list(in: tree())
        let entry = try #require(entries.first)
        let expected = entry.project.chats.filter { $0.session.isRunning != false }.map { ChatTitle.key($0.session) }
        var navigation = WorkspaceNavigation()
        navigation.recordTabFirstSight(entries)
        #expect(navigation.tabs[entry.id]?.open == expected)
        navigation.tabs[entry.id] = TabStrip()
        navigation.recordTabFirstSight(entries)
        #expect(navigation.tabs[entry.id]?.open == [])
        navigation.tabs[entry.id] = TabStrip(open: ["missing", "review", "design"])
        navigation.recordTabFirstSight(entries)
        #expect(navigation.tabs[entry.id]?.open == ["review", "design"])
    }

    @Test("History keeps the latest twenty distinct selections per workspace in view state")
    func history() throws {
        var navigation = WorkspaceNavigation()
        for number in 0..<25 { navigation.recordTabSelection("chat-\(number)", in: "/repo") }
        navigation.recordTabSelection("chat-5", in: "/repo")
        navigation.recordTabSelection("notes", in: "/other")
        let expected = (6..<25).map { "chat-\($0)" } + ["chat-5"]
        #expect(navigation.tabHistory["/repo"] == expected)
        #expect(navigation.tabHistory["/other"] == ["notes"])
        navigation.tabs["/repo"] = TabStrip(open: ["chat-5"])
        let data = try JSONEncoder().encode(navigation)
        let restored = try JSONDecoder().decode(WorkspaceNavigation.self, from: data)
        #expect(restored.tabHistory == navigation.tabHistory)
        #expect(restored.tabs.isEmpty)
        #expect(try JSONDecoder().decode(WorkspaceNavigation.self, from: Data("{}".utf8)).tabHistory.isEmpty)
    }

    @Test("After a close the last open history entry wins; empty history falls back to strip order")
    func selection() {
        #expect(TabStrip.selectionAfterClose(history: ["design", "review", "fix"], open: ["review", "design"]) == "review")
        #expect(TabStrip.selectionAfterClose(history: ["missing"], open: ["design", "review"]) == "design")
        #expect(TabStrip.selectionAfterClose(history: ["design"], open: []) == nil)
    }

    @Test("The tab builder follows only the open keys and puts starts first")
    func storedTabOrder() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        let rows = entry.project.chats
        var pending = PendingChats()
        let start = pending.add(directory: entry.id, workspace: entry.id, previous: nil)
        let tabs = ChatTab.tabs(rows, strip: .init(open: ["review", "design"]), pending: pending.items, closing: [], now: 1)
        #expect(tabs.map(\.id) == [pending[start]!.tabID, "review", "design"])
        #expect(ChatTab.tabs(rows, strip: .init(), closing: [], now: 1).isEmpty)
        #expect(ChatTab.tabs(rows.reversed(), strip: .init(open: ["design", "review"]), closing: [], now: 1)
            .map(\.id) == ["design", "review"])
    }

    @Test("A sidebar open goes right of the current chat; closing selects history and leaves the chat running")
    func openAndHide() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        let design = try #require(entry.chats.first { $0.id.rawValue == "design" })
        let review = try #require(entry.chats.first { $0.id.rawValue == "review" })
        var navigation = WorkspaceNavigation()
        navigation.tabs[entry.id] = TabStrip(open: ["design", "ended"])
        navigation.select(entry, chat: design.id)
        navigation.openTab(review, in: entry)
        #expect(navigation.tabs[entry.id]?.open == ["design", "review", "ended"])
        navigation.select(entry, chat: review.id)
        #expect(navigation.closeTab("review", in: entry) == design.id)
        #expect(navigation.selectedChat(in: entry)?.id == design.id)
        #expect(entry.chats.first { $0.id == review.id }?.isRunning == true)
        navigation.recordTabFirstSight([entry])
        #expect(navigation.tabs[entry.id]?.open == ["design", "ended"])
        navigation.openTab(review, in: entry)
        #expect(navigation.tabs[entry.id]?.open == ["design", "review", "ended"])
        #expect(navigation.closeTab("ended", in: entry) == design.id)
        #expect(navigation.closeTab("review", in: entry) == design.id)
        #expect(navigation.closeTab("design", in: entry) == nil)
        #expect(navigation.selectedChat(in: entry) == nil)
        #expect(entry.chats.count == 3)
    }

    @Test("A handoff keeps the stored tab and history key while selecting the current session")
    func continuedTab() throws {
        var oldest = try #require(tree().projects.first?.workspaces.first?.sessions.first?.session)
        oldest.createdAt = 1
        var current = oldest
        current.id = .init("continuation")
        current.createdAt = 2
        current.continuationOf = oldest.id
        let chat = SwarmProjectSession(sessions: [current, oldest], title: "Design", isRunning: true)
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo",
                                  workspaces: [WorkspaceNode(path: "/repo", name: "repo", sessions: [chat])])
        let entry = try #require(WorkspaceEntry.list(in: SessionsTree(projects: [project])).first)
        var navigation = WorkspaceNavigation()
        navigation.tabs[entry.id] = .init(open: [oldest.id.rawValue])
        navigation.select(entry, chat: current.id)
        navigation.recordTabFirstSight([entry])
        #expect(navigation.tabs[entry.id]?.open == [oldest.id.rawValue])
        #expect(navigation.tabHistory[entry.id] == [oldest.id.rawValue])
        #expect(navigation.selectedChat(in: entry)?.id == current.id)
        #expect(ChatTab.tabs(project.chats, strip: navigation.tabs[entry.id]!, closing: [], now: 2)
            .map(\.id) == [oldest.id.rawValue])
        #expect(navigation.closeTab(oldest.id.rawValue, in: entry) == nil)
    }

    @Test("Dropping reorders only tabs of that workspace and keeps selection and shortcuts stable")
    func tabDrop() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        var navigation = WorkspaceNavigation()
        navigation.tabs[entry.id] = .init(open: ["design", "review", "ended"])
        navigation.tabs["/other"] = .init(open: ["notes"])
        navigation.select(entry, chat: .init("review"))
        let movedToEnd = navigation.moveTab("design", onto: "ended", in: entry.id)
        #expect(movedToEnd)
        #expect(navigation.tabs[entry.id]?.open == ["review", "ended", "design"])
        #expect(navigation.selectedChat(in: entry)?.id == .init("review"))
        let tabs = ChatTab.tabs(entry.project.chats, strip: navigation.tabs[entry.id]!, closing: [], now: 1)
        #expect(tabs.map(\.id) == ["review", "ended", "design"])
        #expect(tabs[0].id == "review")
        #expect(tabs[2].id == "design")
        let movedToStart = navigation.moveTab("design", onto: "review", in: entry.id)
        #expect(movedToStart)
        #expect(navigation.tabs[entry.id]?.open == ["design", "review", "ended"])
        let foreignSource = navigation.moveTab("notes", onto: "review", in: entry.id)
        let foreignTarget = navigation.moveTab("design", onto: "notes", in: entry.id)
        let missingWorkspace = navigation.moveTab("design", onto: "review", in: "/missing")
        let sameTab = navigation.moveTab("review", onto: "review", in: entry.id)
        let pendingSource = navigation.moveTab("pending:start", onto: "review", in: entry.id)
        #expect(!foreignSource)
        #expect(!foreignTarget)
        #expect(!missingWorkspace)
        #expect(!sameTab)
        #expect(!pendingSource)
        #expect(navigation.tabs["/other"]?.open == ["notes"])
    }

    @Test("The overflow menu follows strip order and excludes tabs before or at the right edge")
    func overflow() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        let tabs = ChatTab.tabs(entry.project.chats, strip: .init(open: ["review", "ended", "design"]),
                                closing: [], now: 1)
        let edges: [String: Double] = ["design": 500, "ended": 250, "review": 400]
        #expect(ChatTab.overflow(tabs, trailingEdges: edges, viewportWidth: 250).map(\.id) == ["review", "design"])
        #expect(ChatTab.overflow(tabs, trailingEdges: ["design": 251, "ended": 250, "review": -10], viewportWidth: 250)
            .map(\.id) == ["design"])
        #expect(ChatTab.overflow(tabs, trailingEdges: edges, viewportWidth: 500).isEmpty)
        #expect(ChatTab.overflow(tabs, trailingEdges: edges, viewportWidth: 0).isEmpty)
        #expect(ChatTab.overflow(tabs, trailingEdges: [:], viewportWidth: 250).isEmpty)
        #expect(ChatTab.overflow([], trailingEdges: edges, viewportWidth: 250).isEmpty)
    }

    @Test("Group actions create, transfer, remove, rename, recolor, fold and delete without hiding tabs")
    func grouping() {
        let original = TabStrip(open: ["design", "review", "fix", "notes"])
        let planning = original.grouping(.new(id: "planning", name: " Planning ", color: .blue, tab: "design"))
        #expect(planning.groups.first?.name == "Planning")
        let joined = planning.grouping(.add("fix", to: "planning"))
        #expect(joined.open == ["design", "fix", "review", "notes"])
        #expect(joined.groups.first?.members == ["design", "fix"])
        let renamed = joined.grouping(.rename("planning", to: "Build"))
            .grouping(.recolor("planning", to: .green)).grouping(.fold("planning", true))
        #expect(renamed.groups.first?.name == "Build")
        #expect(renamed.groups.first?.color == .green)
        #expect(renamed.groups.first?.folded == true)
        #expect(renamed.grouping(.fold("planning", false)).groups.first?.folded == false)
        let second = renamed.grouping(.new(id: "reading", name: "Reading", color: .purple, tab: "review"))
        let transferred = second.grouping(.add("fix", to: "reading"))
        #expect(transferred.open == ["design", "review", "fix", "notes"])
        #expect(transferred.groups.first?.members == ["design"])
        #expect(transferred.groups.last?.members == ["review", "fix"])
        #expect(transferred.grouping(.remove("review")).groups.last?.members == ["fix"])
        #expect(transferred.grouping(.remove("design")).groups.map(\.id) == ["reading"])
        let deleted = transferred.grouping(.delete("reading"))
        #expect(deleted.open == transferred.open)
        #expect(deleted.groups.map(\.id) == ["planning"])
        #expect(planning.grouping(.new(id: "planning", name: "Duplicate", color: .red, tab: "review")) == planning)
        #expect(planning.grouping(.new(id: "empty", name: "  ", color: .red, tab: "review")) == planning)
        #expect(planning.grouping(.new(id: "missing", name: "Missing", color: .red, tab: "absent")) == planning)
        #expect(planning.grouping(.add("review", to: "absent")) == planning)
        #expect(planning.grouping(.add("absent", to: "planning")) == planning)
        #expect(planning.grouping(.rename("planning", to: "  ")) == planning)
        #expect(planning.grouping(.rename("absent", to: "Name")) == planning)
        #expect(planning.grouping(.recolor("absent", to: .red)) == planning)
        #expect(planning.grouping(.fold("absent", true)) == planning)
        #expect(planning.grouping(.remove("absent")) == planning)
        #expect(planning.grouping(.delete("absent")) == planning)
    }

    @Test("Opening, moving, closing and pruning preserve contiguous group runs")
    func groupOrder() {
        let grouped = TabStrip(open: ["design", "review", "fix", "notes"])
            .grouping(.new(id: "planning", name: "Planning", color: .blue, tab: "design"))
            .grouping(.add("review", to: "planning"))
            .grouping(.add("fix", to: "planning"))
        #expect(grouped.opening("draft", after: "design").open == ["design", "review", "fix", "draft", "notes"])
        #expect(grouped.moving("fix", to: 0).groups.first?.members == ["fix", "design", "review"])
        let outside = grouped.moving("review", to: 3)
        #expect(outside.open == ["design", "fix", "notes", "review"])
        #expect(outside.groups.first?.members == ["design", "fix"])
        let inside = grouped.moving("notes", to: 1)
        #expect(inside.open == ["design", "notes", "review", "fix"])
        #expect(inside.groups.first?.members == inside.open)
        #expect(grouped.closing("review").groups.first?.members == ["design", "fix"])
        #expect(grouped.closing("design").open == ["review", "fix", "notes"])
        #expect(grouped.closing("design").closing("review").closing("fix").groups.isEmpty)
        let scattered = TabStrip(open: ["design", "notes", "review", "fix"], groups: grouped.groups)
        #expect(scattered.pruned(to: Set(scattered.open)).open == grouped.open)
    }

    @Test("A sidebar open after a grouped current tab stays ungrouped after the whole group")
    func openAfterGroup() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        let ended = try #require(entry.chats.first { $0.id.rawValue == "ended" })
        var navigation = WorkspaceNavigation()
        navigation.tabs[entry.id] = TabStrip(open: ["design", "review"])
            .grouping(.new(id: "planning", name: "Planning", color: .blue, tab: "design"))
            .grouping(.add("review", to: "planning"))
        navigation.select(entry, chat: .init("design"))
        navigation.openTab(ended, in: entry)
        #expect(navigation.tabs[entry.id]?.open == ["design", "review", "ended"])
        #expect(navigation.tabs[entry.id]?.groups.first?.members == ["design", "review"])
        let tabs = ChatTab.tabs(entry.project.chats, strip: navigation.tabs[entry.id]!, closing: [], now: 1)
        #expect(tabs.last?.group == nil)
    }

    @Test("Tab presentation projects contiguous runs and keeps folded members for shortcuts")
    func groupRuns() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        let strip = TabStrip(open: ["design", "review", "ended"])
            .grouping(.new(id: "planning", name: "Planning", color: .blue, tab: "design"))
            .grouping(.add("review", to: "planning")).grouping(.fold("planning", true))
        let tabs = ChatTab.tabs(entry.project.chats, strip: strip, closing: [], now: 1)
        let runs = ChatTab.runs(tabs)
        #expect(tabs.map(\.id) == ["design", "review", "ended"])
        #expect(runs.count == 2)
        #expect(runs.first?.group?.name == "Planning")
        #expect(runs.first?.group?.folded == true)
        #expect(runs.first?.tabs.map(\.id) == ["design", "review"])
        #expect(runs.last?.group == nil)
        #expect(runs.last?.tabs.map(\.id) == ["ended"])
        var navigation = WorkspaceNavigation()
        navigation.tabs[entry.id] = strip
        let added = navigation.groupTab(.add("ended", to: "planning"), in: entry.id)
        let foreign = navigation.groupTab(.add("notes", to: "planning"), in: entry.id)
        let missing = navigation.groupTab(.fold("planning", false), in: "/missing")
        #expect(added)
        #expect(!foreign)
        #expect(!missing)
        #expect(navigation.tabs[entry.id]?.groups.first?.members == ["design", "review", "ended"])
    }

    @Test("Child badges count live children and focus the first waiting sidebar child")
    func childBadges() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        let design = try #require(entry.chats.first { $0.id.rawValue == "design" })
        let strip = TabStrip(open: ["design", "review"])
        let chair = SwarmAgent(id: .init("orchestrator"), role: "chair", pane: "chair-pane", alive: true, state: "waiting")
        let builder = SwarmAgent(id: .init("builder"), role: "build", pane: "build-pane", alive: true, state: "working")
        let reviewer = SwarmAgent(id: .init("reviewer"), role: "review", pane: "review-pane", alive: true, state: "waiting")
        let analyst = SwarmAgent(id: .init("analyst"), role: "analysis", pane: "analysis-pane", alive: true, state: "waiting")
        let idle = SwarmAgent(id: .init("idle"), role: "review", pane: "idle-pane", alive: true, state: "done")
        let failed = SwarmAgent(id: .init("failed"), role: "build", pane: "failed-pane", alive: true, state: "failed")
        let ended = SwarmAgent(id: .init("ended"), role: "review", pane: "old-pane", alive: false, state: "waiting")
        let registered = SwarmAgent(id: .init("registered"), role: "review", pane: nil, alive: true, state: "waiting")
        let unknown = SwarmAgent(id: .init("unknown"), role: "review", pane: "unknown-pane", alive: nil, state: "waiting")
        let tabs = ChatTab.tabs(entry.project.chats, strip: strip, closing: [], now: 1,
                                agentsBySession: [design.id: [chair, builder, reviewer, ended, registered, unknown]])
        #expect(tabs[0].children?.text == "2 · 1 waiting")
        #expect(tabs[0].children?.firstWaiting == reviewer.id)
        #expect(tabs[1].children == nil)
        let multiple = ChatTab.ChildCount.make(session: design.session, agents: [reviewer, builder, analyst])
        #expect(multiple?.text == "3 · 2 waiting")
        #expect(multiple?.firstWaiting == analyst.id)
        let noWaiting = ChatTab.ChildCount.make(session: design.session, agents: [chair, builder, idle, failed])
        #expect(noWaiting?.text == "3")
        #expect(noWaiting?.firstWaiting == nil)
        #expect(ChatTab.ChildCount.make(session: design.session, agents: [chair, ended, registered, unknown]) == nil)
        #expect(ChatTab.ChildCount.make(session: design.session, agents: []) == nil)
        var namedChairSession = design.session
        namedChairSession.chairID = .init("chair-custom")
        let namedChair = SwarmAgent(id: .init("chair-custom"), role: "chat", pane: "chair-pane", alive: true, state: "waiting")
        #expect(ChatTab.ChildCount.make(session: namedChairSession, agents: [namedChair, builder])?.text == "1")
        let rowID = SidebarRows.childID(chat: design, agent: reviewer)
        let target = SidebarRows.selection(for: rowID, in: [entry], agentsBySession: [design.id: [chair, builder, reviewer]])
        #expect(target?.chatID == design.id)
        #expect(target?.agentSessionID == design.id)
        #expect(target?.agentID == tabs[0].children?.firstWaiting)
        #expect(tabs[0].waitingChildSelection(in: [entry], agentsBySession: [design.id: [chair, builder, reviewer]]) == target)
        #expect(tabs[0].waitingChildSelection(in: [entry], agentsBySession: [:]) == nil)
        #expect(tabs[1].waitingChildSelection(in: [entry], agentsBySession: [:]) == nil)
    }

    @Test("An ended tab can hide, and leaving it removes only its open key and history")
    func endedTabs() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        let strip = TabStrip(open: ["design", "ended", "review"])
            .grouping(.new(id: "finished", name: "Finished", color: .grey, tab: "ended"))
        let tabs = ChatTab.tabs(entry.project.chats, strip: strip, closing: [], now: 1)
        let ended = try #require(tabs.first { $0.id == "ended" })
        #expect(ended.canHide)
        #expect(!ended.canClose)
        var pending = PendingChats()
        _ = pending.add(directory: entry.id, workspace: entry.id, previous: nil)
        #expect(ChatTab.tabs(entry.project.chats, strip: strip, pending: pending.items, closing: [], now: 1)
            .first?.canHide == false)
        var navigation = WorkspaceNavigation()
        navigation.tabs[entry.id] = strip
        navigation.select(entry, chat: .init("design"))
        navigation.select(entry, chat: .init("ended"))
        let stayed = navigation.hideEndedTab(leaving: .init("ended"), selecting: .init("ended"), in: [entry])
        #expect(!stayed)
        #expect(navigation.tabs[entry.id] == strip)
        let left = navigation.hideEndedTab(leaving: .init("ended"), selecting: .init("review"), in: [entry])
        #expect(left)
        #expect(navigation.tabs[entry.id]?.open == ["design", "review"])
        #expect(navigation.tabs[entry.id]?.groups.isEmpty == true)
        #expect(navigation.tabHistory[entry.id] == ["design"])
        #expect(entry.chats.count == 3)
        #expect(entry.chats.first { $0.id.rawValue == "ended" }?.isRunning == false)
        navigation.openTab(try #require(entry.chats.first { $0.id.rawValue == "ended" }), in: entry)
        #expect(navigation.tabs[entry.id]?.open.contains("ended") == true)
        let leftForHomeOrStart = navigation.hideEndedTab(leaving: .init("ended"), selecting: nil, in: [entry])
        #expect(leftForHomeOrStart)
        let liveLeft = navigation.hideEndedTab(leaving: .init("design"), selecting: nil, in: [entry])
        let unknownLeft = navigation.hideEndedTab(leaving: .init("missing"), selecting: nil, in: [entry])
        let nothingLeft = navigation.hideEndedTab(leaving: nil, selecting: .init("review"), in: [entry])
        let alreadyHidden = navigation.hideEndedTab(leaving: .init("ended"), selecting: nil, in: [entry])
        #expect(!liveLeft)
        #expect(!unknownLeft)
        #expect(!nothingLeft)
        #expect(!alreadyHidden)
    }

    @Test("Leaving an ended chat across workspaces hides only its tab; a same-chat handoff keeps it")
    func endedWorkspaceAndHandoff() throws {
        let entry = try #require(WorkspaceEntry.list(in: tree()).first)
        var navigation = WorkspaceNavigation()
        navigation.tabs[entry.id] = .init(open: ["design", "ended"])
        navigation.tabs["/other"] = .init(open: ["notes"])
        let changedWorkspace = navigation.hideEndedTab(leaving: .init("ended"), selecting: .init("notes"), in: [entry])
        #expect(changedWorkspace)
        #expect(navigation.tabs[entry.id]?.open == ["design"])
        #expect(navigation.tabs["/other"]?.open == ["notes"])
        let ended = try #require(entry.chats.first { $0.id.rawValue == "ended" })
        var current = ended.session
        current.id = .init("continuation")
        current.continuationOf = ended.id
        current.createdAt += 1
        let chat = SwarmProjectSession(sessions: [current, ended.session], title: "Ended", isRunning: false)
        let project = ProjectNode(id: .folder("/repo"), path: "/repo", launchDirectory: "/repo",
                                  workspaces: [WorkspaceNode(path: "/repo", name: "repo", sessions: [chat])])
        let continuedEntry = try #require(WorkspaceEntry.list(in: SessionsTree(projects: [project])).first)
        navigation.tabs[entry.id] = .init(open: ["ended"])
        let continued = navigation.hideEndedTab(leaving: ended.id, selecting: current.id, in: [continuedEntry])
        #expect(!continued)
        #expect(navigation.tabs[entry.id]?.open == ["ended"])
    }

    private func tree() -> SessionsTree {
        let chats = [("design", true), ("review", true), ("ended", false)].map { key, running in
            SwarmProjectSession(
                sessions: [SwarmSession(id: .init(key), talkMode: "lane", adapter: nil, cwd: "/repo",
                                      createdAt: 1, chairProvider: "codex", chairLog: nil, agents: 1,
                                      messages: 0, lastMessageAt: nil)], title: key,
                isRunning: running, provider: "codex"
            )
        }
        return SessionsTree(projects: [ProjectNode(
            id: .folder("/repo"), path: "/repo", launchDirectory: "/repo",
            workspaces: [WorkspaceNode(path: "/repo", name: "repo", sessions: chats)]
        )])
    }
}
