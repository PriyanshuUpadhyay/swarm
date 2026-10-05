import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Transcript rows")
struct TranscriptRowBuilderTests {
    @Test("Rows use conversation labels")
    func labels() {
        #expect(TranscriptRow(kind: .user, text: "", eventID: "1").label(chair: "claude") == "You")
        #expect(TranscriptRow(kind: .assistant, text: "", eventID: "2").label(chair: "claude") == "Claude")
        #expect(TranscriptRow(kind: .assistant, text: "", eventID: "3").label(chair: nil) == "Chair")
        #expect(TranscriptRow(kind: .toolUse, text: "", eventID: "4").label(chair: nil) == "Tool")
    }

    @Test("Agent chunks with one UUID form one row")
    func agentChunksFold() {
        let first = TranscriptEvent.agentMessageChunk(
            text: "Hello ", meta: Meta(agentSessionID: "s1", uuid: "turn-1", timestamp: "t1")
        )
        let second = TranscriptEvent.agentMessageChunk(
            text: "world!", meta: Meta(agentSessionID: "s1", uuid: "turn-1", timestamp: "t2")
        )
        let rows = TranscriptRowBuilder.rows(from: [first, second])
        var merged = TranscriptRow(kind: .assistant, text: "Hello world!", eventID: "turn-1")
        merged.sourceIDs = ["raw-0", "raw-1"]
        #expect(rows == [merged])
    }

    @Test("A different UUID starts a new row")
    func distinctChunksStayApart() {
        let first = TranscriptEvent.agentMessageChunk(text: "A", meta: Meta(uuid: "one"))
        let second = TranscriptEvent.agentMessageChunk(text: "B", meta: Meta(uuid: "two"))
        #expect(TranscriptRowBuilder.rows(from: [first, second]).map(\.text) == ["A", "B"])
    }

    @Test("An AskUserQuestion row reads as a question, not a permission ask")
    func questionLabel() {
        let rows = TranscriptRowBuilder.rows(from: [
            .elicitation(toolCallID: "ask-1", questions: [Question(question: "Which color?")], meta: Meta()),
        ])
        #expect(rows.map { $0.label(chair: "claude") } == ["Question"])
        #expect(rows.map(\.text) == ["Which color?"])
    }

