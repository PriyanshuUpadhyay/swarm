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

    @Test("Chair titles use pasted text and skip command and AGENTS context")
    func chairLogTitleSkipsInjectedContext() throws {
        let log = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-chair-injected-\(UUID().uuidString).jsonl")
        let command = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-chair-command-\(UUID().uuidString).jsonl")
        let agents = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-chair-agents-\(UUID().uuidString).jsonl")
        defer {
            try? FileManager.default.removeItem(at: log)
            try? FileManager.default.removeItem(at: command)
            try? FileManager.default.removeItem(at: agents)
        }
        let pasted = #"{"type":"user","message":{"content":"\n\n<pasted_content id=\"c845\">\nThe whole setup is funny\n</pasted_content id=\"c845\">"}}"#
        let compact = #"{"type":"user","message":{"content":"<command-name>/compact </command-name>"}}"#
        let instructions = ##"{"type":"user","message":{"content":"# AGENTS.md instructions for /x"}}"##
        let prompt = #"{"type":"user","message":{"content":"fix the build"}}"#
        try Data(pasted.utf8).write(to: log)
        try Data([compact, prompt].joined(separator: "\n").utf8).write(to: command)
        try Data(instructions.utf8).write(to: agents)

        #expect(ChairLogTitle.firstUserPrompt(path: log.path) == "The whole setup is funny")
        #expect(ChairLogTitle.firstUserPrompt(path: command.path) == "fix the build")
        #expect(ChairLogTitle.firstUserPrompt(path: agents.path) == nil)
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

    @Test("Launch roles follow the selected provider, including AGY")
    func providerRoles() {
        let roles = [
            SwarmRole(role: "claude-role", runner: "x", provider: "claude", model: "opus", effort: nil, sandbox: nil, fallbacks: []),
            SwarmRole(role: "agy-role", runner: "y", provider: "agy", model: "gemini", effort: nil, sandbox: nil, fallbacks: []),
        ]
        #expect(SwarmLaunchChoice.roles(roles, for: "claude").map(\.id) == ["claude-role"])
        #expect(SwarmLaunchChoice.roles(roles, for: "agy").map(\.id) == ["agy-role"])
        var choice = SwarmLaunchChoice()
        let accepted = choice.selectRole(roles[1])
        #expect(accepted)
        #expect(SwarmChatLaunchPlan(directory: "/work", role: roles[1], account: .auto)?.account == nil)
        #expect(SwarmChatLaunchPlan(directory: "/work", role: roles[0], account: .auto)?.account == "auto")
        #expect(SwarmChatLaunchPlan(directory: "/work", role: roles[0], account: nil)?.account == nil)
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
