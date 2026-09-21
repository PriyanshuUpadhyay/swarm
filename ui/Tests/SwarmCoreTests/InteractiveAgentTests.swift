import Foundation
import Testing
@testable import SwarmCore

@Suite("Interactive CLI agents")
struct InteractiveAgentTests {
    @Test("Terminal setup previews stay compact and cannot execute prompt escape sequences")
    func setupPreview() {
        let output = AgentKind.codex.interactiveSetupOutput(
            prompt: "first line\nsecond line\u{1b}[2J" + String(repeating: "x", count: 250),
            log: "Installing packages\nDone\n"
        )
        #expect(output.contains("Codex"))
        #expect(output.contains("first line second line[2J"))
        #expect(!output.contains("\u{1b}[2J"))
        #expect(!output.contains(String(repeating: "x", count: 200)))
        #expect(output.hasSuffix("Installing packages\nDone\n"))
        let empty = AgentKind.claudeCode.interactiveSetupOutput(prompt: nil, log: "")
        #expect(!empty.contains("Queued prompt"))
        #expect(empty.contains("Waiting for setup output"))
    }

    @Test("Native CLI busy markers exclude idle prompts and old history")
    func screenActivity() {
        #expect(AgentKind.codex.interactiveScreenIsBusy(lines: ["• Working (12s • esc to interrupt)"]))
        #expect(AgentKind.codex.interactiveScreenIsBusy(lines: ["  Thinking (1m 12s • esc to interrupt)"]))
        #expect(AgentKind.claudeCode.interactiveScreenIsBusy(lines: ["  esc to interrupt · ctrl+t to hide tasks"]))
        #expect(AgentKind.claudeCode.interactiveScreenIsBusy(lines: ["  esc interrupt"]))
        #expect(AgentKind.claudeCode.interactiveScreenIsBusy(
            lines: ["esc to interrupt"] + Array(repeating: "", count: 40)
        ))
        #expect(!AgentKind.codex.interactiveScreenIsBusy(lines: ["› explain (12s • esc to interrupt)"]))
        #expect(!AgentKind.codex.interactiveScreenIsBusy(lines: ["› What should I work on?"]))
        #expect(!AgentKind.claudeCode.interactiveScreenIsBusy(lines: ["❯ explain esc to interrupt"]))
        #expect(!AgentKind.claudeCode.interactiveScreenIsBusy(
            lines: ["esc to interrupt"] + Array(repeating: "idle output", count: 6)
        ))
        #expect(!AgentKind.codex.interactiveScreenIsBusy(
            lines: ["• Working (12s • esc to interrupt)"] + Array(repeating: "idle output", count: 12)
        ))
    }

    @Test("Interactive arguments preserve the prompt and normal permission choices")
    func arguments() throws {
        let id = SessionID.new()
        let prompt = "--help\n$(touch unwanted); 'quoted'"
        let claude = try #require(AgentKind.claudeCode.interactiveArguments(
            prompt: prompt, sessionID: id, model: "opus", effort: "high", permissionMode: .plan
        ))
        #expect(claude.suffix(2) == ["--", prompt])
        #expect(claude.contains("--session-id"))
        #expect(claude.contains(id.rawValue))
        #expect(claude.contains("plan"))
        #expect(!claude.contains("--print"))
        let index = try #require(claude.firstIndex(of: "--settings"))
        let settings = try #require(JSONSerialization.jsonObject(with: Data(claude[index + 1].utf8)) as? [String: Any])
        let hooks = try #require(settings["hooks"] as? [String: Any])
        let permission = try #require(hooks["PermissionRequest"] as? [[String: Any]])
        let permissionHooks = try #require(permission.first?["hooks"] as? [[String: Any]])
        #expect(permissionHooks.first?["timeout"] as? Int == 130)
        #expect(permissionHooks.first?["command"] as? String == AgentKind.interactivePermissionHookCommand)
        let notifications = try #require(hooks["Notification"] as? [[String: Any]])
        #expect(notifications.first?["matcher"] as? String == "permission_prompt|idle_prompt")

        let codex = try #require(AgentKind.codex.interactiveArguments(
            prompt: prompt, sessionID: id, model: "gpt-test", effort: "high", permissionMode: .acceptEdits
        ))
        #expect(codex.suffix(2) == ["--", prompt])
        #expect(!codex.contains("--no-alt-screen"))
        #expect(codex.contains("workspace-write"))
        #expect(codex.contains("on-request"))
        #expect(!codex.contains("exec"))
        #expect(!codex.contains("--dangerously-bypass-hook-trust"))
        #expect(codex.contains("model_reasoning_effort=\"high\""))
        // Nobody is at the pane of a chat Swarm started to answer "Update available! 1. Update
        // now 2. Skip", and Codex asks that before it reads the prompt.
        #expect(codex.contains("check_for_update_on_startup=false"))
        let codexPermission = try #require(codex.first { $0.hasPrefix("hooks.PermissionRequest=") })
        #expect(codexPermission.contains("timeout=130"))
        // The command is TOML-escaped, so the test matches a phrase only the waiting hook has.
        #expect(codexPermission.contains("swarm_permission_dir"))
        let codexStop = try #require(codex.first { $0.hasPrefix("hooks.Stop=") })
        #expect(codexStop.contains("timeout=3"))
        #expect(!codexStop.contains("swarm_permission_dir"))
    }

    @Test("Claude model aliases use the same translation as managed chats")
    func modelAlias() throws {
        let arguments = try #require(AgentKind.claudeCode.interactiveArguments(
            prompt: "", sessionID: .new(), model: "opus-5-1m", effort: ""
        ))
        let index = try #require(arguments.firstIndex(of: "--model"))
        #expect(arguments[index + 1] == ModelAlias.cliValue(for: "opus-5-1m"))
    }

    @Test("Shell launch delivers literal prompt text to the CLI")
    func literalPrompt() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("swarm-interactive-\(UUID())")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let fakeCLI = root.appendingPathComponent("claude")
        try Data("#!/bin/sh\nfor argument do printf '%s\\n' \"$argument\"; done\n".utf8).write(to: fakeCLI)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fakeCLI.path)
        let prompt = "first line\n$(touch unwanted); 'quoted' `touch unwanted`"
        let command = try #require(AgentKind.claudeCode.interactiveCommand(
            directory: root.path, prompt: prompt, sessionID: .new(), model: "", effort: "", permissionMode: .auto
        ))
        #expect(!command.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }))
        let result = try await Shell.run("/bin/sh", ["-c", command], env: ["PATH": root.path + ":/usr/bin:/bin"])
        #expect(result.ok)
        #expect(result.stdout.hasSuffix("--\n" + prompt + "\n"))
        #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("unwanted").path))
    }

    @Test("Resume preserves native identity without replaying the opening prompt", arguments: [AgentKind.claudeCode, .codex])
    func resume(agent: AgentKind) throws {
        let arguments = try #require(agent.interactiveArguments(
            prompt: "", sessionID: .new(), model: "", effort: "", resuming: "native-session"
        ))
        #expect(arguments.prefix(2) == [agent == .claudeCode ? "--resume" : "resume", "native-session"])
        #expect(!arguments.contains("--session-id"))
        #expect(!arguments.contains("--permission-mode"))
        #expect(!arguments.contains("--sandbox"))
        #expect(!arguments.contains("--model"))
        #expect(!arguments.contains("--"))
    }

    @Test("Lifecycle hook states use events, never process presence", arguments: [
        ("UserPromptSubmit", SessionState.running), ("PreToolUse", .running), ("PostToolUse", .running),
        ("PermissionRequest", .waiting), ("Stop", .idle), ("SessionStart", .idle),
        ("SessionEnd", .idle), ("StopFailure", .failed)
    ])
    func hookState(event: String, expected: SessionState) {
        let data = Data("{\"hook_event_name\":\"\(event)\",\"session_id\":\"native-id\"}".utf8)
        #expect(AgentKind.interactiveHookState(data: data) == expected)
        #expect(AgentKind.interactiveHookSessionID(data: data) == "native-id")
    }

    @Test("Malformed and unrelated hooks do not invent activity")
    func malformedHooks() {
        #expect(AgentKind.interactiveHookState(data: Data("broken".utf8)) == nil)
        #expect(AgentKind.interactiveHookState(data: Data("{\"hook_event_name\":\"unknown\"}".utf8)) == nil)
        #expect(AgentKind.interactiveHookSessionID(data: Data("{\"session_id\":\"\"}".utf8)) == nil)
        let compact = Data("{\"hook_event_name\":\"SessionStart\",\"source\":\"compact\"}".utf8)
        #expect(AgentKind.interactiveHookState(data: compact) == .running)
    }

    @Test("Hook snapshots recover missed events through the session lifecycle")
    func recoversMissedEvents() {
        let states: [SessionState] = [.idle, .running, .waiting, .failed, .cancelled]
        let date = Date(timeIntervalSince1970: 100)
        for initial in states {
            for observed in states {
                var session = Session(workspaceID: WorkspaceID("workspace"), state: initial)
                session.applyInteractiveState(observed, at: date)
                #expect(session.state == observed)
                if initial != observed { #expect(session.updatedAt == date) }
            }
        }
    }

    @Test("Hook writer replaces complete JSON and never blocks the CLI")
    func writesHook() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("swarm-hook-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        let destination = AgentKind.interactiveStatusURL(sessionID: SessionID("../unsafe"), base: root)
        #expect(destination.deletingLastPathComponent().deletingLastPathComponent().lastPathComponent == "swarm-cli-status")
        let payload = "{\"hook_event_name\":\"Stop\",\"session_id\":\"native\"}"
        let result = try await Shell.run("/bin/sh", ["-c", AgentKind.interactiveHookCommand],
                                         env: ["SWARM_UI_CLI_STATUS_FILE": destination.path], stdin: payload)
        #expect(result.ok)
        #expect(result.stdout.isEmpty)
        #expect(try String(contentsOf: destination, encoding: .utf8) == payload)
        let failed = try await Shell.run("/bin/sh", ["-c", AgentKind.interactiveHookCommand],
                                         env: ["SWARM_UI_CLI_STATUS_FILE": "/dev/null/impossible"])
        #expect(failed.ok)
        #expect(failed.stdout.isEmpty)
    }

    @Test("Permission answers use Claude's measured response shapes")
    func permissionAnswers() throws {
        #expect(String(decoding: try InteractivePermissionAnswer.allow.data, as: UTF8.self)
            == #"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#)
        #expect(String(decoding: try InteractivePermissionAnswer.deny.data, as: UTF8.self)
            == #"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Denied in Swarm"}}}"#)
        #expect(try InteractivePermissionAnswer.terminal.data.isEmpty)
    }

    @Test("Interactive questions keep every question and build Claude's exact answer")
    func interactiveQuestions() throws {
        let token = UUID().uuidString
        let input = try #require(JSONValue.parse(#"""
        {"questions":[
          {"question":"Which colour do you prefer?","header":"Colour","options":[
            {"label":"Red","description":"The colour red"},
            {"label":"Blue","description":"The colour blue"}
          ],"multiSelect":false},
          {"question":"Which checks?","header":"Checks","options":[
            {"label":"Build","description":"Compile it"},
            {"label":"Tests","description":"Run tests"}
          ],"multiSelect":true}
        ]}
        """#))
        let parsed = try interactiveAsk(token: token, toolName: "AskUserQuestion", input: input)
        let ask = try #require(parsed)

        let questions = AgentQuestionnaire.questions(in: ask.input)
        #expect(ask.isQuestion)
        #expect(questions.count == 2)
        #expect(questions[0].header == "Colour")
        #expect(questions[0].question == "Which colour do you prefer?")
        #expect(questions[0].options.map(\.label) == ["Red", "Blue"])
        #expect(questions[0].options.map(\.description) == ["The colour red", "The colour blue"])
        #expect(!questions[0].multiSelect)
        #expect(questions[1].multiSelect)

        let answered = AgentQuestionnaire.answered(input, answers: [
            "Which colour do you prefer?": "Red",
            "Which checks?": AgentQuestionnaire.joined(["Build", "Tests"]),
        ])
        let json = String(decoding: try InteractivePermissionAnswer.answer(input: answered).data, as: UTF8.self)
        let expected =
            #"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedInput":"#
            + #"{"answers":{"Which checks?":"Build, Tests","Which colour do you prefer?":"Red"},"questions":["#
            + #"{"header":"Colour","multiSelect":false,"options":[{"description":"The colour red","label":"Red"},"#
            + #"{"description":"The colour blue","label":"Blue"}],"question":"Which colour do you prefer?"},"#
            + #"{"header":"Checks","multiSelect":true,"options":[{"description":"Compile it","label":"Build"},"#
            + #"{"description":"Run tests","label":"Tests"}],"question":"Which checks?"}]}}}}"#
        #expect(json == expected)
    }

    @Test("An unknown interactive tool keeps the existing permission shape")
    func unknownInteractiveTool() throws {
        let token = UUID().uuidString
        let input = try #require(JSONValue.parse(#"{"opaque":42}"#))
        let parsed = try interactiveAsk(token: token, toolName: "FutureTool", input: input)
        let ask = try #require(parsed)

        #expect(ask.toolName == "FutureTool")
        #expect(ask.input == input)
        #expect(!ask.isQuestion)
        #expect(AgentQuestionnaire.questions(in: ask.input).isEmpty)
    }

    @Test("Permission cards follow the marker and the latest hook event")
    func permissionCardState() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("swarm-card-\(UUID())")
        let status = root.appendingPathComponent("status.json")
        let permission = root.appendingPathComponent("permission", isDirectory: true)
        try FileManager.default.createDirectory(at: permission, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let firstToken = UUID().uuidString
        let firstData = permissionEnvelope(token: firstToken, command: "printf first")
        let firstMarker = permission.appendingPathComponent(firstToken + ".pending")
        try Data().write(to: firstMarker)
        let first = try #require(InteractivePermissionCard(data: firstData, statusURL: status))
        #expect(first.ask.toolName == "Bash")
        #expect(first.ask.subject == "printf first")

        #expect(try first.answer(.deny))
        #expect(InteractivePermissionCard(data: firstData, statusURL: status) == nil)
        let firstAnswer = permission.appendingPathComponent(firstToken + ".answer")
        #expect(try Data(contentsOf: firstAnswer) == InteractivePermissionAnswer.deny.data)

        try FileManager.default.removeItem(at: firstAnswer)
        let newer = Data(#"{"hook_event_name":"PreToolUse","tool_name":"Bash"}"#.utf8)
        #expect(InteractivePermissionCard(data: newer, statusURL: status) == nil)
        try FileManager.default.removeItem(at: firstMarker)
        #expect(InteractivePermissionCard(data: firstData, statusURL: status) == nil)
        #expect(try !first.answer(.allow))
        #expect(!FileManager.default.fileExists(atPath: firstAnswer.path))

        let secondToken = UUID().uuidString
        let secondData = permissionEnvelope(token: secondToken, command: "printf second")
        try Data().write(to: permission.appendingPathComponent(secondToken + ".pending"))
        #expect(InteractivePermissionCard(data: secondData, statusURL: status)?.id == secondToken)
    }

    @Test("Permission hook publishes the request, waits for an answer, and prints it")
    func permissionHook() async throws {
        let command = AgentKind.interactivePermissionHookCommand
        #expect(command.contains("uuidgen"))
        #expect(command.contains(".pending"))
        #expect(command.contains(".answer"))
        #expect(command.contains("-lt 120"))
        #expect(command.contains("cat \"$swarm_answer\""))
        #expect(command.hasSuffix("exit 0"))

        let root = FileManager.default.temporaryDirectory.appendingPathComponent("swarm-permission-hook-\(UUID())")
        let status = root.appendingPathComponent("status.json")
        defer { try? FileManager.default.removeItem(at: root) }
        let payload = #"{"hook_event_name":"PermissionRequest","session_id":"native","tool_name":"Bash","tool_input":{"command":"printf ok"}}"#
        async let running = Shell.run(
            "/bin/sh", ["-c", command], env: ["SWARM_UI_CLI_STATUS_FILE": status.path], stdin: payload
        )
        var card: InteractivePermissionCard?
        for _ in 0..<100 where card == nil {
            if let data = try? Data(contentsOf: status) {
                card = InteractivePermissionCard(data: data, statusURL: status)
            }
            if card == nil { try await Task.sleep(for: .milliseconds(20)) }
        }
        let pending = try #require(card)
        #expect(AgentKind.interactiveHookState(data: try Data(contentsOf: status)) == .waiting)
        #expect(try pending.answer(.allow))
        let result = try await running
        #expect(result.ok)
        #expect(result.stdout == String(decoding: try InteractivePermissionAnswer.allow.data, as: UTF8.self))
        #expect(!pending.isPending)
    }

    private func interactiveAsk(
        token: String, toolName: String, input: JSONValue
    ) throws -> PermissionAsk? {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("swarm-card-\(token)")
        defer { try? FileManager.default.removeItem(at: root) }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let data = try encoder.encode(JSONValue.object([
            "token": .string(token),
            "payload": .object([
                "hook_event_name": .string("PermissionRequest"),
                "tool_name": .string(toolName),
                "tool_input": input,
            ]),
        ]))
        let status = root.appendingPathComponent("status.json")
        let permission = root.appendingPathComponent("permission", isDirectory: true)
        try FileManager.default.createDirectory(at: permission, withIntermediateDirectories: true)
        try Data().write(to: permission.appendingPathComponent(token + ".pending"))
        return InteractivePermissionCard(data: data, statusURL: status)?.ask
    }

    private func permissionEnvelope(token: String, command: String) -> Data {
        Data(#"{"token":"\#(token)","payload":{"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"\#(command)"}}}"#.utf8)
    }

    @Test("Generated resume commands remain offerable despite hook options", arguments: [AgentKind.claudeCode, .codex])
    func offerResume(agent: AgentKind) throws {
        let command = try #require(agent.interactiveCommand(
            directory: "/tmp/worktree", prompt: "", sessionID: .new(),
            model: "", effort: "", resuming: "native-session"
        ))
        #expect(TerminalCommandMemory.offerable(command, maximumLength: 262_144) == command)
    }

    @Test("Copied session IDs cannot share hook files between app builds")
    func isolatedStatusFiles() {
        let id = SessionID("copied-session")
        let production = AgentKind.interactiveStatusURL(
            sessionID: id, namespace: "io.github.priyanshuupadhyay.swarm"
        )
        let development = AgentKind.interactiveStatusURL(
            sessionID: id, namespace: "io.github.priyanshuupadhyay.swarm.dev"
        )
        #expect(production != development)
    }
    @Test("Private launch files keep long prompts out of terminal input", arguments: [AgentKind.claudeCode, .codex])
    func launchFile(agent: AgentKind) async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let fake = root.appendingPathComponent(agent.executableName)
        try Data("#!/bin/sh\nfor argument do printf '%s\n' \"$argument\"; done\n".utf8).write(to: fake)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fake.path)
        let id = SessionID.new()
        let prompt = String(repeating: "A long task. ", count: 1000) + "\n'quoted' $(touch unwanted)"
        let command = try #require(try agent.prepareInteractiveCommand(
            directory: root.path, prompt: prompt, sessionID: id, model: "", effort: "", base: root
        ))
        #expect(command.utf8.count < 1_000)
        #expect(!command.contains("A long task"))
        let folder = try AgentKind.interactiveLaunchDirectory(sessionID: id, base: root)
        let file = folder.appendingPathComponent("start.sh")
        let attributes = try FileManager.default.attributesOfItem(atPath: file.path)
        #expect(attributes[.posixPermissions] as? Int == 0o600)
        let result = try await Shell.run("/bin/sh", ["-c", command], env: ["PATH": root.path + ":/usr/bin:/bin"])
        #expect(result.ok)
        #expect(result.stdout.hasSuffix("--\n" + prompt + "\n"))
        #expect(!FileManager.default.fileExists(atPath: root.appendingPathComponent("unwanted").path))
        let resume = try #require(try agent.prepareInteractiveCommand(
            directory: root.path, prompt: "", sessionID: id, model: "", effort: "",
            resuming: "native-id", base: root
        ))
        #expect(resume != command)
        let resumed = try await Shell.run("/bin/sh", ["-c", resume], env: ["PATH": root.path + ":/usr/bin:/bin"])
        #expect(resumed.ok)
        #expect(resumed.stdout.contains("native-id"))
        #expect(!resumed.stdout.contains("A long task"))
        try AgentKind.removeInteractiveLaunch(sessionID: id, base: root)
        #expect(!FileManager.default.fileExists(atPath: folder.path))
    }

    @Test("Interactive launch restores terminal colour capabilities")
    func terminalColours() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let fake = root.appendingPathComponent("claude")
        try Data(#"""
        #!/bin/sh
        printf '%s|%s|%s' "${NO_COLOR-unset}" "$TERM" "$COLORTERM"
        """#.utf8).write(to: fake)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fake.path)
        let command = try #require(AgentKind.claudeCode.interactiveCommand(
            directory: root.path, prompt: "", sessionID: .new(), model: "", effort: ""
        ))
        let result = try await Shell.run("/bin/sh", ["-c", command], env: [
            "PATH": root.path + ":/usr/bin:/bin", "NO_COLOR": "1", "TERM": "dumb", "COLORTERM": ""
        ])
        #expect(result.ok)
        #expect(result.stdout == "unset|xterm-256color|truecolor")
    }

}
