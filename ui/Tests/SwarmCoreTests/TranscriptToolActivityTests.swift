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
}
