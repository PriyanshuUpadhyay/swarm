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

private final class TranscriptProcessLifecycle: @unchecked Sendable {
    private let lock = NSLock()
    private let process: Process
    private var started = false
    private var stopped = false
    private var reaped = false

    init(binary: URL, arguments: [String], stdout: Pipe, stderr: Pipe) {
        process = Process()
        process.executableURL = binary
        process.arguments = arguments
        process.standardOutput = stdout
        process.standardError = stderr
    }

    func start() throws -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !stopped else { return false }
        try process.run()
        started = true
        return true
    }

    /// Signals the process and returns at once. `stop` runs from `deinit` and from stream
    /// termination, which can be the main thread inside a SwiftUI update. `waitUntilExit` spins
    /// the calling thread's run loop, and on the main thread that re-enters the update and
    /// AttributeGraph aborts, so only `reap` waits.
    func stop() {
        lock.lock()
        defer { lock.unlock() }
        stopped = true
        if started, !reaped, process.isRunning { process.terminate() }
    }

    /// Waits for the process to exit. Call it only from the reader task, never the main thread.
    /// The wait happens outside the lock, so a `stop` from another thread never blocks behind it.
    func reap() {
        lock.lock()
        let waiting = started && !reaped
        lock.unlock()
        guard waiting else { return }
        process.waitUntilExit()
        lock.lock()
        reaped = true
        lock.unlock()
    }

    var processIdentifier: Int32? {
        lock.lock()
        defer { lock.unlock() }
        return started && !reaped ? process.processIdentifier : nil
    }

    var terminationStatus: Int32? {
        lock.lock()
        defer { lock.unlock() }
        return reaped ? process.terminationStatus : nil
    }
}

/// Stderr bytes gathered on one GCD thread and read on another after the group wait.
private final class StderrBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var bytes = Data()

    func append(_ chunk: Data) {
        lock.lock()
        bytes.append(chunk)
        lock.unlock()
    }

    var data: Data {
        lock.lock()
        defer { lock.unlock() }
        return bytes
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
    public let stream: AsyncThrowingStream<TranscriptRecord, Error>
    private let lifecycle: TranscriptProcessLifecycle

    public var events: AsyncThrowingStream<TranscriptRecord, Error> { stream }
    public var processIdentifier: Int32? { lifecycle.processIdentifier }

    public init(binary: URL, format: String, log: URL, tail: Int? = nil, before: UInt64? = nil, follow: Bool) {
        self.binary = binary
        self.format = format
        self.log = log
        self.tail = tail
        self.follow = follow

        var arguments: [String] = ["--format", format]
        if let tail {
            arguments.append(contentsOf: ["--tail", String(tail)])
        }
        if let before {
            arguments.append(contentsOf: ["--before", String(before)])
        }
        if follow {
            arguments.append("--follow")
        }
        arguments.append(log.standardizedFileURL.path)

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let lifecycle = TranscriptProcessLifecycle(
            binary: binary, arguments: arguments, stdout: stdoutPipe, stderr: stderrPipe
        )
        self.lifecycle = lifecycle
        self.stream = AsyncThrowingStream { continuation in

            continuation.onTermination = { @Sendable _ in
                lifecycle.stop()
            }

            // The reads below block until the process writes. On the Swift concurrency pool they
            // held two of its few threads per open transcript, and a few chat switches starved
            // every other task in the app, so they run on GCD threads instead.
            let queue = DispatchQueue.global(qos: .userInitiated)
            queue.async {
                let stdoutHandle = stdoutPipe.fileHandleForReading
                let stderrHandle = stderrPipe.fileHandleForReading

                do {
                    guard try lifecycle.start() else {
                        continuation.finish()
                        return
                    }
                } catch {
                    continuation.finish(throwing: error)
                    return
                }

                // Consume stderr concurrently to prevent pipe buffer deadlocks.
                let stderrBuffer = StderrBuffer()
                let stderrDone = DispatchGroup()
                stderrDone.enter()
                queue.async {
                    while true {
                        let chunk = stderrHandle.availableData
                        if chunk.isEmpty { break }
                        stderrBuffer.append(chunk)
                    }
                    stderrDone.leave()
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
                        continuation.yield(TranscriptRecord(
                            event: TranscriptEvent.decode(line: line), rawLine: line
                        ))
                    }
                }

                if !pending.isEmpty {
                    let line = String(decoding: pending, as: UTF8.self)
                    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !trimmed.isEmpty {
                        continuation.yield(TranscriptRecord(
                            event: TranscriptEvent.decode(line: line), rawLine: line
                        ))
                    }
                }

                lifecycle.stop()
                lifecycle.reap()
                stderrDone.wait()

                if let status = lifecycle.terminationStatus, status != 0 {
                    let stderrText = String(decoding: stderrBuffer.data, as: UTF8.self)
                    continuation.finish(
                        throwing: TranscriptToolError.processExited(
                            status: status, stderr: stderrText
                        )
                    )
                } else {
                    continuation.finish()
                }
            }
        }
    }

    deinit { stop() }

    public func stop() {
        lifecycle.stop()
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
