import Foundation
import Testing
@testable import SwarmCore

@Suite("Swarm session listing", .scratchDirectory)
struct SwarmSessionListingTests {
    private let projectID = RepoID("project")

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
        ) == "Session 10")
        let long = String(repeating: "x", count: SwarmSessionTitle.limit + 1)
        let capped = SwarmSessionTitle.make(
            sessionID: SwarmSessionID("10"), firstUserPrompt: long
        )
        #expect(capped.count == SwarmSessionTitle.limit)
        #expect(capped.hasSuffix("…"))
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

    @Test("chair parsing hides Claude system user lines")
    func hidesChairSystemLines() throws {
        let transcript = SubagentTranscript.parseChair(
            [
                Self.claudeMetaUserLine,
                Self.claudeTaskNotificationUserLine,
                Self.ownerUserLine,
            ].joined(separator: "\n"),
            sessionID: SessionID("chair")
        )

        #expect(transcript.messages.count == 1)
        let row = try #require(transcript.messages.first)
        #expect(row.kind == .user)
        #expect(UserTurnPrompt.text(in: row.payload) == "Open the session view")
    }

    @Test(
        "chair parsing hides every Claude system prefix",
        arguments: [
            "<local-command-caveat>",
            "<local-command-stdout>",
            "<command-name>",
            "<command-message>",
            "<command-args>",
            "<system-reminder>",
            "<task-notification>",
        ]
    )
    func hidesChairSystemPrefix(_ prefix: String) {
        let transcript = SubagentTranscript.parseChair(
            #"{"type":"user","message":{"content":"\#(prefix)system text"}}"#,
            sessionID: SessionID("chair")
        )

        #expect(transcript.messages.isEmpty)
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
