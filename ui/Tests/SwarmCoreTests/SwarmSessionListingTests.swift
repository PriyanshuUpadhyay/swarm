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
            agents: agents, messages: messages
        ).first)
        #expect(digest.id == coder)
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

    @Test("title extraction reads the first user prompt from the chair log")
    func extractsFirstUserPrompt() throws {
        let path = TestScratch.path("chair.jsonl")
        try """
        {"type":"progress","data":"ignored"}
        {"type":"user","message":{"content":[{"type":"text","text":"First question\\nMore"}]}}
        {"type":"user","message":{"content":"Second question"}}
        """.write(toFile: path, atomically: true, encoding: .utf8)

        #expect(ChairTranscriptOutput.firstUserPrompt(path: path) == "First question\nMore")
    }

    private func fixture(
        id: String, cwd: String = "/repo/wt/main", createdAt: Int = 1
    ) -> SwarmSession {
        SwarmSession(
            id: SwarmSessionID(id), talkMode: "lane", adapter: nil, cwd: cwd, createdAt: createdAt,
            chairLog: nil, agents: 2, messages: 4, lastMessageAt: nil
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
}
