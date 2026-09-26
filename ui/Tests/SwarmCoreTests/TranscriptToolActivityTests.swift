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

    @Test("Raw exec input is a command, while nested JavaScript is not guessed")
    func rawInput() {
        #expect(TranscriptToolActivity.command(in: .string("ls -la"), name: "exec_command") == "ls -la")
        let wrapped = JSONElement.object(["code": .string("await tools.exec_command({cmd: 'ls'})")])
        #expect(TranscriptToolActivity.command(in: wrapped, name: "functions.exec") == nil)
        #expect(TranscriptToolActivity.command(in: .string("some text"), name: "write_file") == nil)
    }
}
