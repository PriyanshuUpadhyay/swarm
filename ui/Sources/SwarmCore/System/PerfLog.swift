import Darwin
import Foundation

/// A small, bounded record of the work that can make the app feel slow.
///
/// Calls only enqueue a value. Directory creation, encoding and file I/O all run on this type's
/// private queue, so adding a diagnostic can never extend the work being measured.
public final class PerfLog: @unchecked Sendable {
    public enum Event: Sendable {
        case stall(milliseconds: Double, paneKind: String, drawnRowCount: Int)
        case chatRead(milliseconds: Double, messageCount: Int, rowCount: Int)
        case busRead(milliseconds: Double, sessionCount: Int)
        case processCapture(milliseconds: Double, executable: String)
        case heartbeat(residentMemoryBytes: UInt64, uptimeSeconds: Double)
    }

    /// The defaults key the General settings toggle writes, and the only thing that turns any of
    /// this on.
    ///
    /// **Off by default, because the log is a cost of its own.** It is a diagnostic, so it is
    /// switched on for the run that is being diagnosed and switched off again afterwards, rather
    /// than kept because it might one day be read.
    public static let enabledKey = "recordsPerformanceLog"

    public static let shared = PerfLog(
        directory: Store.defaultDirectory.appendingPathComponent("diagnostics", isDirectory: true),
        isRecording: { UserDefaults.standard.bool(forKey: PerfLog.enabledKey) }
    )

    private static let filePrefix = "perf-"
    private static let fileSuffix = ".jsonl"
    private static let retentionDays = 7
    private static let lineLimit = 2_000
    private static let paneKinds: Set<String> = [
        "browser", "chat", "home", "notes", "review", "swarmAgent", "terminal", "unknown",
    ]

    private let directory: URL
    private let queue = DispatchQueue(label: "swarm.performance-log", qos: .utility)
    private let now: @Sendable () -> Date
    private let calendar: Calendar
    private let processStarted = ContinuousClock.now
    private let encoder: JSONEncoder
    private let isoFormatter: ISO8601DateFormatter
    private let dayFormatter: DateFormatter
    private var cleanupTimer: DispatchSourceTimer?
    private var heartbeatTimer: DispatchSourceTimer?
    private var started = false
    private var hourStart: Date?
    private var writtenThisHour = 0
    private var droppedThisHour = 0
    private var dropFlushHour: Date?
    private let isRecording: @Sendable () -> Bool

    /// - Parameter isRecording: asked on every event rather than read once, so the switch takes
    ///   effect on the pass after it is thrown rather than on the next launch. Defaults to on for
    ///   a caller that made its own log, which is every test: only `shared` reads the setting.
    init(
        directory: URL, now: @escaping @Sendable () -> Date = Date.init,
        calendar: Calendar = .autoupdatingCurrent,
        isRecording: @escaping @Sendable () -> Bool = { true }
    ) {
        self.directory = directory
        self.now = now
        self.calendar = calendar
        self.isRecording = isRecording
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        self.encoder = encoder
        let isoFormatter = ISO8601DateFormatter()
        isoFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        self.isoFormatter = isoFormatter
        let dayFormatter = DateFormatter()
        dayFormatter.calendar = calendar
        dayFormatter.locale = Locale(identifier: "en_US_POSIX")
        dayFormatter.timeZone = calendar.timeZone
        dayFormatter.dateFormat = "yyyy-MM-dd"
        self.dayFormatter = dayFormatter
    }

    /// Starts retention and heartbeat work. The app calls this during launch, and again when the
    /// setting is switched on, which is why it does nothing rather than remembering it was asked.
    public func start() {
        guard isRecording() else { return }
        queue.async { [self] in
            guard !started else { return }
            started = true
            prepareDirectory()
            removeExpiredFiles(at: now())

            let cleanup = DispatchSource.makeTimerSource(queue: queue)
            cleanup.schedule(deadline: .now() + .seconds(86_400), repeating: .seconds(86_400))
            cleanup.setEventHandler { [weak self] in
                guard let self else { return }
                self.removeExpiredFiles(at: self.now())
            }
            cleanup.resume()
            cleanupTimer = cleanup

            let heartbeat = DispatchSource.makeTimerSource(queue: queue)
            heartbeat.schedule(deadline: .now() + .seconds(60), repeating: .seconds(60))
            heartbeat.setEventHandler { [weak self] in self?.writeHeartbeat() }
            heartbeat.resume()
            heartbeatTimer = heartbeat
        }
    }

