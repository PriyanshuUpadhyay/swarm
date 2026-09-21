import Foundation
import Testing
@testable import SwarmCore

@Suite("Swarm session listing", .scratchDirectory)
struct SwarmSessionListingTests {
    private let projectID = RepoID("project")

    @Test("a session the app did not start is not listed")
    func aSessionTheAppDidNotStartIsNotListed() async {
        let outsideSession = fixture(id: "outside", cwd: "/work/project")

        #expect(await listedChats(for: outsideSession, localChats: [:]).isEmpty)
    }

    @Test("a session the app started is listed")
    func aSessionTheAppStartedIsListed() async {
        let appSession = fixture(id: "app", cwd: "/work/project")
        let appChat = SessionID("app-chat")

        let chats = await listedChats(for: appSession, localChats: [appSession.id: appChat])

        #expect(chats.map(\.id) == [appSession.id])
        #expect(chats.first?.localSessionID == appChat)
    }

    @Test("a chat with no swarm link is not listed")
    func aChatWithNoSwarmLinkIsNotListed() async throws {
        let path = TestScratch.unique("swarm-link-migration") + ".sqlite"
        let store = try Store(path: path)
        let oldChat = try await store.upsert(Session(workspaceID: nil, title: "Old chat"))
        let oldSwarm = fixture(id: "old", cwd: "/work/project")
        try await SwarmChatSession.save(oldSwarm.id, sessionID: oldChat.id, in: store)

        let raw = try SQLiteDatabase(path: path)
        try raw.setUserVersion(try raw.readUserVersion() - 1)

        let reopened = try Store(path: path)
        #expect(try await reopened.session(id: oldChat.id) != nil)
        #expect(try await reopened.setting("session.\(oldChat.id.rawValue).swarmSession") == "")
        let oldLinks = await SwarmChatSession.loadAll(from: reopened)
        #expect(await listedChats(for: oldSwarm, localChats: oldLinks).isEmpty)
    }

    @Test("a new chat still records its swarm session")
    func aNewChatStillRecordsItsSwarmSession() async throws {
        let store = try Store(path: ":memory:")
        let newChat = try await store.upsert(Session(workspaceID: nil, title: "New chat"))
        let newSwarm = fixture(id: "new", cwd: "/work/project")
        try await SwarmChatSession.save(newSwarm.id, sessionID: newChat.id, in: store)

        #expect(await SwarmChatSession.load(sessionID: newChat.id, from: store) == newSwarm.id)
        let newLinks = await SwarmChatSession.loadAll(from: store)
        let chats = await listedChats(for: newSwarm, localChats: newLinks)
        #expect(chats.map(\.localSessionID) == [newChat.id])
    }

    @Test("not listing does not touch the session")
    func notListingDoesNotTouchTheSession() async throws {
        let outsideSession = fixture(id: "outside", cwd: "/work/project")
        let store = try Store(path: ":memory:")
        try await store.setSetting("outside-session", outsideSession.id.rawValue)

        #expect(await listedChats(for: outsideSession, localChats: [:]).isEmpty)
        #expect(try await store.setting("outside-session") == outsideSession.id.rawValue)
    }

    private func listedChats(
        for session: SwarmSession, localChats: [SwarmSessionID: SessionID]
    ) async -> [SwarmProjectSession] {
        let repo = Repo(id: projectID, name: "Project", path: "/work/project")
        let discovered = await SwarmSessionDiscovery().discover(
            sessions: [session], repos: [repo], workspaces: [],
            localChats: localChats, running: [], excluding: []
        )
        return discovered[projectID] ?? []
    }

    @Test("matches linked worktrees by their common git directory")
    func matchesCommonDirectory() {
        let session = fixture(id: "10", cwd: "/work/repo/wt/feature")
        let grouped = SwarmSessionListing.grouped(
            sessions: [session],
            projects: [(projectID, .repository(commonDirectory: "/work/repo/.git"))],
            sessionIdentities: [
                session.id: .repository(commonDirectory: "/work/repo/.git"),
            ],
            excluding: []
        )

        #expect(grouped[projectID] == [session])
    }