    @Test("Claude bookkeeping records with no chat content produce no rows")
    func bookkeepingRecordsHaveNoRows() async throws {
        let binary = try #require(ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"])
        let log = FileManager.default.temporaryDirectory
            .appendingPathComponent("bookkeeping-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try [
            #"{"type":"attachment","uuid":"u1","sessionId":"s1","attachment":{"type":"credential_org","organizationUuid":"org-1"}}"#,
            #"{"type":"worktree-state","worktreeSession":{"originalCwd":"/work/main","worktreePath":"/work/wt","worktreeName":"wt","worktreeBranch":"wt","sessionId":"s1","enteredExisting":true},"sessionId":"s1"}"#,
            #"{"type":"relocated","sessionId":"s1","relocatedCwd":"/work/wt"}"#,
            #"{"type":"agent-name","agentName":"council-claude","sessionId":"s1"}"#,
            #"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"add a test"}"#,
            #"{"type":"queue-operation","operation":"dequeue","timestamp":"t2","sessionId":"s1"}"#,
        ].joined(separator: "\n").appending("\n").write(to: log, atomically: true, encoding: .utf8)
        let process = TranscriptToolProcess(binary: URL(fileURLWithPath: binary), format: "claude", log: log, follow: false)
        var records: [TranscriptRecord] = []
        for try await record in process.stream { records.append(record) }
        #expect(records.count == 6)
        #expect(records.suffix(2).allSatisfy {
            if case .queueOperation = $0.event { true } else { false }
        })
        #expect(TranscriptRowBuilder.rows(from: records).isEmpty)
    }

    @Test("A queued message the owner typed shows as a user row, a CLI notification shows as a notice, and the turn still ends")
    func absorbedQueuedMessage() async throws {
        let binary = try #require(ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"])
        let log = FileManager.default.temporaryDirectory
            .appendingPathComponent("queued-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try [
            #"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"Fix the bug"}}"#,
            #"{"type":"assistant","uuid":"a1","sessionId":"s1","message":{"role":"assistant","model":"m","content":[{"type":"text","text":"On it"}]}}"#,
            #"{"type":"attachment","uuid":"q1","sessionId":"s1","attachment":{"type":"queued_command","prompt":"keep the old name","origin":{"kind":"human"}}}"#,
            #"{"type":"attachment","uuid":"q2","sessionId":"s1","attachment":{"type":"queued_command","prompt":"<task-notification>done</task-notification>","origin":{"kind":"task-notification"}}}"#,
            #"{"type":"assistant","uuid":"a2","sessionId":"s1","message":{"role":"assistant","model":"m","content":[{"type":"text","text":"Kept it"}]}}"#,
            #"{"type":"system","subtype":"turn_duration","durationMs":1200,"uuid":"d1","sessionId":"s1"}"#,
        ].joined(separator: "\n").appending("\n").write(to: log, atomically: true, encoding: .utf8)
        let process = TranscriptToolProcess(binary: URL(fileURLWithPath: binary), format: "claude", log: log, follow: false)
        var records: [TranscriptRecord] = []
        for try await record in process.stream { records.append(record) }
        let rows = TranscriptRowBuilder.rows(from: records).filter { !$0.isHiddenByDefault }

        #expect(rows.map(\.kind) == [.user, .assistant, .user, .notice, .assistant, .result])
        #expect(rows.map(\.text).dropFirst(2).first == "keep the old name")
        #expect(ChairTurn.isActive(Array(rows.dropLast())))
        #expect(!ChairTurn.isActive(rows))
    }

    @Test("Known tool output joins its call, while questions and errors remain distinct")
    func actionRows() {
        let meta = Meta(uuid: "event-1")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "call-1", name: "ls", input: .object([:]), status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "call-1", status: .completed, content: "one\ntwo", meta: meta),
            .elicitation(toolCallID: "ask-1", questions: [Question(question: "Proceed?")], meta: meta),
            .error(message: "failed", meta: meta),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .question, .error])
        #expect(Set(rows.map(\.eventID)).count == rows.count)
        #expect(rows[0].eventID == "call-1:call")
        #expect(rows[0].tool?.output == "one\ntwo")
        #expect(rows[0].tool?.state == .finished)
    }

    @Test("Tool calls show a short label and keep input in detail")
    func toolSummary() {
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "one", name: "exec", input: .object([
                "description": .string("List files"), "command": .string("ls\npwd")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "two", name: "exec", input: .object([
                "command": .string("pwd\nls")
            ]), status: .pending, meta: Meta()),
        ])
        #expect(rows.map(\.text) == ["exec · List files", "exec · pwd"])
        #expect(rows[0].detail?.contains("\"command\"") == true)
    }

    @Test("Codex and AGY command shapes give distinct collapsed labels")
    func providerCommandLabels() {
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "codex", name: "exec", input: .object([
                "cmd": .string("git status")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "agy", name: "exec", input: .object([
                "CommandLine": .string("pwd")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "file", name: "view_file", input: .object([
                "TargetFile": .string("/work/source.swift")
            ]), status: .pending, meta: Meta()),
            .toolCall(toolCallID: "summary", name: "view_file", input: .object([
                "toolSummary": .string("Read source"), "AbsolutePath": .string("/work/source.swift")
            ]), status: .pending, meta: Meta()),
        ])
        #expect(rows.map(\.text) == [
            "exec · git status", "exec · pwd", "view_file · source.swift", "view_file · Read source"
        ])
    }

    @Test("Low value notices and empty system rows are hidden by default")
    func hiddenRows() {
        let rows = TranscriptRowBuilder.rows(from: [
            .sessionInfo(kind: "title", value: "Chat", meta: Meta()),
            .hookResult(kind: "hook_success", hookEvent: "", hookName: "check", toolCallID: "", exitCode: 0, meta: Meta()),
            .systemMessage(kind: "system", text: "  ", meta: Meta()),
            .systemMessage(kind: "system", text: "Ready", meta: Meta()),
        ])
        #expect(rows.map(\.isHiddenByDefault) == [true, true, true, false])
    }

    @Test("A background task notification shows its summary as a notice, not raw XML from You")
    func taskNotification() {
        let xml = "<task-notification>\n<task-id>b4</task-id>\n<status>completed</status>\n<summary>Background command \"Wait 20 seconds\" completed (exit code 0)</summary>\n</task-notification>"
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "task_notification", text: xml, meta: Meta()),
            .systemMessage(kind: "queued_command", text: "<task-notification>\n<status>failed</status>\n</task-notification>", meta: Meta()),
            .systemMessage(kind: "task_notification", text: "<task-notification></task-notification>", meta: Meta()),
        ])
        #expect(rows.map(\.kind) == [.notice, .notice, .notice])
        #expect(rows.map(\.text) == [
            "Background command \"Wait 20 seconds\" completed (exit code 0)",
            "Background task failed",
            "Background task finished",
        ])
    }

    @Test("A background task notification after a finished turn starts a running turn")
    func taskNotificationStartsTurn() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .userMessageChunk(text: "Run it in the background", meta: Meta(agentSessionID: "s", uuid: "u1")),
            .turnEnded(durationMs: 1, reason: .completed, meta: meta),
            .systemMessage(kind: "task_notification", text: " <task-notification><summary>done</summary></task-notification>", meta: meta),
            .toolCall(toolCallID: "t1", name: "Bash", input: .object([:]), status: .pending, meta: meta),
        ])
        #expect(rows.map(\.kind).contains(.notice))
        #expect(ChairTurn.isActive(rows))
        #expect(!ChairTurn.isActive(Array(rows.prefix(2))))
    }

    @Test("Interleaved calls keep their own output and their call order")
    func interleavedCalls() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "a", name: "exec", input: .object(["command": .string("first")]), status: .pending, meta: meta),
            .toolCall(toolCallID: "b", name: "exec", input: .object(["command": .string("second")]), status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "b", status: .completed, content: "B result", meta: meta),
            .toolCallUpdate(toolCallID: "a", status: .completed, content: "A result", meta: meta),
        ])
        #expect(rows.map(\.eventID) == ["a:call", "b:call"])
        #expect(rows.map { $0.tool?.output } == ["A result", "B result"])
        #expect(rows.map { $0.tool?.command } == ["first", "second"])
    }

    @Test("Updates are snapshots and confirmed diffs join across event order")
    func snapshotsAndDiffs() {
        let meta = Meta(agentSessionID: "s")
        let before = diff("a", path: "/work/first.txt")
        let after = diff("a", path: "/work/second.txt")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCallUpdate(toolCallID: "a", status: .pending, content: "partial", meta: meta),
            before,
            .toolCall(toolCallID: "a", name: "edit", input: .object(["file_path": .string("/work/first.txt")]), status: .pending, meta: meta),
            .agentMessageChunk(text: "I edited it.", meta: Meta(uuid: "answer")),
            .toolCallUpdate(toolCallID: "a", status: .completed, content: "final output", meta: meta),
            after,
        ])
        #expect(rows.map(\.kind) == [.toolUse, .assistant])
        #expect(rows[0].tool?.output == "final output")
        #expect(rows[0].tool?.state == .finished)
        #expect(rows[0].tool?.diffs.map(\.path) == ["/work/first.txt", "/work/second.txt"])
        #expect(rows[0].tool?.path == "/work/first.txt")
        let sourceInput = try? JSONDecoder().decode(
            JSONElement.self, from: Data((rows[0].detail ?? "").utf8)
        )
        #expect(sourceInput == .object(["file_path": .string("/work/first.txt")]))
        #expect(rows[1].text == "I edited it.")
    }

    @Test("A page without a call keeps each result and diff visible")
    func missingCallAndPageBoundary() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "a", name: "edit", input: .null, status: .pending, meta: meta),
            .page(start: 0, end: 10),
            .toolCallUpdate(toolCallID: "a", status: .completed, content: "page result", meta: meta),
            diff("a", path: "/work/page.txt"),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .toolResult, .diff])
        #expect(rows[0].tool?.state == .waiting)
        #expect(rows[0].tool?.output == nil)
        #expect(rows[1].text == "page result")
        #expect(rows[2].diff?.path == "/work/page.txt")
    }

    @Test("Missing and ambiguous IDs do not attach results to a call")
    func ambiguousIDs() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "dup", name: "first", input: .null, status: .pending, meta: meta),
            .toolCall(toolCallID: "dup", name: "second", input: .null, status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "dup", status: .completed, content: "ambiguous", meta: meta),
            .toolCall(toolCallID: "", name: "third", input: .null, status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "", status: .completed, content: "missing ID", meta: meta),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .toolUse, .toolResult, .toolUse, .toolResult])
        #expect(Set(rows.map(\.eventID)).count == rows.count)
        #expect(rows[0].tool?.output == nil)
        #expect(rows[1].tool?.output == nil)
        #expect(rows[2].text == "ambiguous")
        #expect(rows[2].toolStatus == .completed)
        #expect(rows[4].text == "missing ID")
        #expect(rows[4].toolStatus == .completed)
    }

    @Test("A blank session cannot resolve a reused tool ID")
    func blankSessionIsAmbiguous() {
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "same", name: "one", input: .null, status: .pending, meta: Meta(agentSessionID: "one")),
            .toolCall(toolCallID: "same", name: "two", input: .null, status: .pending, meta: Meta(agentSessionID: "two")),
            .toolCallUpdate(toolCallID: "same", status: .failed, content: "unknown owner", meta: Meta()),
            diff("same", path: "/work/ambiguous.txt", session: ""),
        ])
        #expect(rows.map(\.kind) == [.toolUse, .toolUse, .toolResult, .diff])
        #expect(rows[0].tool?.diffs.isEmpty == true)
        #expect(rows[1].tool?.diffs.isEmpty == true)
        #expect(rows[2].toolStatus == .failed)
        #expect(rows[2].text == "unknown owner")
    }

    @Test("Session and turn boundaries limit matching when IDs repeat")
    func scopeBoundaries() {
        let first = Meta(agentSessionID: "one")
        let second = Meta(agentSessionID: "two")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "same", name: "first", input: .null, status: .pending, meta: first),
            .toolCallUpdate(toolCallID: "same", status: .completed, content: "one", meta: first),
            .toolCall(toolCallID: "same", name: "second", input: .null, status: .pending, meta: second),
            .toolCallUpdate(toolCallID: "same", status: .failed, content: "two", meta: second),
            .turnEnded(durationMs: nil, reason: .completed, meta: second),
            .toolCall(toolCallID: "same", name: "third", input: .null, status: .pending, meta: second),
            .toolCallUpdate(toolCallID: "same", status: .completed, content: "three", meta: second),
        ])
        #expect(rows.filter { $0.kind == .toolUse }.map { $0.tool?.output } == ["one", "two", "three"])
        #expect(rows.filter { $0.kind == .toolUse }.map { $0.tool?.state } == [.finished, .failed, .finished])
        #expect(Set(rows.map(\.eventID)).count == rows.count)
    }

    @Test("Failed, aborted, and completed turns give unresolved calls clear states")
    func endStates() {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "a", name: "a", input: .null, status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "a", status: .failed, content: "error text", meta: meta),
            .toolCall(toolCallID: "b", name: "b", input: .null, status: .pending, meta: meta),
            .turnEnded(durationMs: nil, reason: .aborted, meta: meta),
            .toolCall(toolCallID: "c", name: "c", input: .null, status: .pending, meta: meta),
            .turnEnded(durationMs: 41_000, reason: .completed, meta: meta),
            .toolCall(toolCallID: "d", name: "d", input: .null, status: .pending, meta: meta),
        ])
        #expect(rows.filter { $0.kind == .toolUse }.map { $0.tool?.state } == [.failed, .interrupted, .unreported, .waiting])
        #expect(rows.filter(\.endsTurn).map(\.detail) == [nil, "41s"])
        #expect(rows.first?.tool?.output == "error text")
    }

    @Test("A shell output joins its input into one shell row with the input's id")
    func shellPair() throws {
        let rows = TranscriptRowBuilder.rows(from: [
            shellInput("ls", uuid: "input"),
            shellOutput("<bash-stdout>a &amp; b</bash-stdout><bash-stderr></bash-stderr>", parent: "input"),
        ])
        try #require(rows.map(\.kind) == [.shell])
        #expect(rows[0].eventID == "input:shell")
        #expect(rows[0].shell == TranscriptShellRun(command: "ls", output: "a & b", exitCode: nil))
        #expect(rows[0].printLine == "shell $ ls\na & b")
    }

    @Test("A shell input without output and an output without input each stay a shell row that starts a turn")
    func unmatchedShell() {
        let lone = TranscriptRowBuilder.rows(from: [shellInput("pwd", uuid: "input")])
        #expect(lone.map(\.shell) == [TranscriptShellRun(command: "pwd", output: "", exitCode: nil)])
        let orphan = TranscriptRowBuilder.rows(from: [
            shellOutput("<bash-stdout>/work</bash-stdout>", parent: "outside-window"),
        ])
        #expect(orphan.map(\.kind) == [.shell])
        #expect(orphan.map(\.shell) == [TranscriptShellRun(command: nil, output: "/work", exitCode: nil)])
        #expect((lone + orphan).map(\.startsTurn) == [true, true])
    }

    @Test("Two shell inputs with one id do not take the output that names it")
    func duplicateShellInputs() {
        let rows = TranscriptRowBuilder.rows(from: [
            shellInput("first", uuid: "same"),
            shellInput("second", uuid: "same"),
            shellOutput("<bash-stdout>which one</bash-stdout>", parent: "same"),
        ])
        #expect(rows.map(\.kind) == [.shell, .shell, .shell])
        #expect(rows.map { $0.shell?.command } == ["first", "second", nil])
        #expect(rows.map { $0.shell?.output } == ["", "", "which one"])
        #expect(Set(rows.map(\.eventID)).count == rows.count)
    }

    @Test("A command record with no closed name tag gets no chip and keeps its skill body as its own row")
    func cutCommandHasNoChip() {
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command", text: "<command-name>/flo", meta: Meta(uuid: "command")),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(uuid: "body", parentUUID: "command")),
        ])
        #expect(rows.map(\.command) == [nil, nil])
        #expect(rows.map(\.text) == ["<command-name>/flo", "Base directory for this skill: /skills/flow"])
    }

    @Test("A typed skill command takes its body, keeps its raw text, and starts a turn")
    func skillBodyToCommand() throws {
        let commandText = "<command-message>flow</command-message>\n<command-name>/flow</command-name>\n<command-args>start\nthe chat</command-args>"
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command", text: commandText, meta: Meta(uuid: "command")),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(uuid: "body", parentUUID: "command")),
        ])
        try #require(rows.map(\.kind) == [.system])
        #expect(rows[0].text == commandText)
        #expect(rows[0].systemKind == "command")
        #expect(rows[0].command == TranscriptCommandChip(
            name: "/flow", arguments: "start\nthe chat", skillBody: "Base directory for this skill: /skills/flow"
        ))
        #expect(rows[0].startsTurn)
    }

    @Test("A skill body that a Skill call loaded joins that call and adds no row")
    func skillBodyToTool() throws {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "skill-call", name: "Skill", input: .object(["skill": .string("flow")]), status: .pending, meta: meta),
            .toolCallUpdate(toolCallID: "skill-call", status: .completed, content: "Launching skill: flow", meta: meta),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(agentSessionID: "s", uuid: "body", parentUUID: "tool-result", sourceToolUseID: "skill-call")),
        ])
        try #require(rows.map(\.kind) == [.toolUse])
        #expect(rows[0].tool?.skillBody == "Base directory for this skill: /skills/flow")
        let twice = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "skill-call", name: "Skill", input: .object(["skill": .string("flow")]), status: .pending, meta: meta),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(agentSessionID: "s", uuid: "first-body", sourceToolUseID: "skill-call")),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow/more", meta: Meta(agentSessionID: "s", uuid: "second-body", sourceToolUseID: "skill-call")),
        ])
        #expect(twice.map(\.tool?.skillBody) == ["Base directory for this skill: /skills/flow\n\nBase directory for this skill: /skills/flow/more"])
    }

    @Test("A bundled skill's injected body joins the Skill call that names it, and not another tool")
    func bundledSkillBodyToTool() throws {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "skill-call", name: "Skill", input: .object(["skill": .string("simplify")]), status: .completed, meta: meta),
            .systemMessage(kind: "injected", text: "Review target: the changes", meta: Meta(agentSessionID: "s", uuid: "body", parentUUID: "tool-result", sourceToolUseID: "skill-call")),
            .toolCall(toolCallID: "bash-call", name: "Bash", input: .object([:]), status: .completed, meta: meta),
            .systemMessage(kind: "injected", text: "Hook note", meta: Meta(agentSessionID: "s", uuid: "note", sourceToolUseID: "bash-call")),
        ])
        try #require(rows.map(\.kind) == [.toolUse, .toolUse, .system])
        #expect(rows[0].tool?.skillBody == "Review target: the changes")
        #expect(rows[1].tool?.skillBody == nil)
        #expect(rows[2].isHiddenByDefault)
    }

    @Test("A skill body with no command or call is a system row, never the owner's")
    func unlinkedSkillBody() throws {
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(uuid: "body", parentUUID: "outside-window")),
        ])
        try #require(rows.map(\.kind) == [.system])
        #expect(rows[0].systemKind == "skill_body")
        #expect(!rows[0].isHiddenByDefault)
    }

    @Test("A /model command takes its stdout as output and does not start a turn")
    func modelCommandOutput() throws {
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command", text: "<command-name>/model</command-name>\n<command-args>sonnet</command-args>", meta: Meta(uuid: "command")),
            .systemMessage(kind: "command_output", text: "<local-command-stdout>Set model to \u{1B}[1msonnet\u{1B}[22m</local-command-stdout>", meta: Meta(uuid: "stdout", parentUUID: "command")),
        ])
        try #require(rows.count == 1)
        #expect(rows[0].command == TranscriptCommandChip(name: "/model", arguments: "sonnet", output: "Set model to sonnet"))
        #expect(!rows[0].startsTurn)
        #expect(!ChairTurn.isActive(rows))
    }

    @Test("After a finished turn a shell command and a skill each start a turn, and an interrupt ends it")
    func turnStateForInjectedRows() {
        let finished: [TranscriptEvent] = [
            .userMessageChunk(text: "Fix the bug", meta: Meta(uuid: "prompt")),
            .turnEnded(durationMs: 1, reason: .completed, meta: Meta()),
        ]
        let shell: [TranscriptEvent] = [
            shellInput("git status", uuid: "input"),
            shellOutput("<bash-stdout>clean</bash-stdout>", parent: "input"),
        ]
        let skill: [TranscriptEvent] = [
            .systemMessage(kind: "command", text: "<command-name>/flow</command-name>", meta: Meta(uuid: "command")),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(uuid: "body", parentUUID: "command")),
        ]
        let interrupt = TranscriptEvent.systemMessage(kind: "interrupted", text: "[Request interrupted by user]", meta: Meta(uuid: "stop"))

        let reply = TranscriptEvent.agentMessageChunk(text: "The tree is clean.", meta: Meta(uuid: "reply"))
        let replyEnded = TranscriptEvent.turnEnded(durationMs: 1, reason: .completed, meta: Meta(uuid: "reply-end"))
        #expect(ChairTurn.isActive(TranscriptRowBuilder.rows(from: finished + shell)))
        #expect(ChairTurn.isActive(TranscriptRowBuilder.rows(from: finished + shell + [reply])))
        #expect(!ChairTurn.isActive(TranscriptRowBuilder.rows(from: finished + shell + [reply, replyEnded])))
        #expect(ChairTurn.isActive(TranscriptRowBuilder.rows(from: finished + [shell[0]])))
        #expect(ChairTurn.isActive(TranscriptRowBuilder.rows(from: finished + skill)))
        let interrupted = TranscriptRowBuilder.rows(from: finished + skill + [interrupt])
        #expect(interrupted.last?.kind == .notice)
        #expect(interrupted.last?.text == "Interrupted")
        #expect(!ChairTurn.isActive(interrupted))
    }

    @Test("Injected records are hidden system rows, while a typed chunk stays the owner's")
    func injectedRows() {
        let rows = TranscriptRowBuilder.rows(from: [
            .userMessageChunk(text: "Fix the bug", meta: Meta(uuid: "prompt")),
            .systemMessage(kind: "injected", text: "<system-reminder>Be brief</system-reminder>", meta: Meta(uuid: "prompt")),
        ])
        #expect(rows.map(\.kind) == [.user, .system])
        #expect(rows.map(\.isHiddenByDefault) == [false, true])
    }

    @Test("After a finished turn a message from another agent shows without its outer tag and starts a turn, while an injected record does not")
    func peerMessageStartsTurn() throws {
        let finished: [TranscriptEvent] = [
            .userMessageChunk(text: "Fix the bug", meta: Meta(uuid: "prompt")),
            .turnEnded(durationMs: 1, reason: .completed, meta: Meta()),
        ]
        let teammate = TranscriptEvent.systemMessage(
            kind: "peer_message", text: "<teammate-message teammate_id=\"lead\">\nReview the parser\n</teammate-message>",
            meta: Meta(uuid: "teammate")
        )
        let peerText = "Another Claude session sent a message:\n<agent-message>Done</agent-message>"
        let peer = TranscriptEvent.systemMessage(kind: "peer_message", text: peerText, meta: Meta(uuid: "peer"))
        let injected = TranscriptEvent.systemMessage(kind: "injected", text: "Stop hook feedback: fix it", meta: Meta(uuid: "hook"))

        let rows = TranscriptRowBuilder.rows(from: finished + [teammate])
        let message = try #require(rows.last)
        #expect(message.kind == .notice)
        #expect(message.text == "Review the parser")
        #expect(!message.isHiddenByDefault)
        #expect(ChairTurn.isActive(rows))
        #expect(TranscriptRowBuilder.rows(from: [peer]).map(\.text) == [peerText])
        let modelWord = TranscriptEvent.systemMessage(
            kind: "peer_message", text: "<teammate-message teammate_id=\"lead\">model: use opus</teammate-message>",
            meta: Meta(uuid: "model-word")
        )
        #expect(TranscriptRowBuilder.rows(from: [modelWord]).map(\.isHiddenByDefault) == [false])
        #expect(!ChairTurn.isActive(TranscriptRowBuilder.rows(from: finished + [injected])))
    }

    @Test("A bundled skill's meta body without the base directory line joins its command and starts a turn")
    func bundledSkillBodyToCommand() throws {
        let rows = TranscriptRowBuilder.rows(from: [
            .userMessageChunk(text: "Fix the bug", meta: Meta(uuid: "prompt")),
            .turnEnded(durationMs: 1, reason: .completed, meta: Meta()),
            .systemMessage(kind: "command", text: "<command-name>/simplify</command-name>", meta: Meta(uuid: "command")),
            .systemMessage(kind: "injected", text: "Review target: the changes", meta: Meta(uuid: "body", parentUUID: "command")),
        ])
        try #require(rows.map(\.kind) == [.user, .result, .system])
        #expect(rows[2].command?.skillBody == "Review target: the changes")
        #expect(rows[2].startsTurn)
        #expect(ChairTurn.isActive(rows))
    }

    @Test("A system reminder or a command caveat whose parent is a command stays a hidden System row and starts no turn")
    func reminderUnderCommandStaysHidden() throws {
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command", text: "<command-name>/simplify</command-name>", meta: Meta(uuid: "command")),
            .systemMessage(kind: "injected", text: "\n<system-reminder>Be brief</system-reminder>", meta: Meta(uuid: "reminder", parentUUID: "command")),
        ])
        try #require(rows.map(\.kind) == [.system, .system])
        #expect(rows[0].command?.skillBody == nil)
        #expect(!rows[0].startsTurn)
        #expect(rows[1].systemKind == "injected")
        #expect(rows[1].isHiddenByDefault)
        let caveat = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command", text: "<command-name>/usage</command-name>", meta: Meta(uuid: "command")),
            .systemMessage(kind: "injected", text: "<local-command-caveat>Caveat: do not respond</local-command-caveat>", meta: Meta(uuid: "caveat", parentUUID: "command")),
        ])
        #expect(caveat.map(\.command?.skillBody) == [nil, nil])
        #expect(caveat.map(\.isHiddenByDefault) == [false, true])
    }

    @Test("Two skill bodies of one command join with a blank line in log order")
    func twoSkillBodiesJoin() throws {
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command", text: "<command-name>/flow</command-name>", meta: Meta(uuid: "command")),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(uuid: "first-body", parentUUID: "command")),
            .systemMessage(kind: "injected", text: "Review target: the changes", meta: Meta(uuid: "second-body", parentUUID: "command")),
        ])
        try #require(rows.count == 1)
        #expect(rows[0].command?.skillBody == "Base directory for this skill: /skills/flow\n\nReview target: the changes")
    }

    @Test("Find text holds a command's output and skill body and a Skill call's body")
    func searchTextHoldsJoinedRecords() throws {
        let meta = Meta(agentSessionID: "s")
        let rows = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: "command", text: "<command-name>/model</command-name>", meta: Meta(agentSessionID: "s", uuid: "model-command")),
            .systemMessage(kind: "command_output", text: "<local-command-stdout>Set model to sonnet</local-command-stdout>", meta: Meta(agentSessionID: "s", uuid: "stdout", parentUUID: "model-command")),
            .systemMessage(kind: "command", text: "<command-name>/flow</command-name>", meta: Meta(agentSessionID: "s", uuid: "flow-command")),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/flow", meta: Meta(agentSessionID: "s", uuid: "command-body", parentUUID: "flow-command")),
            .toolCall(toolCallID: "skill-call", name: "Skill", input: .object(["skill": .string("review")]), status: .pending, meta: meta),
            .systemMessage(kind: "skill_body", text: "Base directory for this skill: /skills/review", meta: Meta(agentSessionID: "s", uuid: "tool-body", sourceToolUseID: "skill-call")),
        ])
        try #require(rows.count == 3)
        #expect(rows[0].searchText.contains("Set model to sonnet"))
        #expect(rows[1].searchText.contains("/skills/flow"))
        #expect(rows[2].searchText.contains("/skills/review"))
    }

    private func shellInput(_ command: String, uuid: String) -> TranscriptEvent {
        .systemMessage(kind: "shell_input", text: "<bash-input>\(command)</bash-input>", meta: Meta(uuid: uuid))
    }

    private func shellOutput(_ text: String, parent: String) -> TranscriptEvent {
        .systemMessage(kind: "shell_output", text: text, meta: Meta(uuid: "output-of-\(parent)", parentUUID: parent))
    }

    private func diff(_ id: String, path: String, session: String = "s") -> TranscriptEvent {
        .decode(line: """
        {"type":"tool_diff","tool_call_id":"\(id)","path":"\(path)","hunks":[],"meta":{"session_id":"\(session)"}}
        """)
    }
}

