import Testing
@testable import SwarmCore

@Suite("Shell record")
struct ShellRecordTests {
    @Test("The input command is the trimmed text inside bash-input")
    func inputCommand() {
        #expect(ShellRecord.command(fromInput: "<bash-input> git status\n</bash-input>") == "git status")
        #expect(ShellRecord.command(fromInput: "git status") == nil)
    }

    @Test("Stdout alone becomes the output")
    func stdoutOnly() {
        let run = ShellRecord.run(command: "ls", outputText: "<bash-stdout>README.md</bash-stdout><bash-stderr></bash-stderr>")
        #expect(run == TranscriptShellRun(command: "ls", output: "README.md", exitCode: nil))
    }

    @Test("Stderr alone becomes the output")
    func stderrOnly() {
        let run = ShellRecord.run(command: "cargo build", outputText: "<bash-stdout></bash-stdout><bash-stderr>Compiling swarm</bash-stderr>")
        #expect(run.output == "Compiling swarm")
    }

    @Test("Stdout and stderr join on a new line")
    func bothJoined() {
        let text = "<bash-stdout>built</bash-stdout><bash-stderr>warning: unused</bash-stderr>"
        #expect(ShellRecord.run(command: nil, outputText: text).output == "built\nwarning: unused")
        let endsInNewline = "<bash-stdout>built\n</bash-stdout><bash-stderr>warning: unused\n</bash-stderr>"
        #expect(ShellRecord.run(command: nil, outputText: endsInNewline).output == "built\nwarning: unused")
    }

    @Test("The no-output placeholder gives empty output")
    func placeholderDropped() {
        let text = "<bash-stdout>(Bash completed with no output)</bash-stdout><bash-stderr></bash-stderr>"
        #expect(ShellRecord.run(command: "true", outputText: text).output == "")
    }

    @Test("A missing output record gives empty output")
    func missingOutputRecord() {
        #expect(ShellRecord.run(command: "true", outputText: nil) == TranscriptShellRun(command: "true", output: "", exitCode: nil))
    }

    @Test("The exit code is read when present and nil when absent")
    func exitCode() {
        let failed = "<bash-stdout></bash-stdout><bash-stderr>no such file</bash-stderr><bash-exit-code>2</bash-exit-code>"
        #expect(ShellRecord.run(command: nil, outputText: failed).exitCode == 2)
        #expect(ShellRecord.run(command: nil, outputText: "<bash-stdout>ok</bash-stdout>").exitCode == nil)
    }

    @Test("Entities decode exactly once")
    func entitiesDecodeOnce() {
        #expect(ShellRecord.clean("a &amp;&amp; b") == "a && b")
        #expect(ShellRecord.clean("&amp;lt;") == "&lt;")
        #expect(ShellRecord.clean("&lt;div&gt;") == "<div>")
    }

    @Test("Text inside persisted-output keeps its entities")
    func persistedOutputKept() {
        let text = "&amp; <persisted-output>a &amp; b</persisted-output> &amp;"
        #expect(ShellRecord.clean(text) == "& <persisted-output>a &amp; b</persisted-output> &")
    }

    @Test("CSI color codes are removed")
    func colorCodesRemoved() {
        #expect(ShellRecord.clean("\u{1B}[31mred\u{1B}[0m") == "red")
    }

    @Test("OSC hyperlinks are removed")
    func hyperlinkRemoved() {
        #expect(ShellRecord.clean("\u{1B}]8;;https://x\u{07}link\u{1B}]8;;\u{07}") == "link")
        #expect(ShellRecord.clean("\u{1B}]0;title\u{1B}\\text") == "text")
    }

    @Test("C0 controls are removed but new lines and tabs stay")
    func controlsRemoved() {
        #expect(ShellRecord.clean("one\u{07}\u{08}\n\ttwo\u{00}") == "one\n\ttwo")
    }

    @Test("CRLF becomes a new line and a lone carriage return goes")
    func carriageReturns() {
        #expect(ShellRecord.clean("first\r\nsecond\rthird") == "first\nsecondthird")
    }
}
