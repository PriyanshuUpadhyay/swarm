import Foundation
import TranscriptTool

/// Protocol representing an actor capable of reading and streaming transcript events.
public protocol TranscriptReading: Actor {
    func read() async throws -> [TranscriptEvent]
    func readIfChanged() async throws -> [TranscriptEvent]?
}

/// Reads interactive CLI transcripts via the external Zig transcript subprocess.
///
/// Spawns `transcript --follow --tail <messageLimit>` on initial read and streams new events
/// directly into the transcript message list.
public actor ToolTranscriptReader: TranscriptReading {
    public static let messageLimit = 500

    private let binary: URL
    private let format: String
    private let log: URL
    private let limit: Int

    private var process: TranscriptToolProcess?
    private var streamTask: Task<Void, Never>?
    private var catchUpTimer: Task<Void, Never>?

    private var events: [TranscriptEvent] = []
    private var messageStart = 0

    private var appendedThisRead = 0
    private var rebuiltThisRead = false
    private var hasStarted = false
    private var startContinuation: CheckedContinuation<Void, Never>?
    private var isCatchingUp = true

    public init(
        binary: URL,
        format: String,
        log: URL,
        messageLimit: Int = 500
    ) {
        self.binary = binary
        self.format = format
        self.log = log
        self.limit = max(1, messageLimit)
    }

    deinit {
        streamTask?.cancel()
        catchUpTimer?.cancel()
        process?.stop()
    }

    private func scheduleCatchUpCompletion(delayMilliseconds: UInt64 = 80) {
        catchUpTimer?.cancel()
        catchUpTimer = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(delayMilliseconds))
            guard !Task.isCancelled else { return }
            await self?.finishInitialCatchUp()
        }
    }

    private func startIfNeeded() async {
        guard !hasStarted else { return }
        hasStarted = true
        rebuiltThisRead = true

        let proc = TranscriptToolProcess(
            binary: binary,
            format: format,
            log: log,
            tail: limit,
            follow: true
        )
        self.process = proc

        streamTask = Task { [weak self] in
            await withTaskCancellationHandler {
                do {
                    for try await event in proc.stream {
                        guard !Task.isCancelled else { break }
                        await self?.ingest(event)
                    }
                } catch {
                    // Stream ended or process failed.
                }
                await self?.finishInitialCatchUp()
            } onCancel: {
                proc.stop()
            }
        }

        // Wait for initial events from --tail to settle into messages.
        // Fall back after 3 seconds so an unresponsive or missing tool does not stall.
        await withCheckedContinuation { continuation in
            self.startContinuation = continuation
            scheduleCatchUpCompletion(delayMilliseconds: 3000)
        }
    }

    private func finishInitialCatchUp() {
        catchUpTimer?.cancel()
        catchUpTimer = nil
        isCatchingUp = false
        if let cont = startContinuation {
            startContinuation = nil
            cont.resume()
        }
    }

    private func ingest(_ event: TranscriptEvent) {
        if isCatchingUp {
            if case .page(let start, let end) = event, start == end {
                finishInitialCatchUp()
                return
            }
            scheduleCatchUpCompletion(delayMilliseconds: 80)
        }

        if case .page = event {
            return
        }

        events.append(event)
        appendedThisRead += 1
        trim()
    }

    private func trim() {
        var trimmed = false
        while events.count - messageStart > limit {
            messageStart += 1
            trimmed = true
        }
        if messageStart > 1024, messageStart * 2 > events.count {
            events.removeFirst(messageStart)
            messageStart = 0
        }
        if trimmed {
            rebuiltThisRead = true
        }
    }

    public func read() async throws -> [TranscriptEvent] {
        await startIfNeeded()
        let visible = Array(events.dropFirst(messageStart))
        appendedThisRead = 0
        rebuiltThisRead = false
        return visible
    }

    public func readIfChanged() async throws -> [TranscriptEvent]? {
        await startIfNeeded()
        guard appendedThisRead > 0 || rebuiltThisRead else { return nil }
        let visible = Array(events.dropFirst(messageStart))
        appendedThisRead = 0
        rebuiltThisRead = false
        return visible
    }

    func processIdentifier() -> Int32? {
        process?.processIdentifier
    }
}