@Suite("Transcript debug data")
struct TranscriptDebugDataTests {
    @Test("Keeps raw lines and labels rendered, hidden, and omitted events")
    func entries() {
        let records = [
            TranscriptRecord(
                event: .userMessageChunk(text: "Hello", meta: Meta(uuid: "one")),
                rawLine: #"{"type":"user_message_chunk","text":"Hello"}"#
            ),
            TranscriptRecord(
                event: .sessionInfo(kind: "title", value: "Chat", meta: Meta(uuid: "two")),
                rawLine: #"{"type":"session_info","kind":"title","value":"Chat"}"#
            ),
            TranscriptRecord(
                event: .turnStarted(meta: Meta(uuid: "three")),
                rawLine: #"{"type":"turn_started"}"#
            ),
        ]

        let entries = TranscriptDebugData.entries(from: records)
        #expect(entries.map(\.rowKind) == ["user", "hidden", "no row"])
        #expect(entries[0].rawLine == records[0].rawLine)
        #expect(entries[0].displayText.contains("\n"))
    }

    @Test("A turn is active from the user's last message until a turn ends")
    func chairTurn() {
        let question = TranscriptRow(kind: .user, text: "hi", eventID: "question")
        let answer = TranscriptRow(kind: .assistant, text: "hello", eventID: "answer")
        var ended = TranscriptRow(kind: .result, text: "completed", eventID: "ended")
        ended.endsTurn = true
        let decision = TranscriptRow(kind: .result, text: "allow", eventID: "decision")

        #expect(!ChairTurn.isActive([]))
        #expect(ChairTurn.isActive([question, answer]))
        #expect(ChairTurn.isActive([question, decision]))
        #expect(!ChairTurn.isActive([question, answer, ended]))
        #expect(ChairTurn.isActive([question, answer, ended, question]))
    }

