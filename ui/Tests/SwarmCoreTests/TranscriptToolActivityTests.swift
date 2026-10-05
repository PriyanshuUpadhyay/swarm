import Foundation
import Testing
import TranscriptTool
@testable import SwarmCore

@Suite("Transcript tool activity")
struct TranscriptToolActivityTests {
    @Test("Known command and path keys keep exact values")
    func knownInputKeys() {
        let command = "printf 'one\ntwo'"
        let path = "/work/Folder With Spaces/file.txt"
        #expect(TranscriptToolActivity.command(in: .object(["command": .string(command)]), name: "Bash") == command)
        #expect(TranscriptToolActivity.command(in: .object(["cmd": .string(command)]), name: "exec_command") == command)
        #expect(TranscriptToolActivity.command(in: .object(["CommandLine": .string(command)]), name: "run") == command)
        #expect(TranscriptToolActivity.path(in: .object(["file_path": .string(path)])) == path)
        #expect(TranscriptToolActivity.path(in: .object(["TargetFile": .string(path)])) == path)
        #expect(TranscriptToolActivity.path(in: .object(["AbsolutePath": .string(path)])) == path)
    }

    @Test("A tool's duration runs from its call to its result")
    func duration() {
        #expect(TranscriptToolActivity.duration(from: "2026-09-29T10:00:00Z", to: "2026-09-29T10:00:12Z") == 12)
        #expect(TranscriptToolActivity.duration(
            from: "2026-09-29T10:00:00.250Z", to: "2026-09-29T10:00:00.750Z"
        ) == 0.5)
        #expect(TranscriptToolActivity.duration(from: "", to: "2026-09-29T10:00:12Z") == nil)
        #expect(TranscriptToolActivity.duration(from: "2026-09-29T10:00:12Z", to: "2026-09-29T10:00:00Z") == nil)
        #expect(TranscriptToolActivity.durationLabel(0.42) == "0.4s")
        #expect(TranscriptToolActivity.durationLabel(12.4) == "12s")
        #expect(TranscriptToolActivity.durationLabel(185) == "3m 5s")
    }

    @Test("Raw exec input is a command, while nested JavaScript is not guessed")
    func rawInput() {
        #expect(TranscriptToolActivity.command(in: .string("ls -la"), name: "exec_command") == "ls -la")
        let wrapped = JSONElement.object(["code": .string("await tools.exec_command({cmd: 'ls'})")])
        #expect(TranscriptToolActivity.command(in: wrapped, name: "functions.exec") == nil)
        #expect(TranscriptToolActivity.command(in: .string("some text"), name: "write_file") == nil)
    }

    @Test("The header title prefers the description, then the command's first line, then the search pattern, then the file and line range")
    func headerTitle() {
        let described = TranscriptToolActivity(
            name: "Bash", input: .object(["description": .string("Build the UI package"), "command": .string("swift build")]),
            state: .finished, command: "swift build"
        )
        #expect(described.headerTitle == "Build the UI package")
        let multiline = TranscriptToolActivity(
            name: "Bash", input: .object([:]), state: .finished, command: "\n  swift build \\\n  --package-path ui"
        )
        #expect(multiline.headerTitle == "swift build \\")
        let longLine = TranscriptToolActivity(
            name: "Bash", input: .object([:]), state: .finished, command: String(repeating: "x", count: 5_000)
        )
        #expect(longLine.headerTitle.count == 200)
        let path = "/work/ui/SessionDetail.swift"
        func read(_ fields: [String: JSONElement]) -> String {
            TranscriptToolActivity(name: "Read", input: .object(fields), state: .finished, path: path).headerTitle
        }
        #expect(read(["offset": .integer(300), "limit": .integer(61)]) == "SessionDetail.swift · lines 300–360")
        #expect(read(["limit": .integer(40)]) == "SessionDetail.swift · lines 1–40")
        #expect(read(["offset": .integer(12)]) == "SessionDetail.swift · from line 12")
        #expect(read([:]) == "SessionDetail.swift")
        #expect(TranscriptToolActivity(name: "Skill", input: .object(["skill": .string("research")]), state: .finished).headerTitle == "research")
        func search(_ fields: [String: JSONElement]) -> String {
            TranscriptToolActivity(name: "Grep", input: .object(fields), state: .finished).headerTitle
        }
        #expect(search(["pattern": .string("ShellRecord"), "path": .string("/work/ui/Sources")]) == "\"ShellRecord\" in Sources")
        #expect(search(["pattern": .string("**/*.swift")]) == "\"**/*.swift\"")
        #expect(TranscriptToolActivity(name: "Unknown", input: .null, state: .waiting).headerTitle.isEmpty)
    }

    @Test("A Read range that is empty or runs past Int64 shows only its first line")
    func readRangeEdges() {
        func read(_ fields: [String: JSONElement]) -> String {
            TranscriptToolActivity(name: "Read", input: .object(fields), state: .finished, path: "/work/a.swift").headerTitle
        }
        #expect(read(["offset": .integer(2), "limit": .integer(.max)]) == "a.swift · from line 2")
        #expect(read(["offset": .integer(1), "limit": .integer(.max)]) == "a.swift · lines 1–\(Int64.max)")
        #expect(read(["limit": .integer(0)]) == "a.swift · from line 1")
        #expect(read(["offset": .integer(5), "limit": .integer(.min)]) == "a.swift · from line 5")
    }

    @Test("A tool row's text is the tool name and the header title")
    func rowTextUsesHeaderTitle() {
        let input = JSONElement.object(["pattern": .string("ShellRecord"), "path": .string("/work/ui/Sources")])
        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "grep", name: "Grep", input: input, status: .pending, meta: Meta()),
        ])
        #expect(rows.map(\.text) == ["Grep · \"ShellRecord\" in Sources"])
    }

    @Test("Diff counts add every diff's added and removed lines, and a call without diffs has none")
    func diffCounts() throws {
        let payload: [String: Any] = [
            "type": "tool_diff", "tool_call_id": "edit-1", "path": "/work/a.swift",
            "hunks": [
                ["old_start": 1, "old_lines": 2, "new_start": 1, "new_lines": 3, "lines": [" keep", "-old", "+new", "+more"]],
                ["old_start": 9, "old_lines": 1, "new_start": 10, "new_lines": 0, "lines": ["-gone"]],
            ],
        ]
        let line = String(decoding: try JSONSerialization.data(withJSONObject: payload), as: UTF8.self)
        guard case .toolDiff(let diff, _) = TranscriptEvent.decode(line: line) else {
            Issue.record("Expected structured diff")
            return
        }
        let edit = TranscriptToolActivity(name: "Edit", input: .null, diffs: [diff, diff], state: .finished)
        let counts = try #require(edit.diffCounts)
        #expect(counts.added == 4)
        #expect(counts.removed == 4)
        #expect(TranscriptToolActivity(name: "Read", input: .null, state: .finished).diffCounts == nil)
    }

    @Test("A command's exit code comes from Claude Code's first result line only")
    func exitCode() {
        func activity(_ output: String?, command: String? = "false") -> TranscriptToolActivity {
            TranscriptToolActivity(name: "Bash", input: .null, output: output, state: .failed, command: command)
        }
        #expect(activity("Exit code 1\nusage: swarm").exitCode == 1)
        #expect(activity("Exit code 127").exitCode == 127)
        #expect(activity("build ok\nExit code 1").exitCode == nil)
        #expect(activity("Exit code 1", command: nil).exitCode == nil)
        #expect(activity(nil).exitCode == nil)
    }

    @Test("A Codex exec script's command is each decoded exec_command cmd literal; a script with none stays raw")
    func codexExecCommand() {
        let inbox = #"text(await tools.exec_command({cmd:"swarm inbox","sandbox_permissions":"require_escalated","max_output_tokens":1000}));"#
        #expect(TranscriptToolActivity.command(in: .string(inbox), name: "exec") == "swarm inbox")
        let quotedKey = #"text(await tools.exec_command({"cmd":"swarm ack 7"}));"#
        #expect(TranscriptToolActivity.command(in: .string(quotedKey), name: "exec") == "swarm ack 7")
        let heredoc = #"text(await tools.exec_command({cmd:"python3 - <<'PY'\nprint(\"hi\")\nPY"}));"#
        #expect(TranscriptToolActivity.command(in: .string(heredoc), name: "exec") == "python3 - <<'PY'\nprint(\"hi\")\nPY")
        let two = "const r = await Promise.allSettled([\ntools.exec_command({cmd:\"swarm roles get council.claude\"}),\ntools.exec_command({cmd:\"swarm roles get council.gpt\"}),\n]);"
        #expect(TranscriptToolActivity.command(in: .string(two), name: "exec") == "swarm roles get council.claude\nswarm roles get council.gpt")
        let patch = #"text(await tools.apply_patch("*** Begin Patch\n*** End Patch"));"#
        #expect(TranscriptToolActivity.command(in: .string(patch), name: "exec") == patch)
        let cut = #"text(await tools.exec_command({cmd:"swarm inb"#
        #expect(TranscriptToolActivity.command(in: .string(cut), name: "exec") == cut)
        let row = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "c1", name: "exec", input: .string(inbox), status: .pending, meta: Meta()),
        ])
        #expect(row.map(\.text) == ["exec · swarm inbox"])
    }

    @Test("A Codex exec fails on Script failed or a non-zero exit_code; exit 0 shows no code")
    func codexExecFailure() {
        func activity(_ output: String) -> TranscriptToolActivity {
            TranscriptToolActivity(name: "exec", input: .string("x"), output: output, state: .finished, command: "x")
        }
        let failed = activity("Script completed\nWall time 3.6 seconds\nOutput:\n{\"chunk_id\":\"a\",\"exit_code\":1,\"output\":\"Traceback\\n\"}")
        #expect(failed.exitCode == 1)
        #expect(failed.reportsFailure)
        let ok = activity("Script completed\nWall time 3.1 seconds\nOutput:\n{\"exit_code\":0,\"output\":\"7 claude reply\\n\"}")
        #expect(ok.exitCode == nil)
        #expect(!ok.reportsFailure)
        let second = activity("Script completed\nWall time 1 seconds\nOutput:\n{\"exit_code\":0}\n{\"exit_code\":2}")
        #expect(second.exitCode == 2)
        let nested = activity("Script completed\nWall time 1 seconds\nOutput:\n{\"exit_code\":0,\"output\":\"{\\\"exit_code\\\":1}\"}")
        #expect(nested.exitCode == nil)
        let script = activity("Script failed\nWall time 10.8 seconds\nOutput:\nScript error:\nexec_command failed")
        #expect(script.exitCode == nil)
        #expect(script.reportsFailure)
        #expect(activity("Script completed\nOutput:\nexit_code: 1").exitCode == nil)

        let rows = TranscriptRowBuilder.rows(from: [
            .toolCall(toolCallID: "c1", name: "exec", input: .string("x"), status: .pending, meta: Meta()),
            .toolCallUpdate(toolCallID: "c1", status: .completed, content: failed.output ?? "", meta: Meta()),
            .toolCall(toolCallID: "c2", name: "exec", input: .string("x"), status: .pending, meta: Meta()),
            .toolCallUpdate(toolCallID: "c2", status: .completed, content: ok.output ?? "", meta: Meta()),
        ])
        #expect(rows.map(\.tool?.state) == [.failed, .finished])
    }
}
