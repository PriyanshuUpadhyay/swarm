import Foundation
import Synchronization
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Session detail")
struct SwarmSessionDetailTests {
    @Test("Child headers use the model before the provider and copy a safe attach command")
    func childHeaderValues() {
        var child = SwarmAgent(id: .init("reviewer"), role: "review", pane: "pane", alive: true,
                               provider: "codex", state: "done")
        child.model = "gpt-6"
        #expect(SwarmAgentCell(agent: child).model == "gpt-6")
        #expect(SwarmAgentCell(agent: child).id.rawValue == "reviewer")
        #expect(child.id.attachCommand == "swarm attach reviewer")
        child.model = nil
        #expect(SwarmAgentCell(agent: child).model == "codex")
        child.provider = nil
        #expect(SwarmAgentCell(agent: child).model == "unknown")
        #expect(SwarmAgentID("reviewer's $(touch file)").attachCommand
            == "swarm attach 'reviewer'\"'\"'s $(touch file)'")
    }

    @Test("Closing a working or waiting child asks first, but an idle or ended child does not")
    func childCloseConfirmation() {
        for state in ["working", "waiting", "done", "failed"] {
            var child = SwarmAgent(id: .init("reviewer"), role: "review", pane: "pane", alive: true,
                                   state: state)
            #expect(SwarmAgentCell(agent: child).requiresCloseConfirmation
                == ["working", "waiting"].contains(state))
            child.alive = false
            #expect(!SwarmAgentCell(agent: child).requiresCloseConfirmation)
            child.alive = true
            child.pane = nil
            #expect(!SwarmAgentCell(agent: child).requiresCloseConfirmation)
        }
    }

    @Test("The grid keeps finished agents and sorts by creation time")
    func agentCells() {
        var value = session(adapter: "tmux-solo")
        value.chairID = .init("other-chair")
        let agents = [
            SwarmAgent(id: .init("dead"), role: "code", pane: "%1", alive: false, createdAt: 1),
            SwarmAgent(id: .init("later"), role: "code", pane: "%2", alive: true, createdAt: 3),
            SwarmAgent(id: .init("orchestrator"), role: "chair", pane: "%3", alive: true),
            SwarmAgent(id: .init("early"), role: "code", pane: "%4", alive: true, createdAt: 2),
            SwarmAgent(id: .init("other-chair"), role: "chair", pane: "%5", alive: true),
            SwarmAgent(id: .init("no-pane"), role: "code", pane: nil, alive: nil, createdAt: 4),
        ]
        let cells = SwarmPanePolicy.cells(session: value, agents: agents)
        #expect(cells.map(\.id.rawValue) == ["dead", "early", "later", "no-pane"])
        #expect(SwarmPanePolicy.cells(session: value, agents: agents, dismissed: ["dead", "no-pane"])
            .map(\.id.rawValue) == ["early", "later"])
        #expect(SwarmPanePolicy.liveCells(session: value, agents: agents).map(\.id.rawValue) == ["early", "later"])
    }

    @Test("A live child reusing a dismissed id returns after a model switch")
    func reusedDismissedID() {
        let value = session(adapter: "herdr")
        let live = SwarmAgent(id: .init("child"), role: "code", pane: "new-pane", alive: true)
        let ended = SwarmAgent(id: live.id, role: "code", pane: "old-pane", alive: false)
        #expect(SwarmPanePolicy.cells(session: value, agents: [ended], dismissed: ["child"]).isEmpty)
        #expect(SwarmPanePolicy.cells(session: value, agents: [live], dismissed: ["child"]).map(\.id) == [live.id])
    }

    @Test("The live child view excludes dead agents and agents without panes")
    func liveChildAgents() {
        let value = session(adapter: "tmux-solo")
        let ended = [SwarmAgent(id: .init("coder"), role: "code", pane: "%1", alive: false)]
        let live = [SwarmAgent(id: .init("coder"), role: "code", pane: "%1", alive: true)]
        let closed = [SwarmAgent(id: .init("coder"), role: "code", pane: nil, alive: true)]
        #expect(!SwarmPanePolicy.hasLiveChildAgents(session: value, agents: ended))
        #expect(!SwarmPanePolicy.hasLiveChildAgents(session: value, agents: closed))
        #expect(SwarmPanePolicy.hasLiveChildAgents(session: value, agents: live))
        let unknown = [SwarmAgent(id: .init("coder"), role: "code", pane: "%1", alive: nil)]
        #expect(SwarmPanePolicy.liveCells(session: value, agents: unknown).isEmpty)
        #expect(SwarmPanePolicy.cells(session: value, agents: unknown).count == 1)
    }

    @Test("Dismiss one or clear finished keeps live children, chairs, and other chats unchanged")
    func dismissFinishedChildren() {
        let value = session(adapter: "herdr")
        let chat = SwarmProjectSession(sessions: [value], title: "Work")
        var finished = SwarmAgent(id: .init("finished"), role: "code", pane: "old-pane", alive: false)
        finished.log = "/tmp/finished.jsonl"
        let closed = SwarmAgent(id: .init("closed"), role: "review", pane: nil, alive: true)
        let live = SwarmAgent(id: .init("live"), role: "code", pane: "live-pane", alive: true, state: "done")
        let chair = SwarmAgent(id: SwarmPanePolicy.chair, role: "chair", pane: nil, alive: false)
        let agents = [finished, closed, live, chair]
        let key = ChatTitle.key(chat)
        var navigation = WorkspaceNavigation()
        navigation.dismissedChildren = ["other-chat": ["other-child"]]
        navigation.dismissFinishedChildren([live.id.rawValue, chair.id.rawValue, "missing"], in: chat, agents: agents)
        #expect(navigation.dismissedChildren[key] == nil)
        navigation.dismissFinishedChildren([finished.id.rawValue], in: chat, agents: agents)
        #expect(navigation.dismissedChildren[key] == ["finished"])
        let kept = SwarmPanePolicy.cells(session: value, agents: agents,
                                         dismissed: navigation.dismissedChildren[key] ?? [])
        #expect(kept.map(\.id.rawValue) == ["closed", "live"])
        #expect(SwarmPanePolicy.cells(session: value, agents: agents).first { $0.id == finished.id }?.agent.log
            == finished.log)
        navigation.dismissFinishedChildren(agents.map(\.id.rawValue), in: chat, agents: agents)
        navigation.dismissFinishedChildren(agents.map(\.id.rawValue), in: chat, agents: agents)
        #expect(navigation.dismissedChildren[key] == ["finished", "closed"])
        #expect(navigation.dismissedChildren["other-chat"] == ["other-child"])
        #expect(SwarmPanePolicy.cells(session: value, agents: agents,
                                      dismissed: navigation.dismissedChildren[key] ?? []).map(\.id) == [live.id])
    }

    @Test("Agent creation time decodes from the CLI")
    func agentCreationTime() throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let agent = try decoder.decode(SwarmAgent.self, from: Data(
            #"{"id":"coder","role":"code","pane":"%2","alive":true,"created_at":42}"#.utf8
        ))
        #expect(agent.createdAt == 42)
    }

    @Test("A missing chair log stays in retry state")
    func missingLog() async {
        var value = session(adapter: "herdr")
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in true }) == .waiting)
        value.chairLog = "/tmp/not-written.jsonl"
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in false }) == .waiting)
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in true })
            == .ready(log: URL(fileURLWithPath: value.chairLog!), format: "claude"))
        let reader = SwarmChairTranscript()
        #expect(await reader.poll(session: value) == .waiting)
        value.chairProvider = nil
        #expect(await reader.poll(session: value, chairProvider: "agy")
            == .waiting)
        #expect(await reader.poll(session: value)
            == .notice("No transcript reader for this provider yet"))
    }

    @Test("A known AGY log uses the existing translator and retains failed tool output")
    func agyLog() async throws {
        let binary = try #require(TranscriptToolProcess.bundled)
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("agy-ui-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try Data("""
            {"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-09-26T00:00:00Z","tool_calls":[{"name":"run_command","args":{"CommandLine":"swift test"}}]}
            {"step_index":2,"source":"MODEL","type":"RUN_COMMAND","status":"ERROR","created_at":"2026-09-26T00:00:01Z","content":"One check failed."}
            """.appending("\n").utf8).write(to: log)
        var value = session(adapter: "herdr")
        value.chairProvider = "agy"
        value.chairLog = log.path
        #expect(ChairTranscriptSource.resolve(session: value, logExists: { _ in true })
            == .ready(log: log, format: "agy"))
        let reader = SwarmChairTranscript(binary: binary)
        guard case .rows(let rows, let raw) = await reader.poll(session: value) else {
            Issue.record("AGY log did not reach the UI reader")
            return
        }
        #expect(rows.count == 1)
        #expect(rows.first?.tool?.command == "swift test")
        #expect(rows.first?.tool?.state == .failed)
        #expect(rows.first?.tool?.output == "One check failed.")
        #expect(raw.count == 2)
    }

    @Test("A missing log explains whether the chat ended")
    func missingLogMessage() {
        #expect(ChairTranscriptSnapshot.waitingMessage(isRunning: true)
            == "The chair has not written its log yet")
        #expect(ChairTranscriptSnapshot.waitingMessage(isRunning: false)
            == "This chat ended before its log was found")
    }

    @Test("The real transcript tool makes rows from a chair log")
    func fixtureLog() async throws {
        let binary = try #require(TranscriptToolProcess.bundled, "tool not built")
        let log = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-chair-fixture-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        var value = session(adapter: "herdr")
        value.chairProvider = "codex"
        value.chairLog = log.path
        let reader = SwarmChairTranscript(binary: binary)
        #expect(await reader.poll(session: value) == .waiting)
        try Data("""
            {"type":"session_meta","payload":{"id":"session-1","cwd":"/work"}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"List files"}]}}
            """.appending("\n").utf8).write(to: log)
        let snapshot = await reader.poll(session: value)
        guard case .rows(let rows, _) = snapshot else {
            Issue.record("The tool did not return rows")
            return
        }
        #expect(rows.contains { $0.kind == .user && $0.text.contains("List files") })
        try FileManager.default.removeItem(at: log)
        guard case .rows(let retained, _) = await reader.poll(session: value) else {
            Issue.record("A missing log cleared the loaded transcript")
            return
        }
        #expect(retained == rows)
    }

    @Test("The chair reports the actual Claude model through the transcript tool")
    func actualModel() async throws {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("model-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try Data(#"{"type":"assistant","uuid":"a","message":{"model":"claude-opus-4-6","content":[{"type":"text","text":"Ready"}]}}"#.appending("\n").utf8).write(to: log)
        var value = session(adapter: "tmux-solo")
        value.chairLog = log.path
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))
        _ = await reader.poll(session: value)
        #expect(await reader.currentModel == "claude-opus-4-6")
    }

    @Test("A transcript starts after its reader becomes available")
    func readerRetry() async throws {
        let realBinary = try #require(TranscriptToolProcess.bundled)
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-reader-retry-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let binary = directory.appendingPathComponent("transcript")
        let log = directory.appendingPathComponent("rollout.jsonl")
        try Data("""
            {"type":"session_meta","payload":{"id":"session-1","cwd":"/work"}}
            {"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"List files"}]}}
            """.appending("\n").utf8).write(to: log)
        var value = session(adapter: "herdr")
        value.chairProvider = "codex"
        value.chairLog = log.path
        let reader = SwarmChairTranscript(binary: binary)

        guard case .unavailable = await reader.poll(session: value) else {
            Issue.record("The missing reader did not report a failure")
            return
        }
        try FileManager.default.copyItem(at: realBinary, to: binary)
        guard case .rows(let rows, _) = await reader.poll(session: value) else {
            Issue.record("The reader did not retry")
            return
        }
        #expect(rows.contains { $0.kind == .user && $0.text.contains("List files") })
    }

    @Test("An answer names the agent, the question it saw, and the choice, in the session")
    func answerArguments() async throws {
        let calls = CloseCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await calls.reply(arguments: arguments, environment: environment)
        }
        let value = session(adapter: "tmux-solo")
        let prompt = SwarmPrompt(id: "015475c015dc4eae", question: "Do you want to proceed?", choices: ["Yes", "No"])
        try await bus.answer(prompt, choice: 1, to: .init("seat"), in: value)
        #expect(await calls.arguments == [["answer", "seat", "015475c015dc4eae", "1"]])
        #expect(await calls.adapters == ["tmux-solo"])
    }

    @Test("A key press names the agent and the key, in the session")
    func pressKeyArguments() async throws {
        let calls = CloseCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await calls.reply(arguments: arguments, environment: environment)
        }
        try await bus.pressKey("Up", agent: .init("seat"), session: session(adapter: "tmux-solo"))
        #expect(await calls.arguments == [["key", "seat", "Up"]])
        #expect(await calls.adapters == ["tmux-solo"])
    }

    @Test("The hooks plan decodes the CLI's answer, and setup sends the plan's digest")
    func hooksStatusPlanAndSetup() async throws {
        let calls = CloseCalls()
        let plan = #"""
            {"digest":"d1","files":[{"path":"/h/.codex/config.toml","diff":"--- /h/.codex/config.toml\n+++ /h/.codex/config.toml\n@@ -1,1 +1,3 @@\n model = 1\n+[a]\n+b = 2\n"}],
             "conflicts":[{"file":"/h/.gemini/config/hooks.json","entry":"group \"swarm\"","found":"{}","wanted":"{\"Stop\":[]}","fix":"rename or delete it"}]}
            """#
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            _ = await calls.reply(arguments: arguments, environment: environment)
            return ShellResult(status: 0, stdout: arguments.contains("--plan") ? plan : "", stderr: "")
        }
        let decoded = try await bus.hooksPlan()
        #expect(decoded.digest == "d1")
        #expect(decoded.files.map { [$0.added, $0.removed] } == [[2, 0]])
        #expect(decoded.files.first?.patch.hasPrefix(
            #"diff --git "a/h/.codex/config.toml" "b/h/.codex/config.toml"\#n--- /h/.codex/config.toml\#n"#
        ) == true)
        #expect(decoded.conflicts.first?.entry == #"group "swarm""#)
        #expect(!decoded.isSetUp && !decoded.canApply)
        #expect(SwarmHooksPlan(digest: "d", files: decoded.files, conflicts: []).canApply)
        #expect(SwarmHooksPlan(digest: "d", files: [], conflicts: []).isSetUp)
        #expect(decoded.summary == "1 file to change, 1 conflict.")
        #expect(SwarmHooksPlan(digest: "d", files: decoded.files + decoded.files, conflicts: []).summary
            == "2 files to change, 0 conflicts.")
        #expect(SwarmHooksPlan(digest: "d", files: [], conflicts: []).summary == "Swarm's hooks are already set up.")
        let plusLine = SwarmHooksPlan.File(path: "/f", diff: "--- /f\n+++ /f\n@@ -0,0 +1 @@\n+++ x\n")
        #expect(plusLine.added == 1 && plusLine.removed == 0)

        try await bus.setUpHooks(digest: decoded.digest)
        #expect(await calls.arguments == [
            ["hooks", "setup", "--plan", "--json"],
            ["hooks", "setup", "--digest", "d1"],
        ])
    }

    @Test("Setup status and plan come from swarm setup, and setup sends the plan's digest (ADR 0043)")
    func setupStatusPlanAndApply() async throws {
        let calls = CloseCalls()
        let plan = #"""
            {"digest":"d2","consent":"ask","files":[
              {"group":"trust","path":"/h/.swarm/consent.json","diff":"--- /h/.swarm/consent.json\n+++ /h/.swarm/consent.json\n@@ -0,0 +1,3 @@\n+{\n+  \"trust\": \"standing\"\n+}\n"},
              {"path":"/h/old.json","diff":""}],
             "conflicts":[],"skipped":[]}
            """#
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            _ = await calls.reply(arguments: arguments, environment: environment)
            let stdout = arguments.contains("--plan")
                ? plan : #"{"hooks":true,"guard":false,"trust":false,"herdr":true}"#
            return ShellResult(status: 0, stdout: stdout, stderr: "")
        }
        let status = try await bus.setupStatus()
        #expect(status == SwarmSetupStatus(hooks: true, trust: false, herdr: true))
        // A Mac that updates has no consent yet, so the sheet opens once even after a hooks
        // "Not now" (Q3).
        #expect(status.needsSheet(hooksDeclined: true, trustDeclined: false))
        #expect(!SwarmSetupStatus(hooks: false, trust: true, herdr: true).needsSheet(hooksDeclined: true, trustDeclined: false))
        #expect(SwarmSetupStatus(hooks: false, trust: true, herdr: true).needsSheet(hooksDeclined: false, trustDeclined: false))
        // A group the owner left unchecked when they applied the rest is not asked again.
        #expect(!status.needsSheet(hooksDeclined: false, trustDeclined: true))

        let decoded = try await bus.setupPlan()
        #expect(decoded.files.map(\.group) == ["trust", nil])
        #expect(decoded.files.first?.added == 3)
        #expect(decoded.groupIDs == ["trust"])
        // Hooks and trust can both change one file; each row keeps its own id (AP-6).
        let hooksRow = SwarmHooksPlan.File(path: "/h/.codex/config.toml", diff: "", group: "hooks")
        let trustRow = SwarmHooksPlan.File(path: "/h/.codex/config.toml", diff: "", group: "trust")
        #expect(hooksRow.id != trustRow.id)
        try await bus.setUp(digest: decoded.digest)

        // The first plan, with no --consent, sets the owner's recorded answer, so the radio
        // starts there; from then on the radio's answer is sent each time.
        #expect(decoded.consent == "ask")
        var first = SwarmSetupChoice()
        first.take(decoded)
        #expect(first.standing == false)
        #expect(first.groups == ["trust"])
        #expect(first.arguments == ["--consent", "ask"])
        first.standing = true
        #expect(first.arguments == ["--consent", "standing"])

        // Each checkbox the owner clears leaves its group out, and the radio sets the consent,
        // so the plan and its digest cover what the sheet shows (02-design screen 1).
        var choice = SwarmSetupChoice(groups: ["hooks", "trust", "herdr"])
        #expect(choice.arguments.isEmpty)
        choice.unchecked = ["trust"]
        choice.standing = false
        #expect(choice.checked == ["hooks", "herdr"])
        _ = try await bus.setupPlan(choice)
        try await bus.setUp(digest: "d3", choice: choice)
        #expect(await calls.arguments == [
            ["setup", "status", "--json"],
            ["setup", "--plan", "--json"],
            ["setup", "--digest", "d2"],
            ["setup", "--plan", "--json", "--only", "hooks,herdr", "--consent", "ask"],
            ["setup", "--digest", "d3", "--only", "hooks,herdr", "--consent", "ask"],
        ])
    }

    @Test("Not now with a group unchecked declines only that group, so the rest are asked again (02-design screen 1)")
    func notNowDeclinesOnlyTheClearedGroups() {
        var choice = SwarmSetupChoice(groups: ["hooks", "trust"])
        // Every box checked: Not now declines the whole sheet.
        #expect(choice.notNowDeclines == nil)
        choice.unchecked = ["trust"]
        #expect(choice.notNowDeclines == ["trust"])
        // Hooks stay undeclined, so the next start asks for them again.
        let pending = SwarmSetupStatus(hooks: false, trust: false, herdr: true)
        #expect(pending.needsSheet(hooksDeclined: false, trustDeclined: true))
    }

    @Test("A plan with only skipped items names each with its reason, not 'already set up' (02-design)")
    func skippedItemsShowTheirReasons() throws {
        let json = #"""
            {"digest":"d4","consent":"standing","files":[],"conflicts":[],
             "skipped":[{"group":"trust","reason":"/tmp is too broad to trust"}]}
            """#
        let plan = try JSONDecoder().decode(SwarmHooksPlan.self, from: Data(json.utf8))
        #expect(plan.skipped == [.init(group: "trust", reason: "/tmp is too broad to trust")])
        #expect(plan.isSetUp)
        #expect(plan.skippedLines == ["Folder trust: /tmp is too broad to trust"])
        #expect(plan.unchangedText("Swarm is already set up.")
            == "No file changes. Swarm left these as they are:\nFolder trust: /tmp is too broad to trust")
        // A `hooks setup` plan or an undo has no skipped list.
        let none = SwarmHooksPlan(digest: "d", files: [], conflicts: [])
        #expect(none.skippedLines.isEmpty)
        #expect(none.unchangedText("Swarm is already set up.") == "Swarm is already set up. No file changes.")
    }

    @Test("A launch reports each trust write it made, so the app shows it (owner answer I1)")
    func launchReportsTrustWrites() async throws {
        // A diff line keeps its mark, so its context line ` model = …` is not the model (L-12).
        let stderr = """
            --- /h/.codex/config.toml
            +++ /h/.codex/config.toml
            @@ -1 +1,3 @@
             model = "gpt-5.5"
             trusted x y
            +[projects."/r one"]
            swarm: trusted /r one for claude in /h/.claude.json
            trusted claude /r one
            trusted codex /r one
            model opus
            """
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, _, _, _, _, _ in ShellResult(status: 0, stdout: "w1:p1\n", stderr: stderr)
        }
        let launch = try await bus.launch(
            .init("orchestrator"), role: "chair", provider: nil, model: nil, account: nil,
            in: SwarmSessionID("s"), directory: "/r one"
        )
        #expect(launch.model == "opus")
        #expect(launch.trustWrites == [
            SwarmTrustWrite(provider: "claude", directory: "/r one"),
            SwarmTrustWrite(provider: "codex", directory: "/r one"),
        ])
        // Standing consent prints the same line as a chair's folder pick, so the notice names
        // no reason it cannot know.
        #expect(launch.trustWrites.first?.notice
            == "Swarm marked /r one as trusted for Claude. Undo it in Swarm › Managed Changes.")
        #expect(SwarmTrustWrite(provider: "agy", directory: "/r").notice.contains("for AGY."))
        // VoiceOver hears every write in one announcement (UA-11).
        #expect(launch.trustAnnouncement
            == "Swarm marked /r one as trusted for Claude. Swarm marked /r one as trusted for Codex. Undo it in Swarm › Managed Changes.")
        #expect(SwarmLaunch(pane: "p", account: nil).trustAnnouncement == nil)
    }

    @Test("A child's chat reads the log its hooks reported and waits before one exists")
    func childTranscript() async throws {
        let transcript = SwarmChairTranscript()
        #expect(await transcript.poll(childLog: nil, provider: "claude") == .waiting)
        #expect(await transcript.poll(childLog: "/missing/child.jsonl", provider: "codex") == .waiting)
        #expect(await transcript.poll(childLog: "/missing/child.jsonl", provider: "gemini")
            == .notice("No transcript reader for this provider yet"))
    }

    @Test("A finished child cell keeps its final answer readable from its log")
    func finishedChildTranscript() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let log = try chat([
            Self.USER_ASKS,
            #"{"type":"assistant","uuid":"reply","message":{"role":"assistant","content":[{"type":"text","text":"The fix is complete."}]}}"#,
        ], in: directory, named: "finished-child.jsonl")
        var finished = SwarmAgent(id: .init("finished"), role: "code", pane: nil, alive: false, provider: "claude")
        finished.log = log.path
        let cell = try #require(SwarmPanePolicy.cells(session: session(adapter: "herdr"), agents: [finished]).first)
        #expect(cell.agent.status == .ended)
        let transcript = SwarmChairTranscript()
        guard case .rows(let rows, _) = await transcript.poll(childLog: cell.agent.log, provider: cell.agent.provider) else {
            Issue.record("No transcript for the finished child")
            return
        }
        #expect(rows.contains { $0.kind == .assistant && $0.text == "The fix is complete." })
    }

    @Test("Close uses the session adapter and closes live children before the chair")
    func closeOrderAndAdapter() async throws {
        let calls = CloseCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/tmp", resolveExecutable: { $0 }) {
            _, arguments, _, environment, _, _ in
            await calls.reply(arguments: arguments, environment: environment)
        }
        let value = session(adapter: "herdr")

        try await SwarmSessionCloser.end(session: SwarmProjectSession(sessions: [value], title: "Chat"), bus: bus)

        #expect(await calls.arguments == [
            ["agents", "--json", "--all"],
            ["agents", "--json"],
            ["close", "child-b"],
            ["close", "child-a"],
            ["close", "orchestrator"],
        ])
        #expect(await calls.adapters == ["tmux-solo", "herdr", "herdr", "herdr", "herdr"])
    }

    private func chat(_ lines: [String], in directory: URL, named name: String) throws -> URL {
        let log = directory.appendingPathComponent(name)
        try Data((lines.joined(separator: "\n") + "\n").utf8).write(to: log)
        return log
    }

    private static let USER_ASKS = #"{"type":"user","uuid":"u1","message":{"role":"user","content":"Fix the bug"}}"#
    private static let CLEAR_LOG = [
        #"{"type":"attachment","uuid":"h1","attachment":{"type":"hook_success","hookName":"SessionStart:clear","hookEvent":"SessionStart","toolUseID":"t1","exitCode":0}}"#,
        #"{"type":"user","uuid":"m1","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>Caveat: local command output follows.</local-command-caveat>"}}"#,
        #"{"type":"user","uuid":"c1","message":{"role":"user","content":"<command-name>/clear</command-name>\n<command-message>clear</command-message>"}}"#,
        #"{"type":"user","uuid":"o1","message":{"role":"user","content":"<local-command-stdout></local-command-stdout>"}}"#,
        #"{"type":"user","uuid":"u2","message":{"role":"user","content":"Start fresh"}}"#,
    ]

    @Test("A /clear log keeps the earlier rows behind one divider and drops the /clear command, caveat, and empty output rows")
    func clearKeepsEarlierRows() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-clear-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))

        value.chairLog = try chat([Self.USER_ASKS], in: directory, named: "first.jsonl").path
        _ = await reader.poll(session: value)
        value.chairLog = try chat(Self.CLEAR_LOG, in: directory, named: "second.jsonl").path
        guard case .rows(let rows, _) = await reader.poll(session: value) else {
            Issue.record("No rows after the clear")
            return
        }
        let visible = rows.filter { !$0.isHiddenByDefault }
        #expect(visible.map(\.kind) == [.user, .divider, .user])
        try #require(visible.count == 3)
        #expect(visible.map(\.text).first == "Fix the bug")
        #expect(visible[1].text.hasPrefix("Context cleared · "))
        #expect(visible[1].eventID == "clear-second.jsonl")
        #expect(visible[2].text == "Start fresh")
        // The new log numbers raw entries from 0 again, so a frozen row has no sources to show.
        #expect(visible[0].sourceIDs.isEmpty)
        #expect(visible[2].sourceIDs == ["raw-4"])
        #expect(!rows.contains { $0.text.contains("<command-name>/clear") })
        #expect(!rows.contains { $0.text.contains("<local-command-") })
    }

    @Test("A /clear log keeps the model of the log before it until a reply names one")
    func clearKeepsModel() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-clear-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))
        let reply = #"{"type":"assistant","uuid":"a1","message":{"model":"claude-opus-4-6","content":[{"type":"text","text":"Fixed"}]}}"#

        value.chairLog = try chat([Self.USER_ASKS, reply], in: directory, named: "first.jsonl").path
        _ = await reader.poll(session: value)
        value.chairLog = try chat(Self.CLEAR_LOG, in: directory, named: "second.jsonl").path
        _ = await reader.poll(session: value)
        #expect(await reader.currentModel == "claude-opus-4-6")

        value.chairLog = try chat([Self.USER_ASKS], in: directory, named: "other.jsonl").path
        _ = await reader.poll(session: value)
        #expect(await reader.currentModel == nil)
    }

    @Test("A clear log read while it holds only bookkeeping lines keeps the old rows and still gets its divider")
    func clearReadBeforeItsRecords() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-clear-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))

        value.chairLog = try chat([Self.USER_ASKS], in: directory, named: "first.jsonl").path
        _ = await reader.poll(session: value)
        let cleared = try chat([
            #"{"type":"mode","mode":"normal","sessionId":"s2"}"#,
            #"{"type":"file-history-snapshot","messageId":"m1","snapshot":{"messageId":"m1","trackedFileBackups":{},"timestamp":"t1"},"isSnapshotUpdate":false}"#,
        ], in: directory, named: "second.jsonl")
        value.chairLog = cleared.path
        guard case .rows(let early, _) = await reader.poll(session: value) else {
            Issue.record("No rows while the new log holds only bookkeeping lines")
            return
        }
        #expect(early.map(\.text) == ["Fix the bug"])

        let handle = try FileHandle(forWritingTo: cleared)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data((Self.CLEAR_LOG.joined(separator: "\n") + "\n").utf8))
        try handle.close()
        var rows: [TranscriptRow] = []
        for _ in 0..<40 {
            if case .rows(let latest, _) = await reader.poll(session: value) { rows = latest }
            if rows.contains(where: { $0.text == "Start fresh" }) { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        let visible = rows.filter { !$0.isHiddenByDefault }
        #expect(visible.map(\.kind) == [.user, .divider, .user])
        #expect(visible.map(\.text).first == "Fix the bug")
    }

    @Test("Two clears in a row give two dividers")
    func twoClears() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-clear-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))

        value.chairLog = try chat([Self.USER_ASKS], in: directory, named: "first.jsonl").path
        _ = await reader.poll(session: value)
        value.chairLog = try chat(Self.CLEAR_LOG, in: directory, named: "second.jsonl").path
        _ = await reader.poll(session: value)
        value.chairLog = try chat(Self.CLEAR_LOG, in: directory, named: "third.jsonl").path
        guard case .rows(let rows, _) = await reader.poll(session: value) else {
            Issue.record("No rows after the second clear")
            return
        }
        #expect(rows.filter { $0.kind == .divider }.map(\.eventID)
            == ["clear-second.jsonl", "clear-third.jsonl"])
        #expect(rows.filter { $0.kind == .user }.map(\.text) == ["Fix the bug", "Start fresh", "Start fresh"])
    }

    @Test("A new log that is not a clear, such as a model switch, replaces the rows with no divider")
    func switchGivesNoDivider() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-clear-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))

        value.chairLog = try chat([Self.USER_ASKS], in: directory, named: "first.jsonl").path
        _ = await reader.poll(session: value)
        value.chairLog = try chat(
            [#"{"type":"user","uuid":"u3","message":{"role":"user","content":"Resumed"}}"#],
            in: directory, named: "second.jsonl"
        ).path
        guard case .rows(let rows, _) = await reader.poll(session: value) else {
            Issue.record("No rows after the switch")
            return
        }
        #expect(rows.map(\.kind) == [.user])
        #expect(rows.first?.text == "Resumed")
    }

    @Test("The chair's queued messages come from its log, and a clear's new log starts empty")
    func queuedMessagesFollowLog() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-queue-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))

        value.chairLog = try chat([
            Self.USER_ASKS,
            #"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"keep the old name"}"#,
        ], in: directory, named: "first.jsonl").path
        _ = await reader.poll(session: value)
        #expect(await reader.queuedMessages == ["keep the old name"])
        value.chairLog = try chat(Self.CLEAR_LOG, in: directory, named: "second.jsonl").path
        _ = await reader.poll(session: value)
        #expect(await reader.queuedMessages.isEmpty)
    }

    @Test("Pull-back presses Up, reads the popAll records the log gets, and clears the CLI box")
    func pullBackReadsLog() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-pull-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))
        let enqueue = #"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"keep the old name"}"#
        let log = try chat([Self.USER_ASKS, enqueue], in: directory, named: "chat.jsonl")
        value.chairLog = log.path
        _ = await reader.poll(session: value)
        let keys = Mutex<[String]>([])

        let pulled = try await reader.pullBack { key in
            keys.withLock { $0.append(key) }
            guard key == "Up" else { return }
            let handle = try FileHandle(forWritingTo: log)
            try handle.seekToEnd()
            try handle.write(contentsOf: Data((enqueue.replacingOccurrences(of: "enqueue", with: "popAll") + "\n").utf8))
            try handle.close()
        }

        #expect(pulled == "keep the old name")
        #expect(keys.withLock { $0 } == ["Up", "C-u", "C-u", "C-u"])
    }

    @Test("Pull-back keeps a message sent after the last poll, from its popAll")
    func pullBackKeepsUnpolledMessage() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-pull-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))
        let keepName = #"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"keep the old name"}"#
        let addTest = #"{"type":"queue-operation","operation":"enqueue","timestamp":"t2","sessionId":"s1","content":"add a test"}"#
        let log = try chat([Self.USER_ASKS, keepName], in: directory, named: "chat.jsonl")
        value.chairLog = log.path
        _ = await reader.poll(session: value)
        try append([addTest], to: log)

        let pulled = try await reader.pullBack { key in
            guard key == "Up" else { return }
            try append([keepName, addTest].map {
                $0.replacingOccurrences(of: "enqueue", with: "popAll").replacingOccurrences(of: "t1", with: "t2")
            }, to: log)
        }

        #expect(pulled == "keep the old name\nadd a test")
    }

    @Test("Pull-back presses no key while the queue holds a CLI-made entry the agent still needs")
    func pullBackLeavesCLIEntry() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-pull-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))
        let notification = #"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"<task-notification>done</task-notification>"}"#
        let owner = #"{"type":"queue-operation","operation":"enqueue","timestamp":"t2","sessionId":"s1","content":"keep the old name"}"#
        value.chairLog = try chat([Self.USER_ASKS, notification, owner], in: directory, named: "chat.jsonl").path
        _ = await reader.poll(session: value)
        let keys = Mutex<[String]>([])

        await #expect(throws: SwarmProfileError.failed(
            "The agent has its own message in the queue. Edit after it is delivered."
        )) {
            _ = try await reader.pullBack { key in keys.withLock { $0.append(key) } }
        }
        #expect(keys.withLock { $0 }.isEmpty)
    }

    @Test("Pull-back with no queue record by the deadline presses no C-u and says to check the input box")
    func pullBackUnconfirmed() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-pull-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))
        let enqueue = #"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"keep the old name"}"#
        value.chairLog = try chat([Self.USER_ASKS, enqueue], in: directory, named: "chat.jsonl").path
        _ = await reader.poll(session: value)
        let keys = Mutex<[String]>([])

        await #expect(throws: SwarmProfileError.failed("Could not confirm the pull-back. Check the agent's input box.")) {
            _ = try await reader.pullBack { key in keys.withLock { $0.append(key) } }
        }
        #expect(keys.withLock { $0 } == ["Up"])
    }

    private func append(_ lines: [String], to log: URL) throws {
        let handle = try FileHandle(forWritingTo: log)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data((lines.joined(separator: "\n") + "\n").utf8))
        try handle.close()
    }

    @Test("A gap in the log on the same path never freezes the rows")
    func gapKeepsRows() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-clear-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        var value = session(adapter: "tmux-solo")
        let reader = SwarmChairTranscript(binary: try #require(TranscriptToolProcess.bundled))

        let path = try chat(Self.CLEAR_LOG, in: directory, named: "only.jsonl")
        value.chairLog = path.path
        _ = await reader.poll(session: value)
        value.chairLog = nil
        _ = await reader.poll(session: value)
        value.chairLog = path.path
        guard case .rows(let rows, _) = await reader.poll(session: value) else {
            Issue.record("No rows after the gap")
            return
        }
        #expect(!rows.contains { $0.kind == .divider })
    }

    private func session(adapter: String) -> SwarmSession {
        SwarmSession(
            id: .init("01a0c8e6-7afc-7544-95cc-37c77567c776"),
            talkMode: "lane", adapter: adapter, cwd: "/tmp", createdAt: 1,
            chairProvider: "claude", chairID: nil, chairLog: nil,
            agents: 1, messages: 0, lastMessageAt: nil
        )
    }
}

private actor CloseCalls {
    private(set) var arguments: [[String]] = []
    private(set) var adapters: [String] = []

    func reply(arguments: [String], environment: [String: String]) -> ShellResult {
        self.arguments.append(arguments)
        adapters.append(environment["SWARM_ADAPTER"] ?? "")
        if arguments == ["agents", "--json"] {
            return ShellResult(status: 0, stdout: """
                {"agents":[
                  {"id":"orchestrator","role":"chair","pane":"%1","alive":true},
                  {"id":"child-b","role":"code","pane":"%2","alive":true},
                  {"id":"dead","role":"test","pane":null,"alive":false},
                  {"id":"child-a","role":"review","pane":"%3","alive":true}
                ]}
                """, stderr: "")
        }
        return ShellResult(status: 0, stdout: "", stderr: "")
    }
}