    private static let ring = "swarm: new message. Run swarm inbox and read each body at /home/owner/.swarm/<body_path>."

    @Test("A ring to an idle agent is a ring row that starts a turn; a ring mid-turn starts none")
    func ringRows() {
        let idle = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring-1")),
        ])
        #expect(idle.map(\.kind) == [.system])
        #expect(idle[0].systemKind == TranscriptSystemKind.swarmRing)
        #expect(idle[0].eventID == "ring-1:ring")
        #expect(idle[0].startsTurn)
        #expect(!idle[0].isHiddenByDefault)
        #expect(ChairTurn.isActive(idle))
        // Find matches what the row draws, so "inbox" has no hit on a ring line.
        #expect(idle[0].searchText == "New swarm message")

        let rows = TranscriptRowBuilder.rows(from: [
            .userMessageChunk(text: "Run the council", meta: Meta(uuid: "prompt")),
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring-2")),
            .turnEnded(durationMs: 1000, reason: .completed, meta: Meta(uuid: "end")),
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring-3")),
        ])
        #expect(rows.map(\.eventID) == ["prompt", "ring-2:ring", "end:result", "ring-3:ring"])
        #expect(rows.map(\.startsTurn) == [false, false, false, true])
        #expect(ChairTurn.isActive(rows))
        #expect(!ChairTurn.isActive(Array(rows.prefix(3))))
    }

    @Test("A window that starts mid-turn keeps its first ring mid-turn, so the run around it folds as one")
    func windowStartsMidTurn() {
        let tail: [TranscriptEvent] = [
            .toolCall(toolCallID: "c1", name: "exec", input: .string("ls"), status: .pending, meta: Meta()),
            .toolCallUpdate(toolCallID: "c1", status: .completed, content: "ok", meta: Meta()),
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring-1")),
            .toolCall(toolCallID: "c2", name: "exec", input: .string("swarm inbox"), status: .pending, meta: Meta()),
            .toolCallUpdate(toolCallID: "c2", status: .completed, content: "ok", meta: Meta()),
            .agentMessageChunk(text: "Done.", meta: Meta(uuid: "answer")),
            .turnEnded(durationMs: 1000, reason: .completed, meta: Meta(uuid: "end")),
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring-2")),
        ]
        // The app's tail window starts at index 0 and only goes below 0 on Load earlier, so the
        // window's older-entries flag, not the offset, says it starts mid-log.
        let rows = TranscriptRowBuilder.rows(from: tail, indexOffset: 0, hasOlder: true)
        #expect(rows.filter { $0.systemKind == TranscriptSystemKind.swarmRing }.map(\.startsTurn) == [false, true])
        let folds = ToolRunFold.items(in: rows).compactMap { if case .fold(let group) = $0 { group.map(\.eventID) } else { nil } }
        #expect(folds == [["c1:call", "ring-1:ring", "c2:call"]])
        // The owner's prompt is above the window, so the mid-turn ring is what shows the turn runs.
        let running = TranscriptRowBuilder.rows(from: Array(tail.prefix(5)), indexOffset: 0, hasOlder: true)
        #expect(ChairTurn.isActive(running))

        // A prompt the owner queued mid-turn starts no turn, so the ring before it stays mid-turn.
        let queued = TranscriptRowBuilder.rows(from: Array(tail.prefix(4)) + [
            .systemMessage(kind: TranscriptSystemKind.queuedPrompt, text: "also check the docs", meta: Meta(uuid: "queued")),
            .toolCallUpdate(toolCallID: "c2", status: .completed, content: "ok", meta: Meta()),
            .turnEnded(durationMs: 1000, reason: .completed, meta: Meta(uuid: "end")),
        ], indexOffset: 0, hasOlder: true)
        #expect(queued.filter { $0.systemKind == TranscriptSystemKind.swarmRing }.map(\.startsTurn) == [false])
        let queuedFolds = ToolRunFold.items(in: queued).compactMap { if case .fold(let group) = $0 { group.map(\.eventID) } else { nil } }
        #expect(queuedFolds == [["c1:call", "ring-1:ring", "c2:call"]])

        // A Codex steer (Enter mid-turn) and a task notice Claude queued mid-turn start no turn either.
        let midTurnInputs: [(TranscriptEvent, Bool)] = [
            (.userMessageChunk(text: "also check docs", meta: Meta(uuid: "steer")), true),
            (.systemMessage(kind: "queued_command", text: "<task-notification><summary>done</summary></task-notification>", meta: Meta(uuid: "task")), false),
        ]
        for (input, marksTurnStarts) in midTurnInputs {
            let events: [TranscriptEvent] = Array(tail.prefix(5)) + [
                input, .turnEnded(durationMs: 1000, reason: .completed, meta: Meta(uuid: "end")),
            ]
            let window = TranscriptRowBuilder.rows(from: events, indexOffset: 0, hasOlder: true, marksTurnStarts: marksTurnStarts)
            #expect(window.filter { $0.systemKind == TranscriptSystemKind.swarmRing }.map(\.startsTurn) == [false])
            let inputFolds = ToolRunFold.items(in: window).compactMap { if case .fold(let group) = $0 { group.map(\.eventID) } else { nil } }
            #expect(inputFolds == [["c1:call", "ring-1:ring", "c2:call"]])
            #expect(ChairTurn.isActive(Array(window.dropLast())))
        }
        // In a Codex log the prompt follows its turn-started record, so it still starts a turn.
        let codexPrompt = TranscriptRowBuilder.rows(from: [
            .turnEnded(durationMs: 1000, reason: .completed, meta: Meta(uuid: "end")),
            .turnStarted(meta: Meta(uuid: "start")),
            .userMessageChunk(text: "Run the council", meta: Meta(uuid: "prompt")),
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring-4")),
        ], indexOffset: 0, hasOlder: true, marksTurnStarts: true)
        #expect(codexPrompt.filter { $0.systemKind == TranscriptSystemKind.swarmRing }.map(\.startsTurn) == [false])

        let idleWindow = TranscriptRowBuilder.rows(from: [
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring-3")),
            .userMessageChunk(text: "Run the council", meta: Meta(uuid: "prompt")),
        ], indexOffset: 0, hasOlder: true)
        #expect(idleWindow[0].startsTurn)
    }

    @Test("A Codex log with no turn-started records still starts a turn with a prompt at the log's start or after a turn ends")
    func codexLogWithoutTurnStarts() {
        let rows = TranscriptRowBuilder.rows(from: [
            .userMessageChunk(text: "Fix the build", meta: Meta(uuid: "prompt")),
            .toolCall(toolCallID: "c1", name: "exec", input: .string("make"), status: .pending, meta: Meta()),
            .toolCallUpdate(toolCallID: "c1", status: .completed, content: "ok", meta: Meta()),
            .agentMessageChunk(text: "Done.", meta: Meta(uuid: "answer")),
            .turnEnded(durationMs: 1000, reason: .aborted, meta: Meta(uuid: "end")),
            .userMessageChunk(text: "Try again", meta: Meta(uuid: "retry")),
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring")),
        ], marksTurnStarts: true)
        #expect(rows.filter { $0.kind == .user }.map(\.arrivesMidTurn) == [false, false])
        #expect(rows.filter { $0.systemKind == TranscriptSystemKind.swarmRing }.map(\.startsTurn) == [false])
    }

    @Test("A ring between a tool call and its result does not split them")
    func ringKeepsToolScope() {
        let rows = TranscriptRowBuilder.rows(from: [
            .userMessageChunk(text: "Run the council", meta: Meta(uuid: "prompt")),
            .toolCall(toolCallID: "c1", name: "exec", input: .string("ls"), status: .pending, meta: Meta()),
            .systemMessage(kind: TranscriptSystemKind.swarmRing, text: Self.ring, meta: Meta(uuid: "ring")),
            .toolCallUpdate(toolCallID: "c1", status: .completed, content: "ok", meta: Meta()),
        ])
        #expect(rows.map(\.eventID) == ["prompt", "c1:call", "ring:ring"])
        #expect(rows[1].tool?.output == "ok")
    }

    @Test("Each row names its own event and every joined or merged event as its sources, and Show Source finds them")
    func rowSources() {
        let records: [TranscriptRecord] = [
            .init(event: .agentMessageChunk(text: "Hel", meta: Meta(uuid: "a1")), rawLine: "{}"),
            .init(event: .agentMessageChunk(text: "lo", meta: Meta(uuid: "a1")), rawLine: "{}"),
            .init(event: .toolCall(toolCallID: "c1", name: "Bash", input: .object(["command": .string("ls")]), status: .pending, meta: Meta()), rawLine: "{}"),
            .init(event: .ignored(kind: "usage", meta: Meta()), rawLine: "{}"),
            .init(event: .toolCallUpdate(toolCallID: "c1", status: .completed, content: "ok", meta: Meta()), rawLine: "{}"),
            .init(event: .systemMessage(kind: TranscriptSystemKind.shellInput, text: "<bash-input>pwd</bash-input>", meta: Meta(uuid: "in")), rawLine: "{}"),
            .init(event: .systemMessage(kind: TranscriptSystemKind.shellOutput, text: "<bash-stdout>/w</bash-stdout>", meta: Meta(uuid: "out", parentUUID: "in")), rawLine: "{}"),
        ]
        let rows = TranscriptRowBuilder.rows(from: records, indexOffset: 10)
        #expect(rows.map(\.sourceIDs) == [["raw-10", "raw-11"], ["raw-12", "raw-14"], ["raw-15", "raw-16"]])
        let raw = TranscriptDebugData.entries(from: records, indexOffset: 10)
        #expect(TranscriptSource.entries(for: rows[1], in: raw).map(\.index) == [12, 14])
        #expect(TranscriptSource.entries(for: TranscriptRow(kind: .notice, text: "", eventID: "x"), in: raw).isEmpty)
    }

    @Test("Show Source's lookup is equal by source ids, so SwiftUI can skip a row a parent update did not change")
    func sourceLookupEquality() {
        let shown = TranscriptSource.Lookup(ids: ["raw-1"]) { [] }
        let rebuilt = TranscriptSource.Lookup(ids: ["raw-1"]) { [RawTranscriptEntry(index: 1, rawLine: "{}", rowKind: "user")] }
        let merged = TranscriptSource.Lookup(ids: ["raw-1", "raw-2"]) { [] }
        #expect(shown == rebuilt)
        #expect(shown != merged)
        #expect(rebuilt.entries().map(\.index) == [1])
    }
}
