import Foundation

/// Finds a provider log when Swarm has not received the chair's native session id.
public enum ChairLogDiscovery {
    public static func path(
        provider: String, cwd: String, createdAt: Int,
        homes: [URL]
    ) -> URL? {
        let manager = FileManager.default
        let roots = Array(Set(homes.map(\.standardizedFileURL)))
        var earliest: (path: URL, date: Date)?

        for home in roots {
            let candidates: [URL]
            switch provider {
            case "claude":
                let projects = home.appendingPathComponent("projects", isDirectory: true)
                let directories = (try? manager.contentsOfDirectory(
                    at: projects, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
                )) ?? []
                candidates = directories.flatMap { directory in
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
                candidates = (files?.allObjects as? [URL] ?? []).filter {
                    $0.lastPathComponent.hasPrefix("rollout-") && $0.pathExtension == "jsonl"
                }
            default:
                return nil
            }

            for candidate in candidates {
                guard let record = firstRecord(in: candidate),
                      record.cwd == URL(fileURLWithPath: cwd).standardizedFileURL.path else { continue }
                let delay = record.date.timeIntervalSince1970 - TimeInterval(createdAt)
                guard delay >= 0, delay <= 10 * 60 else { continue }
                if earliest == nil || record.date < earliest!.date
                    || (record.date == earliest!.date && candidate.path > earliest!.path.path) {
                    earliest = (candidate, record.date)
                }
            }
        }
        return earliest?.path
    }

    private static func firstRecord(in path: URL) -> (cwd: String, date: Date)? {
        guard let file = try? FileHandle(forReadingFrom: path) else { return nil }
        defer { try? file.close() }
        guard let data = try? file.read(upToCount: 65_536) else { return nil }
        let formatter = ISO8601DateFormatter()
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
