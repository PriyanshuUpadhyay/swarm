import Foundation
import TranscriptTool

/// Protocol representing an actor capable of reading and streaming transcript events.
public protocol TranscriptReading: Actor {
    func read() async throws -> [TranscriptRecord]
    func readIfChanged() async throws -> [TranscriptRecord]?
}

enum TranscriptReaderError: Error, CustomStringConvertible {
    case streamEnded(String)

    var description: String {
        switch self {
        case .streamEnded(let reason): "Transcript reader stopped: \(reason)"
        }
    }
}

/// Reads interactive CLI transcripts via the external Zig transcript subprocess.
///
/// Spawns `transcript --follow --tail <messageLimit>` on initial read and streams new events
/// directly into the transcript message list. The limit counts source log lines, not emitted
/// events; all events in each line are retained. Loaded pages stay until the reader is released.
public actor ToolTranscriptReader: TranscriptReading {
    public static let messageLimit = 100

    private let binary: URL
    private let format: String
    private let log: URL
    private let limit: Int

    private var process: TranscriptToolProcess?
    private var streamTask: Task<Void, Never>?
    private var catchUpTimer: Task<Void, Never>?

    private var records: [TranscriptRecord] = []
    public private(set) var usage = ChatUsage()
    public private(set) var historyStartIndex = 0
    public private(set) var olderOffset: UInt64?
    private var loadingOlder = false
    public var hasOlder: Bool { (olderOffset ?? 0) > 0 }

    private var appendedThisRead = 0
    private var rebuiltThisRead = false
    private var hasStarted = false
    private var streamFailure: String?
    private var startContinuation: CheckedContinuation<Void, Never>?
    private var isCatchingUp = true

    public init(
        binary: URL,
        format: String,
        log: URL,
        messageLimit: Int = ToolTranscriptReader.messageLimit
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
                var failure: String?
                do {
                    for try await record in proc.stream {
                        guard !Task.isCancelled else { break }
                        await self?.ingest(record)
                    }
                } catch {
                    failure = String(describing: error)
                }
                await self?.streamDidEnd(failure)
            } onCancel: {
                proc.stop()
            }
        }

        // Wait for initial events from --tail to settle into messages.
        // Fall back after 3 seconds so an unresponsive or missing tool does not stall.
        let timing = SwarmPerformance.begin("TranscriptCatchUp")
        await withCheckedContinuation { continuation in
            self.startContinuation = continuation
            scheduleCatchUpCompletion(delayMilliseconds: 3000)
        }
        timing.end(count: records.count)
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

    private func streamDidEnd(_ failure: String?) {
        streamFailure = failure ?? "the process exited"
        finishInitialCatchUp()
    }

    private func ingest(_ record: TranscriptRecord) {
        if isCatchingUp {
            if case .page(let start, let end) = record.event, start == end {
                finishInitialCatchUp()
                return
            }
            scheduleCatchUpCompletion(delayMilliseconds: 80)
        }

        if case .page(let start, let end) = record.event {
            if start != end { olderOffset = start }
            return
        }

        usage.ingest(record.event)
        records.append(record)
        appendedThisRead += 1
    }

    /// Fetch the preceding source window without restarting the live stream.
    public func loadOlder() async throws -> [TranscriptRecord] {
        await startIfNeeded()
        guard !loadingOlder, let before = olderOffset, before > 0 else { return records }
        loadingOlder = true
        defer { loadingOlder = false }
        let page = TranscriptToolProcess(
            binary: binary, format: format, log: log, tail: limit, before: before, follow: false
        )
        var older: [TranscriptRecord] = []
        var nextOffset: UInt64?
        try await withTaskCancellationHandler {
            for try await record in page.stream {
                try Task.checkCancellation()
                if case .page(let start, let end) = record.event {
                    guard end == before, start < before else {
                        throw TranscriptReaderError.streamEnded("the history file changed; reopen this chat")
                    }
                    nextOffset = start
                } else {
                    older.append(record)
                }
            }
        } onCancel: {
            page.stop()
        }
        guard let nextOffset else {
            throw TranscriptReaderError.streamEnded("the history page has no cursor")
        }
        // Live events may arrive while the page is read. Prepend to the current array.
        records.insert(contentsOf: older, at: 0)
        historyStartIndex -= older.count
        olderOffset = nextOffset
        usage = ChatUsage()
        for record in records { usage.ingest(record.event) }
        rebuiltThisRead = true
        return records
    }

    public func read() async throws -> [TranscriptRecord] {
        await startIfNeeded()
        if records.isEmpty, let streamFailure {
            throw TranscriptReaderError.streamEnded(streamFailure)
        }
        let visible = records
        appendedThisRead = 0
        rebuiltThisRead = false
        return visible
    }

    public func readIfChanged() async throws -> [TranscriptRecord]? {
        await startIfNeeded()
        if records.isEmpty, let streamFailure {
            throw TranscriptReaderError.streamEnded(streamFailure)
        }
        guard appendedThisRead > 0 || rebuiltThisRead else {
            if let streamFailure { throw TranscriptReaderError.streamEnded(streamFailure) }
            return nil
        }
        let visible = records
        appendedThisRead = 0
        rebuiltThisRead = false
        return visible
    }

    func window() -> (records: [TranscriptRecord], indexOffset: Int, hasOlder: Bool, usage: ChatUsage) {
        (records, historyStartIndex, hasOlder, usage)
    }

    func processIdentifier() -> Int32? {
        process?.processIdentifier
    }
}
