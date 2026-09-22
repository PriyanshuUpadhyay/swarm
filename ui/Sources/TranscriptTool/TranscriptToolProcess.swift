import Foundation

public enum TranscriptToolError: Error, Sendable, CustomStringConvertible {
    case processExited(status: Int32, stderr: String)

    public var description: String {
        switch self {
        case .processExited(let status, let stderr):
            "transcript process exited with code \(status): \(stderr)"
        }
    }
}

/// Spawns and supervises the Zig transcript CLI process.
///
/// The process runs long-lived with `--follow` to stream transcript updates as the CLI writes
/// them. When the stream task is cancelled, the underlying process is terminated immediately.
public final class TranscriptToolProcess: Sendable {
    public let binary: URL
    public let format: String
    public let log: URL
    public let tail: Int?
    public let follow: Bool
    public let stream: AsyncThrowingStream<TranscriptEvent, Error>

    public var events: AsyncThrowingStream<TranscriptEvent, Error> { stream }

    public init(binary: URL, format: String, log: URL, tail: Int? = nil, follow: Bool) {
        self.binary = binary
        self.format = format
        self.log = log
        self.tail = tail
        self.follow = follow

        var arguments: [String] = ["--format", format]
        if let tail {
            arguments.append(contentsOf: ["--tail", String(tail)])
        }
        if follow {
            arguments.append("--follow")
        }
        arguments.append(log.standardizedFileURL.path)

        self.stream = AsyncThrowingStream { continuation in
            let process = Process()
            process.executableURL = binary
            process.arguments = arguments

            let stdoutPipe = Pipe()
            let stderrPipe = Pipe()
            process.standardOutput = stdoutPipe
            process.standardError = stderrPipe

            continuation.onTermination = { @Sendable _ in
                if process.isRunning {
                    process.terminate()
                }
            }

            Task {
                let stdoutHandle = stdoutPipe.fileHandleForReading
                let stderrHandle = stderrPipe.fileHandleForReading

                // Consume stderr concurrently to prevent pipe buffer deadlocks.
                let stderrTask = Task {
                    var buffer = Data()
                    while true {
                        let chunk = stderrHandle.availableData
                        if chunk.isEmpty { break }
                        buffer.append(chunk)
                    }
                    return buffer
                }

                do {
                    try process.run()
                } catch {
                    continuation.finish(throwing: error)
                    return
                }

                var pending = Data()
                while true {
                    let chunk = stdoutHandle.availableData
                    if chunk.isEmpty { break }
                    pending.append(chunk)
                    while let newlineIndex = pending.firstIndex(of: 0x0a) {
                        let lineData = pending[pending.startIndex..<newlineIndex]
                        pending = Data(pending[pending.index(after: newlineIndex)...])
                        let line = String(decoding: lineData, as: UTF8.self)
                        continuation.yield(TranscriptEvent.decode(line: line))
                    }
                }

                if !pending.isEmpty {
                    let line = String(decoding: pending, as: UTF8.self)
                    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !trimmed.isEmpty {
                        continuation.yield(TranscriptEvent.decode(line: line))
                    }
                }

                process.waitUntilExit()
                let stderrBuffer = await stderrTask.value

                if process.terminationStatus != 0 {
                    let stderrText = String(decoding: stderrBuffer, as: UTF8.self)
                    continuation.finish(
                        throwing: TranscriptToolError.processExited(
                            status: process.terminationStatus, stderr: stderrText
                        )
                    )
                } else {
                    continuation.finish()
                }
            }
        }
    }

    /// Locates the bundled transcript binary or the test override from the environment.
    public static var bundled: URL? {
        if let executableURL = Bundle.main.executableURL {
            let candidate = executableURL.deletingLastPathComponent().appendingPathComponent("transcript")
            if FileManager.default.fileExists(atPath: candidate.path) {
                return candidate
            }
        }
        if let env = ProcessInfo.processInfo.environment["SWARM_TRANSCRIPT_TOOL"], !env.isEmpty {
            let candidate = URL(fileURLWithPath: env)
            if FileManager.default.fileExists(atPath: candidate.path) {
                return candidate
            }
        }
        let checkoutTool = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/transcript/zig-out/bin/transcript")
        if FileManager.default.fileExists(atPath: checkoutTool.path) { return checkoutTool }
        return nil
    }
}