    /// Where the log is written, for the settings pane that offers to reveal it.
    public var directoryPath: String { directory.path }

    /// Enqueues one event and returns before any encoding or file I/O starts.
    ///
    /// Nothing is enqueued while the setting is off, so a call site costs one `UserDefaults` read
    /// and can stay where it is rather than being wrapped by every caller.
    public func record(_ event: Event) {
        guard isRecording() else { return }
        queue.async { [self] in write(event, at: now()) }
    }

    /// Deletes a dated performance file only when its name proves it is past the retention window.
    static func shouldDelete(fileName: String, today: Date, calendar: Calendar) -> Bool {
        guard fileName.hasPrefix(filePrefix), fileName.hasSuffix(fileSuffix) else { return false }
        let start = fileName.index(fileName.startIndex, offsetBy: filePrefix.count)
        let end = fileName.index(fileName.endIndex, offsetBy: -fileSuffix.count)
        let dateText = String(fileName[start..<end])
        guard dateText.count == 10 else { return false }

        let formatter = DateFormatter()
        formatter.calendar = calendar
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "yyyy-MM-dd"
        guard let fileDay = formatter.date(from: dateText),
              let cutoff = calendar.date(byAdding: .day, value: -retentionDays, to: calendar.startOfDay(for: today))
        else { return false }
        return fileDay < cutoff
    }

    private func write(_ event: Event, at date: Date) {
        guard let eventHour = calendar.dateInterval(of: .hour, for: date)?.start else { return }
        if hourStart != eventHour {
            flushDropped()
            hourStart = eventHour
            writtenThisHour = 0
            droppedThisHour = 0
            dropFlushHour = nil
        }

        // One line stays available for the dropped count. This keeps the hard bound true even
        // though whether an hour overflows cannot be known when its 2,000th event arrives.
        guard writtenThisHour < Self.lineLimit - 1 else {
            droppedThisHour += 1
            scheduleDropFlush(for: eventHour)
            return
        }
        append(event, at: date)
        writtenThisHour += 1
    }

    private func scheduleDropFlush(for eventHour: Date) {
        guard dropFlushHour != eventHour,
              let end = calendar.date(byAdding: .hour, value: 1, to: eventHour)
        else { return }
        dropFlushHour = eventHour
        queue.asyncAfter(deadline: .now() + max(0, end.timeIntervalSince(now()))) { [weak self] in
            guard let self, self.hourStart == eventHour else { return }
            self.flushDropped()
            self.hourStart = nil
            self.writtenThisHour = 0
            self.droppedThisHour = 0
            self.dropFlushHour = nil
        }
    }

    private func flushDropped() {
        guard droppedThisHour > 0, let hourStart,
              let end = calendar.date(byAdding: .hour, value: 1, to: hourStart)
        else { return }
        let entry = Entry(
            at: isoFormatter.string(from: end.addingTimeInterval(-0.001)),
            kind: "dropped", ms: nil,
            detail: ["count": .integer(droppedThisHour)]
        )
        append(entry, fileDate: end.addingTimeInterval(-0.001))
        writtenThisHour += 1
    }

    private func append(_ event: Event, at date: Date) {
        append(entry(for: event, at: date), fileDate: date)
    }

    private func append(_ entry: Entry, fileDate: Date) {
        prepareDirectory()
        guard var data = try? encoder.encode(entry) else { return }
        data.append(0x0A)
        let file = directory.appendingPathComponent(
            Self.filePrefix + dayFormatter.string(from: fileDate) + Self.fileSuffix
        )
        if !FileManager.default.fileExists(atPath: file.path) {
            _ = FileManager.default.createFile(atPath: file.path, contents: nil)
        }
        guard let handle = try? FileHandle(forWritingTo: file) else { return }
        defer { try? handle.close() }
        do {
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
        } catch {
            // Diagnostics must not affect the app path they observe.
        }
    }

