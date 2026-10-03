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
}
