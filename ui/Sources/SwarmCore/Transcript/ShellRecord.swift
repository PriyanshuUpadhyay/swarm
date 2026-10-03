import Foundation

/// One `!` shell command and its output, as Claude Code writes them into the session log.
public struct TranscriptShellRun: Hashable, Sendable {
    /// Nil when the input record is outside the loaded window.
    public var command: String?
    /// Stdout, then stderr, cleaned for display.
    public var output: String
    public var exitCode: Int?

    public init(command: String?, output: String, exitCode: Int?) {
        self.command = command
        self.output = output
        self.exitCode = exitCode
    }
}

/// Reads the `<bash-input>` and `<bash-stdout>`/`<bash-stderr>` text of Claude Code shell records.
public enum ShellRecord {
    public static func command(fromInput text: String) -> String? {
        tagged("bash-input", in: text)?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func run(command: String?, outputText: String?) -> TranscriptShellRun {
        let text = outputText ?? ""
        var stdout = clean(tagged("bash-stdout", in: text) ?? "")
        if stdout == "(Bash completed with no output)" { stdout = "" }
        let stderr = clean(tagged("bash-stderr", in: text) ?? "")
        let output = [stdout, stderr]
            .map { String($0.reversed().drop(while: \.isNewline).reversed()) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
        let exitCode = tagged("bash-exit-code", in: text)
            .flatMap { Int($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
        return TranscriptShellRun(command: command, output: output, exitCode: exitCode)
    }

    /// Decodes the HTML entities Claude Code adds and removes terminal escape and control codes.
    public static func clean(_ text: String) -> String {
        removingControls(decodingEntities(text))
    }

    /// Claude Code escapes `<`, `>`, and `&` once in stdout, but not inside `<persisted-output>`.
    private static func decodingEntities(_ text: String) -> String {
        let open = "<persisted-output>", close = "</persisted-output>"
        var result = ""
        var rest = text[...]
        while let start = rest.range(of: open) {
            result += decode(rest[..<start.lowerBound])
            let end = rest.range(of: close, range: start.upperBound..<rest.endIndex)?.upperBound ?? rest.endIndex
            result += rest[start.lowerBound..<end]
            rest = rest[end...]
        }
        return result + decode(rest)
    }

    /// `&amp;` goes last, so `&amp;lt;` becomes `&lt;` and not `<`.
    private static func decode(_ text: Substring) -> String {
        text.replacingOccurrences(of: "&lt;", with: "<")
            .replacingOccurrences(of: "&gt;", with: ">")
            .replacingOccurrences(of: "&amp;", with: "&")
    }

    /// Removes CSI and OSC sequences, other two-byte ESC sequences, and C0 controls except `\n` and `\t`.
    private static func removingControls(_ text: String) -> String {
        let scalars = Array(text.replacingOccurrences(of: "\r\n", with: "\n").unicodeScalars)
        var kept = String.UnicodeScalarView()
        var index = 0
        while index < scalars.count {
            let scalar = scalars[index]
            index += 1
            if scalar == "\u{1B}", index < scalars.count {
                let kind = scalars[index]
                index += 1
                if kind == "[" {
                    // Parameter and intermediate bytes, then one final byte.
                    while index < scalars.count, (0x20...0x3F).contains(scalars[index].value) { index += 1 }
                    if index < scalars.count, (0x40...0x7E).contains(scalars[index].value) { index += 1 }
                } else if kind == "]" {
                    // Ends at BEL or at ESC `\`.
                    while index < scalars.count, scalars[index] != "\u{07}", scalars[index] != "\u{1B}" { index += 1 }
                    if index < scalars.count, scalars[index] == "\u{1B}" { index += 1 }
                    index += 1
                }
                continue
            }
            if scalar.value < 0x20, scalar != "\n", scalar != "\t" { continue }
            kept.append(scalar)
        }
        return String(kept)
    }

    static func tagged(_ name: String, in text: String) -> String? {
        guard let open = text.range(of: "<\(name)>"),
              let close = text.range(of: "</\(name)>", range: open.upperBound..<text.endIndex)
        else { return nil }
        return String(text[open.upperBound..<close.lowerBound])
    }
}
