import Foundation
import Testing
@testable import SwarmCore

@Suite("Bus-backed UI core")
struct SwarmSpikeTests {
    @Test("A UUID session id decodes as text")
    func sessionIDIsText() throws {
        let json = #"{"sessions":[{"id":"01996d95-1cab-7e21-8abd-000000000001","talk_mode":"lane","cwd":"/tmp/project","created_at":1,"chair_log":null,"agents":0,"messages":0}]}"#
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let sessions = try decoder.decode(SwarmSessionList.self, from: Data(json.utf8)).sessions
        #expect(sessions.first?.id.rawValue == "01996d95-1cab-7e21-8abd-000000000001")
    }

    @Test("The bus accepts message sequence zero")
    func firstSequenceIsZero() async throws {
        let queue = ReplyQueue([ShellResult(status: 0, stdout: "0\n", stderr: "")])
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in
            await queue.reply(to: arguments)
        }
        let seq = try await bus.send("hello", to: SwarmAgentID("coder"), in: SwarmSessionID("session-1"))
        #expect(seq == 0)
        #expect(await queue.calls == [["send", "coder", "ask"]])
    }

    @Test("Session creation accepts a UUID string")
    func createdSessionIsText() async throws {
        let id = "01996d95-1cab-7e21-8abd-000000000001"
        let queue = ReplyQueue([
            ShellResult(status: 0, stdout: "", stderr: ""),
            ShellResult(status: 0, stdout: id + "\n", stderr: ""),
            ShellResult(status: 0, stdout: "", stderr: ""),
        ])
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, _, _, _ in await queue.reply(to: arguments)
        }
        #expect(try await bus.startSession().rawValue == id)
    }

    @Test("A project shows every bus session in its folder")
    func projectDiscovery() async {
        let first = session("first", cwd: "/tmp/swarm-project", chair: "chair-1")
        let second = session("second", cwd: "/tmp/swarm-project/work", chair: "chair-1")
        let other = session("other", cwd: "/tmp/swarm-project-two", chair: nil)
        let chats = await SwarmSessionDiscovery().discover(
            sessions: [first, second, other], projects: ["/tmp/swarm-project"]
        )
        #expect(chats["/tmp/swarm-project"]?.count == 1)
        #expect(Set(chats["/tmp/swarm-project"]?.first?.sessions.map(\.id) ?? []) == [first.id, second.id])
    }

    @Test("A chair log gives its session a title")
    func chairLogTitle() throws {
        let log = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-chair-title-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try Data(#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Build the app"}]}}"#.utf8)
            .write(to: log)
        #expect(ChairLogTitle.firstUserPrompt(path: log.path) == "Build the app")
    }

    @Test("The live pane answers permission prompts")
    func permissionHookIsAbsent() {
        let arguments = AgentKind.claudeCode.interactiveArguments(
            prompt: "", sessionID: SwarmSessionID("01996d95-1cab-7e21-8abd-000000000001"),
            model: "", effort: ""
        ) ?? []
        #expect(!arguments.joined(separator: " ").contains("PermissionRequest"))
    }

    @Test("A Claude model alias reaches the CLI in its accepted form")
    func claudeModelAlias() {
        let arguments = AgentKind.claudeCode.interactiveArguments(
            prompt: "", sessionID: SwarmSessionID("01996d95-1cab-7e21-8abd-000000000001"),
            model: "opus-5-1m", effort: ""
        ) ?? []
        #expect(arguments.suffix(2) == ["--model", "claude-opus-5[1m]"])
    }

    @Test("A pane name uses the bus session id")
    func paneName() {
        let name = TmuxSessions.sessionName(sessionID: SwarmSessionID("session-1"), paneID: "pane-1")
        #expect(TmuxSessions.sessionID(ofSessionName: name) == "session-1")
        #expect(TmuxSessions.paneID(ofSessionName: name) == "pane-1")
    }

    @Test("The process table finds an agent in a shell's child")
    func processTable() {
        let table = ProcessTable(psOutput: "100 1 100 100 /bin/zsh\n101 100 101 101 codex resume abc\n")
        #expect(table.interactiveAgent(ofShell: 100) == .codex)
    }

    @Test("Launch choice rejects a role the app cannot run")
    func launchChoice() {
        var choice = SwarmLaunchChoice()
        let role = SwarmRole(
            role: "code", runner: "external", provider: "unknown", model: "model",
            effort: nil, sandbox: nil, fallbacks: []
        )
        let accepted = choice.selectRole(role)
        #expect(!accepted)
        #expect(choice.roleID == nil)
    }

    private func session(_ id: String, cwd: String, chair: String?) -> SwarmSession {
        SwarmSession(
            id: SwarmSessionID(id), talkMode: "lane", adapter: "tmux-solo", cwd: cwd,
            createdAt: 1, chairProvider: chair == nil ? nil : "codex",
            chairID: chair.map(SwarmChairID.init), chairLog: nil,
            agents: 0, messages: 0, lastMessageAt: nil
        )
    }
}

private actor ReplyQueue {
    private var replies: [ShellResult]
    private(set) var calls: [[String]] = []

    init(_ replies: [ShellResult]) { self.replies = replies }

    func reply(to arguments: [String]) -> ShellResult {
        calls.append(arguments)
        return replies.removeFirst()
    }
}
