import Foundation

/// Finds a provider log by its chair id or session start time.
public enum ChairLogDiscovery {
    private static let firstRecordCache = FirstRecordCache()

    public static func path(
        provider: String, chairID: String?, cwd: String, createdAt: Int,
        homes: [URL]
    ) -> URL? {
        let timing = SwarmPerformance.begin("LogDiscovery")
        var candidatesSeen = 0
        defer { timing.end(count: candidatesSeen) }
        let manager = FileManager.default
        var seen: Set<String> = []
        let roots = homes.map(\.standardizedFileURL).filter { seen.insert($0.path).inserted }

        if let chairID {
            for home in roots {
                if let match = candidates(provider: provider, home: home, manager: manager)
                    .sorted(by: { $0.path < $1.path })
                    .first(where: { candidate in
                        candidatesSeen += 1
                        return switch provider {
                        case "claude": candidate.lastPathComponent == "\(chairID).jsonl"
                        case "codex": candidate.lastPathComponent.hasSuffix("-\(chairID).jsonl")
                        default: false
                        }
                    }) {
                    return match
                }
            }
            return nil
        }

        var closest: (path: URL, distance: TimeInterval)?
        let formatter = ISO8601DateFormatter()
        let codexWindow = provider == "codex" ? codexFileWindow(createdAt: createdAt) : nil

        for home in roots {
            for candidate in candidates(provider: provider, home: home, manager: manager) {
                if let codexWindow, !codexWindow(candidate) { continue }
                candidatesSeen += 1
                guard let record = firstRecord(in: candidate, formatter: formatter),
                      record.cwd == URL(fileURLWithPath: cwd).standardizedFileURL.path else { continue }
                let delay = record.date.timeIntervalSince1970 - TimeInterval(createdAt)
                let distance = abs(delay)
                guard distance <= 10 * 60 else { continue }
                if closest == nil || distance < closest!.distance
                    || (distance == closest!.distance && candidate.path > closest!.path.path) {
                    closest = (candidate, distance)
                }
            }
        }
        return closest?.path
    }

    private static func codexFileWindow(createdAt: Int) -> (URL) -> Bool {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd'T'HH-mm-ss"
        // File names can use a local clock; allow every time zone around the record's 10-minute window.
        let margin = 26 * 60 * 60
        let lower = formatter.string(from: Date(timeIntervalSince1970: TimeInterval(createdAt - margin)))
        let upper = formatter.string(from: Date(timeIntervalSince1970: TimeInterval(createdAt + margin)))
        return { candidate in
            let name = candidate.lastPathComponent
            let stamp = String(name.dropFirst("rollout-".count).prefix(19))
            // Older or custom rollout names have no date stamp, so inspect those as before.
            guard stamp.count == 19, stamp[stamp.index(stamp.startIndex, offsetBy: 10)] == "T" else {
                return true
            }
            return stamp >= lower && stamp <= upper
        }
    }

    static func homes(provider: String, accountHomes: [String], userHome: URL) -> [URL] {
        accountHomes.map(URL.init(fileURLWithPath:)) + [
            userHome.appendingPathComponent(provider == "codex" ? ".codex" : ".claude")
        ]
    }

    private static func candidates(
        provider: String, home: URL, manager: FileManager
    ) -> [URL] {
        switch provider {
        case "claude":
            let projects = home.appendingPathComponent("projects", isDirectory: true)
            let directories = (try? manager.contentsOfDirectory(
                at: projects, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
            )) ?? []
            return directories.flatMap { directory in
                (try? manager.contentsOfDirectory(
                    at: directory, includingPropertiesForKeys: nil,
                    options: [.skipsHiddenFiles]
                )) ?? []
            }.filter { $0.pathExtension == "jsonl" }
        case "codex":
            let sessions = home.appendingPathComponent("sessions", isDirectory: true)
            let files = manager.enumerator(
                at: sessions, includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            )
            return (files?.allObjects as? [URL] ?? []).filter {
                $0.lastPathComponent.hasPrefix("rollout-") && $0.pathExtension == "jsonl"
            }
        default:
            return []
        }
    }

    private static func firstRecord(
        in path: URL, formatter: ISO8601DateFormatter
    ) -> (cwd: String, date: Date)? {
        let key = path.standardizedFileURL as NSURL
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: key.path ?? path.path),
              let size = attributes[.size] as? NSNumber,
              let modified = attributes[.modificationDate] as? Date else { return nil }
        let fileNumber = attributes[.systemFileNumber] as? NSNumber
        if let cached = firstRecordCache.object(forKey: key),
           cached.size == size, cached.modified == modified,
           cached.fileNumber == fileNumber {
            return cached.record
        }
        let record = parseFirstRecord(in: path, formatter: formatter)
        firstRecordCache.setObject(CachedFirstRecord(
            size: size, modified: modified, fileNumber: fileNumber, record: record
        ), forKey: key)
        return record
    }

    private static func parseFirstRecord(
        in path: URL, formatter: ISO8601DateFormatter
    ) -> (cwd: String, date: Date)? {
        guard let file = try? FileHandle(forReadingFrom: path) else { return nil }
        defer { try? file.close() }
        guard let data = try? file.read(upToCount: 65_536) else { return nil }
        for line in data.split(separator: 10) {
            guard let value = try? JSONSerialization.jsonObject(with: Data(line)) as? [String: Any],
                  let timestamp = value["timestamp"] as? String,
                  let cwd = ((value["payload"] as? [String: Any])?["cwd"] as? String)
                    ?? (value["cwd"] as? String) else { continue }
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            let date = formatter.date(from: timestamp)
            formatter.formatOptions = [.withInternetDateTime]
            guard let date = date ?? formatter.date(from: timestamp) else { continue }
            return (URL(fileURLWithPath: cwd).standardizedFileURL.path, date)
        }
        return nil
    }
}

private final class FirstRecordCache: @unchecked Sendable {
    // NSCache synchronizes access to its entries, including calls from both discovery actors.
    private let cache = NSCache<NSURL, CachedFirstRecord>()

    func object(forKey key: NSURL) -> CachedFirstRecord? { cache.object(forKey: key) }
    func setObject(_ value: CachedFirstRecord, forKey key: NSURL) {
        cache.setObject(value, forKey: key)
    }
}

private final class CachedFirstRecord {
    let size: NSNumber
    let modified: Date
    let fileNumber: NSNumber?
    let record: (cwd: String, date: Date)?

    init(size: NSNumber, modified: Date, fileNumber: NSNumber?, record: (cwd: String, date: Date)?) {
        self.size = size
        self.modified = modified
        self.fileNumber = fileNumber
        self.record = record
    }
}