    private func entry(for event: Event, at date: Date) -> Entry {
        let at = isoFormatter.string(from: date)
        switch event {
        case .stall(let milliseconds, let paneKind, let drawnRowCount):
            return Entry(
                at: at, kind: "stall", ms: milliseconds,
                detail: [
                    "paneKind": .string(Self.paneKinds.contains(paneKind) ? paneKind : "unknown"),
                    "drawnRowCount": .integer(drawnRowCount),
                ]
            )
        case .chatRead(let milliseconds, let messageCount, let rowCount):
            return Entry(
                at: at, kind: "chatRead", ms: milliseconds,
                detail: [
                    "messageCount": .integer(messageCount),
                    "rowCount": .integer(rowCount),
                ]
            )
        case .busRead(let milliseconds, let sessionCount):
            return Entry(
                at: at, kind: "busRead", ms: milliseconds,
                detail: ["sessionCount": .integer(sessionCount)]
            )
        case .processCapture(let milliseconds, let executable):
            return Entry(
                at: at, kind: "processCapture", ms: milliseconds,
                detail: [
                    "executable": .string(Self.short((executable as NSString).lastPathComponent))
                ]
            )
        case .heartbeat(let residentMemoryBytes, let uptimeSeconds):
            return Entry(
                at: at, kind: "heartbeat", ms: nil,
                detail: [
                    "residentMemoryBytes": .unsigned(residentMemoryBytes),
                    "uptimeSeconds": .number(uptimeSeconds),
                ]
            )
        }
    }

    private static func short(_ value: String) -> String {
        String(value.replacingOccurrences(of: "\n", with: " ").prefix(80))
    }

    private func prepareDirectory() {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    private func removeExpiredFiles(at date: Date) {
        guard let files = try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: nil
        ) else { return }
        for file in files where Self.shouldDelete(
            fileName: file.lastPathComponent, today: date, calendar: calendar
        ) {
            try? FileManager.default.removeItem(at: file)
        }
    }

    private func writeHeartbeat() {
        guard isRecording() else { return }
        let duration = processStarted.duration(to: .now).components
        let uptime = Double(duration.seconds) + Double(duration.attoseconds) / 1e18
        write(.heartbeat(
            residentMemoryBytes: Self.residentMemoryBytes(), uptimeSeconds: uptime
        ), at: now())
    }

    private static func residentMemoryBytes() -> UInt64 {
        var info = mach_task_basic_info()
        var count = mach_msg_type_number_t(
            MemoryLayout<mach_task_basic_info>.size / MemoryLayout<natural_t>.size
        )
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
            }
        }
        return result == KERN_SUCCESS ? UInt64(info.resident_size) : 0
    }

    // Test seams are synchronous only after the public caller has returned.
    func flushForTesting() { queue.sync {} }

    func finishHourForTesting() { queue.sync { flushDropped() } }

    func entriesForTesting() -> [[String: Any]] {
        queue.sync {
            guard let files = try? FileManager.default.contentsOfDirectory(
                at: directory, includingPropertiesForKeys: nil
            ) else { return [] }
            return files.sorted { $0.lastPathComponent < $1.lastPathComponent }.flatMap { file -> [[String: Any]] in
                guard let text = try? String(contentsOf: file, encoding: .utf8) else { return [] }
                return text.split(separator: "\n").compactMap { line in
                    guard let data = String(line).data(using: .utf8) else { return nil }
                    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
                }
            }
        }
    }
}

private extension PerfLog {
    struct Entry: Encodable {
        var at: String
        var kind: String
        var ms: Double?
        var detail: [String: DetailValue]
    }

    enum DetailValue: Encodable {
        case string(String)
        case integer(Int)
        case unsigned(UInt64)
        case number(Double)

        func encode(to encoder: any Encoder) throws {
            var value = encoder.singleValueContainer()
            switch self {
            case .string(let string): try value.encode(string)
            case .integer(let integer): try value.encode(integer)
            case .unsigned(let integer): try value.encode(integer)
            case .number(let number): try value.encode(number)
            }
        }
    }
}