    @Test("a non-repository project uses path containment with a component boundary")
    func matchesPlainFolder() {
        #expect(SwarmSessionListing.matches(
            project: .folder("/work/project"),
            session: .folder("/work/project/topic"),
            sessionPath: "/work/project/topic"
        ))
        #expect(!SwarmSessionListing.matches(
            project: .folder("/work/project"),
            session: .folder("/work/project-two"),
            sessionPath: "/work/project-two"
        ))
    }

    @Test("saved sessions are excluded and the rest are newest first")
    func excludesAndOrders() {
        let oldest = fixture(id: "8", createdAt: 10)
        let lowerID = fixture(id: "9", createdAt: 20)
        let higherID = fixture(id: "10", createdAt: 20)
        let saved = fixture(id: "11", createdAt: 30)
        let common = SwarmPathIdentity.repository(commonDirectory: "/repo/.git")
        let grouped = SwarmSessionListing.grouped(
            sessions: [oldest, lowerID, higherID, saved],
            projects: [(projectID, common)],
            sessionIdentities: [
                oldest.id: common,
                lowerID.id: common,
                higherID.id: common,
                saved.id: common,
            ],
            excluding: [saved.id]
        )

        #expect(grouped[projectID]?.map(\.id) == [higherID.id, lowerID.id, oldest.id])
    }

    @Test("sessions with one chair log become one newest-first chat group")
    func groupsChairChat() {
        let old = fixture(id: "6", createdAt: 1, lastMessageAt: 20, chairLog: "/chat.jsonl")
        let new = fixture(id: "10", createdAt: 2, lastMessageAt: 30, chairLog: "/chat.jsonl")
        let separate = fixture(id: "11", createdAt: 3, lastMessageAt: 40)
        let otherSeparate = fixture(id: "12", createdAt: 4, lastMessageAt: 50)

        let groups = SwarmSessionListing.chatGroups([old, separate, new, otherSeparate])

        #expect(groups.map { $0.map(\.id) } == [
            [otherSeparate.id], [separate.id], [new.id, old.id],
        ])
    }

    @Test("chair identity groups sessions when the log path changes")
    func groupsChairIdentity() {
        var old = fixture(id: "6", chairLog: "/old.jsonl")
        old.chairProvider = "claude"
        old.chairID = SwarmChairID("chat-id")
        var new = fixture(id: "10", chairLog: "/new.jsonl")
        new.chairProvider = "claude"
        new.chairID = SwarmChairID("chat-id")

        #expect(SwarmSessionListing.chatGroups([old, new]).count == 1)
    }

    @Test("workspace ownership uses path boundaries and the deepest match")
    func workspaceOwnership() {
        let outer = WorkspaceID("outer")
        let inner = WorkspaceID("inner")
        let workspaces = [
            (outer, "/Users/me/swarm/workspaces.noindex/project"),
            (inner, "/Users/me/swarm/workspaces.noindex/project/chat"),
        ]

        #expect(SwarmSessionListing.workspaceOwner(
            sessionPath: "/Users/me/swarm/workspaces.noindex/project/chat/subdir",
            workspaces: workspaces
        ) == inner)
        #expect(SwarmSessionListing.workspaceOwner(
            sessionPath: "/Users/me/swarm/workspaces.noindex/project-two",
            workspaces: workspaces
        ) == nil)
    }

    @Test("the title is the trimmed first line with a fallback")
    func titles() {
        #expect(SwarmSessionTitle.make(
            sessionID: SwarmSessionID("10"), firstUserPrompt: "\n  Fix the parser  \nIgnore this"
        ) == "Fix the parser")
        #expect(SwarmSessionTitle.make(
            sessionID: SwarmSessionID("10"), firstUserPrompt: nil
        ) == "Chat")
        let long = String(repeating: "x", count: SwarmSessionTitle.limit + 1)
        let capped = SwarmSessionTitle.make(
            sessionID: SwarmSessionID("10"), firstUserPrompt: long
        )
        #expect(capped.count == SwarmSessionTitle.limit)
        #expect(capped.hasSuffix("…"))
    }

    @Test("the newest workspace chat is represented by the workspace row")
    func workspaceChats() {
        let workspace = WorkspaceID("workspace")
        let old = fixture(id: "10", createdAt: 10)
        let new = fixture(id: "11", createdAt: 20)
        let other = fixture(id: "12", createdAt: 30)
        let rows = [
            SwarmProjectSession(sessions: [old], title: "Old", workspaceID: workspace),
            SwarmProjectSession(sessions: [other], title: "Other", workspaceID: WorkspaceID("other")),
            SwarmProjectSession(sessions: [new], title: "New", workspaceID: workspace),
        ]

        #expect(SwarmSessionListing.workspaceChats(
            rows, workspaceID: workspace
        ).map(\.id) == [new.id, old.id])
    }

    @Test("archive targets cover a whole chat group")
    func chatArchiveTargets() {
        let old = fixture(id: "10")
        let new = fixture(id: "11")
        let chat = SwarmProjectSession(sessions: [new, old], title: "Grouped chat")

        #expect(SwarmSessionListing.archiveIDs(for: chat) == [new.id, old.id])
    }

    /// The window held session 33 while the chair had opened 36 in the same chat, and the pane it
    /// drew was Home.
    @Test("a chat is found by an older session inside it, not only by its newest")
    func chatFoundByAnyMember() {
        let old = fixture(id: "33")
        let new = fixture(id: "36")
        let elsewhere = fixture(id: "12")
        let chats = [
            SwarmProjectSession(sessions: [new, old], title: "Testing swarm"),
            SwarmProjectSession(sessions: [elsewhere], title: "Another"),
        ]

        #expect(SwarmSessionListing.chat(old.id, in: chats)?.title == "Testing swarm")
        #expect(SwarmSessionListing.chat(new.id, in: chats)?.title == "Testing swarm")
        #expect(SwarmSessionListing.chat(SwarmSessionID("99"), in: chats) == nil)
    }

    /// Two council seats stopped at "Not logged in" because the app passed no profile at all.
    @Test("the signed-in account names the home a seat inherits")
    func autoAccountEnvironment() {
        let list = SwarmAccountList(
            provider: "claude",
            source: "yelo",
            accounts: [
                SwarmAccount(
                    name: "priyanshu", email: nil, home: "/homes/priyanshu",
                    env: ["CLAUDE_CONFIG_DIR": "/homes/priyanshu"],
                    signedIn: true, remainingPct: 100, summary: nil
                ),
                SwarmAccount(
                    name: "sirsendu", email: nil, home: "/homes/sirsendu",
                    env: ["CLAUDE_CONFIG_DIR": "/homes/sirsendu"],
                    signedIn: true, remainingPct: 34, summary: nil
                ),
            ],
            auto: "sirsendu"
        )

        #expect(list.autoEnvironment == ["CLAUDE_CONFIG_DIR": "/homes/sirsendu"])
        #expect(SwarmAccountList(
            provider: "agy", source: nil, accounts: [], auto: nil
        ).autoEnvironment.isEmpty)
    }

    @Test("workspace archive targets use a path component boundary")
    func workspaceArchiveTargets() {
        let root = fixture(id: "10", cwd: "/work/project")
        let child = fixture(id: "11", cwd: "/work/project/topic")
        let sibling = fixture(id: "12", cwd: "/work/project-two")

        #expect(SwarmSessionListing.archiveIDs(
            forWorkspaceAt: "/work/project", sessions: [root, child, sibling]
        ) == [root.id, child.id])
    }

    @Test("agent digests omit the chair and pick the newest summary")
    func agentDigests() throws {
        let chair = SwarmAgentID("orchestrator")
        let coder = SwarmAgentID("coder-1")
        let agents = [
            SwarmAgent(id: chair, role: "orchestrator", pane: nil, alive: nil),
            SwarmAgent(id: coder, role: "code", pane: "%1", alive: true),
        ]
        let messages = [
            message(seq: 1, sender: chair, recipient: coder, kind: "ask", body: "Build it"),
            message(seq: 4, sender: coder, recipient: chair, kind: "summary", body: "Done"),
            message(seq: 3, sender: coder, recipient: chair, kind: "note", body: "Ignore"),
            message(seq: 2, sender: coder, recipient: chair, kind: "summary", body: "Started"),
        ]

        let digest = try #require(SwarmSessionAgents.digests(
            sessionID: SwarmSessionID("10"), agents: agents, messages: messages
        ).first)
        #expect(digest.agent.id == coder)
        #expect(digest.agent.role == "code")
        #expect(digest.latestSummary == "Done")
        #expect(digest.conversation.map(\.seq) == [1, 2, 4])
    }

    @Test("chair parsing keeps each user turn in the transcript")
    func parsesChairChat() {
        let transcript = SubagentTranscript.parseChair("""
        {"type":"user","message":{"content":"First question"}}
        {"type":"assistant","message":{"content":[{"type":"text","text":"First answer"}]}}
        {"type":"user","message":{"content":[{"type":"text","text":"Second question"}]}}
        """, sessionID: SessionID("chair"))

        #expect(transcript.prompt.isEmpty)
        #expect(transcript.messages.map(\.kind) == [.user, .assistantText, .user])
        #expect(UserTurnPrompt.text(in: transcript.messages[0].payload) == "First question")
        #expect(UserTurnPrompt.text(in: transcript.messages[2].payload) == "Second question")
    }

    @Test("a Claude system user line is never drawn as the owner's turn")
    func chairSystemLinesAreNotOwnerTurns() throws {
        let transcript = SubagentTranscript.parseChair(
            [
                Self.claudeMetaUserLine,
                Self.claudeTaskNotificationUserLine,
                Self.ownerUserLine,
            ].joined(separator: "\n"),
            sessionID: SessionID("chair")
        )

        // The load-bearing half: exactly one row is the owner's, and it is the one they typed.
        // These lines arrive as `user` records, and drawing them in the owner's bubble was the
        // reason they were hidden in the first place.
        let owner = transcript.messages.filter { $0.kind == .user }
        #expect(owner.count == 1)
        #expect(UserTurnPrompt.text(in: try #require(owner.first).payload) == "Open the session view")

        // The other half, which used to be nothing at all: the scaffolding still reaches the pane
        // as its own collapsed kind, so a turn the reader watched happen leaves a trace.
        #expect(transcript.messages.map(\.kind) == [.system, .system, .user])
    }

    @Test(
        "every Claude system prefix becomes a named collapsed row, not a dropped line",
        arguments: [
            ("<local-command-caveat>", "local command"),
            ("<local-command-stdout>", "command output"),
            ("<command-name>", "local command"),
            ("<command-message>", "local command"),
            ("<command-args>", "local command"),
            ("<system-reminder>", "system reminder"),
            ("<task-notification>", "task notification"),
        ]
    )
    func chairSystemPrefixBecomesARow(_ prefix: String, _ title: String) throws {
        let transcript = SubagentTranscript.parseChair(
            #"{"type":"user","message":{"content":"\#(prefix)system text"}}"#,
            sessionID: SessionID("chair")
        )

        let row = try #require(transcript.messages.first)
        #expect(transcript.messages.count == 1)
        #expect(row.kind == .system)
        // The row says which kind of scaffolding it was, which is the whole difference between
        // this and the bare unexplained message `/compact` used to leave behind.
        #expect(OpaqueRecord.read(row.payload)?.title == title)
    }

    @Test("title extraction reads the first user prompt from the chair log")
    func extractsFirstUserPrompt() throws {
        let path = TestScratch.path("chair.jsonl")
        try """
        {"type":"progress","data":"ignored"}
        \(Self.claudeMetaUserLine)
        \(Self.claudeTaskNotificationUserLine)
        {"type":"user","message":{"content":[{"type":"text","text":"First question\\nMore"}]}}
        {"type":"user","message":{"content":"Second question"}}
        """.write(toFile: path, atomically: true, encoding: .utf8)

        #expect(ChairTranscriptOutput.firstUserPrompt(path: path) == "First question\nMore")
    }

    @Test("chair log reading appends complete lines and restarts after truncation")
    func incrementallyReadsChairLog() async throws {
        let path = TestScratch.path("growing-chair.jsonl")
        try #"{"type":"user","message":{"content":"First"}}"#.appending("\n")
            .write(toFile: path, atomically: true, encoding: .utf8)
        let reader = TranscriptLogReader(
            url: URL(fileURLWithPath: path), format: .claude(sessionID: SessionID("chair"))
        )

        let first = try await reader.read()
        #expect(first.messages.count == 1)

        let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(#"{"type":"user","message":{"content":"Sec"#.utf8))
        try handle.close()
        #expect(try await reader.read().messages.count == 1)

        let append = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
        try append.seekToEnd()
        try append.write(contentsOf: Data(#"ond"}}"#.appending("\n").utf8))
        try append.close()
        #expect(try await reader.read().messages.count == 2)

        try #"{"type":"user","message":{"content":"Restarted"}}"#.appending("\n")
            .write(toFile: path, atomically: true, encoding: .utf8)
        let restarted = try await reader.read()
        #expect(restarted.messages.count == 1)
        #expect(UserTurnPrompt.text(in: restarted.messages[0].payload) == "Restarted")
    }

    @Test("a chair log that has not grown is read as no change")
    func quietChairLogReportsNoChange() async throws {
        let path = TestScratch.path("quiet-chair.jsonl")
        try #"{"type":"user","message":{"content":"First"}}"#.appending("\n")
            .write(toFile: path, atomically: true, encoding: .utf8)
        let reader = TranscriptLogReader(
            url: URL(fileURLWithPath: path), format: .claude(sessionID: SessionID("chair"))
        )

        #expect(try await reader.readIfChanged()?.messages.count == 1)
        #expect(try await reader.readIfChanged() == nil)

        let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(#"{"type":"user","message":{"content":"Second"}}"#
            .appending("\n").utf8))
        try handle.close()
        #expect(try await reader.readIfChanged()?.messages.count == 2)

        // A truncated file is a change even though it adds no bytes, because the pane's rows are
        // now a conversation that is not there any more.
        try #"{"type":"user","message":{"content":"Restarted"}}"#.appending("\n")
            .write(toFile: path, atomically: true, encoding: .utf8)
        #expect(try await reader.readIfChanged()?.messages.count == 1)
    }

    @Test("chair log reading reports rows omitted by its byte cap")
    func capsChairLog() async throws {
        let path = TestScratch.path("capped-chair.jsonl")
        let lines = (0..<8).map {
            #"{"type":"user","message":{"content":"Question \#($0)"}}"#
        }.joined(separator: "\n") + "\n"
        try lines.write(toFile: path, atomically: true, encoding: .utf8)
        let reader = TranscriptLogReader(
            url: URL(fileURLWithPath: path),
            format: .claude(sessionID: SessionID("chair")), byteLimit: 180
        )

        let transcript = try await reader.read()

        #expect(transcript.droppedRows > 0)
        #expect(UserTurnPrompt.text(in: transcript.messages.last?.payload ?? Data()) == "Question 7")
    }

    /// The cap that keeps the fold cheap. A day-long chat is re-folded into rows four times a
    /// second while a turn runs, so what it keeps has to be bounded by count and not only by size.
    @Test("a chat keeps only its newest messages and counts the rest")
    func capsChairLogByCount() async throws {
        let path = TestScratch.path("counted-chair.jsonl")
        let total = TranscriptLogReader.messageLimit + 20
        let lines = (0..<total).map {
            #"{"type":"user","message":{"content":"Question \#($0)"}}"#
        }.joined(separator: "\n") + "\n"
        try lines.write(toFile: path, atomically: true, encoding: .utf8)
        let reader = TranscriptLogReader(
            url: URL(fileURLWithPath: path), format: .claude(sessionID: SessionID("chair"))
        )

        let transcript = try await reader.read()

        #expect(transcript.messages.count == TranscriptLogReader.messageLimit)
        #expect(transcript.droppedRows == 20)
        #expect(
            UserTurnPrompt.text(in: transcript.messages.last?.payload ?? Data())
                == "Question \(total - 1)"
        )
    }

    @Test("session interaction reports input limits and last activity")
    func sessionInteraction() {
        let session = fixture(createdAt: 10, lastMessageAt: 20)

        #expect(SwarmSessionInteraction.lastActivity(of: session) == 20)
        #expect(SwarmSessionInteraction.disabledReason(
            adapter: nil, pane: "%1", target: .chair
        ) == SwarmSessionInteraction.missingAdapterSentence)
        #expect(SwarmSessionInteraction.disabledReason(
            adapter: "herdr", pane: nil, target: .chair
        ) == "The chair has no pane to receive input.")
        #expect(SwarmSessionInteraction.disabledReason(
            adapter: "herdr", pane: nil, target: .agent
        ) == "This agent has no pane to receive input.")
        #expect(SwarmSessionInteraction.canSubmit(
            "Continue", adapter: "herdr", pane: "%1", target: .agent
        ))
        #expect(!SwarmSessionInteraction.canSubmit(
            "  \n", adapter: "herdr", pane: "%1", target: .agent
        ))
    }

    @Test("a herdr session keeps its stored adapter")
    func aHerdrSessionKeepsItsStoredAdapter() throws {
        let herdrSession = fixture()

        #expect(try SwarmSessionInteraction.adapter(for: herdrSession) == "herdr")
    }

    @Test("a session with no adapter still explains itself")
    func aSessionWithNoAdapterStillExplainsItself() {
        var unrecordedSession = fixture()
        unrecordedSession.adapter = nil

        #expect(throws: SwarmProfileError.failed(
            SwarmSessionInteraction.missingAdapterSentence
        )) { try SwarmSessionInteraction.adapter(for: unrecordedSession) }

        unrecordedSession.adapter = "  "
        #expect(throws: SwarmProfileError.failed(
            SwarmSessionInteraction.missingAdapterSentence
        )) { try SwarmSessionInteraction.adapter(for: unrecordedSession) }
    }

    private func fixture(
        id: String = "10", cwd: String = "/repo/wt/main", createdAt: Int = 1,
        lastMessageAt: Int? = nil, chairLog: String? = nil
    ) -> SwarmSession {
        SwarmSession(
            id: SwarmSessionID(id), talkMode: "lane", adapter: "herdr",
            cwd: cwd, createdAt: createdAt,
            chairLog: chairLog, agents: 2, messages: 4, lastMessageAt: lastMessageAt
        )
    }

    private func message(
        seq: Int, sender: SwarmAgentID, recipient: SwarmAgentID,
        kind: String, body: String
    ) -> SwarmMessage {
        SwarmMessage(
            seq: seq, sender: sender, recipient: recipient, kind: kind,
            body: body, createdAt: seq, read: true
        )
    }

    private static let claudeMetaUserLine = #"{"parentUuid":"parent","isSidechain":false,"type":"user","message":{"role":"user","content":"<local-command-caveat>Caveat: local command output follows.</local-command-caveat>"},"isMeta":true,"uuid":"meta-user","cwd":"/work/project"}"#

    private static let claudeTaskNotificationUserLine = #"{"parentUuid":"parent","isSidechain":false,"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>task-1</task-id>\n<output-file>/tmp/task.output</output-file>\n<status>completed</status>\n</task-notification>"},"uuid":"task-user","cwd":"/work/project"}"#

    private static let ownerUserLine = #"{"parentUuid":"parent","isSidechain":false,"type":"user","message":{"role":"user","content":"Open the session view"},"uuid":"owner-user","cwd":"/work/project"}"#
}
